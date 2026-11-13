//! Artifact loading and the background reload worker.
//!
//! Startup loads synchronously (the first frame needs data). A reload triggered
//! by `r` runs on a worker thread so the dashboard keeps drawing, and its
//! outcome arrives as [`crate::event::Event::Reloaded`].

use std::path::Path;

#[cfg(feature = "terminal")]
use std::path::PathBuf;
#[cfg(feature = "terminal")]
use std::sync::mpsc::SyncSender;
#[cfg(feature = "terminal")]
use std::thread::{self, JoinHandle};

use crate::data::{
    load_ladder_auto, load_ladder_embedded, load_ladder_site, load_live_auto, load_live_embedded,
    load_live_site, load_results_auto, load_results_embedded, load_results_site, LoadedArtifacts,
};
#[cfg(feature = "terminal")]
use crate::event::Event;

/// Read every artifact the dashboard consumes, in one consistent snapshot.
///
/// `explicit` is the user-named results path; when absent the normal
/// target → sample → embedded search order applies.
#[must_use]
pub fn load_all(explicit: Option<&Path>) -> LoadedArtifacts {
    let (source, state) = load_results_auto(explicit);
    LoadedArtifacts {
        results: (source, state),
        ladder: load_ladder_auto(),
        live: load_live_auto(),
    }
}

/// Read every artifact from the copies compiled into the binary.
///
/// The browser build has no filesystem and, until a fetch succeeds, no network
/// document either; this is the same loader the native binary uses as its last
/// resort, minus the path probes. Nothing here opens a file.
#[must_use]
pub fn load_all_embedded() -> LoadedArtifacts {
    LoadedArtifacts {
        results: load_results_embedded(),
        ladder: load_ladder_embedded(),
        live: load_live_embedded(),
    }
}

/// Read the three documents a browser fetched from the site origin, falling
/// back to the compiled-in copy of any document the fetch did not deliver.
///
/// `None` means the fetch failed (network error, non-success status); `Some`
/// bytes always go through the same parsers the file paths use. A fetched
/// document that is present but malformed therefore stays a hard `Invalid`
/// naming its request path rather than silently degrading to embedded data.
#[must_use]
pub fn load_all_site(
    results: Option<&[u8]>,
    ladder: Option<&[u8]>,
    live: Option<&[u8]>,
) -> LoadedArtifacts {
    LoadedArtifacts {
        results: results.map_or_else(load_results_embedded, load_results_site),
        ladder: ladder.map_or_else(load_ladder_embedded, load_ladder_site),
        live: live.map_or_else(load_live_embedded, load_live_site),
    }
}

/// Spawn a background reload. The returned handle is only useful to tests and
/// to a caller that wants to join before exiting; the event loop does not.
#[cfg(feature = "terminal")]
pub fn spawn_reload(explicit: Option<PathBuf>, sender: SyncSender<Event>) -> JoinHandle<()> {
    thread::spawn(move || {
        let loaded = load_all(explicit.as_deref());
        let _ = sender.send(Event::Reloaded(Box::new(loaded)));
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{
        embedded, ArtifactOrigin, LadderLoadState, LadderSource, LiveLoadState, LiveSource,
        LoadState,
    };

    #[test]
    fn the_embedded_load_needs_no_filesystem_and_labels_its_sources() {
        let loaded = load_all_embedded();
        assert!(
            matches!(loaded.results.1, LoadState::Loaded(_)),
            "the compiled-in results must parse: {:?}",
            loaded.results.1
        );
        assert_eq!(
            loaded.results.0.origin(),
            crate::data::ArtifactOrigin::Embedded,
            "the results source must say embedded"
        );
        assert_eq!(loaded.ladder.source(), Some(LadderSource::Embedded));
        assert!(
            matches!(
                loaded.live.view().map(|view| *view.source()),
                Some(LiveSource::Embedded)
            ),
            "the capture must be the compiled-in recording"
        );
    }

    #[test]
    fn a_site_load_parses_every_fetched_document_and_names_its_request_path() {
        let loaded = load_all_site(
            Some(embedded::RESULTS),
            Some(embedded::LADDER_VERIFICATION),
            Some(embedded::DNS_CAPTURE),
        );
        assert!(matches!(loaded.results.1, LoadState::Loaded(_)));
        assert_eq!(loaded.results.0.origin(), ArtifactOrigin::Site);
        assert_eq!(
            loaded.results.0.label(),
            "[site] /data/climb-results.json",
            "the status bar must name the fetched request path"
        );
        assert_eq!(loaded.ladder.source(), Some(LadderSource::Site));
        assert_eq!(
            loaded.live.view().map(|view| *view.source()),
            Some(LiveSource::Site)
        );
    }

    #[test]
    fn a_failed_fetch_falls_back_to_the_embedded_copy_of_that_artifact_only() {
        let loaded = load_all_site(Some(embedded::RESULTS), None, None);
        assert_eq!(
            loaded.results.0.origin(),
            ArtifactOrigin::Site,
            "the results fetch succeeded"
        );
        assert_eq!(
            loaded.ladder.source(),
            Some(LadderSource::Embedded),
            "a failed ladder fetch keeps the compiled-in copy"
        );
        assert_eq!(
            loaded.live.view().map(|view| *view.source()),
            Some(LiveSource::Embedded),
            "a failed capture fetch keeps the compiled-in copy"
        );
        assert!(matches!(loaded.ladder, LadderLoadState::Loaded { .. }));
        assert!(matches!(loaded.live, LiveLoadState::Loaded(_)));
    }

    fn sample_results() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/sample-output/climb-results.json")
    }

    #[test]
    fn load_all_reads_the_sample_artifact_and_its_siblings() {
        let loaded = load_all(Some(&sample_results()));
        assert!(
            matches!(loaded.results.1, LoadState::Loaded(_)),
            "the committed sample must load: {:?}",
            loaded.results.1
        );
        assert!(
            !matches!(loaded.ladder, LadderLoadState::Missing { .. }),
            "the committed ladder artifact must not be reported missing"
        );
        assert!(
            !matches!(loaded.live, LiveLoadState::Missing { .. }),
            "the committed capture must not be reported missing"
        );
    }

    #[cfg(feature = "terminal")]
    #[test]
    fn spawn_reload_returns_immediately_and_delivers_its_result() {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let started = std::time::Instant::now();
        let handle = spawn_reload(Some(sample_results()), sender);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "spawn_reload must not block on the load"
        );
        let event = receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the worker reports a reload outcome");
        match event {
            Event::Reloaded(loaded) => {
                assert!(matches!(loaded.results.1, LoadState::Loaded(_)));
            }
            other => panic!("unexpected event: {other:?}"),
        }
        handle.join().expect("the worker thread finishes");
    }

    #[test]
    fn an_explicit_missing_path_is_a_hard_missing_state() {
        let path = Path::new("target/definitely-not-a-results-file.json");
        let loaded = load_all(Some(path));
        assert!(matches!(
            loaded.results.1,
            LoadState::Missing { ref path } if path == Path::new("target/definitely-not-a-results-file.json")
        ));
    }
}
