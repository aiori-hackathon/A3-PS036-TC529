//! A threaded event pump (phase 18.4).
//!
//! Deliberately *not* async: a crossterm input thread polls with a timeout, a second
//! thread emits a fixed-rate [`Event::Tick`], and both feed one `std::sync::mpsc` channel.
//! [`EventHandler::sender`] hands workers a cloneable `Sender<Event>` to report progress
//! as [`Event::Worker`] messages. Dropping the handler signals both threads and joins
//! them, so shutdown is deterministic.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvError, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event::{self as crossterm_event, KeyEvent};

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
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
    Worker(WorkerMsg),
}

/// Owns the input and tick threads and the receiving end of the event channel.
pub struct EventHandler {
    rx: Receiver<Event>,
    tx: Sender<Event>,
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
        Self::build(tick_rate, false)
    }

    fn build(tick_rate: Duration, spawn_input: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));

        let input = spawn_input.then(|| {
            let tx = tx.clone();
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let poll_timeout = Duration::from_millis(50);
                while !stop.load(Ordering::SeqCst) {
                    match crossterm_event::poll(poll_timeout) {
                        Ok(true) => match crossterm_event::read() {
                            Ok(crossterm_event::Event::Key(key)) => {
                                if tx.send(Event::Key(key)).is_err() {
                                    break;
                                }
                            }
                            Ok(crossterm_event::Event::Resize(width, height)) => {
                                if tx.send(Event::Resize(width, height)).is_err() {
                                    break;
                                }
                            }
                            Ok(_) => {}
                            Err(_) => break,
                        },
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
                if tick_tx.send(Event::Tick).is_err() {
                    break;
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

    /// A cloneable sender for worker threads, to emit [`Event::Worker`] messages.
    pub fn sender(&self) -> Sender<Event> {
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
}
