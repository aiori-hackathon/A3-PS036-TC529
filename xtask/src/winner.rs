//! `cargo xtask winner`: the regional-finals winner banner and its README
//! block.
//!
//! The banner is two self-contained SVG cards — a wide one shown at 840 px and
//! a narrow one shown at 390 px — drawn from the `[winner]` table of
//! `docs/readme-config.toml`. Both variants are a pure function of that table:
//! no timestamp, no network request and no font file is read at render time.
//! Every character is emitted as a glyph outline from the same embedded faces
//! the commit-graph cards use, so the banner looks identical in every viewer.
//!
//! The README block between `<!-- winner:begin -->` and `<!-- winner:end -->`
//! is generated from the same table. `--check` compares the committed block
//! and both committed SVGs with a fresh render and names the marker pair and
//! the fix command when they differ.
//!
//! Text is laid out from exact glyph ink boxes. A line that leaves the card's
//! content box, a pair of lines that overlap, or a line that covers the ladder
//! or a rule is refused at generation time rather than published.

use anyhow::{bail, Context, Result};
use clap::Args;
use serde::Deserialize;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::commit_graph::text::{glyph_defs, text_markup, Font, TextBox, Typeface, UsedGlyphs};
use crate::commit_graph::{civil_from_days, parse_iso_date};

/// Marker opening the generated winner block in the README.
pub(crate) const BLOCK_BEGIN: &str = "<!-- winner:begin -->";

/// Marker closing the generated winner block in the README.
pub(crate) const BLOCK_END: &str = "<!-- winner:end -->";

/// The wide card, relative to the workspace root.
pub(crate) const SVG_PATH: &str = "docs/media/regional-winner.svg";

/// The narrow card, relative to the workspace root.
pub(crate) const NARROW_PATH: &str = "docs/media/regional-winner-narrow.svg";

/// The configuration file, relative to the workspace root.
pub(crate) const CONFIG_PATH: &str = "docs/readme-config.toml";

/// Self-imposed ceiling for one card, so the README stays light.
pub(crate) const CARD_BUDGET_BYTES: usize = 12 * 1024;

/// The command that rewrites the banner and its README block.
const FIX_COMMAND: &str = "cargo xtask winner";

/// The card ground.
const INK: &str = "#07080A";
/// The 1 px card border.
const SLATE: &str = "#3A4A66";
/// The ladder, the top rule and the short rule under the title.
const ORANGE: &str = "#FF5400";
/// The word `WINNER`.
const CREAM: &str = "#F2EDE4";
/// Small capital lines and the next line.
///
/// Slate measures 2.24:1 against the ink, below every WCAG text floor, so the
/// lines use the commit-graph card's muted ink (6.8:1) and slate stays a
/// structural colour. The palette rules in `docs/media/brand.toml` reserve
/// slate for light surfaces for the same reason.
const MUTED: &str = "#8B98A5";

/// Every colour a card may carry. The renderer draws only these; the palette
/// contract test checks it against the committed files.
#[cfg(test)]
const PALETTE: [&str; 5] = [INK, SLATE, ORANGE, CREAM, MUTED];

/// The wide card's nominal width.
pub(crate) const WIDE_WIDTH: f64 = 840.0;

/// The wide card's nominal height.
const WIDE_HEIGHT: f64 = 280.0;

/// The narrow card's nominal width.
pub(crate) const NARROW_WIDTH: f64 = 390.0;

/// The narrow card's nominal height.
const NARROW_HEIGHT: f64 = 280.0;

/// The `[winner]` table of the README configuration file.
///
/// The table is the single source of truth for the badge, the two cards, the
/// README block and the sentence under them. Unknown keys are rejected so a
/// typo cannot silently change nothing.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Winner {
    /// The visible heading of the README block and the card's accessible title.
    pub(crate) heading: String,
    /// The event, as the plaque names it.
    pub(crate) event: String,
    /// The zone won.
    pub(crate) zone: String,
    /// Where the regional finals were held.
    pub(crate) venue: String,
    /// The regional-finals day, `YYYY-MM-DD`.
    pub(crate) date: String,
    /// The project line under the title.
    pub(crate) project: String,
    /// The line naming the next round, such as `Next: Finals · 17 Dec`.
    pub(crate) next: String,
    /// The national-finals day, `YYYY-MM-DD`.
    pub(crate) finals_date: String,
    /// The left-hand text of the badge.
    pub(crate) badge_label: String,
    /// The right-hand text of the badge.
    pub(crate) badge_message: String,
    /// The badge's anchor target: a fragment naming the README block.
    pub(crate) link: String,
}

impl Winner {
    /// The DOM id the badge links to, without the leading `#`.
    pub(crate) fn anchor(&self) -> &str {
        self.link.trim_start_matches('#')
    }

    /// Reject values that would render an empty line, an invalid date or a
    /// next-round line that disagrees with the finals date.
    pub(crate) fn validate(&self) -> Result<()> {
        for (field, value) in [
            ("heading", &self.heading),
            ("event", &self.event),
            ("zone", &self.zone),
            ("venue", &self.venue),
            ("date", &self.date),
            ("project", &self.project),
            ("next", &self.next),
            ("finals_date", &self.finals_date),
            ("badge_label", &self.badge_label),
            ("badge_message", &self.badge_message),
        ] {
            if value.trim().is_empty() {
                bail!("`[winner].{field}` must not be empty");
            }
        }
        if !self.link.starts_with('#') || self.anchor().is_empty() {
            bail!(
                "`[winner].link` must be a fragment such as `#regional-finals-winner`, found `{}`",
                self.link
            );
        }
        if self.anchor().chars().any(|character| {
            !(character.is_ascii_alphanumeric() || character == '-' || character == '_')
        }) {
            bail!(
                "`[winner].link` names `{}`, which is not a valid HTML id",
                self.anchor()
            );
        }
        let _ = parse_iso_date(&self.date)
            .with_context(|| format!("`[winner].date` is not a date: `{}`", self.date))?;
        let _ = parse_iso_date(&self.finals_date).with_context(|| {
            format!(
                "`[winner].finals_date` is not a date: `{}`",
                self.finals_date
            )
        })?;
        let expected = format!("Next: Finals · {}", day_month(&self.finals_date, true)?);
        if self.next != expected {
            bail!(
                "`[winner].next` must read `{expected}` so it agrees with `[winner].finals_date`, \
                 found `{}`",
                self.next
            );
        }
        Ok(())
    }

    /// The sentence under the banner, built from the same fields the cards use.
    fn sentence(&self) -> Result<String> {
        Ok(format!(
            "Winner of the {}, {} ({}, {}). The finals are on {}.",
            self.event,
            self.zone,
            self.venue,
            long_date(&self.date)?,
            day_month(&self.finals_date, false)?
        ))
    }
}

/// Read and validate the `[winner]` table.
pub(crate) fn load(root: &Path) -> Result<Winner> {
    let path = root.join(CONFIG_PATH);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&text).with_context(|| format!("{} is not valid TOML", path.display()))?;
    let table = value.get("winner").with_context(|| {
        format!(
            "{} carries no [winner] table, so the banner cannot be generated",
            path.display()
        )
    })?;
    let winner: Winner = table
        .clone()
        .try_into()
        .with_context(|| format!("{} [winner] table is not valid", path.display()))?;
    winner.validate()?;
    Ok(winner)
}

/// The fixed month names.
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `YYYY-MM-DD` as `19 November 2026`.
fn long_date(iso: &str) -> Result<String> {
    let (year, month, day) = civil_from_days(parse_iso_date(iso)?);
    Ok(format!("{day} {} {year}", MONTHS[month as usize - 1]))
}

/// `YYYY-MM-DD` as `17 December` (or `17 Dec` when `short` is set).
fn day_month(iso: &str, short: bool) -> Result<String> {
    let (_, month, day) = civil_from_days(parse_iso_date(iso)?);
    let name = MONTHS[month as usize - 1];
    if short {
        Ok(format!("{day} {}", &name[..3]))
    } else {
        Ok(format!("{day} {name}"))
    }
}

/// A rectangle the text may not cover.
#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Rect {
    fn right(&self) -> f64 {
        self.x + self.width
    }

    fn bottom(&self) -> f64 {
        self.y + self.height
    }

    fn intersects(&self, box_: &TextBox) -> bool {
        self.x < box_.right()
            && box_.x < self.right()
            && self.y < box_.bottom()
            && box_.y < self.bottom()
    }
}

/// One card variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    Wide,
    Narrow,
}

impl Variant {
    fn width(self) -> f64 {
        match self {
            Self::Wide => WIDE_WIDTH,
            Self::Narrow => NARROW_WIDTH,
        }
    }

    fn height(self) -> f64 {
        match self {
            Self::Wide => WIDE_HEIGHT,
            Self::Narrow => NARROW_HEIGHT,
        }
    }

    fn margin(self) -> f64 {
        match self {
            Self::Wide => 40.0,
            Self::Narrow => 24.0,
        }
    }
}

/// One line of the card.
struct Line {
    name: &'static str,
    text: String,
    x: f64,
    baseline: f64,
    size: f64,
    face: Typeface,
    fill: &'static str,
    tracking: f64,
}

/// A rendered card plus the ink boxes its layout placed.
#[derive(Debug)]
pub(crate) struct Rendered {
    /// The SVG document.
    pub(crate) svg: String,
    /// Every text line's name and exact ink box. The layout check uses these
    /// while rendering; the palette and fit tests read them back.
    #[cfg(test)]
    pub(crate) boxes: Vec<(&'static str, TextBox)>,
}

/// Build one card.
fn render(winner: &Winner, variant: Variant) -> Result<Rendered> {
    let font = Font::load()?;
    let width = variant.width();
    let height = variant.height();
    let margin = variant.margin();
    let mut used = UsedGlyphs::new();
    let mut body = String::new();
    let mut boxes: Vec<(&'static str, TextBox)> = Vec::new();
    let mut obstacles: Vec<Rect> = Vec::new();

    // Card ground and border.
    let _ = write!(
        body,
        "<rect x=\"0.5\" y=\"0.5\" width=\"{}\" height=\"{}\" rx=\"14\" fill=\"{INK}\" \
         stroke=\"{SLATE}\" stroke-width=\"1\"/>",
        number(width - 1.0),
        number(height - 1.0)
    );

    let (lines, ladder, rules) = match variant {
        Variant::Wide => {
            let ladder = Rect {
                x: 704.0,
                y: 72.0,
                width: 96.0,
                height: 128.0,
            };
            let rules = vec![
                Rect {
                    x: margin,
                    y: 26.0,
                    width: 120.0,
                    height: 3.0,
                },
                Rect {
                    x: margin + 8.0,
                    y: 152.0,
                    width: 88.0,
                    height: 3.0,
                },
            ];
            let lines = vec![
                Line {
                    name: "event",
                    text: winner.event.to_uppercase(),
                    x: margin + 8.0,
                    baseline: 68.0,
                    size: 12.0,
                    face: Typeface::Mono,
                    fill: MUTED,
                    tracking: 0.12,
                },
                Line {
                    name: "winner",
                    text: "WINNER".to_string(),
                    x: margin + 8.0,
                    baseline: 132.0,
                    size: 62.0,
                    face: Typeface::Mono,
                    fill: CREAM,
                    tracking: 0.0,
                },
                Line {
                    name: "venue",
                    text: format!(
                        "{} · {} · {}",
                        winner.zone.to_uppercase(),
                        winner.venue.to_uppercase(),
                        long_date(&winner.date)?.to_uppercase()
                    ),
                    x: margin + 8.0,
                    baseline: 188.0,
                    size: 13.0,
                    face: Typeface::Mono,
                    fill: MUTED,
                    tracking: 0.06,
                },
                Line {
                    name: "project",
                    text: winner.project.to_uppercase(),
                    x: margin + 8.0,
                    baseline: 212.0,
                    size: 12.0,
                    face: Typeface::Mono,
                    fill: MUTED,
                    tracking: 0.04,
                },
                Line {
                    name: "next",
                    // The cards set every small-caps line in upper case, the
                    // same treatment as the event and venue lines. The exact
                    // mixed-case string lives in `[winner].next` and in the
                    // card's accessible title.
                    text: winner.next.to_uppercase(),
                    x: margin + 8.0,
                    baseline: 238.0,
                    size: 12.0,
                    face: Typeface::Mono,
                    fill: MUTED,
                    tracking: 0.02,
                },
            ];
            (lines, ladder, rules)
        }
        Variant::Narrow => {
            let ladder = Rect {
                x: 286.0,
                y: 58.0,
                width: 56.0,
                height: 78.0,
            };
            let rules = vec![
                Rect {
                    x: margin,
                    y: 22.0,
                    width: 72.0,
                    height: 3.0,
                },
                Rect {
                    x: margin,
                    y: 132.0,
                    width: 64.0,
                    height: 3.0,
                },
            ];
            let lines = vec![
                Line {
                    name: "event",
                    text: winner.event.to_uppercase(),
                    x: margin,
                    baseline: 48.0,
                    size: 10.0,
                    face: Typeface::Mono,
                    fill: MUTED,
                    tracking: 0.08,
                },
                Line {
                    name: "winner",
                    text: "WINNER".to_string(),
                    x: margin,
                    baseline: 112.0,
                    size: 42.0,
                    face: Typeface::Mono,
                    fill: CREAM,
                    tracking: 0.0,
                },
                Line {
                    name: "zone",
                    text: winner.zone.to_uppercase(),
                    x: margin,
                    baseline: 162.0,
                    size: 11.0,
                    face: Typeface::Mono,
                    fill: MUTED,
                    tracking: 0.06,
                },
                Line {
                    name: "venue",
                    text: format!(
                        "{} · {}",
                        winner.venue.to_uppercase(),
                        long_date(&winner.date)?.to_uppercase()
                    ),
                    x: margin,
                    baseline: 182.0,
                    size: 11.0,
                    face: Typeface::Mono,
                    fill: MUTED,
                    tracking: 0.06,
                },
                Line {
                    name: "project",
                    text: winner.project.to_uppercase(),
                    x: margin,
                    baseline: 210.0,
                    size: 10.0,
                    face: Typeface::Mono,
                    fill: MUTED,
                    tracking: 0.02,
                },
                Line {
                    name: "next",
                    // The cards set every small-caps line in upper case, the
                    // same treatment as the event and venue lines. The exact
                    // mixed-case string lives in `[winner].next` and in the
                    // card's accessible title.
                    text: winner.next.to_uppercase(),
                    x: margin,
                    baseline: 238.0,
                    size: 11.0,
                    face: Typeface::Mono,
                    fill: MUTED,
                    tracking: 0.02,
                },
            ];
            (lines, ladder, rules)
        }
    };

    // The top rule, the short rule under the title, and the ladder glyph.
    for rule in &rules {
        let _ = write!(
            body,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{ORANGE}\"/>",
            number(rule.x),
            number(rule.y),
            number(rule.width),
            number(rule.height)
        );
        obstacles.push(*rule);
    }
    body.push_str(&ladder_markup(variant, ladder));
    obstacles.push(ladder);

    for line in &lines {
        let box_ = font.line_box(
            line.face,
            &line.text,
            line.x,
            line.baseline,
            line.size,
            line.tracking,
        )?;
        if box_.x < margin - 0.01
            || box_.right() > width - margin + 0.01
            || box_.y < margin - 0.01
            || box_.bottom() > height - margin + 0.01
        {
            bail!(
                "winner banner ({variant:?}): the `{}` line overflows the content box: ink box \
                 ({}, {}, {}, {}) against a {width}×{height} card with a {margin} px margin",
                line.name,
                number(box_.x),
                number(box_.y),
                number(box_.width),
                number(box_.height)
            );
        }
        for (other, other_box) in &boxes {
            if box_.intersects(other_box) {
                bail!(
                    "winner banner ({variant:?}): the `{}` line overlaps the `{other}` line",
                    line.name
                );
            }
        }
        for obstacle in &obstacles {
            if obstacle.intersects(&box_) {
                bail!(
                    "winner banner ({variant:?}): the `{}` line overlaps a rule or the ladder",
                    line.name
                );
            }
        }
        boxes.push((line.name, box_));
        let _ = write!(
            body,
            "<g data-name=\"{}\" data-bbox=\"{} {} {} {}\">{}</g>",
            line.name,
            number(box_.x),
            number(box_.y),
            number(box_.width),
            number(box_.height),
            text_markup(
                &font,
                line.face,
                &line.text,
                line.x,
                line.baseline,
                line.size,
                line.fill,
                line.tracking,
                &mut used,
            )?
        );
    }

    let glyphs = glyph_defs(&font, &used);
    let title = format!("{} — {}, {}", winner.heading, winner.event, winner.zone);
    let desc = winner.sentence()?;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" \
         height=\"{}\" role=\"img\" aria-labelledby=\"rw-title rw-desc\">\
         <title id=\"rw-title\">{}</title><desc id=\"rw-desc\">{}</desc>\
         <defs>{glyphs}</defs>{body}</svg>\n",
        number(width),
        number(height),
        number(width),
        number(height),
        escape_xml(&title),
        escape_xml(&desc)
    );
    if svg.len() > CARD_BUDGET_BYTES {
        bail!(
            "winner banner ({variant:?}) is {} B against the {CARD_BUDGET_BYTES} B budget; \
             reduce the text or shorten a glyph run",
            svg.len()
        );
    }
    Ok(Rendered {
        svg,
        #[cfg(test)]
        boxes,
    })
}

/// The ladder glyph: two rails and three rungs, all in the rung orange.
fn ladder_markup(variant: Variant, ladder: Rect) -> String {
    let (rail, rung) = match variant {
        Variant::Wide => (7.0, 9.0),
        Variant::Narrow => (5.0, 7.0),
    };
    let mut out = String::new();
    // The rails run past the outer rungs, the way a ladder's stiles do, so
    // the glyph does not read as a closed window frame.
    let overshoot = rung;
    for x in [ladder.x, ladder.right() - rail] {
        let _ = write!(
            out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{ORANGE}\"/>",
            number(x),
            number(ladder.y - overshoot),
            number(rail),
            number(ladder.height + 2.0 * overshoot)
        );
    }
    for index in 0..3 {
        let y = match index {
            0 => ladder.y,
            1 => ladder.y + middle_rung(ladder.height, rung),
            _ => ladder.bottom() - rung,
        };
        let _ = write!(
            out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{ORANGE}\"/>",
            number(ladder.x),
            number(y),
            number(ladder.width),
            number(rung)
        );
    }
    out
}

/// The middle rung's offset: half the ladder, snapping inside the rails.
fn middle_rung(height: f64, rung: f64) -> f64 {
    ((height - rung) / 2.0).round()
}

/// Shortest decimal form of a coordinate, at one decimal place.
fn number(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded:.1}")
    }
}

/// Escape a value for SVG text content.
fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Escape a value for an HTML attribute.
fn escape_attribute(value: &str) -> String {
    escape_xml(value)
}

/// The wide card.
pub(crate) fn render_wide(winner: &Winner) -> Result<Rendered> {
    render(winner, Variant::Wide)
}

/// The narrow card.
pub(crate) fn render_narrow(winner: &Winner) -> Result<Rendered> {
    render(winner, Variant::Narrow)
}

/// The generated README block, markers included.
pub(crate) fn render_block(winner: &Winner) -> Result<String> {
    winner.validate()?;
    let alt = format!(
        "{}: {}, {}, {}, {}",
        winner.heading,
        winner.event,
        winner.zone,
        winner.venue,
        long_date(&winner.date)?
    );
    Ok(format!(
        "{BLOCK_BEGIN}\n<div align=\"center\" id=\"{}\">\n\n<h3 align=\"center\">{}</h3>\n\n\
         <picture>\n  <source media=\"(max-width: 600px)\" srcset=\"{NARROW_PATH}\" width=\"390\">\n  \
         <img src=\"{SVG_PATH}\" alt=\"{}\" width=\"840\">\n</picture>\n\n{}\n</div>\n{BLOCK_END}\n",
        escape_attribute(winner.anchor()),
        escape_xml(&winner.heading),
        escape_attribute(&alt),
        escape_xml(&winner.sentence()?)
    ))
}

/// The managed block of a README, including its markers.
fn extract_block(readme: &str) -> Option<String> {
    let begin = readme.find(BLOCK_BEGIN)?;
    let end = readme.find(BLOCK_END)?;
    if end < begin {
        return None;
    }
    Some(readme[begin..end + BLOCK_END.len()].to_string())
}

/// The differences between two blocks, line by line, as `-`/`+` text.
fn render_diff(expected: &str, actual: &str) -> String {
    let expected: Vec<&str> = expected.lines().collect();
    let actual: Vec<&str> = actual.lines().collect();
    let mut out = String::new();
    for line in &expected {
        if !actual.contains(line) {
            let _ = writeln!(out, "- {line}");
        }
    }
    for line in &actual {
        if !expected.contains(line) {
            let _ = writeln!(out, "+ {line}");
        }
    }
    out
}

/// Check the committed block and both committed cards against a fresh render.
///
/// A missing marker, a hand-edited block, a missing card or a card that does
/// not reproduce is an error naming the marker pair or the file and the fix
/// command.
pub(crate) fn check(root: &Path, readme_text: &str) -> Result<()> {
    let winner = load(root)?;
    let expected = render_block(&winner)?;
    let Some(committed) = extract_block(readme_text) else {
        bail!(
            "the README carries no winner block (`{BLOCK_BEGIN}` / `{BLOCK_END}`); \
             run `{FIX_COMMAND}`"
        );
    };
    if committed.trim_end() != expected.trim_end() {
        bail!(
            "the winner block between `{BLOCK_BEGIN}` and `{BLOCK_END}` has drifted from what \
             the generator would write; run `{FIX_COMMAND}`\n{}",
            render_diff(&expected, &committed)
        );
    }
    for (relative, rendered) in [
        (SVG_PATH, render_wide(&winner)?),
        (NARROW_PATH, render_narrow(&winner)?),
    ] {
        let path = root.join(relative);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => bail!(
                "{} is missing, so the winner banner cannot be verified; run `{FIX_COMMAND}`",
                path.display()
            ),
            Err(error) => {
                return Err(
                    anyhow::Error::new(error).context(format!("failed to read {}", path.display()))
                )
            }
        };
        if text != rendered.svg {
            bail!(
                "{} does not match a fresh render from `[winner]`; run `{FIX_COMMAND}`",
                path.display()
            );
        }
    }
    Ok(())
}

#[derive(Args)]
pub(crate) struct WinnerArgs {
    /// Workspace root holding the configuration, the README and the media
    /// directory.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,

    /// README whose winner block is regenerated.
    #[arg(long, default_value = "README.md")]
    pub(crate) readme: PathBuf,

    /// Compare the committed block and cards with a fresh render instead of
    /// writing.
    #[arg(long)]
    pub(crate) check: bool,
}

/// Run the generation.
pub(crate) fn run(args: &WinnerArgs) -> Result<()> {
    let winner = load(&args.root)?;
    let readme_path = args.root.join(&args.readme);
    let readme = std::fs::read_to_string(&readme_path)
        .with_context(|| format!("failed to read {}", readme_path.display()))?;

    if args.check {
        check(&args.root, &readme)?;
        println!("the winner block and both cards reproduce from `[winner]` in {CONFIG_PATH}");
        return Ok(());
    }

    let block = render_block(&winner)?;
    let Some(begin) = readme.find(BLOCK_BEGIN) else {
        bail!(
            "{} carries no `{BLOCK_BEGIN}` marker; add the marker pair around the block first",
            readme_path.display()
        );
    };
    let Some(end) = readme.find(BLOCK_END) else {
        bail!(
            "{} carries no `{BLOCK_END}` marker; add the marker pair around the block first",
            readme_path.display()
        );
    };
    if end < begin {
        bail!("the `{BLOCK_BEGIN}` marker comes after the `{BLOCK_END}` marker");
    }
    let mut updated = String::with_capacity(readme.len() + block.len());
    updated.push_str(&readme[..begin]);
    updated.push_str(block.trim_end_matches('\n'));
    updated.push_str(&readme[end + BLOCK_END.len()..]);
    std::fs::write(&readme_path, &updated)
        .with_context(|| format!("failed to write {}", readme_path.display()))?;

    let wide = render_wide(&winner)?;
    let narrow = render_narrow(&winner)?;
    for (relative, rendered) in [(SVG_PATH, &wide), (NARROW_PATH, &narrow)] {
        let path = args.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        std::fs::write(&path, &rendered.svg)
            .with_context(|| format!("failed to write {}", path.display()))?;
        println!("{}: {} B", relative, rendered.svg.len());
    }
    println!("wrote the winner block in {}", readme_path.display());
    Ok(())
}

/// The `winner` subcommand.
pub(crate) fn cmd_winner(args: WinnerArgs) -> Result<()> {
    run(&args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The workspace root, one level above the `xtask` crate.
    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace")
            .to_path_buf()
    }

    fn committed() -> Winner {
        load(&workspace_root()).expect("the committed [winner] table parses")
    }

    /// Every six-digit hex colour a document carries.
    fn hex_colours(text: &str) -> BTreeSet<String> {
        let bytes = text.as_bytes();
        let mut found = BTreeSet::new();
        let mut index = 0;
        while index + 7 <= bytes.len() {
            if bytes[index] == b'#' {
                if !text.is_char_boundary(index + 7) {
                    index += 1;
                    continue;
                }
                let candidate = &text[index + 1..index + 7];
                if candidate.bytes().all(|byte| byte.is_ascii_hexdigit())
                    && (index + 7 == bytes.len() || !bytes[index + 7].is_ascii_hexdigit())
                {
                    found.insert(format!("#{}", candidate.to_ascii_uppercase()));
                    index += 7;
                    continue;
                }
            }
            index += 1;
        }
        found
    }

    #[test]
    fn the_committed_table_names_every_line_of_the_plaque() {
        let winner = committed();
        assert_eq!(winner.zone, "East Zone");
        assert_eq!(winner.venue, "IIEST Shibpur");
        assert_eq!(winner.date, "2026-11-19");
        assert_eq!(winner.project, "CLIMB · PS-036 · Track 2 Cyber Security");
        assert_eq!(winner.next, "Next: Finals · 17 Dec");
        assert_eq!(winner.link, "#regional-finals-winner");
        assert!(winner.event.contains("Regional Finals"));
    }

    #[test]
    fn the_block_carries_the_picture_the_anchor_and_the_sentence() {
        let winner = committed();
        let block = render_block(&winner).expect("the block renders");
        assert!(block.starts_with(BLOCK_BEGIN));
        assert!(block.ends_with(&format!("{BLOCK_END}\n")));
        assert!(block.contains("<div align=\"center\" id=\"regional-finals-winner\">"));
        assert!(block.contains(&format!("srcset=\"{NARROW_PATH}\"")));
        assert!(block.contains(&format!("src=\"{SVG_PATH}\"")));
        assert!(block.contains("media=\"(max-width: 600px)\""));
        assert!(block.contains("width=\"390\""));
        assert!(block.contains("width=\"840\""));
        assert!(block.contains(
            "alt=\"Regional finals winner: AIORI-3 Hackathon Regional Finals, East Zone, \
             IIEST Shibpur, 19 November 2026\""
        ));
        assert!(block.contains(
            "Winner of the AIORI-3 Hackathon Regional Finals, East Zone (IIEST Shibpur, \
             19 November 2026). The finals are on 17 December."
        ));
        assert!(block.contains("<h3 align=\"center\">Regional finals winner</h3>"));
    }

    #[test]
    fn both_cards_carry_only_palette_colours() {
        let winner = committed();
        for rendered in [
            render_wide(&winner).expect("the wide card renders"),
            render_narrow(&winner).expect("the narrow card renders"),
        ] {
            let colours = hex_colours(&rendered.svg);
            let palette: BTreeSet<String> = PALETTE.iter().map(|c| c.to_string()).collect();
            assert!(
                colours.is_subset(&palette),
                "the card carries colours outside the palette: {:?}",
                colours.difference(&palette).collect::<Vec<_>>()
            );
            for required in [INK, SLATE, ORANGE, CREAM] {
                assert!(
                    colours.contains(required),
                    "the card does not carry {required}: {colours:?}"
                );
            }
            // Slate is the 1 px border; cream is the word WINNER; orange is the
            // ladder and both rules. No other ink is allowed.
            assert_eq!(
                colours.len(),
                PALETTE.len(),
                "the card should use every palette colour: {colours:?}"
            );
        }
    }

    #[test]
    fn text_stays_inside_the_card_and_never_overlaps_at_840_and_390() {
        let winner = committed();
        for (variant, rendered) in [
            ("wide", render_wide(&winner).expect("the wide card renders")),
            (
                "narrow",
                render_narrow(&winner).expect("the narrow card renders"),
            ),
        ] {
            let width = if variant == "wide" {
                WIDE_WIDTH
            } else {
                NARROW_WIDTH
            };
            assert!(rendered.boxes.len() >= 5, "{variant}: too few lines");
            for (name, box_) in &rendered.boxes {
                assert!(
                    box_.x >= 0.0 && box_.right() <= width,
                    "{variant}: `{name}` leaves the card: {box_:?}"
                );
            }
            for (index, (name, box_)) in rendered.boxes.iter().enumerate() {
                for (other, other_box) in &rendered.boxes[index + 1..] {
                    assert!(
                        !box_.intersects(other_box),
                        "{variant}: `{name}` overlaps `{other}`"
                    );
                }
            }
        }
    }

    #[test]
    fn the_rendered_text_is_drawn_as_outlines() {
        let winner = committed();
        for rendered in [
            render_wide(&winner).expect("the wide card renders"),
            render_narrow(&winner).expect("the narrow card renders"),
        ] {
            assert!(!rendered.svg.contains("<text"), "a text element leaked in");
            assert!(rendered.svg.contains("<use href=\"#m"));
            assert!(rendered.svg.contains("data-name=\"winner\""));
            assert!(rendered.svg.contains("data-name=\"next\""));
        }
    }

    #[test]
    fn both_cards_stay_under_the_byte_budget() {
        let winner = committed();
        for (relative, rendered) in [
            (
                SVG_PATH,
                render_wide(&winner).expect("the wide card renders"),
            ),
            (
                NARROW_PATH,
                render_narrow(&winner).expect("the narrow card renders"),
            ),
        ] {
            assert!(
                rendered.svg.len() < CARD_BUDGET_BYTES,
                "{relative} is {} B against the {CARD_BUDGET_BYTES} B budget",
                rendered.svg.len()
            );
        }
    }

    #[test]
    fn the_cards_are_deterministic_across_runs() {
        let winner = committed();
        assert_eq!(
            render_wide(&winner).expect("first wide").svg,
            render_wide(&winner).expect("second wide").svg
        );
        assert_eq!(
            render_narrow(&winner).expect("first narrow").svg,
            render_narrow(&winner).expect("second narrow").svg
        );
    }

    #[test]
    fn the_committed_cards_reproduce_byte_identically() {
        let root = workspace_root();
        let winner = committed();
        for (relative, rendered) in [
            (
                SVG_PATH,
                render_wide(&winner).expect("the wide card renders"),
            ),
            (
                NARROW_PATH,
                render_narrow(&winner).expect("the narrow card renders"),
            ),
        ] {
            let committed = std::fs::read_to_string(root.join(relative))
                .unwrap_or_else(|error| panic!("read {relative}: {error}"));
            assert_eq!(committed, rendered.svg, "{relative} does not reproduce");
        }
    }

    #[test]
    fn the_committed_readme_block_reproduces() {
        let root = workspace_root();
        let readme = std::fs::read_to_string(root.join("README.md")).expect("read the README");
        check(&root, &readme).expect("the committed block and cards reproduce");
    }

    #[test]
    fn a_hand_edited_block_is_named_with_its_marker_and_fix_command() {
        let root = workspace_root();
        let readme = std::fs::read_to_string(root.join("README.md")).expect("read the README");
        let edited = readme.replace(
            "The finals are on 17 December.",
            "The finals are on 17 Dec.",
        );
        assert_ne!(edited, readme, "the edit must change the block");
        let error = check(&root, &edited).expect_err("the edit must be caught");
        let message = format!("{error:#}");
        assert!(message.contains("<!-- winner:begin -->"), "{message}");
        assert!(message.contains("cargo xtask winner"), "{message}");
    }

    #[test]
    fn a_stale_card_is_reported_by_path() {
        let winner = committed();
        let dir = std::env::temp_dir().join(format!("climb-winner-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("docs/media")).expect("create the fixture root");
        let mut table = toml::map::Map::new();
        for (key, value) in [
            ("heading", &winner.heading),
            ("event", &winner.event),
            ("zone", &winner.zone),
            ("venue", &winner.venue),
            ("date", &winner.date),
            ("project", &winner.project),
            ("next", &winner.next),
            ("finals_date", &winner.finals_date),
            ("badge_label", &winner.badge_label),
            ("badge_message", &winner.badge_message),
            ("link", &winner.link),
        ] {
            table.insert(key.to_string(), toml::Value::String(value.clone()));
        }
        let root = toml::Value::Table(
            [("winner".to_string(), toml::Value::Table(table))]
                .into_iter()
                .collect(),
        );
        std::fs::write(
            dir.join(CONFIG_PATH),
            toml::to_string(&root).expect("serialize the fixture"),
        )
        .expect("write the fixture configuration");
        std::fs::write(
            dir.join("README.md"),
            format!("# Title\n\n{}\n", render_block(&winner).expect("render")),
        )
        .expect("write the fixture README");
        std::fs::write(dir.join(SVG_PATH), render_wide(&winner).expect("wide").svg)
            .expect("write the wide card");
        let mut narrow = render_narrow(&winner).expect("narrow").svg;
        narrow.push(' ');
        std::fs::write(dir.join(NARROW_PATH), narrow).expect("write the stale narrow card");

        let readme = std::fs::read_to_string(dir.join("README.md")).expect("read the README");
        let error = check(&dir, &readme).expect_err("a stale card must be caught");
        let message = format!("{error:#}");
        assert!(message.contains(NARROW_PATH), "{message}");
        assert!(
            message.contains("does not match a fresh render"),
            "{message}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_next_line_that_disagrees_with_the_finals_date_is_refused() {
        let mut winner = committed();
        winner.next = "Next: Finals · 18 Dec".to_string();
        let error = winner
            .validate()
            .expect_err("a disagreeing next line must be refused")
            .to_string();
        assert!(error.contains("finals_date"), "{error}");
    }

    #[test]
    fn an_invalid_date_is_refused() {
        let mut winner = committed();
        winner.date = "2026-11-31".to_string();
        let error = winner
            .validate()
            .expect_err("an impossible date must be refused")
            .to_string();
        assert!(error.contains("2026-11-31"), "{error}");
    }

    #[test]
    fn a_link_that_is_not_a_fragment_is_refused() {
        let mut winner = committed();
        winner.link = "docs/media/regional-winner.svg".to_string();
        let error = winner
            .validate()
            .expect_err("an absolute link must be refused")
            .to_string();
        assert!(error.contains("[winner].link"), "{error}");
    }
}
