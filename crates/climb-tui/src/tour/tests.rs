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
