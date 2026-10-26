//! `cargo xtask headline`: regenerate the README's headline and classical
//! tables from the consolidated artifact, or check that the committed tables
//! still match it.
//!
//! Both tables are built by [`climb_report::numbers`], the same module
//! `verify-numbers` recomputes from raw rows, so a generated block cannot
//! drift from the values it quotes.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;

use climb_report::numbers::{
    classical_metrics, headline_metrics, render_classical_block, render_headline_block,
};
use climb_report::ClimbResults;

use crate::verify_numbers::{
    parse_classical_rows, parse_marked_rows, CLASSICAL_BEGIN, CLASSICAL_END, HEADLINE_BEGIN,
    HEADLINE_END,
};

#[derive(Args)]
pub(crate) struct HeadlineArgs {
    /// Workspace root.
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Consolidated results artifact.
    #[arg(long, default_value = "docs/sample-output/climb-results.json")]
    results: PathBuf,
    /// README to rewrite (or check).
    #[arg(long, default_value = "README.md")]
    readme: PathBuf,
    /// Check the committed blocks instead of rewriting them.
    #[arg(long)]
    check: bool,
}

fn load_results(path: &Path) -> Result<ClimbResults> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

/// Both generated block bodies (without the marker lines).
pub(crate) fn expected_blocks(results: &ClimbResults) -> Result<(String, String)> {
    let headline = headline_metrics(results);
    let classical = classical_metrics(results);
    let headline_block = render_headline_block(results, &headline)?;
    let classical_block = render_classical_block(&classical);
    Ok((headline_block, classical_block))
}

fn replace_block(text: &str, begin: &str, end: &str, body: &str) -> Result<String> {
    let Some(start) = text.find(begin) else {
        bail!("README is missing the `{begin}` marker");
    };
    let Some(end_pos) = text[start..].find(end) else {
        bail!("README is missing the `{end}` marker");
    };
    let end_pos = start + end_pos + end.len();
    let mut out = String::with_capacity(text.len() + body.len());
    out.push_str(&text[..start]);
    out.push_str(begin);
    out.push('\n');
    out.push_str(body.trim_end());
    out.push('\n');
    out.push_str(end);
    out.push_str(&text[end_pos..]);
    Ok(out)
}

/// Check that the README blocks match the artifact, naming the first
/// disagreement. Used by `docs-check`, so the README cannot drift.
pub(crate) fn check_readme(root: &Path, results_path: &Path) -> Result<()> {
    let results = load_results(results_path)?;
    let (headline_block, classical_block) = expected_blocks(&results)?;
    let readme_path = root.join("README.md");
    let readme = std::fs::read_to_string(&readme_path)
        .with_context(|| format!("failed to read {}", readme_path.display()))?;
    let headline_rows = parse_marked_rows(&readme, HEADLINE_BEGIN, HEADLINE_END);
    let expected_headline = parse_marked_rows(
        &format!("{HEADLINE_BEGIN}\n{headline_block}{HEADLINE_END}"),
        HEADLINE_BEGIN,
        HEADLINE_END,
    );
    if headline_rows != expected_headline {
        bail!(
            "{} headline block drifted from {} — run `cargo xtask headline`",
            readme_path.display(),
            results_path.display()
        );
    }
    let classical_rows = parse_classical_rows(&readme);
    let expected_classical = parse_classical_rows(&format!(
        "{CLASSICAL_BEGIN}\n{classical_block}{CLASSICAL_END}"
    ));
    if classical_rows != expected_classical {
        bail!(
            "{} classical block drifted from {} — run `cargo xtask headline`",
            readme_path.display(),
            results_path.display()
        );
    }
    Ok(())
}

pub(crate) fn cmd_headline(args: HeadlineArgs) -> Result<()> {
    let results_path = args.root.join(&args.results);
    let results = load_results(&results_path)?;
    let (headline_block, classical_block) = expected_blocks(&results)?;
    let readme_path = args.root.join(&args.readme);
    let readme = std::fs::read_to_string(&readme_path)
        .with_context(|| format!("failed to read {}", readme_path.display()))?;
    let updated = replace_block(&readme, HEADLINE_BEGIN, HEADLINE_END, &headline_block)?;
    let updated = replace_block(&updated, CLASSICAL_BEGIN, CLASSICAL_END, &classical_block)?;

    if args.check {
        if updated == readme {
            println!("headline: README blocks match {}", results_path.display());
            return Ok(());
        }
        bail!(
            "{} is out of date with {} — run `cargo xtask headline`",
            readme_path.display(),
            results_path.display()
        );
    }
    if updated == readme {
        println!("headline: no change ({} blocks already current)", 2);
        return Ok(());
    }
    std::fs::write(&readme_path, updated)
        .with_context(|| format!("failed to write {}", readme_path.display()))?;
    println!("headline: rewrote both blocks in {}", readme_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_block_keeps_the_markers_and_surrounding_text() {
        let text = "before\n<!-- a:begin -->\nold\n<!-- a:end -->\nafter\n";
        let out = replace_block(text, "<!-- a:begin -->", "<!-- a:end -->", "new\n").unwrap();
        assert_eq!(
            out,
            "before\n<!-- a:begin -->\nnew\n<!-- a:end -->\nafter\n"
        );
    }

    #[test]
    fn replace_block_fails_without_markers() {
        let err = replace_block("no markers", "<!-- a:begin -->", "<!-- a:end -->", "x")
            .unwrap_err()
            .to_string();
        assert!(err.contains("missing"), "{err}");
    }
}
