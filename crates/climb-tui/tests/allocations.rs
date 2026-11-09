//! Allocation-growth guard: rendering 10,000 frames must not grow the
//! per-frame allocation count.
//!
//! A dashboard that allocates a little more every frame eventually stalls a
//! long-running session. This counts every allocation through a wrapping global
//! allocator and compares the first and last thousand frames of a 10,000-frame
//! run. The count per frame should be constant; a small tolerance covers
//! allocator bookkeeping while still catching a leak.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use clap::Parser;
use climb_tui::app::{App, Tab};
use climb_tui::cli::Cli;
use climb_tui::theme::ColorDepth;
use climb_tui::{runtime, ui, Clock};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: this is a thin counter in front of the system allocator. Every
// method forwards the exact pointer and layout it was given, so the system
// allocator's contract is unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract, and the
        // layout is forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` and `layout` came from this allocator's `alloc`, which
        // forwarded them to `System`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same contract as `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same contract as `alloc`; the original pointer and layout are
        // forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

const FRAMES: usize = 10_000;
const WINDOW: usize = 1_000;

fn sample_results() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/sample-output/climb-results.json")
}

fn sample_app() -> App {
    let path = sample_results();
    let path_text = path.to_str().expect("the sample path is UTF-8");
    let cli = Cli::parse_from(["climb-tui", "--results", path_text]);
    let mut app = App::from_cli(&cli, ColorDepth::TrueColor);
    app.set_clock(Clock::fake());
    app.apply_reload(runtime::load_all(Some(Path::new(path_text))));
    app.tab = Tab::Overview;
    app
}

#[test]
fn ten_thousand_frames_do_not_grow_the_allocation_count() {
    let app = sample_app();
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("in-memory terminal");

    // Warm up: the first frames build the terminal's internal buffers.
    for _ in 0..WINDOW {
        terminal
            .draw(|frame| ui::render(frame, &app))
            .expect("a warm-up frame renders");
    }

    let first_start = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..WINDOW {
        terminal
            .draw(|frame| ui::render(frame, &app))
            .expect("a frame renders");
    }
    let first_end = ALLOCATIONS.load(Ordering::Relaxed);

    let remaining = FRAMES - 2 * WINDOW;
    for _ in 0..remaining {
        terminal
            .draw(|frame| ui::render(frame, &app))
            .expect("a frame renders");
    }
    let last_start = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..WINDOW {
        terminal
            .draw(|frame| ui::render(frame, &app))
            .expect("a frame renders");
    }
    let last_end = ALLOCATIONS.load(Ordering::Relaxed);

    let first = first_end - first_start;
    let last = last_end - last_start;
    // Allow one extra allocation per frame for allocator bookkeeping; a leak
    // would grow by at least one per frame and fail this.
    assert!(
        last <= first + WINDOW,
        "allocations grew from {first} in the first {WINDOW} frames to {last} in the last {WINDOW} \
         frames of {FRAMES}"
    );
}
