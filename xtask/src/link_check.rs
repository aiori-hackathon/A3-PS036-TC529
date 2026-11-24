//! `cargo xtask link-check`: resolve every relative link in `README.md` and
//! `docs/**/*.md`.
//!
//! A relative link or image path must resolve to a file in the tree; a broken
//! one is a dead end for a reader and is reported as a violation. Absolute
//! `http(s)` links are counted but not fetched, so the check runs offline in
//! CI and never depends on a third-party site being up. A fragment-only
//! reference (`#section`) names an anchor on the same page and is ignored.
//!
//! The same scan feeds the audit pack's link-check summary, so the gate and the
//! report cannot disagree about what was checked.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Args)]
pub(crate) struct LinkCheckArgs {
    /// Workspace root holding `README.md` and `docs/`.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,
}

/// One broken relative target, with the reason when it looks like a URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrokenLink {
    /// The document the target appears in, relative to the scan root.
    pub(crate) file: String,
    /// The target exactly as written.
    pub(crate) target: String,
    /// A hint for a scheme-less host name; `None` for a plain missing file.
    pub(crate) hint: Option<String>,
}

impl std::fmt::Display for BrokenLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.file, self.target)
    }
}

/// The result of one scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LinkCheck {
    /// Every link and image attribute found.
    pub(crate) total: u64,
    /// Relative targets that do not resolve to a file.
    pub(crate) broken: u64,
    /// The broken targets, sorted, at most eight entries.
    pub(crate) broken_list: Vec<BrokenLink>,
    /// Absolute links, counted but never fetched.
    pub(crate) external: u64,
}

/// Top-level domains the scheme-less-host hint recognises. The list is
/// deliberately small and explicit: an unknown suffix is treated as a missing
/// relative file, so a file name is never mistaken for a host.
const KNOWN_TLDS: &[&str] = &[
    "ai", "app", "au", "biz", "ca", "cc", "cloud", "co", "com", "de", "dev", "edu", "eu", "fr",
    "gg", "gov", "in", "info", "int", "io", "jp", "me", "mil", "net", "online", "org", "page",
    "sh", "site", "so", "to", "tv", "uk", "us", "xyz",
];

/// A hint for a target that is a host name written without its scheme.
///
/// Markdown resolves such a target as a relative path, which GitHub renders as
/// a 404 even though the host itself answers. A real relative file has already
/// been resolved before this function is called, so only a missing target gets
/// here; a target whose last label is a known TLD and whose labels are a valid
/// host name is almost certainly meant as a URL.
fn scheme_less_host_hint(target: &str) -> Option<String> {
    let trimmed = target
        .split('#')
        .next()
        .unwrap_or(target)
        .split('?')
        .next()
        .unwrap_or(target);
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    let host = trimmed.split('/').next()?;
    if host.contains('@') || host.contains(':') {
        return None;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return None;
    }
    if labels.iter().any(|label| {
        label.is_empty() || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    }) {
        return None;
    }
    let tld = labels.last()?.to_ascii_lowercase();
    if !KNOWN_TLDS.contains(&tld.as_str()) {
        return None;
    }
    Some(format!(
        "looks like a URL without https://; did you mean https://{target}?"
    ))
}

/// Scan `README.md` and `docs/**/*.md` for link targets.
///
/// Relative targets resolve against the document's own directory. Absolute
/// links are counted as external and never fetched.
pub(crate) fn scan(root: &Path) -> Result<LinkCheck> {
    let mut report = LinkCheck::default();
    for path in crate::docs_check::all_markdown_files(root) {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let base = path.parent().unwrap_or(root);
        for url in extract_urls(&text) {
            report.total += 1;
            if url.is_empty() || url.starts_with('#') {
                continue;
            }
            if url.starts_with("http://")
                || url.starts_with("https://")
                || url.starts_with("//")
                || url.starts_with("mailto:")
                || url.starts_with("tel:")
            {
                report.external += 1;
                continue;
            }
            if base.join(&url).is_file() {
                continue;
            }
            report.broken += 1;
            let relative = path.strip_prefix(root).unwrap_or(&path);
            report.broken_list.push(BrokenLink {
                file: relative.display().to_string(),
                target: url.clone(),
                hint: scheme_less_host_hint(&url),
            });
        }
    }
    report
        .broken_list
        .sort_by(|a, b| (&a.file, &a.target).cmp(&(&b.file, &b.target)));
    report.broken_list.truncate(8);
    Ok(report)
}

/// Extract link and attachment targets from markdown and HTML attributes.
///
/// Recognises `[text](target)` and `href="target"` / `src="target"`.
pub(crate) fn extract_urls(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut cursor = 0;
    while cursor < chars.len() {
        if chars[cursor] == ']' && cursor + 1 < chars.len() && chars[cursor + 1] == '(' {
            let mut i = cursor + 2;
            while i < chars.len() && chars[i] != ')' {
                i += 1;
            }
            if i < chars.len() {
                let url: String = chars[cursor + 2..i].iter().collect();
                out.push(url.trim().to_string());
            }
            cursor = i.saturating_add(1);
            continue;
        }
        if cursor + 5 < chars.len() && chars[cursor..cursor + 6] == ['h', 'r', 'e', 'f', '=', '"'] {
            let mut i = cursor + 6;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            if i < chars.len() {
                out.push(chars[cursor + 6..i].iter().collect());
            }
            cursor = i.saturating_add(1);
            continue;
        }
        if cursor + 4 < chars.len() && chars[cursor..cursor + 5] == ['s', 'r', 'c', '=', '"'] {
            let mut i = cursor + 5;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            if i < chars.len() {
                out.push(chars[cursor + 5..i].iter().collect());
            }
            cursor = i.saturating_add(1);
            continue;
        }
        cursor += 1;
    }
    out
}

/// The `link-check` subcommand.
pub(crate) fn cmd_link_check(args: LinkCheckArgs) -> Result<()> {
    let report = scan(&args.root)?;
    println!(
        "link-check: {} target(s): {} relative, {} external, {} broken",
        report.total,
        report.total - report.external,
        report.external,
        report.broken
    );
    if report.broken == 0 {
        return Ok(());
    }
    for broken in &report.broken_list {
        eprintln!("broken: {broken}");
        if let Some(hint) = &broken.hint {
            eprintln!("        {hint}");
        }
    }
    bail!("link-check: {} broken relative link(s)", report.broken)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temporary workspace with the given files, keyed by relative path.
    fn workspace(tag: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "climb-links-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        for (path, text) in files {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create the document directory");
            }
            fs::write(&full, text).expect("write the document");
        }
        fs::create_dir_all(&root).expect("create the workspace");
        root
    }

    #[test]
    fn markdown_and_html_targets_are_both_extracted() {
        let text = "[guide](docs/GUIDE.md) and <img src=\"docs/media/a.svg\"> plus [web](https://example.invalid)";
        let urls = extract_urls(text);
        assert_eq!(
            urls,
            vec![
                "docs/GUIDE.md".to_string(),
                "docs/media/a.svg".to_string(),
                "https://example.invalid".to_string()
            ]
        );
    }

    #[test]
    fn a_relative_link_that_does_not_resolve_is_broken() {
        let root = workspace(
            "broken",
            &[(
                "README.md",
                "# Title\n\n[a missing guide](docs/missing.md)\n",
            )],
        );
        let report = scan(&root).expect("the workspace is readable");
        assert_eq!(report.total, 1);
        assert_eq!(report.broken, 1);
        assert_eq!(report.external, 0);
        assert_eq!(
            report.broken_list,
            vec![BrokenLink {
                file: "README.md".to_string(),
                target: "docs/missing.md".to_string(),
                hint: None,
            }]
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// A bare host is what broke the published README: markdown treats it as a
    /// relative path, so the scan must name both the failure and the fix.
    #[test]
    fn a_bare_host_is_reported_with_the_https_hint() {
        let root = workspace(
            "bare-host",
            &[("README.md", "# Title\n\n[site](climb-pqc.duckdns.org)\n")],
        );
        let report = scan(&root).expect("the workspace is readable");
        assert_eq!(report.broken, 1);
        assert_eq!(report.external, 0);
        assert_eq!(report.broken_list[0].target, "climb-pqc.duckdns.org");
        assert_eq!(
            report.broken_list[0].hint.as_deref(),
            Some(
                "looks like a URL without https://; did you mean \
                 https://climb-pqc.duckdns.org?"
            )
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_host_with_a_path_is_reported_with_the_https_hint() {
        let root = workspace(
            "host-path",
            &[(
                "README.md",
                "# Title\n\n[report](climb-pqc.duckdns.org/report/)\n",
            )],
        );
        let report = scan(&root).expect("the workspace is readable");
        assert_eq!(report.broken, 1);
        assert_eq!(
            report.broken_list[0].hint.as_deref(),
            Some(
                "looks like a URL without https://; did you mean \
                 https://climb-pqc.duckdns.org/report/?"
            )
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// A file name with a dotted stem is not a host name.
    #[test]
    fn a_missing_file_with_a_dotted_name_gets_no_host_hint() {
        let root = workspace(
            "dotted-file",
            &[("README.md", "# Title\n\n[notes](notes.md)\n")],
        );
        let report = scan(&root).expect("the workspace is readable");
        assert_eq!(report.broken, 1);
        assert!(report.broken_list[0].hint.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_anchor_is_ignored_and_mailto_is_external() {
        let root = workspace(
            "mailto",
            &[(
                "README.md",
                "# Title\n\n[top](#top) [mail](mailto:team@example.invalid)\n",
            )],
        );
        let report = scan(&root).expect("the workspace is readable");
        assert_eq!(report.total, 2);
        assert_eq!(report.broken, 0);
        assert_eq!(report.external, 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn http_and_https_targets_are_external_and_never_broken() {
        let root = workspace(
            "schemes",
            &[(
                "README.md",
                "# Title\n\n[a](http://example.invalid/a) [b](https://example.invalid/b)\n",
            )],
        );
        let report = scan(&root).expect("the workspace is readable");
        assert_eq!(report.total, 2);
        assert_eq!(report.external, 2);
        assert_eq!(report.broken, 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_relative_link_that_resolves_is_clean() {
        let root = workspace(
            "clean",
            &[
                ("README.md", "# Title\n\n[the guide](docs/GUIDE.md)\n"),
                ("docs/GUIDE.md", "# Guide\n\n[back](../README.md)\n"),
            ],
        );
        let report = scan(&root).expect("the workspace is readable");
        assert_eq!(report.total, 2);
        assert_eq!(report.broken, 0);
        assert!(report.broken_list.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_link_resolves_against_its_own_document_directory() {
        let root = workspace(
            "relative",
            &[
                ("README.md", "# Title\n"),
                ("docs/GUIDE.md", "[image](media/icon.svg)\n"),
                ("docs/media/icon.svg", "<svg/>"),
            ],
        );
        let report = scan(&root).expect("the workspace is readable");
        assert_eq!(report.broken, 0, "{report:?}");
        assert_eq!(report.total, 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn external_and_fragment_targets_are_counted_but_not_fetched() {
        let root = workspace(
            "external",
            &[(
                "README.md",
                "[web](https://example.invalid/a) [anchor](#top)\n",
            )],
        );
        let report = scan(&root).expect("the workspace is readable");
        assert_eq!(report.total, 2);
        assert_eq!(report.external, 1);
        assert_eq!(report.broken, 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_committed_documents_carry_no_broken_relative_link() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let report = scan(root).expect("the workspace is readable");
        assert!(report.total > 0, "the scan found no links at all");
        assert_eq!(
            report.broken, 0,
            "broken relative links: {:?}",
            report.broken_list
        );
    }
}
