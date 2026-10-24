//! Built-in copies of the committed sample artifacts.
//!
//! A bare viewer binary may run outside a checkout: a downloaded release
//! tarball, a container with no bind mount, or an empty working directory. The
//! same three documents that live under `docs/sample-output/` at build time
//! are embedded here so the dashboard still shows recorded data, and labels
//! the source as embedded rather than pretending a file was read.

/// Consolidated results, the same bytes as the committed sample output.
pub const RESULTS: &[u8] = include_bytes!("../../../../docs/sample-output/climb-results.json");

/// Recorded DNS capture, the same bytes as the committed sample output.
pub const DNS_CAPTURE: &[u8] =
    include_bytes!("../../../../docs/sample-output/dns-capture.recorded.json");

/// Recorded ladder verification, the same bytes as the committed sample output.
pub const LADDER_VERIFICATION: &[u8] =
    include_bytes!("../../../../docs/sample-output/ladder-verification.recorded.json");

/// Human-readable names of the embedded documents, used when a label or an
/// error message has to name the source that was actually parsed.
pub const RESULTS_NAME: &str = "docs/sample-output/climb-results.json";
pub const DNS_CAPTURE_NAME: &str = "docs/sample-output/dns-capture.recorded.json";
pub const LADDER_VERIFICATION_NAME: &str = "docs/sample-output/ladder-verification.recorded.json";
