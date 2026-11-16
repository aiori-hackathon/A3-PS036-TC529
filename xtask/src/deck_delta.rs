//! `cargo xtask deck-delta`: diff a plain-text deck dump against the quote sheet.
use anyhow::{Context, Result};
use clap::Args;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct DeckDeltaArgs {
    /// Plain-text deck dump (e.g. from `pdftotext -layout`).
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

    let sheet_set = quote_sheet_values(&sheet);
    let unmatched = unmatched_numbers(&deck, &sheet_set);
    let out_md = render_punch_list(
        &args.deck_text.display().to_string(),
        &args.quote_sheet.display().to_string(),
        &unmatched,
    );

    if let Some(parent) = args.out.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&args.out, out_md)
        .with_context(|| format!("failed to write {}", args.out.display()))?;
    println!("wrote {}", args.out.display());
    Ok(())
}

/// Values the quote sheet already carries, as normalised `value` and
/// `valueunit` tokens.
fn quote_sheet_values(sheet: &str) -> std::collections::HashSet<String> {
    let mut sheet_set = std::collections::HashSet::new();
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
    sheet_set
}

/// Every number+unit in the deck text that the quote sheet does not carry,
/// with the one-based slide numbers it appears on. Slide boundaries are the
/// `===== SLIDE n =====` marker written by the local extractor or a form feed
/// as emitted by `pdftotext`.
pub(crate) fn unmatched_numbers(
    deck: &str,
    sheet_set: &std::collections::HashSet<String>,
) -> BTreeMap<String, Vec<usize>> {
    let re = regex_lite::Regex::new(r"([0-9][0-9,]*\.?[0-9]*)\s*([B%×]|ms|µs|us|s|ns|KiB|MiB|GiB)")
        .expect("the number-and-unit pattern is valid");
    let mut unmatched: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut slide = 1usize;
    for line in deck.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("===== SLIDE ")
            .and_then(|rest| rest.strip_suffix(" ====="))
        {
            if let Ok(number) = rest.trim().parse::<usize>() {
                slide = number;
            }
            continue;
        }
        if line.contains('\u{0c}') {
            slide += 1;
        }
        for cap in re.captures_iter(line) {
            let num = cap.get(1).map_or("", |m| m.as_str()).replace(',', "");
            let unit = cap.get(2).map_or("", |m| m.as_str());
            let key = format!("{num} {unit}");
            if !sheet_set.contains(&format!("{num}{unit}")) && !sheet_set.contains(&num) {
                let slides = unmatched.entry(key).or_default();
                if slides.last().copied() != Some(slide) {
                    slides.push(slide);
                }
            }
        }
    }
    unmatched
}

/// Render the generated punch list. Each bullet carries the slide it appears
/// on and the documentation-check marker: the point of this sheet is to list
/// numbers that deliberately are not in the quote sheet.
pub(crate) fn render_punch_list(
    deck_label: &str,
    sheet_label: &str,
    unmatched: &BTreeMap<String, Vec<usize>>,
) -> String {
    let mut out = String::new();
    out.push_str("# Deck delta (punch-list)\n\n");
    out.push_str(&format!("Deck text: {deck_label}\n\n"));
    out.push_str(&format!("Quote sheet: {sheet_label}\n\n"));
    if unmatched.is_empty() {
        out.push_str("All numbers+units found in deck text appear in the quote sheet.\n");
        return out;
    }
    out.push_str("## Unmatched numbers (in deck text but not found in quote sheet)\n\n");
    for (value, slides) in unmatched {
        let where_seen = if slides.is_empty() {
            "slide unknown".to_string()
        } else {
            format!(
                "slide {}",
                slides
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        out.push_str(&format!(
            "- {value} — {where_seen} <!-- docs-check:allow-number -->\n"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn sheet(values: &[(&str, &str)]) -> HashSet<String> {
        let mut set = HashSet::new();
        for (value, unit) in values {
            set.insert((*value).to_string());
            set.insert(format!("{value}{unit}"));
        }
        set
    }

    #[test]
    fn a_slide_marker_sets_the_slide_for_following_lines() {
        let deck = "===== SLIDE 3 =====\n7,856 B\n===== SLIDE 8 =====\n0.7 s\n";
        let unmatched = unmatched_numbers(deck, &sheet(&[]));
        assert_eq!(unmatched.get("7856 B"), Some(&vec![3]));
        assert_eq!(unmatched.get("0.7 s"), Some(&vec![8]));
    }

    #[test]
    fn a_value_on_two_slides_lists_both_slides_once() {
        let deck = "===== SLIDE 2 =====\n9,869 ×\n===== SLIDE 8 =====\n9,869 ×\n9,869 ×\n";
        let unmatched = unmatched_numbers(deck, &sheet(&[]));
        assert_eq!(unmatched.get("9869 ×"), Some(&vec![2, 8]));
    }

    #[test]
    fn values_in_the_sheet_are_not_reported() {
        let deck = "===== SLIDE 1 =====\n7,856 B\n";
        let unmatched = unmatched_numbers(deck, &sheet(&[("7856", "B")]));
        assert!(unmatched.is_empty());
    }

    #[test]
    fn the_punch_list_keeps_the_allow_number_marker_on_every_line() {
        let mut unmatched = BTreeMap::new();
        unmatched.insert("0.7 s".to_string(), vec![8]);
        let text = render_punch_list("deck.txt", "docs/deck-facts.md", &unmatched);
        assert!(text.contains("- 0.7 s — slide 8 <!-- docs-check:allow-number -->"));
    }

    #[test]
    fn a_clean_deck_says_so_and_has_no_marker() {
        let text = render_punch_list(
            "deck.txt",
            "docs/deck-facts.md",
            &BTreeMap::<String, Vec<usize>>::new(),
        );
        assert!(text.contains("All numbers+units found in deck text appear in the quote sheet."));
        assert!(!text.contains("docs-check:allow-number"));
    }
}
