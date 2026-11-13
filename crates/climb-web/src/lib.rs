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
use climb_tui::data::LoadedArtifacts;
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
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_time::Instant;

/// The dashboard's tick rate, matching the terminal's 30 Hz loop.
const TICK: Duration = Duration::from_millis(33);
/// After a suspended tab resumes, do not replay the whole gap. Four ticks is
/// enough for the replay states to move on; the rest of the backlog is dropped.
const MAX_CATCH_UP_TICKS: u32 = 4;

/// The palette's ink, used as the canvas letterbox so a resize never flashes
/// white between the page and the terminal grid.
const INK: Color = Color::Rgb(7, 8, 10);

/// The three documents the static site publishes beside the page. The paths
/// are absolute so the same module works at `/tui` and at any nested route.
const RESULTS_PATH: &str = climb_tui::data::SITE_RESULTS_PATH;
const LADDER_PATH: &str = climb_tui::data::SITE_LADDER_PATH;
const CAPTURE_PATH: &str = climb_tui::data::SITE_CAPTURE_PATH;

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
    // The first frame is already drawable from the compiled-in recordings.
    // The fetch then replaces them with the site's copies and names the
    // source in the status bar; a failed fetch leaves the embedded copy.
    start_data_fetch(Rc::clone(&state));

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

/// The bytes the site origin served, kept so a reload does not silently
/// degrade to the embedded copies.
#[derive(Debug, Default)]
struct FetchedData {
    results: Option<Vec<u8>>,
    ladder: Option<Vec<u8>>,
    capture: Option<Vec<u8>>,
}

impl FetchedData {
    /// Parse the fetched documents, using the embedded copy for any fetch that
    /// did not deliver bytes.
    fn artifacts(&self) -> LoadedArtifacts {
        runtime::load_all_site(
            self.results.as_deref(),
            self.ladder.as_deref(),
            self.capture.as_deref(),
        )
    }
}

/// The model plus the two pieces of web-only state the model must not own.
struct WebState {
    app: App,
    /// When the last 30 Hz tick was delivered.
    last_tick: Instant,
    /// The last successful fetch. `None` until the fetch settles (or forever
    /// when every request failed).
    fetched: Option<FetchedData>,
}

impl WebState {
    fn new(app: App) -> Self {
        Self {
            app,
            last_tick: Instant::now(),
            fetched: None,
        }
    }

    /// The load path for the current session: the site's documents when the
    /// fetch delivered them, otherwise the compiled-in recordings.
    fn load(&self) -> LoadedArtifacts {
        match &self.fetched {
            Some(fetched) => fetched.artifacts(),
            None => runtime::load_all_embedded(),
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
    /// thread here, so a reload re-parses the bytes this session already has:
    /// the site's documents when the fetch succeeded, the compiled-in
    /// recordings otherwise.
    fn apply(&mut self, action: Action) {
        let Some(effect) = self.app.update(action) else {
            return;
        };
        match effect {
            Effect::Quit => self
                .app
                .set_status(Some(StatusLine::info("close the tab to leave"))),
            Effect::LoadResults(_) => {
                let loaded = self.load();
                self.app.apply_reload(loaded);
            }
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

/// Fetch the site's three artifacts, then install them as one snapshot.
///
/// Each request is independent: a failed one falls back to the compiled-in
/// copy of that document through [`runtime::load_all_site`]. The future runs
/// on the page's microtask queue, so the first frame is never delayed by the
/// network.
fn start_data_fetch(state: Rc<RefCell<WebState>>) {
    spawn_local(async move {
        let results = fetch_bytes(RESULTS_PATH).await;
        let ladder = fetch_bytes(LADDER_PATH).await;
        let capture = fetch_bytes(CAPTURE_PATH).await;
        // Drawing is synchronous on this single-threaded page, so the borrow
        // cannot really be held here. If a re-entrant call ever made it so,
        // keep the embedded data rather than panic over a frame.
        let Ok(mut state) = state.try_borrow_mut() else {
            return;
        };
        state.fetched = Some(FetchedData {
            results,
            ladder,
            capture,
        });
        let loaded = state.load();
        state.app.apply_reload(loaded);
        // Clearing the one-shot "reloaded" line exposes the persistent source
        // tail, which names all three artifacts and their fallbacks.
        state.app.set_status(None);
    });
}

/// Fetch one document from the site origin.
///
/// `None` means the request failed (no window, a network error, a non-2xx
/// status, or a body that is not text). A body that is text but not the
/// expected JSON is *not* a failed fetch: it reaches the parser and becomes a
/// hard `Invalid`, named in the UI.
async fn fetch_bytes(path: &str) -> Option<Vec<u8>> {
    let window = web_sys::window()?;
    let response = JsFuture::from(window.fetch_with_str(path)).await.ok()?;
    let response: web_sys::Response = response.dyn_into().ok()?;
    if !response.ok() {
        warn(&format!(
            "climb-web: {path} returned HTTP {}; using the embedded copy",
            response.status()
        ));
        return None;
    }
    let text = JsFuture::from(response.text().ok()?).await.ok()?;
    text.as_string().map(String::into_bytes)
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
    fn the_data_paths_are_the_site_origin_paths() {
        assert_eq!(RESULTS_PATH, "/data/climb-results.json");
        assert_eq!(LADDER_PATH, "/data/ladder-verification.recorded.json");
        assert_eq!(CAPTURE_PATH, "/data/dns-capture.recorded.json");
        for path in [RESULTS_PATH, LADDER_PATH, CAPTURE_PATH] {
            assert!(path.starts_with("/data/"), "{path} is origin-absolute");
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
