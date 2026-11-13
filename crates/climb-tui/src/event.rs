//! A threaded event pump for the terminal front end.
//!
//! Deliberately *not* async: a crossterm input thread polls with a timeout, a second
//! thread emits a fixed-rate [`Event::Tick`], and both feed one `std::sync::mpsc` channel.
//! The browser runtime drives the same model without threads or crossterm.
//! [`EventHandler::sender`] hands workers a cloneable `Sender<Event>` to report progress
//! as [`Event::Worker`] messages. Dropping the handler signals both threads and joins
//! them, so shutdown is deterministic.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvError, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event::{self as crossterm_event, Event as CrosstermEvent};

use crate::data::LoadedArtifacts;
use crate::keymap::Key;

/// Upper bound on queued events. A paste flood or a stuck worker can fill this,
/// and the input thread then drops new events instead of growing memory without
/// limit. Ticks are droppable too; the next one is 33 ms away.
const MAX_PENDING_EVENTS: usize = 256;

/// Most events read from the terminal in one poll wake-up. Held keys and paste
/// floods arrive in bursts; reading a bounded batch keeps one burst from
/// monopolising the thread, and [`coalesce`] folds repeated keys before they
/// reach the queue.
const MAX_INPUT_BATCH: usize = 32;

/// A message from a future worker thread (benchmark run, replay loader, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerMsg {
    /// A human-readable status line.
    Status(String),
    /// Progress toward a total, for gauges.
    Progress { done: u64, total: u64 },
    /// The worker finished successfully.
    Finished,
    /// The worker failed with a message.
    Failed(String),
}

/// Everything the application loop reacts to.
#[derive(Debug)]
pub enum Event {
    Key(Key),
    Resize(u16, u16),
    Tick,
    Worker(WorkerMsg),
    /// A background reload finished. Boxed so the channel's other variants
    /// stay small.
    Reloaded(Box<LoadedArtifacts>),
}

/// Translate one crossterm event. Mouse, bracketed-paste, focus and other
/// terminal reports are deliberately ignored: the dashboard has no mouse or
/// paste feature, and forwarding them would only add queue pressure.
fn translate(event: CrosstermEvent) -> Option<Event> {
    match event {
        CrosstermEvent::Key(key) => Some(Event::Key(Key::from(key))),
        CrosstermEvent::Resize(width, height) => Some(Event::Resize(width, height)),
        _ => None,
    }
}

/// Fold a burst of events before it enters the bounded queue.
///
/// Consecutive identical key presses collapse to one (a held key is one
/// intent), and only the latest resize in a burst survives (a resize storm is
/// one final size). Order between different events is preserved.
fn coalesce(events: &mut Vec<Event>) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    for event in events.drain(..) {
        let repeated_key = match (out.last(), &event) {
            (Some(Event::Key(previous)), Event::Key(next)) => {
                previous.code == next.code && previous.modifiers == next.modifiers
            }
            _ => false,
        };
        if repeated_key {
            continue;
        }
        if matches!(event, Event::Resize(..)) && matches!(out.last(), Some(Event::Resize(..))) {
            out.pop();
        }
        out.push(event);
    }
    out
}

/// Owns the input and tick threads and the receiving end of the event channel.
pub struct EventHandler {
    rx: Receiver<Event>,
    tx: SyncSender<Event>,
    stop: Arc<AtomicBool>,
    input: Option<JoinHandle<()>>,
    tick: Option<JoinHandle<()>>,
}

impl EventHandler {
    /// Start the input and tick threads; `tick_rate` is the period between ticks (30 Hz is
    /// one tick per ~33 ms).
    pub fn new(tick_rate: Duration) -> Self {
        Self::build(tick_rate, true)
    }

    /// Tick-only handler, used by unit tests so they never touch the real stdin.
    #[cfg(test)]
    pub(crate) fn new_tick_only(tick_rate: Duration) -> Self {
        Self::build_with(tick_rate, false, MAX_PENDING_EVENTS)
    }

    fn build(tick_rate: Duration, spawn_input: bool) -> Self {
        Self::build_with(tick_rate, spawn_input, MAX_PENDING_EVENTS)
    }

    fn build_with(tick_rate: Duration, spawn_input: bool, capacity: usize) -> Self {
        let (tx, rx) = mpsc::sync_channel(capacity);
        let stop = Arc::new(AtomicBool::new(false));

        let input = spawn_input.then(|| {
            let tx = tx.clone();
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let poll_timeout = Duration::from_millis(50);
                let mut batch: Vec<Event> = Vec::with_capacity(MAX_INPUT_BATCH);
                while !stop.load(Ordering::SeqCst) {
                    match crossterm_event::poll(poll_timeout) {
                        Ok(true) => {
                            batch.clear();
                            if let Ok(event) = crossterm_event::read() {
                                batch.extend(translate(event));
                            }
                            while batch.len() < MAX_INPUT_BATCH
                                && matches!(crossterm_event::poll(Duration::ZERO), Ok(true))
                            {
                                match crossterm_event::read() {
                                    Ok(event) => batch.extend(translate(event)),
                                    Err(_) => break,
                                }
                            }
                            for event in coalesce(&mut batch) {
                                match tx.try_send(event) {
                                    Ok(()) => {}
                                    Err(TrySendError::Full(_)) => break,
                                    Err(TrySendError::Disconnected(_)) => return,
                                }
                            }
                        }
                        Ok(false) => {}
                        Err(_) => break,
                    }
                }
            })
        });

        let tick_tx = tx.clone();
        let tick_stop = Arc::clone(&stop);
        let tick = thread::spawn(move || {
            while !tick_stop.load(Ordering::SeqCst) {
                thread::sleep(tick_rate);
                if tick_stop.load(Ordering::SeqCst) {
                    break;
                }
                match tick_tx.try_send(Event::Tick) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => break,
                }
            }
        });

        Self {
            rx,
            tx,
            stop,
            input,
            tick: Some(tick),
        }
    }

    /// Block until the next event arrives, or all senders have dropped.
    pub fn next(&self) -> Result<Event, RecvError> {
        self.rx.recv()
    }

    /// Return the next event without blocking.
    pub fn try_next(&self) -> Result<Event, TryRecvError> {
        self.rx.try_recv()
    }

    /// A cloneable sender for worker threads, to emit [`Event::Worker`] or
    /// [`Event::Reloaded`] messages. The queue is bounded; a worker blocks once
    /// it is full rather than growing it.
    pub fn sender(&self) -> SyncSender<Event> {
        self.tx.clone()
    }
}

impl Drop for EventHandler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.input.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.tick.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn worker_sender_delivers_events() {
        let handler = EventHandler::new_tick_only(Duration::from_secs(60));
        let sender = handler.sender();
        sender
            .send(Event::Worker(WorkerMsg::Progress { done: 1, total: 2 }))
            .expect("channel is open");
        match handler.next().expect("an event is queued") {
            Event::Worker(WorkerMsg::Progress { done, total }) => assert_eq!((done, total), (1, 2)),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn ticks_are_delivered_and_shutdown_joins_quickly() {
        let handler = EventHandler::new_tick_only(Duration::from_millis(5));
        thread::sleep(Duration::from_millis(40));

        let mut saw_tick = false;
        while let Ok(event) = handler.try_next() {
            if matches!(event, Event::Tick) {
                saw_tick = true;
            }
        }
        assert!(saw_tick, "no tick arrived within 40ms at a 5ms tick rate");

        let started = Instant::now();
        drop(handler);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "drop should join the tick thread promptly"
        );
    }

    #[test]
    fn the_event_queue_is_bounded_and_never_blocks_a_producer() {
        let handler = EventHandler::build_with(Duration::from_secs(60), false, 4);
        let sender = handler.sender();
        for done in 0..4 {
            sender
                .try_send(Event::Worker(WorkerMsg::Progress { done, total: 4 }))
                .expect("the first four events fit");
        }
        assert!(
            matches!(sender.try_send(Event::Tick), Err(TrySendError::Full(_))),
            "a full queue must reject, not block or grow"
        );
    }

    #[test]
    fn mouse_paste_and_focus_events_are_ignored() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        assert!(matches!(
            translate(CrosstermEvent::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('q'),
                KeyModifiers::NONE
            ))),
            Some(Event::Key(_))
        ));
        assert!(matches!(
            translate(CrosstermEvent::Resize(100, 30)),
            Some(Event::Resize(100, 30))
        ));
        assert!(translate(CrosstermEvent::Paste("pasted text".to_string())).is_none());
        assert!(translate(CrosstermEvent::FocusGained).is_none());
        assert!(translate(CrosstermEvent::FocusLost).is_none());
        assert!(translate(CrosstermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 4,
            modifiers: KeyModifiers::NONE,
        }))
        .is_none());
    }

    #[test]
    fn a_burst_of_held_keys_and_resizes_is_coalesced() {
        use crate::keymap::KeyCode;
        let held = Event::Key(Key::plain(KeyCode::Char('x')));
        let mut burst = vec![
            Event::Key(Key::plain(KeyCode::Char('x'))),
            Event::Key(Key::plain(KeyCode::Char('x'))),
            Event::Resize(100, 30),
            Event::Resize(120, 40),
            Event::Resize(90, 25),
            Event::Tick,
            held,
        ];
        let out = coalesce(&mut burst);
        assert_eq!(
            out.len(),
            4,
            "two held keys fold to one, three resizes to the last: {out:?}"
        );
        assert!(matches!(out.first(), Some(Event::Key(_))));
        assert!(matches!(out.get(1), Some(Event::Resize(90, 25))));
        assert!(matches!(out.get(2), Some(Event::Tick)));
        assert!(matches!(out.get(3), Some(Event::Key(_))));
    }
}
