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

#[cfg(test)]
mod tests {
    use super::*;

    /// A ceiling just above the committed document's measured size. Raw
    /// per-query records belong in the companion sample document, so a jump
    /// past this budget means the split regressed and the binary grew again.
    const RESULTS_BUDGET_BYTES: usize = 120_000;

    fn results() -> serde_json::Value {
        serde_json::from_slice(RESULTS).expect("embedded results parse")
    }

    #[test]
    fn the_embedded_results_stay_within_the_slim_budget() {
        assert!(
            RESULTS.len() <= RESULTS_BUDGET_BYTES,
            "embedded results are {} bytes, over the {RESULTS_BUDGET_BYTES}-byte budget",
            RESULTS.len()
        );
        assert!(
            RESULTS.len() > 80_000,
            "the budget must sit just above the real document, not above an empty placeholder"
        );
    }

    #[test]
    fn the_embedded_results_carry_rtt_summaries_without_raw_queries() {
        let root = results();
        let section = root
            .pointer("/dns_rtt")
            .expect("the committed sample carries an RTT section");
        assert_eq!(
            section.get("schema_version").and_then(|v| v.as_u64()),
            Some(u64::from(climb_report::DNS_RTT_SCHEMA_VERSION))
        );
        let cells = section
            .get("cells")
            .and_then(serde_json::Value::as_array)
            .expect("cells array");
        assert_eq!(cells.len(), 16, "four servers/modes at four target delays");
        for (index, cell) in cells.iter().enumerate() {
            assert!(
                !cell
                    .as_object()
                    .is_some_and(|object| object.contains_key("queries")),
                "cell {index} must not embed per-query records"
            );
            assert!(
                cell.get("median_ms").is_some(),
                "cell {index} keeps its summary"
            );
        }
    }
}
