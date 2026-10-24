//! `cargo xtask deck-delta`: diff a plain-text deck dump against the quote sheet.
use anyhow::{Context, Result};
use clap::Args;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct DeckDeltaArgs {
    /// Plain-text deck dump (e.g. from pdftotext).
    #[arg(long)]
    pub(crate) deck_text: PathBuf,
    /// Quote sheet to compare against.
    #[arg(long, default_value = "docs/deck-facts.md")]
    pub(crate) quote_sheet: PathBuf,
    /// Output punch-list (generated; gitignored fine).
    #[arg(long, default_value = "docs/deck-delta.md")]
    pub(crate) out: PathBuf,
}

pub(crate) fn cmd_deck_delta(args: DeckDeltaArgs) -> Result<()> {
    let deck = fs::read_to_string(&args.deck_text)
        .with_context(|| format!("failed to read {}", args.deck_text.display()))?;
    let sheet = fs::read_to_string(&args.quote_sheet)
        .with_context(|| format!("failed to read {}", args.quote_sheet.display()))?;

    // Extract numeric values from sheet
    let mut sheet_set: HashSet<String> = HashSet::new();
    for line in sheet.lines() {
        if !line.contains('|') {
            continue;
        }
        let cells: Vec<String> = line.split('|').map(|c| c.trim().to_string()).collect();
        if cells.len() >= 4 {
            let val = &cells[2];
            if val.is_empty() || val.chars().all(|c| c == '-') || val == "Value" {
                continue;
            }
            let norm: String = val
                .chars()
                .filter(|c| !c.is_whitespace() && *c != ',' && *c != '≈')
                .collect();
            if !norm.is_empty() {
                sheet_set.insert(norm.clone());
                let unit = &cells[3];
                if matches!(unit.as_str(), "B" | "%" | "×" | "s" | "ms" | "µs" | "ns") {
                    sheet_set.insert(format!("{}{}", norm, unit));
                }
            }
        }
    }

    // Extract numbers+units from deck text
    let re = regex_lite::Regex::new(r"([0-9][0-9,]*\.?[0-9]*)\s*([B%×]|ms|µs|us|s|ns|KiB|MiB|GiB)")
        .unwrap();
    let mut unmatched = Vec::new();
    for cap in re.captures_iter(&deck) {
        let num = cap.get(1).unwrap().as_str().replace(',', "");
        let unit = cap.get(2).unwrap().as_str();
        let key = format!("{}{}", num, unit);
        if !sheet_set.contains(&key) && !sheet_set.contains(&num) {
            unmatched.push(format!("{} {}", num, unit));
        }
    }
    unmatched.sort();
    unmatched.dedup();

    let mut out_md = String::new();
    out_md.push_str("# Deck delta (punch-list)\n\n");
    out_md.push_str(&format!("Deck text: {}\n\n", args.deck_text.display()));
    out_md.push_str(&format!("Quote sheet: {}\n\n", args.quote_sheet.display()));
    if unmatched.is_empty() {
        out_md.push_str("All numbers+units found in deck text appear in the quote sheet.\n");
    } else {
        out_md.push_str("## Unmatched numbers (in deck text but not found in quote sheet)\n\n");
        for u in &unmatched {
            out_md.push_str(&format!("- {}\n", u));
        }
        out_md.push('\n');
    }
    if let Some(parent) = args.out.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&args.out, out_md)
        .with_context(|| format!("failed to write {}", args.out.display()))?;
    println!("wrote {}", args.out.display());
    Ok(())
}
