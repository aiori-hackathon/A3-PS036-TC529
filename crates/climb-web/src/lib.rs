//! The CLIMB dashboard in a browser tab.
//!
//! This crate is a shell: [`climb_tui::app::App`] is the model, `climb_tui::ui`
//! is the widget tree, and the key table is the terminal's own. What changes is
//! the platform underneath — ratzilla's WebGL2, Canvas 2D or DOM terminal in
//! place of crossterm, `performance.now()` in place of `std::time::Instant`, and
//! a `keydown` listener in place of the terminal's input thread.
//!
//! There is no server and no filesystem. Every byte the browser shows is either
//! compiled into the module (the committed recordings) or fetched from the same
//! origin by the data layer.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use climb_tui::app::{Action, App, Effect, StatusLine};
use climb_tui::{runtime, ui, Cli};
use ratatui::prelude::Backend;
use ratatui::style::Color;
use ratatui::Terminal;
use ratzilla::backend::canvas::{CanvasBackend, CanvasBackendOptions};
use ratzilla::backend::dom::{DomBackend, DomBackendOptions};
use ratzilla::backend::webgl2::{FontAtlasConfig, WebGl2Backend, WebGl2BackendOptions};
use ratzilla::{WebEventHandler, WebRenderer};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_time::Instant;

/// The dashboard's tick rate, matching the terminal's 30 Hz loop.
const TICK: Duration = Duration::from_millis(33);
/// After a suspended tab resumes, do not replay the whole gap. Four ticks is
/// enough for the replay states to move on; the rest of the backlog is dropped.
const MAX_CATCH_UP_TICKS: u32 = 4;

/// The palette's ink, used as the canvas letterbox so a resize never flashes
/// white between the page and the terminal grid.
const INK: Color = Color::Rgb(7, 8, 10);

/// Start the dashboard. `trunk` calls this through the generated module
/// initialiser; a panic is reported in the browser console with a stack.
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    run().map_err(|error| JsValue::from_str(&error))
}

fn run() -> Result<(), String> {
    let cli = Cli::viewer();
    let mut app = App::from_cli(&cli, climb_tui::theme::ColorDepth::TrueColor);
    app.apply_reload(runtime::load_all_embedded());
    if tour_requested() {
        app.start_tour(1.0);
    }

    let state = Rc::new(RefCell::new(WebState::new(app)));
    attach_keyboard(Rc::clone(&state))?;
    hide_boot();

    match WebGl2Backend::new_with_options(
        WebGl2BackendOptions::new()
            .grid_id("grid")
            // The dynamic atlas rasterises every glyph the dashboard draws,
            // including box drawing and braille, instead of relying on a
            // pre-baked subset.
            .font_atlas_config(FontAtlasConfig::dynamic(
                &["JetBrains Mono", "Geist Mono", "monospace"],
                16.0,
            ))
            .canvas_padding_color(INK),
    ) {
        Ok(backend) => return drive(backend, state),
        Err(error) => warn(&format!(
            "climb-web: WebGL2 backend unavailable ({error}); falling back to Canvas 2D"
        )),
    }

    match CanvasBackend::new_with_options(CanvasBackendOptions::new().grid_id("grid")) {
        Ok(backend) => return drive(backend, state),
        Err(error) => warn(&format!(
            "climb-web: Canvas 2D backend unavailable ({error}); falling back to DOM"
        )),
    }

    let backend = DomBackend::new_with_options(DomBackendOptions::new(
        Some("grid".to_string()),
        ratzilla::CursorShape::default(),
    ))
    .map_err(|error| format!("no rendering backend could be created: {error}"))?;
    drive(backend, state)
}

/// The model plus the two pieces of web-only state the model must not own.
struct WebState {
    app: App,
    /// When the last 30 Hz tick was delivered.
    last_tick: Instant,
}

impl WebState {
    fn new(app: App) -> Self {
        Self {
            app,
            last_tick: Instant::now(),
        }
    }

    /// Deliver the ticks that came due since the previous frame.
    ///
    /// `requestAnimationFrame` runs at the display's rate (usually 60 Hz); the
    /// dashboard runs at 30 Hz, so roughly every second frame carries one tick.
    fn advance(&mut self) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_tick);
        let due = elapsed.as_nanos() / TICK.as_nanos();
        if due == 0 {
            return;
        }
        let ticks = due.min(u128::from(MAX_CATCH_UP_TICKS)) as u32;
        self.last_tick = now;
        for _ in 0..ticks {
            self.apply(Action::Tick);
        }
    }

    /// Apply one action and run the side effect the model asked for.
    ///
    /// The native runtime performs a reload on a worker thread. There is one
    /// thread here, so a reload re-reads the compiled-in recordings; fetching
    /// from the site origin is the data layer's job, not the model's.
    fn apply(&mut self, action: Action) {
        let Some(effect) = self.app.update(action) else {
            return;
        };
        match effect {
            Effect::Quit => self
                .app
                .set_status(Some(StatusLine::info("close the tab to leave"))),
            Effect::LoadResults(_) => self.app.apply_reload(runtime::load_all_embedded()),
        }
    }
}

/// Draw through ratzilla's requestAnimationFrame loop.
fn drive<B>(backend: B, state: Rc<RefCell<WebState>>) -> Result<(), String>
where
    B: Backend + WebEventHandler + 'static,
{
    let terminal = Terminal::new(backend).map_err(|error| error.to_string())?;
    terminal.draw_web(move |frame| {
        // A key handler can borrow the model while a frame is in flight. In a
        // single-threaded page that cannot overlap, but a re-entrant call is
        // survivable: skip the frame instead of panicking on the borrow.
        let Ok(mut state) = state.try_borrow_mut() else {
            return;
        };
        state.advance();
        ui::render(frame, &state.app);
    });
    Ok(())
}

/// Attach the one keyboard adapter to the page.
///
/// The listener is on `window`, not the canvas, so the dashboard receives keys
/// whatever element has focus. A key the dashboard binds has its browser
/// default suppressed; a key it does not bind is left alone, so page
/// navigation and browser shortcuts keep working.
fn attach_keyboard(state: Rc<RefCell<WebState>>) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let callback =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
            let key = event.key();
            if let Some(action) =
                climb_tui::map_web_key(&key, event.ctrl_key(), event.alt_key(), event.shift_key())
            {
                event.prevent_default();
                if let Ok(mut state) = state.try_borrow_mut() {
                    state.apply(action);
                }
            }
        });
    window
        .add_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref())
        .map_err(|error| format!("attaching the keyboard listener: {error:?}"))?;
    // The page lives as long as the module, so the listener owns its closure
    // for the lifetime of the document rather than risking a dropped callback.
    callback.forget();
    Ok(())
}

/// Whether a page query string asks for the guided tour.
///
/// `?tour=1` and a bare `?tour` both work; any other value is not a request.
fn is_tour_search(search: &str) -> bool {
    search
        .trim_start_matches('?')
        .split('&')
        .any(|pair| pair == "tour=1" || pair == "tour")
}

/// Whether the page asked for the guided tour.
fn tour_requested() -> bool {
    web_sys::window()
        .and_then(|window| window.location().search().ok())
        .is_some_and(|search| is_tour_search(&search))
}

/// Remove the "loading" line once a backend exists and a frame is scheduled.
fn hide_boot() {
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        if let Some(element) = document.get_element_by_id("boot") {
            let _ = element.set_attribute("hidden", "");
        }
    }
}

fn warn(message: &str) {
    web_sys::console::warn_1(&JsValue::from_str(message));
}

#[cfg(test)]
mod tests {
    use super::*;
    use climb_tui::theme::ColorDepth;

    #[test]
    fn the_tour_is_requested_by_exactly_the_documented_query_values() {
        for requested in ["?tour=1", "?tour", "?a=1&tour=1", "tour=1"] {
            assert!(is_tour_search(requested), "{requested:?} starts the tour");
        }
        for ignored in ["", "?", "?tour=0", "?tour=2", "?tour=1x", "?nottour=1"] {
            assert!(!is_tour_search(ignored), "{ignored:?} must not start it");
        }
    }

    #[test]
    fn the_browser_loop_delivers_ticks_at_the_frame_rate() {
        let mut state = WebState::new(App::from_cli(&Cli::viewer(), ColorDepth::TrueColor));
        let before = state.app.tick;
        std::thread::sleep(Duration::from_millis(40));
        state.advance();
        assert!(
            state.app.tick > before,
            "a 40 ms gap must carry at least one 33 ms tick (before {before}, after {})",
            state.app.tick
        );
    }
}
