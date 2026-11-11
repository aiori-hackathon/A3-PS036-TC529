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

/// The result of one scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LinkCheck {
    /// Every link and image attribute found.
    pub(crate) total: u64,
    /// Relative targets that do not resolve to a file.
    pub(crate) broken: u64,
    /// The broken targets, sorted, at most eight entries.
    pub(crate) broken_list: Vec<String>,
    /// Absolute links, counted but never fetched.
    pub(crate) external: u64,
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
            if url.starts_with("http://") || url.starts_with("https://") {
                report.external += 1;
                continue;
            }
            if base.join(&url).is_file() {
                continue;
            }
            report.broken += 1;
            let relative = path.strip_prefix(root).unwrap_or(&path);
            report
                .broken_list
                .push(format!("{}: {}", relative.display(), url));
        }
    }
    report.broken_list.sort();
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
        assert_eq!(report.broken_list, vec!["README.md: docs/missing.md"]);
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
