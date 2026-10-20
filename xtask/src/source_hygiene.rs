//! `cargo xtask source-hygiene`: keep process references out of the shipped
//! source tree.
//!
//! The repository is mirrored publicly, and the planning and process files
//! (the agent instructions, the blueprint and the progress log) are not part of
//! the mirror. A comment that cites one of those files, or the phase numbering
//! they use, is therefore a dangling reference for every downstream reader.
//! Comments must describe behaviour, not the history of how the code came to
//! be.
//!
//! The scan covers the Rust sources (`crates/**/*.rs`, `xtask/**/*.rs`) plus
//! the shell and container files (`docker/*`, `scripts/*`). It reports each hit
//! with `file:line` and the matched pattern, and fails when any remain. This
//! file skips itself, and any line ending in the `source-hygiene:allow` marker
//! is exempt, so a test fixture can still spell out a forbidden literal.

use anyhow::{bail, Result};
use clap::Args;
use std::path::{Path, PathBuf};

/// Tail marker that exempts its own line from the scan. It is used only for
/// test fixtures and the module that documents the banned literals.
const ALLOW_MARKER: &str = "source-hygiene:allow";

#[derive(Args)]
pub(crate) struct SourceHygieneArgs {
    /// Workspace root whose source tree is scanned.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,
}

/// One scan target's file extension filter, plus whether the match is
/// case-sensitive. Directories are walked recursively.
struct ScanRoot {
    /// Directory to walk, relative to the workspace root.
    dir: &'static str,
    /// Extensions to include. An empty list includes every regular file.
    extensions: &'static [&'static str],
}

/// The directories scanned, with the file types each contributes.
const SCAN_ROOTS: &[ScanRoot] = &[
    ScanRoot {
        dir: "crates",
        extensions: &["rs"],
    },
    ScanRoot {
        dir: "xtask",
        extensions: &["rs"],
    },
    ScanRoot {
        dir: "docker",
        extensions: &[],
    },
    ScanRoot {
        dir: "scripts",
        extensions: &[],
    },
];

/// Banned patterns: the planning/process filenames, the tool name and the model
/// handles that must not appear in the mirrored source. The filenames are
/// matched with their extension so an ordinary identifier that happens to share
/// a prefix is not swept up. They are assembled from fragments so that this
/// scanner's own file does not itself contain the literals it forbids (the scan
/// covers `xtask/**/*.rs`, including this file).
fn banned_patterns() -> Vec<String> {
    vec![
        format!("AGENTS{}", ".md"),
        "BLUE".to_string() + "PRINT.md",
        "PRO".to_string() + "GRESS.md",
        "open".to_string() + "code",
        "Sisy".to_string() + "phus",
        "Big ".to_string() + "Pickle",
    ]
}

/// One located hygiene violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HygieneViolation {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) pattern: String,
}

impl std::fmt::Display for HygieneViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: source cites `{}`; describe behaviour, not process history",
            self.file, self.line, self.pattern
        )
    }
}

/// True when `text` carries a narration phase reference: the literal
/// `Phase <digit>` or a dotted `phase N.M` form. The word must be *followed by
/// whitespace and then a digit*, so glued identifiers and container image tags
/// such as a `…:phase12` suffix are not process narration.
pub(crate) fn phase_reference(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while let Some(pos) = lower[i..].find("phase") {
        let start = i + pos;
        let after = start + "phase".len();
        // Require a word boundary before "phase": not part of a longer word.
        let boundary = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        if boundary && after < bytes.len() && bytes[after] == b' ' {
            // At least one space, then a digit.
            let mut j = after;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            if j < bytes.len() && bytes[j].is_ascii_digit() {
                // Report both the dotted sub-step form and the plain form; the
                // caller's frozen pattern set accepts either.
                let rest: String = line[start..].chars().take(24).collect();
                return Some(rest.trim_end().to_string());
            }
        }
        i = after;
    }
    None
}

/// Check one file's text, pushing a violation per offending line and pattern.
/// A line carrying [`ALLOW_MARKER`] is exempt; the file's own name is never
/// exempt except by the check in [`scan`] for this module's source.
pub(crate) fn check_text(file: &str, text: &str, violations: &mut Vec<HygieneViolation>) {
    let patterns = banned_patterns();
    for (idx, line) in text.lines().enumerate() {
        if line.contains(ALLOW_MARKER) {
            continue;
        }
        if let Some(fragment) = phase_reference(line) {
            violations.push(HygieneViolation {
                file: file.to_string(),
                line: idx + 1,
                pattern: fragment,
            });
        }
        for pattern in &patterns {
            if line.contains(pattern.as_str()) {
                violations.push(HygieneViolation {
                    file: file.to_string(),
                    line: idx + 1,
                    pattern: pattern.clone(),
                });
            }
        }
    }
}

/// Collect every scannable file under `scan.dir`, honouring the extension
/// filter.
fn collect_files(scan: &ScanRoot, workspace_root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let dir = workspace_root.join(scan.dir);
    if !dir.exists() {
        return Ok(());
    }
    let mut stack = vec![dir];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)? {
            let entry = entry?;
            let path = entry.path();
            let meta = entry.metadata()?;
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            let matches_ext = scan.extensions.iter().any(|ext| {
                path.extension()
                    .is_some_and(|e| e.to_string_lossy() == *ext)
            });
            if scan.extensions.is_empty() || matches_ext {
                out.push(path);
            }
        }
    }
    Ok(())
}

/// Scan the configured directories under `workspace_root`.
pub(crate) fn scan(workspace_root: &Path) -> Result<Vec<HygieneViolation>> {
    let mut files = Vec::new();
    for scan_root in SCAN_ROOTS {
        collect_files(scan_root, workspace_root, &mut files)?;
    }
    files.sort();
    files.dedup();

    let mut violations = Vec::new();
    for path in &files {
        // This module's own source carries the forbidden literals as fixture
        // data and fragment fragments; it is the one file allowed to.
        if path
            .file_name()
            .is_some_and(|name| name == "source_hygiene.rs")
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            // Non-UTF-8 files (binary assets under docker/ or scripts/) carry
            // no comments to check.
            continue;
        };
        let label = path
            .strip_prefix(workspace_root)
            .unwrap_or(path)
            .display()
            .to_string();
        check_text(&label, &text, &mut violations);
    }
    Ok(violations)
}

/// Run the hygiene scan and fail when any violation remains.
pub(crate) fn run(workspace_root: &Path) -> Result<()> {
    let violations = scan(workspace_root)?;
    if violations.is_empty() {
        return Ok(());
    }
    for violation in &violations {
        eprintln!("{violation}");
    }
    bail!("source-hygiene: {} violation(s)", violations.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_numbering_is_a_violation() {
        let mut v = Vec::new();
        check_text("a.rs", "// done in Phase 22.4 here", &mut v);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].file, "a.rs");
        assert_eq!(v[0].line, 1);
        assert!(v[0].pattern.contains("Phase 22.4"), "{v:?}");
    }

    #[test]
    fn lowercase_word_phase_without_a_number_is_fine() {
        let mut v = Vec::new();
        check_text("a.rs", "// the phases of the moon", &mut v);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn process_and_model_names_are_violations() {
        let mut v = Vec::new();
        check_text(
            "b.rs",
            "// see the project conventions\n// the design notes table\n// the dev notes notes\n",
            &mut v,
        );
        assert_eq!(v.len(), 3, "{v:?}");
        assert_eq!(v[0].pattern, "the project conventions");
        assert_eq!(v[1].pattern, "the design notes");
        assert_eq!(v[2].pattern, "the dev notes");
    }

    #[test]
    fn an_identifier_sharing_a_filename_prefix_is_not_a_violation() {
        // `PROGRESS_FILE` is a Rust identifier, not a citation of the log.
        let mut v = Vec::new();
        check_text("f.rs", "const PROGRESS_FILE: &str = \"x\";\n", &mut v);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn a_glued_container_tag_is_not_a_phase_reference() {
        let mut v = Vec::new();
        check_text("g.yml", "image: climb-nsd:phase12\n", &mut v);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn tool_and_model_handles_are_violations() {
        let mut v = Vec::new();
        // The handles are assembled from fragments so this fixture does not
        // itself carry the literals the scanner forbids.
        let fixture = format!(
            "// {} ran this\n// {} wrote it\n// {} model\n",
            "open".to_string() + "code",
            "Sisy".to_string() + "phus",
            "Big ".to_string() + "Pickle",
        );
        check_text("c.rs", &fixture, &mut v);
        assert_eq!(v.len(), 3, "{v:?}");
    }

    #[test]
    fn line_numbers_count_from_one() {
        let mut v = Vec::new();
        check_text("d.rs", "ok\nok\n// Phase 3 finding\n", &mut v);
        assert_eq!(v[0].line, 3);
    }

    #[test]
    fn a_clean_comment_reports_nothing() {
        let mut v = Vec::new();
        check_text(
            "e.rs",
            "// Measures the condensed signature length per message.\n",
            &mut v,
        );
        assert!(v.is_empty());
    }

    #[test]
    fn the_shipped_tree_is_hygienic() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let violations = scan(root).expect("scan runs");
        assert!(violations.is_empty(), "{violations:#?}");
    }
}
