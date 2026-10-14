//! Self-contained HTML report rendering (Phase 17).
//!
//! Builds `report.html` from [`ClimbResults`] plus the five Phase 16 SVG
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
use crate::results::{ClimbResults, DnsCaptureSource};

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
const SUMMARY_ROWS: [(&str, &str); 7] = [
    ("Overall wire reduction", "overall-wire-reduction"),
    ("Largest wire reduction", "largest-wire-reduction"),
    ("Trust-cached verify speedup (avg)", "trust-cached-speedup"),
    ("www.a full signed ladder", "dns-full-signature"),
    ("www.a condensed MTL proof", "dns-condensed-proof"),
    ("www.a wire reduction", "dns-wire-reduction"),
    ("www.a capture provenance", "dns-provenance"),
];

/// Figure captions, index-aligned with [`CHART_FILES`].
const CHART_CAPTIONS: [&str; 5] = [
    "Batch signing time per dataset size — solid bars are Criterion-measured means with 95% CI whiskers; hollow bars are extrapolated.",
    "Batch verification time per dataset size, including trust-cached ladder rows (measured only).",
    "Transmitted bytes with MTL vs without MTL — both series measured from real output, no modelling.",
    "With-MTL signing: measured per-message cost and the OLS fit of the batch total.",
    "Peak RSS delta measured against the analytical page-count model, with recomputed ratios.",
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
pub fn build_claims(results: &ClimbResults) -> Result<Vec<Claim>, ReportError> {
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
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape_html(&m.size),
            escape_html(&cell(&m.size, "libmtl-pages")),
            escape_html(&cell(&m.size, "harness-buffers")),
            escape_html(&cell(&m.size, "analytical-total")),
            escape_html(&cell(&m.size, "measured-mean")),
            escape_html(&cell(&m.size, "residual-bytes")),
            escape_html(&cell(&m.size, "residual-percent")),
            escape_html(&cell(&m.size, "measured-analytical-total")),
        ));
    }
    format!(
        "<section aria-labelledby=\"memory-h\">\n<h2 id=\"memory-h\">Memory model vs measurement</h2>\n<p class=\"note\">Analytical total = libMTL page model + harness-output buffers (an ESTIMATE, not a measurement); measured = mean RSS delta across repeats. residual = measured &minus; analytical total; ratio = measured &divide; analytical total. Every value is read from the results artifact.</p>\n<table class=\"memtable\">\n<thead><tr><th>Size</th><th>libMTL pages (B)</th><th>harness buffers (B)</th><th>analytical total (B)</th><th>measured mean (B)</th><th>residual (B)</th><th>residual %</th><th>measured/analytical</th></tr></thead>\n<tbody>\n{rows}</tbody>\n</table>\n</section>\n"
    )
}

/// Render the full HTML report.
///
/// `charts_dir` must contain the five files named in
/// [`crate::charts::CHART_FILES`]; `results_href` is the link placed in the
/// header pointing at the external `results.json` (typically a relative
/// path such as `../climb-results.json`).
pub fn render_html(
    results: &ClimbResults,
    charts_dir: &Path,
    results_href: &str,
) -> Result<String, ReportError> {
    let claims = build_claims(results)?;

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
        let value = claims
            .iter()
            .find(|c| c.id == id)
            .map_or("\u{2014}", |c| c.formatted.as_str());
        summary.push_str(&format!(
            "<div><dt>{}</dt><dd>{}</dd></div>\n",
            escape_html(label),
            escape_html(value),
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
{memory_html}<section aria-labelledby="method-h">
<h2 id="method-h">Methodology &amp; scope</h2>
<p class="note">Timings are Criterion-measured means with 95% confidence intervals. Hollow bars and fitted curves are extrapolations (N &times; mean per-message cost, or an OLS fit of the with-MTL batch total) — descriptive summaries of the measured runs, not measurements themselves; every chart carries a three-line footnote naming its source files. Wire-size and memory figures are exact measurements of real output.</p>
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

/// Validate the claims registry embedded in `html` against `results`.
///
/// Returns one human-readable message per detected problem; an empty vec
/// means the report and the results document agree. The registry is never
/// treated as a source of truth: expected claims are rebuilt from `results`
/// and every embedded claim is compared against them (pointer, value with
/// relative float tolerance 1e-9, and `formatted` string), each expected
/// claim's `formatted` string must appear in the page body (raw or
/// HTML-escaped), and registry entries not backed by `results` are flagged.
pub fn check_claims(html: &str, results: &ClimbResults) -> Vec<String> {
    let Some(start) = html.find(CLAIMS_MARKER) else {
        return vec!["claims registry (#climb-claims) missing from HTML".to_string()];
    };
    let rest = &html[start + CLAIMS_MARKER.len()..];
    let Some(end) = rest.find("</script>") else {
        return vec!["claims registry payload not terminated".to_string()];
    };
    let embedded = match serde_json::from_str::<Vec<Claim>>(&rest[..end]) {
        Ok(c) => c,
        Err(e) => return vec![format!("claims registry payload is not valid JSON: {e}")],
    };
    let expected = match build_claims(results) {
        Ok(c) => c,
        Err(e) => return vec![format!("failed to derive claims from results: {e}")],
    };

    let mut failures = Vec::new();
    for claim in &expected {
        match embedded.iter().find(|c| c.id == claim.id) {
            None => failures.push(format!(
                "claim `{}`: present in results but missing from registry",
                claim.id
            )),
            Some(found) => {
                if found.pointer != claim.pointer {
                    failures.push(format!(
                        "claim `{}`: pointer drift — registry has {}, results imply {}",
                        claim.id, found.pointer, claim.pointer
                    ));
                }
                if !values_match(&found.value, &claim.value) {
                    failures.push(format!(
                        "claim `{}`: value drift — results has {}, report claims {}",
                        claim.id, claim.value, found.value
                    ));
                }
                if found.formatted != claim.formatted {
                    failures.push(format!(
                        "claim `{}`: formatted drift — results imply {:?}, registry has {:?}",
                        claim.id, claim.formatted, found.formatted
                    ));
                }
            }
        }
        if !html.contains(&claim.formatted) && !html.contains(&escape_html(&claim.formatted)) {
            failures.push(format!(
                "claim `{}`: rendered string {:?} not found in HTML body",
                claim.id, claim.formatted
            ));
        }
    }
    for claim in &embedded {
        if !expected.iter().any(|e| e.id == claim.id) {
            failures.push(format!(
                "claim `{}`: present in registry but not in results",
                claim.id
            ));
        }
    }
    failures
}

/// Full report validation for `--check`: [`check_claims`] plus a count of
/// embedded chart data URIs.
pub fn check_report(html: &str, results: &ClimbResults) -> Vec<String> {
    let mut failures = check_claims(html, results);
    let embedded = html.matches("data:image/svg+xml;base64,").count();
    if embedded != CHART_FILES.len() {
        failures.push(format!(
            "embedded chart count is {embedded}, expected {}",
            CHART_FILES.len()
        ));
    }
    failures
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
        AnalyticalMemoryModel, BenchmarkResults, DerivedSummary, DnsCaptureSource,
        DnsPipelineHeadline, EnvironmentFingerprint, MemoryResult, MemoryResults,
    };

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
            derived: DerivedSummary {
                overall_wire_reduction_percent: Some(96.45),
                largest_wire_reduction_percent: Some(97.0),
                overall_verifying_trust_cached_speedup_avg: Some(10.125),
                headline: Some("MTL cuts www.a wire bytes by 97.3%.".to_string()),
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
        for claim in build_claims(&results).expect("claims") {
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
        let claims = build_claims(&results).expect("claims");
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
    fn empty_results_still_render_and_check_clean() {
        let mut results = synthetic_results();
        results.derived = DerivedSummary {
            overall_wire_reduction_percent: None,
            largest_wire_reduction_percent: None,
            overall_verifying_trust_cached_speedup_avg: None,
            headline: None,
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
        let claims = build_claims(&results).expect("claims");
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
}
