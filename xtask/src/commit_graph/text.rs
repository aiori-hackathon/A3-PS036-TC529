//! Text-to-path conversion for the generated SVG cards.
//!
//! Every character is emitted as a `<use>` reference to a `<path>` definition
//! that holds the glyph outline. Sharing one outline per distinct character
//! keeps the file small, and the rendered image carries the shapes themselves:
//! no font file is requested, so the card looks the same in every viewer.
//!
//! A line is one group holding the placement transform, so each `<use>` only
//! carries a pen offset in font units rather than a full screen-space box.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// The embedded font: JetBrains Mono Regular, licensed under the SIL Open Font
/// License 1.1. Its licence travels beside it in `assets/fonts/OFL.txt`.
const FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");

pub(crate) struct Font {
    face: Face<'static>,
    units_per_em: f64,
    advance: f64,
    /// Cap height in font units: the top of the ink box for a line of text.
    cap_height: f64,
    /// Descender depth in font units, positive.
    descender: f64,
}

/// An axis-aligned ink box for one line of text, in SVG user units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextBox {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl TextBox {
    pub(crate) fn right(&self) -> f64 {
        self.x + self.width
    }

    pub(crate) fn bottom(&self) -> f64 {
        self.y + self.height
    }

    pub(crate) fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }

    #[cfg(test)]
    pub(crate) fn intersects(&self, other: &Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// Distance between two boxes: zero when they touch or overlap, otherwise
    /// the Euclidean distance between their nearest edges.
    #[cfg(test)]
    pub(crate) fn gap(&self, other: &Self) -> f64 {
        let dx = (other.x - self.right())
            .max(self.x - other.right())
            .max(0.0);
        let dy = (other.y - self.bottom())
            .max(self.y - other.bottom())
            .max(0.0);
        (dx * dx + dy * dy).sqrt()
    }
}

impl Font {
    pub(crate) fn load() -> Result<Self> {
        let face = Face::parse(FONT_BYTES, 0).context("failed to parse the embedded font")?;
        let units_per_em = f64::from(face.units_per_em());
        if units_per_em <= 0.0 {
            anyhow::bail!("the embedded font reports no em size");
        }
        let advance = face
            .glyph_index('0')
            .and_then(|glyph| face.glyph_hor_advance(glyph))
            .map(f64::from)
            .unwrap_or(units_per_em / 2.0);
        // The card only ever draws ASCII, so the cap height is the top of the
        // ink. Falling back to 0.72 em keeps the box conservative if a future
        // font drops the metric.
        let cap_height = face
            .capital_height()
            .map(f64::from)
            .unwrap_or(units_per_em * 0.72);
        let descender = f64::from(face.descender()).abs();
        Ok(Self {
            face,
            units_per_em,
            advance,
            cap_height,
            descender,
        })
    }

    /// The ink box of one line, used by the layout budget and by the
    /// readability contract in the tests.
    pub(crate) fn line_box(
        &self,
        text: &str,
        x: f64,
        baseline: f64,
        size: f64,
        tracking: f64,
    ) -> TextBox {
        let scale = size / self.units_per_em;
        TextBox {
            x,
            y: baseline - self.cap_height * scale,
            width: text_width(self, text, size, tracking),
            height: (self.cap_height + self.descender) * scale,
        }
    }

    fn glyph(&self, character: char) -> Option<GlyphId> {
        self.face.glyph_index(character)
    }

    fn outline(&self, glyph: GlyphId) -> Option<String> {
        let mut builder = PathBuilder::default();
        self.face.outline_glyph(glyph, &mut builder)?;
        if builder.d.is_empty() {
            None
        } else {
            Some(builder.d)
        }
    }
}

/// Width of one line in pixels, including optional tracking in em units.
pub(crate) fn text_width(font: &Font, text: &str, size: f64, tracking: f64) -> f64 {
    let scale = size / font.units_per_em;
    text.chars()
        .map(|_| (font.advance + tracking * font.units_per_em) * scale)
        .sum()
}

/// Markup for one line of text, drawn as glyph outlines. `used` collects every
/// character whose glyph must be defined.
// One text run is described by its content, placement and style; splitting
// those into a struct would only move the same fields to the call sites.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text_markup(
    font: &Font,
    text: &str,
    x: f64,
    baseline: f64,
    size: f64,
    fill: &str,
    tracking: f64,
    used: &mut BTreeSet<char>,
) -> String {
    let scale = size / font.units_per_em;
    let box_ = font.line_box(text, x, baseline, size, tracking);
    let mut out = String::new();
    let _ = write!(
        out,
        "<g class=\"cg-run\" data-box=\"{} {} {} {}\" \
         transform=\"translate({} {})scale({scale:.5})\"><g fill=\"{fill}\">",
        number(box_.x),
        number(box_.y),
        number(box_.width),
        number(box_.height),
        number(x),
        number(baseline)
    );
    let mut pen = 0.0;
    for character in text.chars() {
        if let Some(glyph) = font.glyph(character) {
            if font.outline(glyph).is_some() {
                used.insert(character);
                let _ = write!(out, "<use href=\"#g{}\"", character as u32);
                if pen != 0.0 {
                    let _ = write!(out, " x=\"{}\"", number(pen));
                }
                out.push_str("/>");
            }
        }
        pen += font.advance + tracking * font.units_per_em;
    }
    out.push_str("</g></g>");
    out
}

/// Path definitions for every collected character.
pub(crate) fn glyph_defs(font: &Font, used: &BTreeSet<char>) -> String {
    let mut out = String::new();
    for character in used {
        let Some(glyph) = font.glyph(*character) else {
            continue;
        };
        let Some(path) = font.outline(glyph) else {
            continue;
        };
        let _ = write!(out, "<path id=\"g{}\" d=\"{path}\"/>", *character as u32);
    }
    out
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

/// Builds an SVG path from the font outline. Font coordinates are y-up; the
/// path is emitted y-down, with relative commands and implicit command
/// repetition, which keeps the repeated glyph definitions small.
#[derive(Default)]
struct PathBuilder {
    d: String,
    last: (i64, i64),
    start: (i64, i64),
    mode: Option<char>,
}

fn unit(value: f32) -> i64 {
    let rounded = value.round();
    if rounded == 0.0 {
        0
    } else {
        rounded as i64
    }
}

fn point(x: f32, y: f32) -> (i64, i64) {
    (unit(x), unit(-y))
}

impl PathBuilder {
    /// Emits one command, letting a repeated command letter stay implicit.
    fn emit(&mut self, command: char, values: &[i64]) {
        if self.mode != Some(command) {
            self.d.push(command);
            self.mode = Some(command);
        }
        for value in values {
            if *value < 0 {
                let _ = write!(self.d, "{value}");
            } else {
                let _ = write!(self.d, " {value}");
            }
        }
    }

    fn line(&mut self, to: (i64, i64)) {
        self.emit('l', &[to.0 - self.last.0, to.1 - self.last.1]);
        self.last = to;
    }
}

impl OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let to = point(x, y);
        let _ = write!(self.d, "M{}", to.0);
        if to.1 < 0 {
            let _ = write!(self.d, "{}", to.1);
        } else {
            let _ = write!(self.d, " {}", to.1);
        }
        self.mode = None;
        self.last = to;
        self.start = to;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.line(point(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let control = point(x1, y1);
        let to = point(x, y);
        self.emit(
            'q',
            &[
                control.0 - self.last.0,
                control.1 - self.last.1,
                to.0 - self.last.0,
                to.1 - self.last.1,
            ],
        );
        self.last = to;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let first = point(x1, y1);
        let second = point(x2, y2);
        let to = point(x, y);
        self.emit(
            'c',
            &[
                first.0 - self.last.0,
                first.1 - self.last.1,
                second.0 - self.last.0,
                second.1 - self.last.1,
                to.0 - self.last.0,
                to.1 - self.last.1,
            ],
        );
        self.last = to;
    }

    fn close(&mut self) {
        self.d.push('z');
        self.mode = None;
        self.last = self.start;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_loads_and_reports_a_monospace_advance() {
        let font = Font::load().expect("embedded font");
        assert!(font.units_per_em >= 1000.0);
        // Monospace: every printable ASCII character has the same advance.
        let width_a = text_width(&font, "a", 10.0, 0.0);
        let width_w = text_width(&font, "W", 10.0, 0.0);
        assert!((width_a - width_w).abs() < f64::EPSILON);
    }

    #[test]
    fn glyph_outlines_are_non_empty_for_letters_and_digits() {
        let font = Font::load().expect("embedded font");
        for character in "ABCxyz0189-./:+".chars() {
            let glyph = font.glyph(character).expect("glyph exists");
            assert!(
                font.outline(glyph).is_some(),
                "expected an outline for {character}"
            );
        }
    }

    #[test]
    fn a_space_advances_but_defines_no_glyph() {
        let font = Font::load().expect("embedded font");
        let mut used = BTreeSet::new();
        let markup = text_markup(&font, "a b", 0.0, 10.0, 10.0, "#fff", 0.0, &mut used);
        assert!(!used.contains(&' '));
        assert_eq!(markup.matches("<use ").count(), 2);
        assert!(text_width(&font, "a b", 10.0, 0.0) > text_width(&font, "ab", 10.0, 0.0));
    }

    #[test]
    fn text_markup_never_emits_a_text_element() {
        let font = Font::load().expect("embedded font");
        let mut used = BTreeSet::new();
        let markup = text_markup(&font, "THE CLIMB", 0.0, 10.0, 12.0, "#fff", 0.0, &mut used);
        assert!(!markup.contains("<text"));
        // The space advances the pen but has no outline, so it draws nothing.
        assert_eq!(markup.matches("<use ").count(), "THECLIMB".chars().count());
    }

    #[test]
    fn one_line_is_one_placement_transform_with_pen_offsets() {
        let font = Font::load().expect("embedded font");
        let mut used = BTreeSet::new();
        let markup = text_markup(&font, "abc", 4.0, 20.0, 10.0, "#fff", 0.0, &mut used);
        assert_eq!(markup.matches("transform=").count(), 1);
        assert_eq!(markup.matches("width=").count(), 0);
        // The first glyph sits at the origin of the transformed group; the
        // next two carry a pen offset in font units.
        assert_eq!(markup.matches(" x=").count(), 2);
        assert_eq!(used.len(), 3);
    }

    #[test]
    fn glyph_definitions_are_unique_and_sorted() {
        let font = Font::load().expect("embedded font");
        let used: BTreeSet<char> = "BAB".chars().collect();
        let defs = glyph_defs(&font, &used);
        assert_eq!(defs.matches("<path id=").count(), 2);
        let a = defs.find("id=\"g65\"").unwrap_or(usize::MAX);
        let b = defs.find("id=\"g66\"").unwrap_or(usize::MAX);
        assert!(a < b);
    }

    #[test]
    fn glyph_paths_are_relative_and_reuse_their_command_letter() {
        let font = Font::load().expect("embedded font");
        let used: BTreeSet<char> = "aeo8".chars().collect();
        let defs = glyph_defs(&font, &used);
        assert!(defs.contains("M"));
        for upper in ["L", "Q", "C", "H", "V"] {
            assert!(
                !defs.contains(upper),
                "absolute command {upper} was emitted"
            );
        }
        assert!(defs.contains('l'));
    }
}
