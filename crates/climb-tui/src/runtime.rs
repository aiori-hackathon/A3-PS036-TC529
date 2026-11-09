//! Artifact loading and the background reload worker.
//!
//! Startup loads synchronously (the first frame needs data). A reload triggered
//! by `r` runs on a worker thread so the dashboard keeps drawing, and its
//! outcome arrives as [`crate::event::Event::Reloaded`].

use std::path::{Path, PathBuf};
use std::sync::mpsc::SyncSender;
use std::thread::{self, JoinHandle};

use crate::data::{load_ladder_auto, load_live_auto, load_results_auto, LoadedArtifacts};
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

/// Spawn a background reload. The returned handle is only useful to tests and
/// to a caller that wants to join before exiting; the event loop does not.
pub fn spawn_reload(explicit: Option<PathBuf>, sender: SyncSender<Event>) -> JoinHandle<()> {
    thread::spawn(move || {
        let loaded = load_all(explicit.as_deref());
        let _ = sender.send(Event::Reloaded(Box::new(loaded)));
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{LadderLoadState, LiveLoadState, LoadState};

    fn sample_results() -> PathBuf {
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
