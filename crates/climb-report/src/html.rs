//! Self-contained HTML report rendering (Phase 17).
//!
//! Builds `report.html` from [`ClimbResults`] plus the six Phase 16 SVG
//! charts, embedding each chart as a base64 data URI so the file stands
//! alone. Every headline number rendered into the page is registered in a
//! claims registry — JSON objects with an RFC 6901 JSON Pointer into
//! `results.json`, the expected value, and the exact string shown on the
//! page — embedded as `<script type="application/json" id="climb-claims">`.
//! [`check_report`] re-validates that registry against a results document,
//! so `cargo xtask report --format html --check` detects drift between a
//! shipped report and freshly regenerated numbers.
//!
//! Narrative text quotes only values present in `results.json`;
//! extrapolated quantities appear solely inside chart footnotes. The page
//! states explicitly that the numbers are empirical measurements and that
//! no formal security guarantees or proofs are claimed.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::charts::{fmt_count, CHART_FILES};
use crate::error::ReportError;
use crate::results::{size_display, ClimbResults, DnsCaptureSource};

/// Opening tag of the embedded claims registry; [`check_report`] locates
/// the payload by searching for this exact marker.
const CLAIMS_MARKER: &str = "<script type=\"application/json\" id=\"climb-claims\">";

/// One auditable statement rendered into the report: an RFC 6901 JSON
/// Pointer locating the value in `results.json`, the expected value itself,
/// and the exact human-readable string that appears in the page body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    /// Stable identifier used by the summary layout and failure messages.
    pub id: String,
    /// JSON Pointer into the results document (e.g. `/derived/headline`).
    pub pointer: String,
    /// Value the pointer resolves to when the report was rendered.
    pub value: Value,
    /// String rendered on the page for this claim.
    pub formatted: String,
}

/// Summary-grid rows: display label → claim id. Claims whose id has no
/// value in the results document render as an em dash.
///
/// The three comparison rows use the `comparison@<size>-<metric>` form:
/// the summary shows the Medium dataset, but comparison claims are indexed
/// by declaration order in `results.json`, and that order is not fixed.
/// [`resolve_summary_id`] turns the size-qualified form into the indexed
/// claim id of whichever comparison declares that size, so reordering or
/// dropping a dataset can never silently repoint the grid at another row.
const SUMMARY_ROWS: [(&str, &str); 11] = [
    ("Overall wire reduction", "overall-wire-reduction"),
    ("Largest wire reduction", "largest-wire-reduction"),
    ("Trust-cached verify speedup (avg)", "trust-cached-speedup"),
    ("www.a full signed ladder", "dns-full-signature"),
    ("www.a condensed MTL proof", "dns-condensed-proof"),
    ("www.a wire reduction", "dns-wire-reduction"),
    ("www.a capture provenance", "dns-provenance"),
    ("Ladder: model vs libMTL", "ladder-all-match"),
    ("Wire C vs B (Medium)", "comparison@Medium-wire-c-vs-b"),
    ("Sign C vs A (Medium)", "comparison@Medium-signing-c-vs-a"),
    (
        "Verify cached C vs A (Medium)",
        "comparison@Medium-verifying-cached-c-vs-a",
    ),
];

/// Resolve a [`SUMMARY_ROWS`] id against the built claims.
///
/// Plain ids pass through unchanged; `comparison@<size>-<metric>` is
/// resolved by finding the comparison whose size claim matches, and yields
/// `None` when no such comparison exists (the row renders as an em dash).
fn resolve_summary_id(id: &str, claims: &[Claim]) -> Option<String> {
    let Some(rest) = id.strip_prefix("comparison@") else {
        return Some(id.to_string());
    };
    let (size, metric) = rest.split_once('-')?;
    let index = claims.iter().find_map(|c| {
        let declared = c.id.strip_prefix("comparison-")?.strip_suffix("-size")?;
        (c.formatted == size).then_some(declared)
    })?;
    Some(format!("comparison-{index}-{metric}"))
}

/// Figure captions, index-aligned with [`CHART_FILES`].
const CHART_CAPTIONS: [&str; 7] = [
    "Batch signing time per dataset size — solid bars are Criterion-measured means with 95% CI whiskers; hollow bars are extrapolated.",
    "Batch verification time per dataset size, including trust-cached ladder rows (measured only).",
    "Transmitted bytes — MTL amortised (condensed signature + one signed ladder) vs no amortisation (full MTL signature on every message); both series measured from real output, no modelling.",
    "MTL-amortised signing: measured per-message cost and the OLS fit of the batch total.",
    "Measured peak and resident RSS deltas against the itemised analytical model (libMTL pages + harness buffers + dataset buffers + keypair), with recomputed ratios and residual.",
    "Ladder reuse model: effective per-message time and bytes as a fraction of the unamortised baseline, against reuse count r (log–log). The flat line at 1.0× is parity; a curve crossing it marks the break-even reuse r*.",
    "Modelled DNS response size over UDP vs dataset size — largest response length per signature configuration, against the 1,232-byte EDNS fit threshold and the 4,096-byte TCP-fallback ceiling. Every bar is analytical; none is a measurement.",
];

/// Escape text for HTML body/attribute context.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Standard base64 encoder (RFC 4648 §4) for embedding SVG documents.
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Push a claim if the pointer resolves to a non-null value and a
/// formatted rendering was supplied.
fn push_claim(
    claims: &mut Vec<Claim>,
    root: &Value,
    id: &str,
    pointer: &str,
    formatted: Option<String>,
) {
    let Some(formatted) = formatted else { return };
    let Some(value) = root.pointer(pointer).cloned() else {
        return;
    };
    if value.is_null() {
        return;
    }
    claims.push(Claim {
        id: id.to_string(),
        pointer: pointer.to_string(),
        value,
        formatted,
    });
}

/// Build the claims registry for `results`.
///
/// Every `Some` headline value in the results document becomes one claim;
/// `None`/absent values are skipped so the registry never references a
/// pointer that does not resolve.
pub fn claims(results: &ClimbResults) -> Result<Vec<Claim>, ReportError> {
    let root = serde_json::to_value(results).map_err(|e| ReportError::MalformedData {
        path: PathBuf::from("results"),
        reason: format!("failed to serialize results for the claims registry: {e}"),
    })?;

    let mut claims = Vec::new();
    push_claim(
        &mut claims,
        &root,
        "schema-version",
        "/version",
        Some(results.version.clone()),
    );
    push_claim(
        &mut claims,
        &root,
        "generated-at",
        "/generated_at",
        Some(results.generated_at.clone()),
    );
    push_claim(
        &mut claims,
        &root,
        "overall-wire-reduction",
        "/derived/overall_wire_reduction_percent",
        results
            .derived
            .overall_wire_reduction_percent
            .map(|v| format!("{v:.1}%")),
    );
    push_claim(
        &mut claims,
        &root,
        "largest-wire-reduction",
        "/derived/largest_wire_reduction_percent",
        results
            .derived
            .largest_wire_reduction_percent
            .map(|v| format!("{v:.1}%")),
    );
    push_claim(
        &mut claims,
        &root,
        "trust-cached-speedup",
        "/derived/overall_verifying_trust_cached_speedup_avg",
        results
            .derived
            .overall_verifying_trust_cached_speedup_avg
            .map(|v| format!("{v:.1}\u{d7}")),
    );
    let dns = results.dns_pipeline.as_ref();
    push_claim(
        &mut claims,
        &root,
        "dns-full-signature",
        "/dns_pipeline/full_signature_bytes_www_a",
        dns.and_then(|d| d.full_signature_bytes_www_a)
            .map(|v| format!("{} B", fmt_count(v as f64))),
    );
    push_claim(
        &mut claims,
        &root,
        "dns-condensed-proof",
        "/dns_pipeline/condensed_proof_bytes_www_a",
        dns.and_then(|d| d.condensed_proof_bytes_www_a)
            .map(|v| format!("{} B", fmt_count(v as f64))),
    );
    push_claim(
        &mut claims,
        &root,
        "dns-wire-reduction",
        "/dns_pipeline/reduction_percent_www_a",
        dns.and_then(|d| d.reduction_percent_www_a)
            .map(|v| format!("{v:.1}%")),
    );
    push_claim(
        &mut claims,
        &root,
        "dns-provenance",
        "/dns_pipeline/source",
        dns.map(|d| match d.source {
            DnsCaptureSource::Live => "live run".to_string(),
            DnsCaptureSource::RecordedFixture => match &d.recorded_at {
                Some(at) => format!("recorded fixture ({at})"),
                None => "recorded fixture".to_string(),
            },
        }),
    );
    push_claim(
        &mut claims,
        &root,
        "headline",
        "/derived/headline",
        results.derived.headline.clone(),
    );
    for (idx, comp) in results.comparisons.iter().enumerate() {
        let base = format!("/comparisons/{idx}");
        push_claim(
            &mut claims,
            &root,
            &format!("comparison-{}-size", idx),
            &format!("{base}/size"),
            Some(comp.size.clone()),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("comparison-{}-wire-c-vs-a", idx),
            &format!("{base}/wire_reduction_c_vs_a_percent"),
            comp.wire_reduction_c_vs_a_percent
                .map(|v| format!("{v:.1}%")),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("comparison-{}-wire-c-vs-b", idx),
            &format!("{base}/wire_reduction_c_vs_b_percent"),
            comp.wire_reduction_c_vs_b_percent
                .map(|v| format!("{v:.1}%")),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("comparison-{}-signing-c-vs-a", idx),
            &format!("{base}/signing_speedup_c_vs_a"),
            comp.signing_speedup_c_vs_a.map(|v| format!("{v:.1}\u{d7}")),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("comparison-{}-signing-c-vs-b", idx),
            &format!("{base}/signing_speedup_c_vs_b"),
            comp.signing_speedup_c_vs_b.map(|v| format!("{v:.1}\u{d7}")),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("comparison-{}-verifying-cached-c-vs-a", idx),
            &format!("{base}/verifying_speedup_cached_vs_a"),
            comp.verifying_speedup_cached_vs_a
                .map(|v| format!("{v:.1}\u{d7}")),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("comparison-{}-verifying-cold-c-vs-a", idx),
            &format!("{base}/verifying_speedup_cold_vs_a"),
            comp.verifying_speedup_cold_vs_a
                .map(|v| format!("{v:.1}\u{d7}")),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("comparison-{}-class", idx),
            &format!("{base}/class"),
            Some(comp.class.label().to_string()),
        );
    }
    // Memory claims: per-size decomposition and residuals
    for m in &results.memory.measured {
        let idx = results
            .memory
            .measured
            .iter()
            .position(|x| x.size == m.size)
            .unwrap_or(0);
        let base = format!("/memory/measured/{}", idx);
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-libmtl-pages", m.size),
            &format!("{}/libmtl_analytical_bytes", base),
            Some(m.libmtl_analytical_bytes.to_string()),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-harness-buffers", m.size),
            &format!("{}/harness_buffers_bytes", base),
            Some(m.harness_buffers_bytes.to_string()),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-analytical-total", m.size),
            &format!("{}/analytical_total_bytes", base),
            Some(m.analytical_total_bytes.to_string()),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-residual-bytes", m.size),
            &format!("{}/residual_bytes", base),
            Some(m.residual_bytes.to_string()),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-residual-percent", m.size),
            &format!("{}/residual_percent", base),
            Some(format!("{:.1}%", m.residual_percent)),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-measured-analytical-total", m.size),
            &format!("{}/measured_analytical_total_ratio", base),
            Some(format!("{:.2}x", m.measured_analytical_total_ratio)),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-measured-mean", m.size),
            &format!("{}/measured_mean_bytes", base),
            Some(m.measured_mean_bytes.to_string()),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-dataset-buffers", m.size),
            &format!("{}/dataset_buffers_bytes", base),
            Some(m.dataset_buffers_bytes.to_string()),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-keypair", m.size),
            &format!("{}/keypair_bytes", base),
            Some(m.keypair_bytes.to_string()),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-measured-peak", m.size),
            &format!("{}/measured_peak_bytes", base),
            Some(m.measured_peak_bytes.to_string()),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-samples", m.size),
            &format!("{}/samples", base),
            Some(m.samples.to_string()),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("memory-{}-iqr", m.size),
            &format!("{}/iqr_bytes", base),
            Some(m.iqr_bytes.to_string()),
        );
    }

    // Reuse model: one claim group per rendered size. Registered only when the
    // section will render (non-empty `derived.reuse`), so registry and body
    // stay in lockstep.
    for (idx, reuse) in results.derived.reuse.iter().enumerate() {
        let base = format!("/derived/reuse/{idx}");
        push_claim(
            &mut claims,
            &root,
            &format!("reuse-{idx}-size"),
            &format!("{base}/size"),
            Some(size_display(&reuse.size)),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("reuse-{idx}-time-r"),
            &format!("{base}/breakeven_reuse_time"),
            reuse.breakeven_reuse_time.map(|r| format!("r={r}")),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("reuse-{idx}-bytes-r"),
            &format!("{base}/breakeven_reuse_bytes"),
            reuse.breakeven_reuse_bytes.map(|r| format!("r={r}")),
        );
        push_claim(
            &mut claims,
            &root,
            &format!("reuse-{idx}-class"),
            &format!("{base}/class"),
            Some(reuse.class.label().to_string()),
        );
    }

    // UDP fit: one claim group per modelled row, plus the capture
    // cross-check. Registered only when the section will render (non-empty
    // `derived.udp_fit`), so the registry and the body stay in lockstep.
    if let Some(udp) = results
        .derived
        .udp_fit
        .as_ref()
        .filter(|section| !section.rows.is_empty())
    {
        push_claim(
            &mut claims,
            &root,
            "udpfit-rt",
            "/derived/udp_fit/rows/0/tcp_round_trips",
            Some(format!("{} round trips", udp.rows[0].tcp_round_trips)),
        );
        push_claim(
            &mut claims,
            &root,
            "udpfit-rtt",
            "/derived/udp_fit/rtt_ms",
            udp.rtt_ms.map(|rtt| format!("{rtt:.1} ms")),
        );
        for (idx, row) in udp.rows.iter().enumerate() {
            let base = format!("/derived/udp_fit/rows/{idx}");
            push_claim(
                &mut claims,
                &root,
                &format!("udpfit-{idx}-messages"),
                &format!("{base}/messages"),
                Some(row.messages.to_string()),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("udpfit-{idx}-max"),
                &format!("{base}/max_response_bytes"),
                Some(row.max_response_bytes.to_string()),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("udpfit-{idx}-fit-pct"),
                &format!("{base}/fit_1232_percent"),
                Some(format!("{:.1}%", row.fit_1232_percent)),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("udpfit-{idx}-tc-pct"),
                &format!("{base}/tc_band_percent"),
                Some(format!("{:.1}%", row.tc_band_percent)),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("udpfit-{idx}-tcp-pct"),
                &format!("{base}/needs_tcp_percent"),
                Some(format!("{:.1}%", row.needs_tcp_percent)),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("udpfit-{idx}-payload"),
                &format!("{base}/tcp_payload_bytes"),
                Some(format!("{} B", row.tcp_payload_bytes)),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("udpfit-{idx}-delay"),
                &format!("{base}/tcp_modeled_ms"),
                row.tcp_modeled_ms.map(|ms| format!("{ms:.1} ms")),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("udpfit-{idx}-class"),
                &format!("{base}/class"),
                Some(row.class.label().to_string()),
            );
        }
        if let Some(v) = udp.validation.as_ref() {
            let base = "/derived/udp_fit/validation";
            push_claim(
                &mut claims,
                &root,
                "udpfit-val-zone",
                &format!("{base}/zone_leaves"),
                Some(v.zone_leaves.to_string()),
            );
            push_claim(
                &mut claims,
                &root,
                "udpfit-val-plain",
                &format!("{base}/plain_signature_len_bytes"),
                Some(format!("{} B", v.plain_signature_len_bytes)),
            );
            push_claim(
                &mut claims,
                &root,
                "udpfit-val-full-recorded",
                &format!("{base}/recorded_full_response_bytes"),
                Some(format!("{} B", v.recorded_full_response_bytes)),
            );
            push_claim(
                &mut claims,
                &root,
                "udpfit-val-full-modeled",
                &format!("{base}/modeled_full_response_bytes"),
                Some(format!("{} B", v.modeled_full_response_bytes)),
            );
            push_claim(
                &mut claims,
                &root,
                "udpfit-val-cond-recorded",
                &format!("{base}/recorded_condensed_response_bytes"),
                Some(format!("{} B", v.recorded_condensed_response_bytes)),
            );
            push_claim(
                &mut claims,
                &root,
                "udpfit-val-cond-modeled",
                &format!("{base}/modeled_condensed_response_bytes"),
                Some(format!("{} B", v.modeled_condensed_response_bytes)),
            );
            push_claim(
                &mut claims,
                &root,
                "udpfit-val-tolerance",
                &format!("{base}/tolerance_bytes"),
                Some(format!("{} B", v.tolerance_bytes)),
            );
            push_claim(
                &mut claims,
                &root,
                "udpfit-val-matches",
                &format!("{base}/matches"),
                Some(if v.matches {
                    "exact match".to_string()
                } else {
                    "MISMATCH".to_string()
                }),
            );
        }
    }

    // Signature context: one claim group per context row. Registered only
    // when the section will render (non-empty `derived.signature_context`),
    // so the registry and the body stay in lockstep.
    if !results.derived.signature_context.is_empty() {
        for (idx, row) in results.derived.signature_context.iter().enumerate() {
            let base = format!("/derived/signature_context/{idx}");
            push_claim(
                &mut claims,
                &root,
                &format!("context-{idx}-sig"),
                &format!("{base}/signature_bytes"),
                row.signature_bytes.map(format_context_bytes),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("context-{idx}-sign"),
                &format!("{base}/signing_mean_ns"),
                row.signing_mean_ns.map(format_context_us_per_op),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("context-{idx}-verify"),
                &format!("{base}/verifying_mean_ns"),
                row.verifying_mean_ns.map(format_context_us_per_op),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("context-{idx}-fit"),
                &format!("{base}/fits_1232_percent"),
                row.fits_1232_percent.map(format_context_percent),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("context-{idx}-class"),
                &format!("{base}/class"),
                Some(row.class.label().to_string()),
            );
        }
    }

    // Ladder: model vs libMTL.  Claims are registered only when the section is
    // actually rendered, so the registry and the body stay in lockstep.
    if let Some(ladder) = results
        .ladder
        .as_ref()
        .filter(|ladder| !ladder.sizes.is_empty())
    {
        let date = ladder.recorded_at.get(..10).unwrap_or(&ladder.recorded_at);
        let source_label = match ladder.source {
            crate::ladder::LadderSource::Live => "live run",
            crate::ladder::LadderSource::RecordedFixture => "recorded fixture",
        };
        push_claim(
            &mut claims,
            &root,
            "ladder-provenance",
            "/ladder/source",
            Some(format!("{source_label} {date}")),
        );
        push_claim(
            &mut claims,
            &root,
            "ladder-recorded-at",
            "/ladder/recorded_at",
            Some(ladder.recorded_at.clone()),
        );
        let agreement = if ladder.all_match {
            format!("all {} sizes match", ladder.sizes.len())
        } else {
            let mismatched = ladder.sizes.iter().filter(|r| !r.model_matches).count();
            format!("{mismatched} of {} sizes mismatch", ladder.sizes.len())
        };
        push_claim(
            &mut claims,
            &root,
            "ladder-all-match",
            "/ladder/all_match",
            Some(agreement),
        );
        for (idx, size) in ladder.sizes.iter().enumerate() {
            let base = format!("/ladder/sizes/{idx}");
            push_claim(
                &mut claims,
                &root,
                &format!("ladder-{idx}-leaf"),
                &format!("{base}/leaf_count"),
                Some(size.leaf_count.to_string()),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("ladder-{idx}-rungs"),
                &format!("{base}/rung_count"),
                Some(size.rung_count.to_string()),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("ladder-{idx}-ladder-buffer"),
                &format!("{base}/ladder_buffer_len"),
                Some(format!("{} B", fmt_count(size.ladder_buffer_len as f64))),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("ladder-{idx}-signed-ladder"),
                &format!("{base}/signed_ladder_len"),
                Some(format!("{} B", fmt_count(size.signed_ladder_len as f64))),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("ladder-{idx}-condensed-min"),
                &format!("{base}/condensed_min"),
                Some(format!("{} B", size.condensed_min)),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("ladder-{idx}-condensed-mean"),
                &format!("{base}/condensed_mean"),
                Some(format!("{:.1} B", size.condensed_mean)),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("ladder-{idx}-condensed-max"),
                &format!("{base}/condensed_max"),
                Some(format!("{} B", size.condensed_max)),
            );
            push_claim(
                &mut claims,
                &root,
                &format!("ladder-{idx}-model"),
                &format!("{base}/model_matches"),
                Some(if size.model_matches {
                    "match".to_string()
                } else {
                    "MISMATCH".to_string()
                }),
            );
        }
    }
    Ok(claims)
}

/// Render the per-size memory model-vs-measurement table.
///
/// Every displayed cell is pulled from the claims registry by id, so the body
/// contains each registered memory value verbatim — the property
/// [`check_claims`] enforces. Returns an empty string when there are no rows.
fn render_memory_section(results: &ClimbResults, claims: &[Claim]) -> String {
    if results.memory.measured.is_empty() {
        return String::new();
    }
    let cell = |size: &str, suffix: &str| -> String {
        let id = format!("memory-{size}-{suffix}");
        claims
            .iter()
            .find(|c| c.id == id)
            .map_or("\u{2014}".to_string(), |c| c.formatted.clone())
    };
    let mut rows = String::new();
    for m in &results.memory.measured {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape_html(&m.size),
            escape_html(&cell(&m.size, "libmtl-pages")),
            escape_html(&cell(&m.size, "harness-buffers")),
            escape_html(&cell(&m.size, "dataset-buffers")),
            escape_html(&cell(&m.size, "keypair")),
            escape_html(&cell(&m.size, "analytical-total")),
            escape_html(&cell(&m.size, "measured-peak")),
            escape_html(&cell(&m.size, "measured-mean")),
            escape_html(&cell(&m.size, "samples")),
            escape_html(&cell(&m.size, "iqr")),
            escape_html(&cell(&m.size, "residual-bytes")),
            escape_html(&cell(&m.size, "residual-percent")),
            escape_html(&cell(&m.size, "measured-analytical-total")),
        ));
    }
    format!(
        "<section aria-labelledby=\"memory-h\">\n<h2 id=\"memory-h\">Memory model vs measurement</h2>\n<p class=\"note\">Analytical total = libMTL page model + harness-output buffers + dataset message buffers + keypair key image (an ESTIMATE, not a measurement). Measured resident = median RSS delta over n isolated-process runs, with the run-to-run IQR shown; measured peak = median high-water-mark delta (hwm &minus; baseline), the headline footprint metric. residual = measured resident &minus; analytical total; ratio convention throughout = measured resident &divide; analytical total. Every value is read from the results artifact.</p>\n<table class=\"memtable\">\n<thead><tr><th>Size</th><th>libMTL pages (B)</th><th>harness buffers (B)</th><th>dataset buffers (B)</th><th>keypair (B)</th><th>analytical total (B)</th><th>measured peak (B)</th><th>measured resident (B)</th><th>n runs</th><th>resident IQR (B)</th><th>residual (B)</th><th>residual %</th><th>measured/analytical</th></tr></thead>\n<tbody>\n{rows}</tbody>\n</table>\n</section>\n"
    )
}

/// Render the per-size ladder section (model vs libMTL).
///
/// Every displayed cell is pulled from the claims registry by id, so the body
/// contains each registered ladder value verbatim — the property
/// [`check_claims`] enforces, and therefore the property `report --check`
/// re-validates. Returns an empty string when no ladder artifact was folded
/// into the results.
fn render_ladder_section(results: &ClimbResults, claims: &[Claim]) -> String {
    let Some(ladder) = results.ladder.as_ref() else {
        return String::new();
    };
    if ladder.sizes.is_empty() {
        return String::new();
    }
    let cell = |idx: usize, suffix: &str| -> String {
        let id = format!("ladder-{idx}-{suffix}");
        claims
            .iter()
            .find(|c| c.id == id)
            .map_or("\u{2014}".to_string(), |c| c.formatted.clone())
    };
    let value = |id: &str| -> String {
        claims
            .iter()
            .find(|c| c.id == id)
            .map_or("\u{2014}".to_string(), |c| c.formatted.clone())
    };
    let mut rows = String::new();
    for idx in 0..ladder.sizes.len() {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape_html(&cell(idx, "leaf")),
            escape_html(&cell(idx, "rungs")),
            escape_html(&cell(idx, "ladder-buffer")),
            escape_html(&cell(idx, "signed-ladder")),
            escape_html(&cell(idx, "condensed-min")),
            escape_html(&cell(idx, "condensed-mean")),
            escape_html(&cell(idx, "condensed-max")),
            escape_html(&cell(idx, "model")),
        ));
    }
    let agreement = value("ladder-all-match");
    let provenance = value("ladder-provenance");
    let recorded_at = value("ladder-recorded-at");
    format!(
        "<section aria-labelledby=\"ladder-h\">\n<h2 id=\"ladder-h\">Ladder: model vs libMTL</h2>\n<p class=\"note\">The closed-form ladder model predicts the ladder-buffer length and every leaf's condensed authentication-path length; the recorded artifact carries what libMTL actually emitted at each leaf count. Agreement: <strong>{agreement}</strong>. Provenance: {provenance} (recorded at {recorded_at}). Every value is read from the results artifact; the section states what was measured, not what is proven.</p>\n<table class=\"memtable\">\n<thead><tr><th>N (leaves)</th><th>rungs</th><th>ladder buffer</th><th>signed ladder</th><th>condensed min</th><th>condensed mean</th><th>condensed max</th><th>model</th></tr></thead>\n<tbody>\n{rows}</tbody>\n</table>\n</section>\n"
    )
}

/// Render the cold/warm ladder-reuse section.
///
/// Reports, per dataset size, the smallest candidate reuse count at which the
/// effective per-message time and bytes fall to or below the unamortised
/// baseline. Every displayed cell is pulled from the claims registry by id, so
/// the body contains each registered value verbatim — the property
/// [`check_claims`] enforces. Returns an empty string when no reuse model was
/// folded into the results.
fn render_reuse_section(results: &ClimbResults, claims: &[Claim]) -> String {
    if results.derived.reuse.is_empty() {
        return String::new();
    }
    let value = |idx: usize, suffix: &str| -> String {
        let id = format!("reuse-{idx}-{suffix}");
        claims
            .iter()
            .find(|c| c.id == id)
            .map_or("\u{2014}".to_string(), |c| c.formatted.clone())
    };
    let mut rows = String::new();
    for idx in 0..results.derived.reuse.len() {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape_html(&value(idx, "size")),
            escape_html(&value(idx, "time-r")),
            escape_html(&value(idx, "bytes-r")),
            escape_html(&value(idx, "class")),
        ));
    }
    format!(
        "<section aria-labelledby=\"reuse-h\">\n<h2 id=\"reuse-h\">Ladder reuse: cold vs warm</h2>\n<p class=\"note\">Effective per-message time = (t_cold + (r &minus; 1) &times; t_warm) &divide; r and effective bytes = condensed + ladder &divide; r, both against the unamortised per-message baseline (plain SLH-DSA verify time, full MTL signature bytes). t_cold is one full ladder validation per message; t_warm reuses a trust-cached ladder. r* is the smallest candidate reuse count whose effective per-message time or bytes is at or below the baseline; an em dash means no candidate in range reached parity. Every value is read from the results artifact.</p>\n<table class=\"memtable\">\n<thead><tr><th>Size</th><th>Time r*</th><th>Bytes r*</th><th>Evidence class</th></tr></thead>\n<tbody>\n{rows}</tbody>\n</table>\n</section>\n"
    )
}

/// Render the analytical UDP-fit section: one row per (profile, config,
/// dataset size) plus the zero-tolerance capture cross-check, every value
/// read from the registered claims so the body contains each registered
/// value verbatim — the property [`check_claims`] enforces. Returns an empty
/// string when no UDP-fit model was folded into the results.
fn render_udp_fit_section(results: &ClimbResults, claims: &[Claim]) -> String {
    let Some(section) = results
        .derived
        .udp_fit
        .as_ref()
        .filter(|section| !section.rows.is_empty())
    else {
        return String::new();
    };
    let value = |id: &str| -> String {
        claims
            .iter()
            .find(|c| c.id == id)
            .map_or("\u{2014}".to_string(), |c| c.formatted.clone())
    };
    let mut rows = String::new();
    for (idx, row) in section.rows.iter().enumerate() {
        let delay = match row.tcp_modeled_ms {
            Some(_) => format!(
                "MODELED {} (RTT {})",
                value(&format!("udpfit-{idx}-delay")),
                value("udpfit-rtt")
            ),
            None => "\u{2014}".to_string(),
        };
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape_html(&row.profile_label),
            escape_html(&row.config_label),
            escape_html(&value(&format!("udpfit-{idx}-messages"))),
            escape_html(&value(&format!("udpfit-{idx}-max"))),
            escape_html(&value(&format!("udpfit-{idx}-fit-pct"))),
            escape_html(&value(&format!("udpfit-{idx}-tc-pct"))),
            escape_html(&value(&format!("udpfit-{idx}-tcp-pct"))),
            escape_html(&value(&format!("udpfit-{idx}-payload"))),
            escape_html(&delay),
            escape_html(&value(&format!("udpfit-{idx}-class"))),
        ));
    }

    let validation_html = match section.validation.as_ref() {
        Some(_) => format!(
            "<p class=\"note\">Capture cross-check at zero tolerance: zone inferred as {} leaves from a plain signature of {}; modelled vs recorded full response {} vs {}, condensed {} vs {}; tolerance {} &mdash; <strong>{}</strong>.</p>\n",
                value("udpfit-val-zone"),
                value("udpfit-val-plain"),
                value("udpfit-val-full-modeled"),
                value("udpfit-val-full-recorded"),
                value("udpfit-val-cond-modeled"),
                value("udpfit-val-cond-recorded"),
                value("udpfit-val-tolerance"),
                value("udpfit-val-matches"),
            ),
        None => String::new(),
    };

    let delay_html = if section.rtt_ms.is_some() {
        format!(
            "Fallback delay is shown as <strong>MODELED</strong> at an assumed RTT of {} &mdash; a computed figure at that assumption, not a measurement.",
            value("udpfit-rtt")
        )
    } else {
        "Fallback delay is omitted: no RTT was supplied to the consolidator, so no delay was modelled."
            .to_string()
    };

    format!(
        "<section aria-labelledby=\"udpfit-h\">\n<h2 id=\"udpfit-h\">DNS over UDP: fit and TCP fallback</h2>\n<p class=\"note\">Analytical model, not measurement (evidence class ANALYTICAL): each response profile&rsquo;s length distribution is modelled for each signature configuration &mdash; A plain SLH-DSA, B MTL non-amortised, C MTL amortised &mdash; and banded against the 1,232-byte EDNS UDP fit threshold and the 4,096-byte ceiling above which UDP is not attempted. Percentages are shares of that row&rsquo;s messages; &ldquo;max response&rdquo; is the largest length carrying at least one modelled message. TCP fallback is modelled as {} with the TCP payload adding 4 bytes of length framing to the response. {}\n{validation_html}<table class=\"memtable\">\n<thead><tr><th>Profile</th><th>Config</th><th>Messages</th><th>Max response (B)</th><th>Fits &le;1,232 B</th><th>TC band (1,232&ndash;4,096 B)</th><th>Needs TCP (&gt;4,096 B)</th><th>TCP payload</th><th>Fallback delay</th><th>Evidence</th></tr></thead>\n<tbody>\n{rows}</tbody>\n</table>\n</section>\n",
        value("udpfit-rt"),
        delay_html,
        rows = rows,
    )
}

/// Render the three-way wire-size comparison table (A/B/C).
fn render_wire_comparison_section(results: &ClimbResults, claims: &[Claim]) -> String {
    if results.comparisons.is_empty() {
        return String::new();
    }
    let cell = |idx: usize, suffix: &str| -> String {
        let id = format!("comparison-{idx}-{suffix}");
        claims
            .iter()
            .find(|c| c.id == id)
            .map_or("\u{2014}".to_string(), |c| c.formatted.clone())
    };
    let mut rows = String::new();
    for (idx, comp) in results.comparisons.iter().enumerate() {
        let c_vs_b = cell(idx, "wire-c-vs-b");
        let c_vs_a = cell(idx, "wire-c-vs-a");
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape_html(&comp.size),
            escape_html(&c_vs_b),
            escape_html(&c_vs_a),
            escape_html(&cell(idx, "class")),
        ));
    }
    format!(
        "<section aria-labelledby=\"wire-comparison-h\">\n<h2 id=\"wire-comparison-h\">Wire-size three-way comparison</h2>\n<p class=\"note\">Config A = plain SLH-DSA (one full signature per message, no ladder); Config B = MTL non-amortised (full MTL signature per message); Config C = MTL amortised (one shared ladder + condensed paths). Reductions are (base − C) / base × 100. Evidence class: MEASURED = direct artifact value; EXTRAPOLATED = N × per-message identity; DERIVED = ratio of other values.</p>\n<table class=\"memtable\">\n<thead><tr><th>Size</th><th>C vs B reduction</th><th>C vs A reduction</th><th>Evidence class</th></tr></thead>\n<tbody>\n{rows}</tbody>\n</table>\n</section>\n"
    )
}

/// Render the three-way timing comparison table (A/B/C).
fn render_timing_comparison_section(results: &ClimbResults, claims: &[Claim]) -> String {
    if results.comparisons.is_empty() {
        return String::new();
    }
    let cell = |idx: usize, suffix: &str| -> String {
        let id = format!("comparison-{idx}-{suffix}");
        claims
            .iter()
            .find(|c| c.id == id)
            .map_or("\u{2014}".to_string(), |c| c.formatted.clone())
    };
    let mut rows = String::new();
    for (idx, comp) in results.comparisons.iter().enumerate() {
        let sign_c_vs_b = cell(idx, "signing-c-vs-b");
        let sign_c_vs_a = cell(idx, "signing-c-vs-a");
        let ver_cached_c_vs_a = cell(idx, "verifying-cached-c-vs-a");
        let ver_cold_c_vs_a = cell(idx, "verifying-cold-c-vs-a");
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape_html(&comp.size),
            escape_html(&sign_c_vs_b),
            escape_html(&sign_c_vs_a),
            escape_html(&ver_cached_c_vs_a),
            escape_html(&ver_cold_c_vs_a),
            escape_html(&cell(idx, "class")),
        ));
    }
    format!(
        "<section aria-labelledby=\"timing-comparison-h\">\n<h2 id=\"timing-comparison-h\">Timing three-way comparison</h2>\n<p class=\"note\">Config A = plain SLH-DSA (per-message mean from Criterion); Config B = MTL non-amortised; Config C = MTL amortised. Speedups are A/C or B/C per-message ratios. Verifying cached uses trust-cached ladder; cold re-validates the ladder. Evidence class: MEASURED = direct artifact value; EXTRAPOLATED = N × per-message identity; DERIVED = ratio of other values.</p>\n<table class=\"memtable\">\n<thead><tr><th>Size</th><th>Sign C vs B (×)</th><th>Sign C vs A (×)</th><th>Verify cached C vs A (×)</th><th>Verify cold C vs A (×)</th><th>Evidence class</th></tr></thead>\n<tbody>\n{rows}</tbody>\n</table>\n</section>\n"
    )
}

/// Format a per-message signature size for the context table: integral byte
/// counts group with thousands separators, fractional per-message shares
/// keep two decimals.
fn format_context_bytes(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{} B", fmt_count(v))
    } else {
        format!("{v:.2} B")
    }
}

/// Format a per-operation mean measured in nanoseconds as microseconds.
fn format_context_us_per_op(ns: f64) -> String {
    format!("{:.2} \u{b5}s/op", ns / 1000.0)
}

/// Format a percentage for the context table: whole percents without
/// decimals, otherwise two.
fn format_context_percent(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}%")
    } else {
        format!("{v:.2}%")
    }
}

/// Render the signature-context table: the classical baselines beside
/// configurations A/B/C at the headline dataset size, every cell read from
/// the registered claims so the body contains each registered value
/// verbatim — the property [`check_claims`] enforces. Returns an empty
/// string when no classical context rows were consolidated.
///
/// The one-line caption must state plainly that the classical algorithms are
/// not quantum-resistant — this table is context, not a post-quantum
/// alternative.
fn render_context_section(results: &ClimbResults, claims: &[Claim]) -> String {
    let section = &results.derived.signature_context;
    if section.is_empty() {
        return String::new();
    }
    let value = |id: &str| -> String {
        claims
            .iter()
            .find(|c| c.id == id)
            .map_or("\u{2014}".to_string(), |c| c.formatted.clone())
    };
    let mut rows = String::new();
    for (idx, row) in section.iter().enumerate() {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape_html(&row.label),
            escape_html(&row.family),
            escape_html(&value(&format!("context-{idx}-sig"))),
            escape_html(&value(&format!("context-{idx}-sign"))),
            escape_html(&value(&format!("context-{idx}-verify"))),
            escape_html(&value(&format!("context-{idx}-fit"))),
            escape_html(&value(&format!("context-{idx}-class"))),
        ));
    }
    format!(
        "<section aria-labelledby=\"context-h\">\n<h2 id=\"context-h\">Signature context: classical beside post-quantum</h2>\n<p class=\"note\">Ed25519, ECDSA P-256 and RSA-2048 are classical algorithms &mdash; <strong>not quantum-resistant</strong> &mdash; measured only to put the post-quantum numbers in context. One row per configuration at the {} dataset size: signature bytes carried per message on the wire, per-message sign and verify cost, and the share of modelled responses that fit 1,232 bytes over UDP.</p>\n<p class=\"note\">The fits column is ANALYTICAL (from the UDP-fit model below); the evidence class covers the measured timing and length inputs only. A cell missing from the artifact renders as an em dash.</p>\n<table class=\"memtable\">\n<thead><tr><th>Configuration</th><th>Family</th><th>Signature size</th><th>Sign</th><th>Verify</th><th>Fits &le;1,232 B</th><th>Evidence class</th></tr></thead>\n<tbody>\n{rows}</tbody>\n</table>\n</section>\n",
        escape_html(&section[0].size),
    )
}

/// Render the full HTML report.
///
/// `charts_dir` must contain the six files named in
/// [`crate::charts::CHART_FILES`]; `results_href` is the link placed in the
/// header pointing at the external `results.json` (typically a relative
/// path such as `../climb-results.json`).
pub fn render_html(
    results: &ClimbResults,
    charts_dir: &Path,
    results_href: &str,
) -> Result<String, ReportError> {
    let claims = claims(results)?;

    let mut figures = String::new();
    for (i, name) in CHART_FILES.iter().enumerate() {
        let path = charts_dir.join(name);
        let svg = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ReportError::MissingFile {
                    file: name,
                    dir: charts_dir.to_path_buf(),
                });
            }
            Err(source) => {
                return Err(ReportError::Io { path, source });
            }
        };
        figures.push_str(&format!(
            "<figure><img src=\"data:image/svg+xml;base64,{}\" alt=\"\"><figcaption>{}</figcaption></figure>\n",
            base64_encode(svg.as_bytes()),
            escape_html(CHART_CAPTIONS[i]),
        ));
    }

    let mut summary = String::new();
    for (label, id) in SUMMARY_ROWS {
        let value = resolve_summary_id(id, &claims)
            .and_then(|resolved| {
                claims
                    .iter()
                    .find(|c| c.id == resolved)
                    .map(|c| c.formatted.clone())
            })
            .unwrap_or_else(|| "\u{2014}".to_string());
        summary.push_str(&format!(
            "<div><dt>{}</dt><dd>{}</dd></div>\n",
            escape_html(label),
            escape_html(&value),
        ));
    }

    let headline_html = claims
        .iter()
        .find(|c| c.id == "headline")
        .map_or_else(String::new, |c| {
            format!("<p class=\"lede\">{}</p>\n", escape_html(&c.formatted))
        });

    let env_json = serde_json::to_string_pretty(&results.environment).map_err(|e| {
        ReportError::MalformedData {
            path: PathBuf::from("results"),
            reason: format!("failed to serialize environment fingerprint: {e}"),
        }
    })?;

    let claims_json = serde_json::to_string(&claims).map_err(|e| ReportError::MalformedData {
        path: PathBuf::from("results"),
        reason: format!("failed to serialize the claims registry: {e}"),
    })?;
    // Keep '</script>' out of the payload so hostile strings cannot close
    // the tag early; '<' is only valid inside JSON strings here.
    let claims_json = claims_json.replace('<', "\\u003c");
    let memory_html = render_memory_section(results, &claims);
    let ladder_html = render_ladder_section(results, &claims);
    let reuse_html = render_reuse_section(results, &claims);
    let udp_fit_html = render_udp_fit_section(results, &claims);
    let wire_comparison_html = render_wire_comparison_section(results, &claims);
    let timing_comparison_html = render_timing_comparison_section(results, &claims);
    let context_html = render_context_section(results, &claims);

    Ok(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>CLIMB — Benchmark Report</title>
<style>
:root {{ --bg:#f7f8fa; --surface:#fff; --ink:#16181d; --muted:#5c6270; --line:#e3e6eb; --accent:#1f77b4; }}
* {{ box-sizing:border-box; }}
body {{ margin:0; background:var(--bg); color:var(--ink);
  font:16px/1.55 system-ui,-apple-system,"Segoe UI",Roboto,Helvetica,Arial,sans-serif; }}
.wrap {{ max-width:56rem; margin:0 auto; padding:0 1.5rem; }}
header {{ padding:3rem 0 1rem; }}
h1 {{ font-size:1.6rem; letter-spacing:-.01em; margin:0 0 .35rem; }}
.subtitle,.links {{ color:var(--muted); margin:.2rem 0; }}
.lede {{ font-size:1.08rem; margin:.9rem 0 0; }}
a {{ color:var(--accent); }}
section {{ background:var(--surface); border:1px solid var(--line); border-radius:10px;
  padding:1.25rem 1.5rem; margin:1.25rem 0; }}
h2 {{ font-size:.95rem; text-transform:uppercase; letter-spacing:.06em; color:var(--muted);
  margin:0 0 1rem; }}
dl.summary {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(14rem,1fr));
  gap:.75rem; margin:0; }}
dl.summary div {{ background:#fbfcfd; border:1px solid var(--line); border-radius:8px;
  padding:.75rem .9rem; }}
dt {{ font-size:.75rem; text-transform:uppercase; letter-spacing:.05em; color:var(--muted); }}
dd {{ margin:.2rem 0 0; font-size:1.3rem; font-weight:650; font-variant-numeric:tabular-nums; }}
figure {{ margin:1.5rem 0; }}
figure img {{ max-width:100%; height:auto; display:block; border:1px solid var(--line);
  border-radius:6px; }}
figcaption {{ font-size:.85rem; color:var(--muted); margin-top:.5rem; }}
.note {{ font-size:.88rem; color:var(--muted); }}
details {{ margin:.5rem 0 0; }}
summary {{ cursor:pointer; color:var(--muted); font-size:.9rem; }}
pre {{ background:#f1f3f6; padding:1rem; border-radius:8px; overflow:auto; font-size:.78rem; }}
.memtable {{ width:100%; border-collapse:collapse; font-size:.82rem; font-variant-numeric:tabular-nums; }}
.memtable th,.memtable td {{ padding:.4rem .5rem; border-bottom:1px solid var(--line); text-align:right; white-space:nowrap; }}
.memtable th:first-child,.memtable td:first-child {{ text-align:left; }}
footer {{ color:var(--muted); font-size:.85rem; padding:1.5rem 0 3rem; }}
@media print {{ body {{ background:#fff; }} section {{ border:none; padding:0; break-inside:avoid; }} }}
</style>
</head>
<body>
<div class="wrap">
<header>
<h1>CLIMB — Cryptographic Ladder Impact Measurement Benchmark</h1>
<p class="subtitle">Merkle Tree Ladder mode vs full-signature DNSSEC signing (PS-036) &middot; schema {version} &middot; generated {generated_at}</p>
<p class="links"><a href="{results_href}">results.json</a> — machine-readable source for every number on this page.</p>
{headline_html}</header>
<section aria-labelledby="summary-h">
<h2 id="summary-h">Key results</h2>
<dl class="summary">
{summary}</dl>
</section>
<section aria-labelledby="charts-h">
<h2 id="charts-h">Charts</h2>
{figures}</section>
<section aria-labelledby="baselines-h">
<h2 id="baselines-h">Signing baselines</h2>
<p class="note">Both signing series sign the same deterministic message set; they differ only in what is put on the wire and what has to be signed.</p>
<ul class="note">
<li><strong>MTL amortised (condensed signature + one signed ladder)</strong> — one signed Merkle tree ladder is shared across the whole batch and every message carries only a condensed authentication path. Wire bytes = <code>ladder + Σ&nbsp;condensed<sub>i</sub></code>.</li>
<li><strong>No amortisation (full MTL signature on every message)</strong> — every message carries a complete MTL signature: its condensed path concatenated with the ladder. Wire bytes = <code>Σ&nbsp;condensed<sub>i</sub> + N &times; ladder</code>.</li>
</ul>
<p class="note">"No amortisation" is the MTL wrapper's non-amortised mode, not a plain SLH-DSA RRSIG (which would carry a single full SLH-DSA signature per RRset and no ladder). The wire-byte formulas for each configuration are documented in <code>docs/BASELINES.md</code>.</p>
</section>
{wire_comparison_html}{timing_comparison_html}{context_html}{memory_html}{ladder_html}{reuse_html}{udp_fit_html}<section aria-labelledby="method-h">
<h2 id="method-h">Methodology &amp; scope</h2>
<p class="note">Timings are Criterion-measured means with 95% confidence intervals. Hollow bars and fitted curves are extrapolations (N &times; mean per-message cost, or an OLS fit of the MTL-amortised batch total) — descriptive summaries of the measured runs, not measurements themselves; every chart carries a three-line footnote naming its source files. Wire-size and memory figures are exact measurements of real output.</p>
<p class="note">This report presents empirical benchmark measurements only. No formal security guarantees or proofs are claimed.</p>
<details>
<summary>Environment fingerprint</summary>
<pre>{env_json}</pre>
</details>
</section>
<footer>CLIMB benchmark report &middot; rendered by <code>cargo xtask report --format html</code></footer>
</div>
<script type="application/json" id="climb-claims">{claims_json}</script>
</body>
</html>
"#,
        version = escape_html(&results.version),
        generated_at = escape_html(&results.generated_at),
        results_href = escape_html(results_href),
        headline_html = headline_html,
        summary = summary,
        figures = figures,
        env_json = escape_html(&env_json),
        claims_json = claims_json,
        memory_html = memory_html,
        ladder_html = ladder_html,
        reuse_html = reuse_html,
        wire_comparison_html = wire_comparison_html,
        timing_comparison_html = timing_comparison_html,
    ))
}

/// Compare two JSON values with float tolerance, recursively.
fn values_match(expected: &Value, found: &Value) -> bool {
    match (expected, found) {
        (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => a == b || ((a - b) / a.abs().max(b.abs()).max(1.0)).abs() < 1e-9,
            _ => a == b,
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| values_match(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| values_match(v, w)))
        }
        _ => expected == found,
    }
}

/// Agreement state of one claim row in a [`CheckReport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// The report carries the claim and it matches the results document.
    Agree,
    /// The report carries the claim but its pointer, value, formatted
    /// string, or rendered body text disagrees with the results document.
    Drift,
    /// The claim is absent from the report (or the whole registry is
    /// unavailable), or a registry entry has no backing in the results.
    Missing,
}

/// One row of the integrity check: a claim as implied by the results
/// document next to what the report actually carries.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimCheck {
    /// Claim identifier (stable across renders).
    pub id: String,
    /// JSON Pointer into the results document.
    pub pointer: String,
    /// Value the pointer resolves to in the checked results document
    /// (`Null` for registry-only rows, which have no results backing).
    pub value_in_results: Value,
    /// Value the report's registry carries for this claim, when present.
    pub value_in_report: Option<Value>,
    /// Agreement state.
    pub status: CheckStatus,
    /// Human-readable explanation of any non-agreeing state.
    pub detail: String,
}

/// Full result of validating a report against a results document.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckReport {
    /// One row per claim implied by the results, plus one per registry
    /// entry without a results backing.
    pub claims: Vec<ClaimCheck>,
    /// One human-readable message per detected problem; empty means the
    /// report and the results document agree.
    pub problems: Vec<String>,
    /// True when the registry was located, parsed, derived from `results`,
    /// and every row agrees.
    pub registry_ok: bool,
    /// False only when the report file did not exist at the checked path.
    pub report_found: bool,
}

impl CheckReport {
    /// The flat problem messages (see [`CheckReport::problems`]) — the
    /// same list `report --check` prints.
    pub fn messages(&self) -> Vec<String> {
        self.problems.clone()
    }

    /// Rows whose status is [`CheckStatus::Drift`].
    pub fn drifts(&self) -> impl Iterator<Item = &ClaimCheck> {
        self.claims
            .iter()
            .filter(|c| c.status == CheckStatus::Drift)
    }

    /// Row counts as `(agree, drift, missing)`.
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut counts = (0, 0, 0);
        for c in &self.claims {
            match c.status {
                CheckStatus::Agree => counts.0 += 1,
                CheckStatus::Drift => counts.1 += 1,
                CheckStatus::Missing => counts.2 += 1,
            }
        }
        counts
    }

    /// The outcome for an absent report file: every claim the results
    /// imply is reported as missing and the single problem names the path.
    fn absent(results: &ClimbResults, report_path: &Path) -> CheckReport {
        let mut problems = vec![format!("report not found at {}", report_path.display())];
        let rows = match claims(results) {
            Ok(expected) => expected
                .into_iter()
                .map(|c| ClaimCheck {
                    id: c.id,
                    pointer: c.pointer,
                    value_in_results: c.value,
                    value_in_report: None,
                    status: CheckStatus::Missing,
                    detail: "report not found".to_string(),
                })
                .collect(),
            Err(e) => {
                problems.push(format!("failed to derive claims from results: {e}"));
                Vec::new()
            }
        };
        CheckReport {
            claims: rows,
            problems,
            registry_ok: false,
            report_found: false,
        }
    }
}

/// The claims registry embedded in a report page, or why it could not be
/// decoded.
enum Registry {
    /// The `#climb-claims` marker is missing from the page.
    MissingMarker,
    /// The marker is present but the payload never terminates.
    Unterminated,
    /// The payload terminated but is not valid JSON.
    Invalid(String),
    /// The decoded registry.
    Claims(Vec<Claim>),
}

/// Locate and decode the claims registry embedded in `html`.
fn extract_registry(html: &str) -> Registry {
    let Some(start) = html.find(CLAIMS_MARKER) else {
        return Registry::MissingMarker;
    };
    let rest = &html[start + CLAIMS_MARKER.len()..];
    let Some(end) = rest.find("</script>") else {
        return Registry::Unterminated;
    };
    match serde_json::from_str::<Vec<Claim>>(&rest[..end]) {
        Ok(c) => Registry::Claims(c),
        Err(e) => Registry::Invalid(e.to_string()),
    }
}

/// Core integrity check: compare the registry embedded in `html` against
/// claims rebuilt from `results`, producing both the row-level
/// [`CheckReport`] and the flat message list printed by `report --check`.
///
/// The registry is never treated as a source of truth: expected claims are
/// rebuilt from `results` and every embedded claim is compared against
/// them (pointer, value with relative float tolerance 1e-9, and
/// `formatted` string), each expected claim's `formatted` string must
/// appear in the page body (raw or HTML-escaped), and registry entries not
/// backed by `results` are flagged. `with_chart_count` additionally tallies
/// embedded chart data URIs.
fn check_html(html: &str, results: &ClimbResults, with_chart_count: bool) -> CheckReport {
    let mut problems = Vec::new();
    let embedded = match extract_registry(html) {
        Registry::MissingMarker => {
            problems.push("claims registry (#climb-claims) missing from HTML".to_string());
            None
        }
        Registry::Unterminated => {
            problems.push("claims registry payload not terminated".to_string());
            None
        }
        Registry::Invalid(e) => {
            problems.push(format!("claims registry payload is not valid JSON: {e}"));
            None
        }
        Registry::Claims(c) => Some(c),
    };
    let expected = match claims(results) {
        Ok(c) => c,
        Err(e) => {
            problems.push(format!("failed to derive claims from results: {e}"));
            return CheckReport {
                claims: Vec::new(),
                problems,
                registry_ok: false,
                report_found: true,
            };
        }
    };

    let mut rows = Vec::with_capacity(expected.len());
    for claim in &expected {
        let mut status = CheckStatus::Agree;
        let mut detail = String::new();
        let mut value_in_report = None;
        let found = embedded
            .as_ref()
            .and_then(|c| c.iter().find(|f| f.id == claim.id));
        match found {
            None => {
                status = CheckStatus::Missing;
                if embedded.is_some() {
                    let msg = format!(
                        "claim `{}`: present in results but missing from registry",
                        claim.id
                    );
                    detail = msg.clone();
                    problems.push(msg);
                } else {
                    detail = "claims registry unavailable".to_string();
                }
            }
            Some(found) => {
                value_in_report = Some(found.value.clone());
                let mut msgs = Vec::new();
                if found.pointer != claim.pointer {
                    msgs.push(format!(
                        "claim `{}`: pointer drift — registry has {}, results imply {}",
                        claim.id, found.pointer, claim.pointer
                    ));
                }
                if !values_match(&found.value, &claim.value) {
                    msgs.push(format!(
                        "claim `{}`: value drift — results has {}, report claims {}",
                        claim.id, claim.value, found.value
                    ));
                }
                if found.formatted != claim.formatted {
                    msgs.push(format!(
                        "claim `{}`: formatted drift — results imply {:?}, registry has {:?}",
                        claim.id, claim.formatted, found.formatted
                    ));
                }
                if !msgs.is_empty() {
                    status = CheckStatus::Drift;
                    detail = msgs.join(" ");
                    problems.extend(msgs);
                }
            }
        }
        if !html.contains(&claim.formatted) && !html.contains(&escape_html(&claim.formatted)) {
            let msg = format!(
                "claim `{}`: rendered string {:?} not found in HTML body",
                claim.id, claim.formatted
            );
            if status == CheckStatus::Agree {
                status = CheckStatus::Drift;
                detail = msg.clone();
            } else {
                detail = format!("{detail} {msg}");
            }
            problems.push(msg);
        }
        rows.push(ClaimCheck {
            id: claim.id.clone(),
            pointer: claim.pointer.clone(),
            value_in_results: claim.value.clone(),
            value_in_report,
            status,
            detail,
        });
    }
    if let Some(embedded) = &embedded {
        for claim in embedded {
            if !expected.iter().any(|e| e.id == claim.id) {
                let msg = format!(
                    "claim `{}`: present in registry but not in results",
                    claim.id
                );
                problems.push(msg.clone());
                rows.push(ClaimCheck {
                    id: claim.id.clone(),
                    pointer: claim.pointer.clone(),
                    value_in_results: Value::Null,
                    value_in_report: Some(claim.value.clone()),
                    status: CheckStatus::Missing,
                    detail: msg,
                });
            }
        }
    }
    if with_chart_count {
        let embedded_charts = html.matches("data:image/svg+xml;base64,").count();
        if embedded_charts != CHART_FILES.len() {
            problems.push(format!(
                "embedded chart count is {embedded_charts}, expected {}",
                CHART_FILES.len()
            ));
        }
    }
    let registry_ok = embedded.is_some() && rows.iter().all(|r| r.status == CheckStatus::Agree);
    CheckReport {
        claims: rows,
        problems,
        registry_ok,
        report_found: true,
    }
}

/// Validate the claims registry embedded in `html` against `results`.
///
/// Returns one human-readable message per detected problem; an empty vec
/// means the report and the results document agree. See [`check_html`] for
/// the exact rules.
pub fn check_claims(html: &str, results: &ClimbResults) -> Vec<String> {
    check_html(html, results, false).problems
}

/// Full report validation for `--check`: [`check_claims`] plus a count of
/// embedded chart data URIs.
pub fn check_report(html: &str, results: &ClimbResults) -> Vec<String> {
    check_html(html, results, true).problems
}

/// Validate the report file at `report_path` against `results`.
///
/// An absent file is a warning-shaped outcome, not an error: the returned
/// [`CheckReport`] has `report_found == false`, names the path in its
/// problem list, and lists every claim the results imply as missing. A
/// file that exists but cannot be read is a hard error naming the path.
pub fn check(results: &ClimbResults, report_path: &Path) -> Result<CheckReport, ReportError> {
    match std::fs::read_to_string(report_path) {
        Ok(html) => Ok(check_html(&html, results, true)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(CheckReport::absent(results, report_path))
        }
        Err(source) => Err(ReportError::Io {
            path: report_path.to_path_buf(),
            source,
        }),
    }
}

/// Compute a relative href from the directory holding the report to the
/// results file, e.g. `target/climb-report` + `target/climb-results.json`
/// → `../climb-results.json`. Falls back to the literal path when the two
/// sides mix absolute and relative forms.
pub fn relative_href(from_dir: &Path, to: &Path) -> String {
    let Some(to_file) = to.file_name() else {
        return to.display().to_string();
    };
    let to_dir = to.parent().unwrap_or(Path::new(""));
    let from: Vec<_> = from_dir
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect();
    let tgt: Vec<_> = to_dir
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect();
    if from.is_empty() && tgt.is_empty() {
        return to_file.to_string_lossy().into_owned();
    }
    if !from.is_empty() && !tgt.is_empty() {
        let from_abs = matches!(from[0], std::path::Component::RootDir);
        let tgt_abs = matches!(tgt[0], std::path::Component::RootDir);
        if from_abs != tgt_abs {
            return to.display().to_string();
        }
    }
    let common = from
        .iter()
        .zip(tgt.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts: Vec<String> = vec!["..".to_string(); from.len() - common];
    parts.extend(
        tgt[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.push(to_file.to_string_lossy().into_owned());
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::{
        AnalyticalMemoryModel, BenchmarkResults, ConfigComparison, DerivedSummary,
        DnsCaptureSource, DnsPipelineHeadline, EnvironmentFingerprint, EvidenceClass, MemoryResult,
        MemoryResults, ReuseCurvePoint, ReuseResult, SignatureContextRow,
    };

    fn synthetic_ladder_section() -> crate::ladder::LadderSection {
        use crate::ladder::{LadderSection, LadderSizeRow, LadderSource};
        LadderSection {
            source: LadderSource::RecordedFixture,
            recorded_at: "2026-10-14T00:16:11+05:30".to_string(),
            algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
            all_match: true,
            sizes: vec![
                LadderSizeRow {
                    leaf_count: 8,
                    rung_count: 2,
                    ladder_buffer_len: 104,
                    signed_ladder_len: 7_964,
                    condensed_min: 108,
                    condensed_max: 108,
                    condensed_mean: 108.0,
                    model_matches: true,
                },
                LadderSizeRow {
                    leaf_count: 13,
                    rung_count: 3,
                    ladder_buffer_len: 132,
                    signed_ladder_len: 7_988,
                    condensed_min: 124,
                    condensed_max: 124,
                    condensed_mean: 124.0,
                    model_matches: true,
                },
            ],
        }
    }

    fn synthetic_results() -> ClimbResults {
        ClimbResults {
            version: "1.0".to_string(),
            generated_at: "2026-10-08T12:00:00Z".to_string(),
            workspace_root: ".".to_string(),
            benchmark: BenchmarkResults {
                signing: Vec::new(),
                verifying: Vec::new(),
            },
            wire_size: Vec::new(),
            memory: MemoryResults {
                measured: Vec::new(),
                analytical_model: AnalyticalMemoryModel {
                    tree_page_size_bytes: 4096,
                    tree_max_pages: 1,
                    randomizer_pages: 1,
                    hash_size_bytes: 32,
                },
            },
            environment: EnvironmentFingerprint {
                schema_version: 1,
                timestamp: "test".to_string(),
                system: serde_json::json!({}),
                toolchain: serde_json::json!({}),
                cpu: serde_json::json!({}),
                submodules: Vec::new(),
                docker: serde_json::json!({}),
                benchmark_runtime: serde_json::json!({}),
            },
            dns_pipeline: Some(DnsPipelineHeadline {
                full_signature_bytes_www_a: Some(24_438),
                condensed_proof_bytes_www_a: Some(658),
                reduction_percent_www_a: Some(97.3),
                notes: None,
                source: DnsCaptureSource::RecordedFixture,
                recorded_at: Some("2026-10-08T12:00:00Z".to_string()),
                algorithm: Some("SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string()),
            }),
            dns_anatomy: None,
            ladder: Some(synthetic_ladder_section()),
            plain: Vec::new(),
            classical: Vec::new(),
            comparisons: vec![ConfigComparison {
                size: "Medium".to_string(),
                wire_reduction_c_vs_a_percent: Some(97.0),
                wire_reduction_c_vs_b_percent: Some(96.7),
                signing_speedup_c_vs_a: Some(100.0),
                signing_speedup_c_vs_b: Some(9800.0),
                verifying_speedup_cached_vs_a: Some(50.0),
                verifying_speedup_cold_vs_a: Some(10.0),
                class: EvidenceClass::Derived,
            }],
            derived: DerivedSummary {
                overall_wire_reduction_percent: Some(96.45),
                largest_wire_reduction_percent: Some(97.0),
                overall_verifying_trust_cached_speedup_avg: Some(10.125),
                headline: Some("MTL cuts www.a wire bytes by 97.3%.".to_string()),
                reuse: Vec::new(),
                udp_fit: None,
                signature_context: Vec::new(),
            },
            warnings: Vec::new(),
        }
    }

    fn unique_temp_dir(tag: &str) -> std::path::PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!("climb-report-{tag}-{}-{n}", std::process::id()))
    }

    fn render_fixture() -> (ClimbResults, String) {
        let results = synthetic_results();
        let dir = unique_temp_dir("html-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        crate::charts::generate_charts(&results, &dir).expect("charts");
        let html = render_html(&results, &dir, "../climb-results.json").expect("render");
        let _ = std::fs::remove_dir_all(&dir);
        (results, html)
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(&[0x00, 0x01, 0xff]), "AAH/");
    }

    #[test]
    fn render_is_self_contained() {
        let (results, html) = render_fixture();
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.trim_end().ends_with("</html>"));
        assert_eq!(
            html.matches("data:image/svg+xml;base64,").count(),
            CHART_FILES.len()
        );
        assert!(html.contains(CLAIMS_MARKER));
        assert!(html.contains(r#"href="../climb-results.json""#));
        assert!(html.contains("No formal security guarantees or proofs are claimed"));
        // Every claim's formatted string must appear in the body.
        for claim in claims(&results).expect("claims") {
            assert!(
                html.contains(&claim.formatted) || html.contains(&escape_html(&claim.formatted)),
                "missing rendered string for claim {}",
                claim.id
            );
        }
        // Summary rows render actual values, not placeholders.
        assert!(html.contains("97.0%"));
        assert!(html.contains("24,438 B"));
        assert!(html.contains("10.2\u{d7}") || html.contains("10.1\u{d7}"));
    }

    #[test]
    fn check_passes_on_fresh_report() {
        let (results, html) = render_fixture();
        let failures = check_report(&html, &results);
        assert_eq!(failures, Vec::<String>::new());
    }

    #[test]
    fn check_detects_value_drift() {
        let (results, html) = render_fixture();
        let mut drifted = synthetic_results();
        drifted.derived.overall_wire_reduction_percent = Some(90.0);
        let failures = check_report(&html, &drifted);
        assert!(
            failures
                .iter()
                .any(|f| f.contains("overall-wire-reduction")),
            "expected drift in overall-wire-reduction, got {failures:?}"
        );
        assert!(failures.iter().any(|f| f.contains("value drift")));
        // The pristine results must still pass.
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }

    #[test]
    fn check_detects_missing_formatted_string() {
        let (results, html) = render_fixture();
        let claims = claims(&results).expect("claims");
        let target = claims
            .iter()
            .find(|c| c.id == "largest-wire-reduction")
            .expect("claim");
        let tampered = html.replace(&target.formatted, "TAMPERED");
        let failures = check_claims(&tampered, &results);
        assert!(
            failures
                .iter()
                .any(|f| f.contains("largest-wire-reduction")),
            "expected missing-string failure, got {failures:?}"
        );
    }

    #[test]
    fn check_detects_missing_registry() {
        let (results, html) = render_fixture();
        let start = html.find(CLAIMS_MARKER).expect("marker");
        let end = html[start..].find("</script>").expect("end") + start + "</script>".len();
        let stripped = format!("{}{}", &html[..start], &html[end..]);
        let failures = check_claims(&stripped, &results);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("missing"));
    }

    #[test]
    fn check_detects_missing_chart() {
        let (results, html) = render_fixture();
        let tampered = html.replacen("data:image/svg+xml;base64,", "", 1);
        let failures = check_report(&tampered, &results);
        assert!(
            failures.iter().any(|f| f.contains("embedded chart count")),
            "expected chart-count failure, got {failures:?}"
        );
    }

    /// Write `html` into a fresh temp directory as `report.html` and hand
    /// back the path; the caller removes the directory when done.
    fn write_fixture_report(html: &str, tag: &str) -> std::path::PathBuf {
        let dir = unique_temp_dir(tag);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("report.html");
        std::fs::write(&path, html).expect("write report");
        path
    }

    #[test]
    fn check_file_api_reports_full_agreement_on_a_fresh_report() {
        let (results, html) = render_fixture();
        let path = write_fixture_report(&html, "check-file-ok");
        let report = check(&results, &path).expect("check");
        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
        assert!(report.report_found);
        assert!(report.registry_ok);
        assert!(
            report.problems.is_empty(),
            "unexpected problems: {:?}",
            report.problems
        );
        let (agree, drift, missing) = report.counts();
        assert!(agree > 0, "expected claim rows");
        assert_eq!((drift, missing), (0, 0));
        assert!(report.drifts().next().is_none());
        // The message list is exactly what `report --check` prints.
        assert_eq!(report.messages(), check_report(&html, &results));
    }

    #[test]
    fn check_file_api_rows_name_the_drifted_claim_and_carry_both_values() {
        let (results, html) = render_fixture();
        let path = write_fixture_report(&html, "check-file-drift");
        let mut drifted = synthetic_results();
        drifted.derived.overall_wire_reduction_percent = Some(90.0);
        let report = check(&drifted, &path).expect("check");
        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
        assert!(report.report_found);
        assert!(!report.registry_ok);
        let row = report
            .drifts()
            .find(|c| c.id == "overall-wire-reduction")
            .unwrap_or_else(|| panic!("expected drift row, got {:?}", report.claims));
        assert_eq!(row.pointer, "/derived/overall_wire_reduction_percent");
        assert_eq!(row.value_in_results, serde_json::json!(90.0));
        let pristine = claims(&results)
            .expect("claims")
            .into_iter()
            .find(|c| c.id == "overall-wire-reduction")
            .expect("pristine claim");
        assert_eq!(row.value_in_report.as_ref(), Some(&pristine.value));
        assert!(row.detail.contains("value drift"), "detail: {}", row.detail);
        let messages = report.messages();
        assert!(messages.iter().any(|m| m.contains("value drift")));
        assert!(messages
            .iter()
            .any(|m| m.contains("overall-wire-reduction")));
    }

    #[test]
    fn check_file_api_absent_report_is_a_warning_naming_the_path() {
        let (results, _html) = render_fixture();
        let dir = unique_temp_dir("check-file-absent");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("report.html");
        let report = check(&results, &path).expect("absent report is not an error");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!report.report_found);
        assert!(!report.registry_ok);
        assert_eq!(report.problems.len(), 1, "problems: {:?}", report.problems);
        assert!(
            report.problems[0].contains(&path.display().to_string()),
            "problem should name the path: {}",
            report.problems[0]
        );
        assert!(!report.claims.is_empty(), "expected claim rows");
        assert!(report
            .claims
            .iter()
            .all(|c| c.status == CheckStatus::Missing));
    }

    #[test]
    fn check_file_api_unreadable_report_is_a_hard_error_naming_the_path() {
        let (results, _html) = render_fixture();
        let dir = unique_temp_dir("check-file-unreadable");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        // A directory where the report should be reads as an I/O error,
        // not NotFound, and must surface as a hard error.
        let err = check(&results, &dir).expect_err("directory is not a readable report");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            err.to_string().contains(&dir.display().to_string()),
            "error should name the path: {err}"
        );
    }

    #[test]
    fn check_rows_flag_registry_only_claims() {
        let (results, html) = render_fixture();
        let mut extra = claims(&results).expect("claims");
        extra.push(Claim {
            id: "ghost-claim".to_string(),
            pointer: "/derived/ghost".to_string(),
            value: serde_json::json!(1),
            formatted: "1".to_string(),
        });
        let payload = serde_json::to_string(&extra).expect("json");
        let start = html.find(CLAIMS_MARKER).expect("marker") + CLAIMS_MARKER.len();
        let end = html[start..].find("</script>").expect("end") + start;
        let tampered = format!("{}{}{}", &html[..start], payload, &html[end..]);

        let failures = check_claims(&tampered, &results);
        assert!(
            failures.iter().any(|f| f.contains("ghost-claim")
                && f.contains("present in registry but not in results")),
            "expected registry-only failure, got {failures:?}"
        );
        let report = check_html(&tampered, &results, false);
        let ghost = report
            .claims
            .iter()
            .find(|c| c.id == "ghost-claim")
            .expect("registry-only row");
        assert_eq!(ghost.status, CheckStatus::Missing);
        assert_eq!(ghost.value_in_results, Value::Null);
        assert_eq!(ghost.value_in_report.as_ref(), Some(&serde_json::json!(1)));
        assert!(!report.registry_ok);
    }

    #[test]
    fn hostile_headline_is_escaped_in_body_and_registry() {
        let mut results = synthetic_results();
        results.derived.headline = Some("<script>alert(1)</script>".to_string());
        let dir = unique_temp_dir("html-hostile");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        crate::charts::generate_charts(&results, &dir).expect("charts");
        let html = render_html(&results, &dir, "../climb-results.json").expect("render");
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        // The registry payload must not contain a raw '</script>'.
        let start = html.find(CLAIMS_MARKER).expect("marker");
        let rest = &html[start + CLAIMS_MARKER.len()..];
        let end = rest.find("</script>").expect("terminated");
        let payload = &rest[..end];
        assert!(!payload.contains("</script"));
        assert!(payload.contains("\\u003c/script>"));
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }

    #[test]
    fn summary_grid_rows_follow_the_size_not_the_declaration_order() {
        let mut results = synthetic_results();
        results.comparisons = ["Small", "OneK", "Medium", "Large"]
            .iter()
            .map(|size| ConfigComparison {
                size: (*size).to_string(),
                wire_reduction_c_vs_a_percent: Some(10.0),
                wire_reduction_c_vs_b_percent: Some(20.0),
                signing_speedup_c_vs_a: Some(2.0),
                signing_speedup_c_vs_b: Some(3.0),
                verifying_speedup_cached_vs_a: Some(4.0),
                verifying_speedup_cold_vs_a: Some(5.0),
                class: EvidenceClass::Derived,
            })
            .collect();
        // Give the Medium row values no other size carries, so the rendered
        // grid can only match it by size rather than by declaration index.
        results.comparisons[2].wire_reduction_c_vs_b_percent = Some(42.5);
        results.comparisons[2].signing_speedup_c_vs_a = Some(7.0);
        results.comparisons[2].verifying_speedup_cached_vs_a = Some(9.0);
        let dir = unique_temp_dir("html-medium");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        crate::charts::generate_charts(&results, &dir).expect("charts");
        let html = render_html(&results, &dir, "../climb-results.json").expect("render");
        let _ = std::fs::remove_dir_all(&dir);
        for needle in ["42.5%", "7.0\u{d7}", "9.0\u{d7}"] {
            assert!(
                html.contains(needle),
                "summary grid missed the Medium value {needle:?}"
            );
        }
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }

    #[test]
    fn summary_grid_missing_comparison_renders_an_em_dash() {
        let mut results = synthetic_results();
        results.comparisons = vec![ConfigComparison {
            size: "Small".to_string(),
            wire_reduction_c_vs_a_percent: Some(10.0),
            wire_reduction_c_vs_b_percent: Some(20.0),
            signing_speedup_c_vs_a: Some(2.0),
            signing_speedup_c_vs_b: Some(3.0),
            verifying_speedup_cached_vs_a: Some(4.0),
            verifying_speedup_cold_vs_a: Some(5.0),
            class: EvidenceClass::Derived,
        }];
        let dir = unique_temp_dir("html-no-medium");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        crate::charts::generate_charts(&results, &dir).expect("charts");
        let html = render_html(&results, &dir, "../climb-results.json").expect("render");
        let _ = std::fs::remove_dir_all(&dir);
        // With no Medium comparison declared, the "(Medium)" rows fall back
        // to the placeholder instead of borrowing another size's numbers.
        let grid = html
            .split("<dl class=\"summary\">")
            .nth(1)
            .expect("summary grid")
            .split("</dl>")
            .next()
            .expect("grid end");
        let medium_rows = [
            "Wire C vs B (Medium)",
            "Sign C vs A (Medium)",
            "Verify cached C vs A (Medium)",
        ];
        let mut seen = 0;
        for line in grid.lines() {
            if medium_rows.iter().any(|r| line.contains(r)) {
                assert!(
                    line.contains("\u{2014}"),
                    "row without a Medium comparison must be an em dash: {line}"
                );
                seen += 1;
            }
        }
        assert_eq!(seen, 3, "expected three (Medium) rows in the grid");
    }

    #[test]
    fn empty_results_still_render_and_check_clean() {
        let mut results = synthetic_results();
        results.derived = DerivedSummary {
            overall_wire_reduction_percent: None,
            largest_wire_reduction_percent: None,
            overall_verifying_trust_cached_speedup_avg: None,
            headline: None,
            udp_fit: None,
            reuse: Vec::new(),
            signature_context: Vec::new(),
        };
        results.dns_pipeline = None;
        let dir = unique_temp_dir("html-empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        crate::charts::generate_charts(&results, &dir).expect("charts");
        let html = render_html(&results, &dir, "../climb-results.json").expect("render");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
        // Absent claims render as em dashes in the summary grid.
        assert!(html.contains("\u{2014}"));
    }

    #[test]
    fn render_reports_missing_chart_file() {
        let results = synthetic_results();
        let empty = unique_temp_dir("html-missing");
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).expect("mkdir");
        let err = render_html(&results, &empty, "r.json").expect_err("must fail");
        assert!(matches!(err, ReportError::MissingFile { .. }));
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn relative_href_works() {
        assert_eq!(
            relative_href(
                Path::new("target/climb-report"),
                Path::new("target/climb-results.json")
            ),
            "../climb-results.json"
        );
        assert_eq!(
            relative_href(Path::new("target/climb-report"), Path::new("report.json")),
            "../../report.json"
        );
        assert_eq!(
            relative_href(Path::new("."), Path::new("results.json")),
            "results.json"
        );
        assert_eq!(
            relative_href(Path::new("a/b"), Path::new("a/b/c.json")),
            "c.json"
        );
        assert_eq!(
            relative_href(Path::new("a/b"), Path::new("/abs/c.json")),
            "/abs/c.json"
        );
    }

    #[test]
    fn values_match_float_tolerance() {
        let a = serde_json::json!(96.45);
        let b = serde_json::json!(96.45);
        let c = serde_json::json!(96.46);
        assert!(values_match(&a, &b));
        assert!(!values_match(&a, &c));
        // 1e-12 relative jitter is tolerated.
        let d = serde_json::json!(96.45 + 1e-12);
        assert!(values_match(&a, &d));
        assert!(values_match(
            &serde_json::json!([1.0, "x"]),
            &serde_json::json!([1.0, "x"])
        ));
        assert!(!values_match(
            &serde_json::json!([1.0]),
            &serde_json::json!([2.0])
        ));
    }

    fn memory_fixture() -> ClimbResults {
        let mut results = synthetic_results();
        results.memory.measured = vec![MemoryResult {
            size: "small".to_string(),
            message_count: 100,
            measured_mean_bytes: 851_968,
            libmtl_analytical_bytes: 2_097_152,
            harness_buffers_bytes: 27_656,
            analytical_total_bytes: 2_124_808,
            residual_bytes: 851_968 - 2_124_808,
            residual_percent: (851_968.0 - 2_124_808.0) / 2_124_808.0 * 100.0,
            measured_analytical_total_ratio: 851_968.0 / 2_124_808.0,
            ratio: 851_968.0 / 2_124_808.0,
            measured_range_min_bytes: 73_728,
            measured_range_max_bytes: 1_630_208,
            measured_stddev_bytes: 778_240.0,
            median_bytes: 851_968,
            iqr_bytes: 1_556_480,
            samples: 2,
            method: "isolated-process".to_string(),
            dataset_buffers_bytes: 53_600,
            keypair_bytes: 186,
            measured_peak_bytes: 851_968,
            stages: None,
        }];
        results
    }

    fn render_memory_fixture() -> (ClimbResults, String) {
        let results = memory_fixture();
        let dir = unique_temp_dir("html-memory");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        crate::charts::generate_charts(&results, &dir).expect("charts");
        let html = render_html(&results, &dir, "../climb-results.json").expect("render");
        let _ = std::fs::remove_dir_all(&dir);
        (results, html)
    }

    #[test]
    fn memory_claims_are_rendered_in_body_and_check_clean() {
        let (results, html) = render_memory_fixture();
        assert!(html.contains("Memory model vs measurement"));
        let claims = claims(&results).expect("claims");
        for suffix in [
            "libmtl-pages",
            "harness-buffers",
            "analytical-total",
            "measured-mean",
            "residual-bytes",
            "residual-percent",
            "measured-analytical-total",
        ] {
            let id = format!("memory-small-{suffix}");
            let claim = claims
                .iter()
                .find(|c| c.id == id)
                .unwrap_or_else(|| panic!("missing claim {id}"));
            assert!(
                html.contains(&claim.formatted) || html.contains(&escape_html(&claim.formatted)),
                "missing rendered string for claim {id} = {}",
                claim.formatted
            );
        }
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }

    #[test]
    fn check_detects_tampered_memory_residual() {
        let (results, html) = render_memory_fixture();
        let mut tampered = memory_fixture();
        tampered.memory.measured[0].residual_bytes += 1;
        let failures = check_report(&html, &tampered);
        assert!(
            failures
                .iter()
                .any(|f| f.contains("memory-small-residual-bytes")),
            "expected memory residual drift, got {failures:?}"
        );
        assert!(failures.iter().any(|f| f.contains("value drift")));
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }

    #[test]
    fn ladder_section_renders_the_model_vs_libmtl_table_and_checks_clean() {
        let (results, html) = render_fixture();
        assert!(html.contains("Ladder: model vs libMTL"));
        assert!(html.contains("all 2 sizes match"));
        assert!(html.contains("recorded fixture 2026-10-14"));
        let claims = claims(&results).expect("claims");
        for id in [
            "ladder-provenance",
            "ladder-all-match",
            "ladder-0-leaf",
            "ladder-0-rungs",
            "ladder-0-ladder-buffer",
            "ladder-0-signed-ladder",
            "ladder-0-condensed-min",
            "ladder-0-condensed-mean",
            "ladder-0-condensed-max",
            "ladder-0-model",
            "ladder-1-condensed-mean",
            "ladder-1-model",
        ] {
            let claim = claims
                .iter()
                .find(|c| c.id == id)
                .unwrap_or_else(|| panic!("missing claim {id}"));
            assert!(
                html.contains(&claim.formatted) || html.contains(&escape_html(&claim.formatted)),
                "missing rendered string for claim {id} = {}",
                claim.formatted
            );
        }
        let model = claims
            .iter()
            .find(|c| c.id == "ladder-1-model")
            .expect("model claim");
        assert_eq!(model.formatted, "match");
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }

    #[test]
    fn check_detects_tampered_ladder_row() {
        let (results, html) = render_fixture();
        let mut tampered = synthetic_results();
        tampered.ladder.as_mut().expect("ladder").sizes[1].ladder_buffer_len += 4;
        let failures = check_report(&html, &tampered);
        assert!(
            failures
                .iter()
                .any(|f| f.contains("ladder-1-ladder-buffer")),
            "expected ladder drift, got {failures:?}"
        );
        assert!(failures.iter().any(|f| f.contains("value drift")));
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }

    #[test]
    fn report_without_a_ladder_artifact_renders_no_ladder_section() {
        let mut results = synthetic_results();
        results.ladder = None;
        let dir = unique_temp_dir("html-no-ladder");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        crate::charts::generate_charts(&results, &dir).expect("charts");
        let html = render_html(&results, &dir, "../climb-results.json").expect("render");
        let _ = std::fs::remove_dir_all(&dir);
        // The summary label itself is always present; what must be absent is
        // the per-size section and its registered claim.
        assert!(!html.contains("<h2 id=\"ladder-h\">"), "no ladder section");
        assert!(!claims(&results)
            .expect("claims")
            .iter()
            .any(|c| c.id == "ladder-all-match"));
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }

    fn reuse_fixture() -> (ClimbResults, String) {
        let mut results = synthetic_results();
        results.derived.reuse = vec![ReuseResult {
            size: "small".to_string(),
            batch_size: 100,
            curve: vec![
                ReuseCurvePoint {
                    reuse: 1,
                    effective_time_ns: 900.0,
                    effective_bytes: 8000.0,
                },
                ReuseCurvePoint {
                    reuse: 2,
                    effective_time_ns: 500.0,
                    effective_bytes: 4000.0,
                },
            ],
            breakeven_reuse_time: Some(1),
            breakeven_reuse_bytes: Some(2),
            r_star_time: Some(1),
            r_star_bytes: Some(2),
            baseline_time_ns: 1000.0,
            baseline_bytes: 7856.0,
            class: EvidenceClass::Derived,
        }];
        let dir = unique_temp_dir("html-reuse");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        crate::charts::generate_charts(&results, &dir).expect("charts");
        let html = render_html(&results, &dir, "../climb-results.json").expect("render");
        let _ = std::fs::remove_dir_all(&dir);
        (results, html)
    }

    #[test]
    fn reuse_section_title_cases_the_size_and_registers_matching_claims() {
        let (results, html) = reuse_fixture();
        assert!(
            html.contains("Ladder reuse: cold vs warm"),
            "section present"
        );
        // The wire/reuse artifacts carry the label lowercase; the report spells
        // it like every other size cell.
        assert!(
            html.contains("<td>Small</td>"),
            "size cell is title-cased: {html}"
        );
        assert!(html.contains("<td>r=1</td>") && html.contains("<td>r=2</td>"));
        let claims = claims(&results).expect("claims");
        let size = claims
            .iter()
            .find(|c| c.id == "reuse-0-size")
            .expect("size claim");
        assert_eq!(size.formatted, "Small");
        assert_eq!(
            size.value,
            serde_json::json!("small"),
            "pointer value is raw"
        );
        assert_eq!(check_claims(&html, &results), Vec::<String>::new());
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }

    #[test]
    fn report_without_a_reuse_model_renders_no_reuse_section() {
        let (results, html) = render_fixture();
        assert!(!html.contains("<h2 id=\"reuse-h\">"), "no reuse section");
        assert!(!claims(&results)
            .expect("claims")
            .iter()
            .any(|c| c.id.starts_with("reuse-")));
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }

    /// A populated signature-context section: three classical baselines
    /// followed by configurations A/B/C, all cells present except one (a
    /// classical row without a verify measurement renders an em dash, and
    /// registers no claim for that cell).
    fn context_rows() -> Vec<SignatureContextRow> {
        let row =
            |config: &str, label: &str, family: &str, bytes, sign, verify| SignatureContextRow {
                config: config.to_string(),
                label: label.to_string(),
                family: family.to_string(),
                size: "Medium".to_string(),
                signature_bytes: bytes,
                signing_mean_ns: sign,
                verifying_mean_ns: verify,
                fits_1232_percent: Some(100.0),
                class: EvidenceClass::Measured,
            };
        vec![
            row(
                "classical-ed25519",
                "E (Ed25519, context)",
                "classical (context)",
                Some(64.0),
                Some(1_100_000.0),
                Some(220_000.0),
            ),
            row(
                "classical-ecdsa-p256",
                "P (ECDSA P-256, context)",
                "classical (context)",
                Some(64.0),
                Some(4_400_000.0),
                Some(880_000.0),
            ),
            row(
                "classical-rsa2048",
                "R (RSA-2048, context)",
                "classical (context)",
                Some(256.0),
                Some(9_900_000.0),
                None,
            ),
            row(
                "plain",
                "A (plain SLH-DSA)",
                "post-quantum",
                Some(1070.0),
                Some(737_922_991.4),
                Some(736_473.46),
            ),
            row(
                "mtl-full",
                "B (MTL non-amortised)",
                "post-quantum",
                Some(244.38),
                Some(400_000.0),
                Some(80_000.0),
            ),
            row(
                "mtl-amortised",
                "C (MTL amortised)",
                "post-quantum",
                Some(6.58),
                Some(10_000.0),
                Some(2_000.0),
            ),
        ]
    }

    #[test]
    fn context_section_renders_classical_beside_post_quantum_with_matching_claims() {
        let mut results = synthetic_results();
        results.derived.signature_context = context_rows();
        let dir = unique_temp_dir("html-context");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        crate::charts::generate_charts(&results, &dir).expect("charts");
        let html = render_html(&results, &dir, "../climb-results.json").expect("render");
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            html.contains("<h2 id=\"context-h\">"),
            "the context section must render"
        );
        // The one-line caption must state plainly that the classical
        // algorithms are not quantum-resistant.
        assert!(
            html.contains("<strong>not quantum-resistant</strong>"),
            "caption must carry the not-quantum-resistant statement"
        );
        for needle in [
            "E (Ed25519, context)",
            "R (RSA-2048, context)",
            "A (plain SLH-DSA)",
            "C (MTL amortised)",
            "classical (context)",
            "post-quantum",
        ] {
            assert!(html.contains(needle), "{needle:?} must appear in the table");
        }

        // Every registered context claim's rendered string appears verbatim
        // in the body — exactly what `report --check` re-validates.
        let claims = claims(&results).expect("claims");
        let context_claims: Vec<_> = claims
            .iter()
            .filter(|c| c.id.starts_with("context-"))
            .collect();
        assert_eq!(
            context_claims.len(),
            6 * 5 - 1,
            "every cell but the missing verify"
        );
        for claim in &context_claims {
            assert!(
                html.contains(&claim.formatted)
                    || html.contains(&claim.formatted.replace('&', "&amp;")),
                "claim {} rendered string missing: {}",
                claim.id,
                claim.formatted
            );
        }
        // The row without a verify measurement has no claim and renders an
        // em dash in that cell.
        assert!(!context_claims.iter().any(|c| c.id == "context-2-verify"));
        assert_eq!(check_claims(&html, &results), Vec::<String>::new());
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }

    #[test]
    fn report_without_classical_context_renders_no_context_section() {
        let results = synthetic_results();
        assert!(results.derived.signature_context.is_empty());
        let dir = unique_temp_dir("html-no-context");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        crate::charts::generate_charts(&results, &dir).expect("charts");
        let html = render_html(&results, &dir, "../climb-results.json").expect("render");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            !html.contains("<h2 id=\"context-h\">"),
            "no context section"
        );
        assert!(!html.contains("not quantum-resistant"), "no caption");
        assert!(!claims(&results)
            .expect("claims")
            .iter()
            .any(|c| c.id.starts_with("context-")));
        assert_eq!(check_report(&html, &results), Vec::<String>::new());
    }
}
