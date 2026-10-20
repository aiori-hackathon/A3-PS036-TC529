//! The scripted guided tour: the walk-through as data, plus a runner with an
//! injectable clock.
//!
//! `--tour` walks the dashboard through every tab in a fixed order, holding
//! each step for a scripted dwell while a one-line caption (rendered by
//! [`crate::ui::tour`]) explains what is on screen — the walk-through a
//! reviewer would otherwise have to be talked through, executable on demand
//! for a screen recording.
//!
//! The tour is pure model logic. It performs no I/O of its own: the tabs it
//! selects read exactly the committed or generated artifacts they read under
//! manual navigation, and nothing here reaches the network, spawns a
//! container, or reads a clock by itself. Time enters through
//! [`TourRunner::on_tick`], which the runtime feeds from an injected
//! [`Clock`] — [`Clock::System`] in a live run, [`Clock::Fake`] in tests and
//! evidence dumps, so every timing behaviour is testable without sleeping.

use std::time::{Duration, Instant};

use crate::app::Tab;

/// Fastest supported playback: a step's dwell shrinks to half its scripted
/// length. Matches the `--tour-speed` CLI range.
pub const MAX_SPEED: f64 = 2.0;
/// Slowest supported playback: a step's dwell doubles. Matches the
/// `--tour-speed` CLI range.
pub const MIN_SPEED: f64 = 0.5;

/// One stop of the tour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TourStep {
    /// The tab shown for the step.
    pub tab: Tab,
    /// How long the step is held at speed 1.0.
    pub dwell: Duration,
    /// The one-line caption drawn in the tour bar.
    pub caption: &'static str,
    /// Model changes applied while the step is on screen.
    pub presses: &'static [Press],
}

/// A scripted model change applied while its step is on screen.
///
/// Presses are model operations, not synthesized key events: the tour never
/// pretends a key was pressed, it sets the same model field the key would.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Set the Ladder tab's modelled message count (clamped by the app to the
    /// explorer's leaf limits).
    LadderN(u64),
}

/// The default walk-through, in recording order.
///
/// The order follows the dashboard's own tab order, with the Ladder explorer
/// visited twice — a small ladder first, then a larger one — so the rung
/// decomposition is seen growing before the tour moves on. Every scripted
/// dwell at speed 1.0 sums to well under three minutes; a unit test pins that
/// total so a caption edit cannot silently turn the tour into a long one.
pub const SCRIPT: &[TourStep] = &[
    TourStep {
        tab: Tab::Overview,
        dwell: Duration::from_secs(20),
        caption: "The story in one screen: signing, verifying, wire, environment.",
        presses: &[],
    },
    TourStep {
        tab: Tab::Signing,
        dwell: Duration::from_secs(20),
        caption: "Signing time: one shared ladder signature vs one per message.",
        presses: &[],
    },
    TourStep {
        tab: Tab::Verifying,
        dwell: Duration::from_secs(20),
        caption: "Verification: trust the cached ladder, or re-verify it cold.",
        presses: &[],
    },
    TourStep {
        tab: Tab::Wire,
        dwell: Duration::from_secs(20),
        caption: "Wire bytes per size, and the DNS-over-UDP fit gauge.",
        presses: &[],
    },
    TourStep {
        tab: Tab::Ladder,
        dwell: Duration::from_secs(15),
        caption: "A small ladder: pick a leaf, follow its short path to a rung.",
        presses: &[Press::LadderN(13)],
    },
    TourStep {
        tab: Tab::Ladder,
        dwell: Duration::from_secs(15),
        caption: "The ladder grown large: rungs tile the batch, paths stay short.",
        presses: &[Press::LadderN(32)],
    },
    TourStep {
        tab: Tab::Live,
        dwell: Duration::from_secs(25),
        caption: "The recorded DNS pipeline, replayed step by step.",
        presses: &[],
    },
    TourStep {
        tab: Tab::Integrity,
        dwell: Duration::from_secs(15),
        caption: "Headline claims with JSON pointers, drift-checked live.",
        presses: &[],
    },
    TourStep {
        tab: Tab::Method,
        dwell: Duration::from_secs(20),
        caption: "Baselines, evidence classes, sampling and known limits.",
        presses: &[],
    },
];

/// The scripted dwell of the whole default tour at speed 1.0.
pub fn script_duration() -> Duration {
    SCRIPT.iter().map(|step| step.dwell).sum()
}

/// The tour's time source.
///
/// A live run uses [`Clock::System`], anchored when the dashboard starts.
/// Tests and evidence dumps use [`Clock::Fake`]: its time only moves when the
/// test moves it, so tour timing is exercised deterministically and without
/// sleeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clock {
    /// Wall-clock time since the anchor instant.
    System(Instant),
    /// A fixed timestamp, advanced by hand.
    Fake(Duration),
}

impl Clock {
    /// A wall-clock source anchored at construction.
    #[must_use]
    pub fn system() -> Self {
        Clock::System(Instant::now())
    }

    /// A fake source starting at zero.
    #[must_use]
    pub fn fake() -> Self {
        Clock::Fake(Duration::ZERO)
    }

    /// The current time on this clock.
    #[must_use]
    pub fn now(&self) -> Duration {
        match self {
            Clock::System(start) => start.elapsed(),
            Clock::Fake(elapsed) => *elapsed,
        }
    }

    /// Advance a fake clock by `dt`; a no-op on a system clock, so a test can
    /// call it unconditionally.
    pub fn advance(&mut self, dt: Duration) {
        if let Clock::Fake(elapsed) = self {
            *elapsed += dt;
        }
    }
}

/// Steps through a [`TourStep`] script, tracking pause, completion and
/// per-step progress.
///
/// The runner never reads a clock: the runtime passes the injected clock's
/// `now` into [`TourRunner::on_tick`] on every dashboard tick (30 Hz), and
/// state-changing calls made between ticks ([`TourRunner::next`],
/// [`TourRunner::toggle_pause`]) act on the most recent tick's timestamp — at
/// most one tick (≈33 ms) stale, which is invisible at tour dwell lengths.
pub struct TourRunner {
    steps: Vec<TourStep>,
    index: usize,
    paused: bool,
    done: bool,
    speed: f64,
    step_started_at: Duration,
    last_now: Duration,
}

impl TourRunner {
    /// Start the default [`SCRIPT`] at `speed` (clamped to
    /// [`MIN_SPEED`]..=[`MAX_SPEED`]).
    #[must_use]
    pub fn new(speed: f64) -> Self {
        Self::with_script(SCRIPT.to_vec(), speed)
    }

    /// Start a custom script; an empty script is immediately complete.
    #[must_use]
    pub fn with_script(steps: Vec<TourStep>, speed: f64) -> Self {
        let done = steps.is_empty();
        Self {
            steps,
            index: 0,
            paused: false,
            done,
            speed: sanitize_speed(speed),
            step_started_at: Duration::ZERO,
            last_now: Duration::ZERO,
        }
    }

    /// Advance the tour to the clock's `now`, stepping through as many dwells
    /// as have elapsed (a large clock jump skips straight ahead). While
    /// paused or complete, only the timestamp is tracked.
    pub fn on_tick(&mut self, now: Duration) {
        self.last_now = now;
        if self.done || self.paused {
            // A paused tour restarts its dwell, so paused time never counts
            // against the step.
            self.step_started_at = now;
            return;
        }
        while !self.done {
            let dwell = self.step_dwell();
            if dwell.is_zero() {
                // A zero dwell would never elapse; step past it rather than
                // spinning on the same step.
                self.advance(now);
                continue;
            }
            if now.saturating_sub(self.step_started_at) < dwell {
                break;
            }
            // The next step starts where the previous dwell ended, so no time
            // is lost across a jump.
            let next = self.step_started_at + dwell;
            self.advance(next);
        }
    }

    /// Toggle pause. The step being shown restarts its dwell when the tour is
    /// resumed, so paused time never counts against it.
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        self.step_started_at = self.last_now;
    }

    /// Skip to the next step; after the last step the tour completes and
    /// stays on it until [`crate::app::Action::QuitTour`].
    ///
    /// The next step's dwell starts at `self.last_now` — the timestamp of the
    /// most recent tick — so a caller that has just moved its clock tells the
    /// runner by ticking first (or by ticking right after, which only trims
    /// the skipped step's first frame).
    pub fn next(&mut self) {
        if self.done {
            return;
        }
        let now = self.last_now;
        self.advance(now);
    }

    /// Move to the next step, or complete at the end of the script.
    fn advance(&mut self, now: Duration) {
        if self.index + 1 < self.steps.len() {
            self.index += 1;
        } else {
            self.done = true;
        }
        self.step_started_at = now;
    }

    /// The current step; `None` only for an empty script.
    #[must_use]
    pub fn step(&self) -> Option<&TourStep> {
        self.steps.get(self.index)
    }

    /// The current step's index (0-based).
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }

    /// The number of steps in the script.
    #[must_use]
    pub fn total(&self) -> usize {
        self.steps.len()
    }

    /// The tab the current step shows; `None` for an empty script.
    #[must_use]
    pub fn tab(&self) -> Option<Tab> {
        self.step().map(|step| step.tab)
    }

    /// The caption of the current step; empty for an empty script.
    #[must_use]
    pub fn caption(&self) -> &'static str {
        self.step().map_or("", |step| step.caption)
    }

    /// The scripted presses of the current step; empty for an empty script.
    #[must_use]
    pub fn presses(&self) -> &'static [Press] {
        self.step().map_or(&[], |step| step.presses)
    }

    /// Whether the tour is paused.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Whether the tour has run through every step.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// The playback speed (clamped to [`MIN_SPEED`]..=[`MAX_SPEED`]).
    #[must_use]
    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// How long the current step is held in wall-clock time: the scripted
    /// dwell divided by the playback speed.
    #[must_use]
    pub fn step_dwell(&self) -> Duration {
        match self.step() {
            Some(step) => scale(step.dwell, self.speed),
            None => Duration::ZERO,
        }
    }

    /// The wall-clock time spent in the current step, clamped to its dwell.
    #[must_use]
    pub fn step_elapsed(&self, now: Duration) -> Duration {
        let elapsed = now.saturating_sub(self.step_started_at);
        elapsed.min(self.step_dwell())
    }

    /// How much of the current step's dwell has elapsed, in `0.0..=1.0`.
    /// A complete or empty-script tour reads as `1.0`.
    #[must_use]
    pub fn progress(&self, now: Duration) -> f64 {
        if self.done || self.steps.is_empty() {
            return 1.0;
        }
        let dwell = self.step_dwell();
        if dwell.is_zero() {
            return 1.0;
        }
        (self.step_elapsed(now).as_secs_f64() / dwell.as_secs_f64()).clamp(0.0, 1.0)
    }

    /// The current step's position in the script, as the tour bar draws it
    /// (`3/9`); a completed tour reads as the last step.
    #[must_use]
    pub fn position(&self) -> (usize, usize) {
        if self.steps.is_empty() {
            return (0, 0);
        }
        let shown = self.index.min(self.steps.len() - 1) + 1;
        (shown, self.steps.len())
    }
}

/// Clamp a requested playback speed into the supported band; a non-finite or
/// non-positive speed falls back to 1.0 rather than stalling or racing the
/// script.
fn sanitize_speed(speed: f64) -> f64 {
    if !speed.is_finite() || speed <= 0.0 {
        return 1.0;
    }
    speed.clamp(MIN_SPEED, MAX_SPEED)
}

/// Shrink a scripted duration by the playback speed (2.0 halves it, 0.5
/// doubles it). The quotient of a finite duration and a sanitized speed is
/// always finite and non-negative.
fn scale(dwell: Duration, speed: f64) -> Duration {
    Duration::from_secs_f64(dwell.as_secs_f64() / speed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-second-step script, so fake-clock arithmetic stays obvious.
    fn tiny_script() -> Vec<TourStep> {
        vec![
            TourStep {
                tab: Tab::Overview,
                dwell: Duration::from_secs(1),
                caption: "first",
                presses: &[Press::LadderN(13)],
            },
            TourStep {
                tab: Tab::Ladder,
                dwell: Duration::from_secs(1),
                caption: "second",
                presses: &[],
            },
        ]
    }

    #[test]
    fn default_script_walks_the_dashboard_in_the_documented_order() {
        let tabs: Vec<Tab> = SCRIPT.iter().map(|step| step.tab).collect();
        assert_eq!(
            tabs,
            vec![
                Tab::Overview,
                Tab::Signing,
                Tab::Verifying,
                Tab::Wire,
                Tab::Ladder,
                Tab::Ladder,
                Tab::Live,
                Tab::Integrity,
                Tab::Method,
            ],
            "the tour visits every tab once, the Ladder explorer twice"
        );
        assert_eq!(
            SCRIPT[4].presses,
            &[Press::LadderN(13)],
            "the first Ladder stop models a small batch"
        );
        assert_eq!(
            SCRIPT[5].presses,
            &[Press::LadderN(32)],
            "the second Ladder stop models the explorer's largest batch"
        );
    }

    #[test]
    fn default_script_fits_under_three_minutes_at_default_speed() {
        let total = script_duration();
        assert!(
            total < Duration::from_secs(180),
            "the scripted tour must stay under three minutes, took {total:?}"
        );
        assert!(
            total >= Duration::from_secs(150),
            "the scripted tour looks too thin to explain nine steps: {total:?}"
        );
    }

    #[test]
    fn every_step_carries_a_caption_and_a_positive_dwell() {
        for step in SCRIPT {
            assert!(!step.caption.is_empty(), "empty caption on {:?}", step.tab);
            assert!(
                !step.caption.contains('\n'),
                "the tour bar is one row; a multi-line caption would be clipped"
            );
            assert!(
                step.dwell > Duration::ZERO,
                "zero dwell on {:?} would flash past",
                step.tab
            );
        }
    }

    #[test]
    fn steps_advance_when_the_scripted_dwell_elapses() {
        let mut runner = TourRunner::with_script(tiny_script(), 1.0);
        let mut now = Duration::ZERO;

        runner.on_tick(now);
        assert_eq!(runner.index(), 0);
        assert_eq!(runner.tab(), Some(Tab::Overview));

        now += Duration::from_millis(999);
        runner.on_tick(now);
        assert_eq!(runner.index(), 0, "one millisecond short of the dwell");

        now += Duration::from_millis(1);
        runner.on_tick(now);
        assert_eq!(runner.index(), 1, "the dwell elapsed, the step advances");
        assert_eq!(runner.tab(), Some(Tab::Ladder));
        assert!(!runner.is_done(), "the last step has its own dwell to run");

        now += Duration::from_secs(1);
        runner.on_tick(now);
        assert!(runner.is_done(), "running past the last dwell completes");
        assert_eq!(runner.tab(), Some(Tab::Ladder), "the last frame is kept");
    }

    #[test]
    fn a_clock_jump_skips_straight_to_the_step_it_lands_in() {
        let mut runner = TourRunner::with_script(tiny_script(), 1.0);
        let jump = Duration::from_secs(1) + Duration::from_millis(500);
        runner.on_tick(jump);
        assert_eq!(runner.index(), 1, "1.5 s lands in the second step");
        assert_eq!(
            runner.progress(jump),
            0.5,
            "the second step is half over: no time is lost across a jump"
        );
        assert_eq!(
            runner.step_elapsed(jump),
            Duration::from_millis(500),
            "the leftover half second is the second step's elapsed time"
        );

        let mut late = TourRunner::with_script(tiny_script(), 1.0);
        late.on_tick(Duration::from_secs(4));
        assert!(late.is_done(), "a jump past the whole script completes");

        // A zero-dwell step must not spin: the first tick walks past it.
        let zero = vec![
            TourStep {
                tab: Tab::Overview,
                dwell: Duration::ZERO,
                caption: "instant",
                presses: &[],
            },
            TourStep {
                tab: Tab::Signing,
                dwell: Duration::from_secs(1),
                caption: "held",
                presses: &[],
            },
        ];
        let mut instant = TourRunner::with_script(zero, 1.0);
        instant.on_tick(Duration::ZERO);
        assert_eq!(instant.index(), 1, "a zero-dwell step is walked past");
        assert!(!instant.is_done());
    }

    #[test]
    fn next_starts_the_new_step_at_the_last_tick_so_ticks_land_on_its_first_frame() {
        let mut runner = TourRunner::with_script(tiny_script(), 1.0);
        let mut now = Duration::ZERO;
        runner.on_tick(now);
        now += Duration::from_millis(300);
        runner.on_tick(now);
        runner.next();
        assert_eq!(runner.index(), 1);
        assert_eq!(
            runner.step_elapsed(now),
            Duration::ZERO,
            "the skipped step restarts the dwell clock"
        );
        runner.on_tick(now);
        assert_eq!(runner.index(), 1, "a tick right after n keeps the new step");
        assert_eq!(runner.progress(now), 0.0, "and shows its first frame");
        // A tick that keeps the shared dwell clock consistent (what the
        // runtime does) also lands on the new step's first frame.
        let mut app_style = TourRunner::with_script(tiny_script(), 1.0);
        app_style.on_tick(Duration::from_millis(300));
        app_style.next();
        app_style.on_tick(Duration::from_millis(300));
        assert_eq!(app_style.index(), 1);
        assert_eq!(app_style.progress(Duration::from_millis(300)), 0.0);
    }

    #[test]
    fn a_paused_tour_holds_its_step_and_resumes_with_a_full_dwell() {
        let mut runner = TourRunner::with_script(tiny_script(), 1.0);
        let mut now = Duration::ZERO;
        runner.on_tick(now);
        now += Duration::from_millis(500);
        runner.on_tick(now);

        runner.toggle_pause();
        assert!(runner.is_paused());
        // Ten fake seconds pass while paused: the step must not advance, and
        // the paused time must not count against the dwell on resume.
        now += Duration::from_secs(10);
        runner.on_tick(now);
        assert_eq!(runner.index(), 0, "a paused tour never advances");

        runner.toggle_pause();
        assert!(!runner.is_paused());
        now += Duration::from_millis(500);
        runner.on_tick(now);
        assert_eq!(runner.index(), 0, "the resumed step restarts its dwell");
        now += Duration::from_millis(500);
        runner.on_tick(now);
        assert_eq!(runner.index(), 1, "a full dwell runs after the resume");
    }

    #[test]
    fn speed_scales_the_wall_clock_dwell() {
        let fast = TourRunner::with_script(tiny_script(), 2.0);
        assert_eq!(fast.step_dwell(), Duration::from_millis(500));
        let slow = TourRunner::with_script(tiny_script(), 0.5);
        assert_eq!(slow.step_dwell(), Duration::from_secs(2));

        let mut runner = TourRunner::with_script(tiny_script(), 2.0);
        runner.on_tick(Duration::from_millis(500));
        assert_eq!(runner.index(), 1, "at 2x the half-second dwell elapses");

        let mut runner = TourRunner::with_script(tiny_script(), 0.5);
        runner.on_tick(Duration::from_millis(500));
        assert_eq!(runner.index(), 0, "at 0.5x the two-second dwell has not");
        runner.on_tick(Duration::from_secs(2));
        assert_eq!(runner.index(), 1, "at 0.5x the full two seconds elapse");
    }

    #[test]
    fn speed_is_clamped_to_the_supported_range() {
        assert_eq!(TourRunner::new(1.0).speed(), 1.0);
        assert_eq!(TourRunner::new(2.0).speed(), MAX_SPEED);
        assert_eq!(TourRunner::new(0.5).speed(), MIN_SPEED);
        assert_eq!(
            TourRunner::new(9.0).speed(),
            MAX_SPEED,
            "out-of-range clamps"
        );
        assert_eq!(TourRunner::new(0.0).speed(), 1.0, "zero would stall; 1.0");
        assert_eq!(
            TourRunner::new(-3.0).speed(),
            1.0,
            "negative is not a speed"
        );
        assert!(
            TourRunner::new(f64::NAN).speed().is_finite(),
            "NaN must never reach the dwell arithmetic"
        );
    }

    #[test]
    fn next_skips_a_step_and_completes_after_the_last_one() {
        let mut runner = TourRunner::with_script(tiny_script(), 1.0);
        runner.on_tick(Duration::ZERO);
        runner.next();
        assert_eq!(runner.index(), 1, "next skips the remaining dwell");
        runner.next();
        assert!(runner.is_done(), "next past the last step completes");
        runner.next();
        assert_eq!(runner.index(), 1, "next on a completed tour is a no-op");
        assert!(runner.is_done());
    }

    #[test]
    fn an_idle_tour_reports_its_progress_and_elapsed_time() {
        let mut runner = TourRunner::with_script(tiny_script(), 1.0);
        runner.on_tick(Duration::ZERO);
        assert_eq!(runner.progress(Duration::ZERO), 0.0);
        assert_eq!(
            runner.step_elapsed(Duration::from_millis(250)),
            Duration::from_millis(250)
        );
        assert_eq!(runner.progress(Duration::from_millis(250)), 0.25);
        assert_eq!(
            runner.step_elapsed(Duration::from_secs(9)),
            Duration::from_secs(1),
            "elapsed clamps at the dwell"
        );
        assert_eq!(runner.progress(Duration::from_secs(9)), 1.0);
        assert_eq!(runner.position(), (1, 2));
    }

    #[test]
    fn an_empty_script_completes_immediately_without_panicking() {
        let mut runner = TourRunner::with_script(Vec::new(), 1.0);
        assert!(runner.is_done());
        assert_eq!(runner.step(), None);
        assert_eq!(runner.tab(), None);
        assert_eq!(runner.caption(), "");
        assert!(runner.presses().is_empty());
        assert_eq!(runner.step_dwell(), Duration::ZERO);
        assert_eq!(runner.progress(Duration::from_secs(5)), 1.0);
        runner.on_tick(Duration::from_secs(5));
        runner.next();
        runner.toggle_pause();
        assert!(runner.is_done());
        assert_eq!(runner.position(), (0, 0));
    }

    #[test]
    fn the_fake_clock_starts_at_zero_and_only_moves_when_advanced() {
        let mut clock = Clock::fake();
        assert_eq!(clock.now(), Duration::ZERO);
        clock.advance(Duration::from_secs(3));
        assert_eq!(clock.now(), Duration::from_secs(3));
        // The system clock only moves forward, and never under a test's hand.
        let system = Clock::system();
        let mut copy = system;
        copy.advance(Duration::from_secs(3));
        assert_eq!(copy, system, "advance is a no-op on a system clock");
        assert!(
            system.now() < Duration::from_secs(1),
            "a fresh anchor is young"
        );
    }

    #[test]
    fn script_duration_matches_the_table() {
        assert_eq!(
            script_duration(),
            Duration::from_secs(20 + 20 + 20 + 20 + 15 + 15 + 25 + 15 + 20)
        );
    }
}
