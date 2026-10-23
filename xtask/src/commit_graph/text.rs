//! Text-to-path conversion for the generated SVG artwork.
//!
//! Every character is emitted as a `<use>` reference to a `<path>` definition
//! that holds the glyph outline. Sharing one outline per distinct character
//! keeps the file small, and the rendered image carries the shapes themselves:
//! no font file is requested, so the card looks the same in every viewer.
//!
//! Two faces are embedded. The display face sets the title and the large
//! numerals; the mono face sets captions, labels and every small number. Glyph
//! ids are namespaced per face so two outlines for the same character cannot
//! collide. A line is one group holding the placement transform, so each
//! `<use>` only carries a pen offset in font units rather than a full
//! screen-space box.
//!
//! Exact ink boxes are measured from the glyph outlines, not from a nominal
//! em box. The layout uses those boxes for collision avoidance, and the
//! rendered groups carry them as `data-bbox` so a test can recompute the same
//! geometry from the artifact.

use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// The embedded fonts, both licensed under the SIL Open Font License 1.1.
/// Their licences travel beside them in `assets/fonts/OFL.txt`.
const MONO_BYTES: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");
const DISPLAY_BYTES: &[u8] = include_bytes!("../../../assets/fonts/InstrumentSerif-Regular.ttf");

/// The two faces a card may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Typeface {
    /// Captions, labels and small numbers.
    Mono,
    /// The title and the large numerals.
    Display,
}

impl Typeface {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Mono => "mono",
            Self::Display => "display",
        }
    }

    fn glyph_prefix(self) -> char {
        match self {
            Self::Mono => 'm',
            Self::Display => 'd',
        }
    }
}

pub(crate) struct Font {
    mono: Face<'static>,
    display: Face<'static>,
    mono_upm: f64,
    display_upm: f64,
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

    pub(crate) fn intersects(&self, other: &Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// Distance between two boxes: zero when they touch or overlap, otherwise
    /// the Euclidean distance between their nearest edges.
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
        let mono = Face::parse(MONO_BYTES, 0).context("failed to parse the mono font")?;
        let display = Face::parse(DISPLAY_BYTES, 0).context("failed to parse the display font")?;
        let mono_upm = f64::from(mono.units_per_em());
        let display_upm = f64::from(display.units_per_em());
        if mono_upm <= 0.0 || display_upm <= 0.0 {
            bail!("an embedded font reports no em size");
        }
        Ok(Self {
            mono,
            display,
            mono_upm,
            display_upm,
        })
    }

    pub(crate) fn face(&self, face: Typeface) -> &Face<'static> {
        match face {
            Typeface::Mono => &self.mono,
            Typeface::Display => &self.display,
        }
    }

    fn upm(&self, face: Typeface) -> f64 {
        match face {
            Typeface::Mono => self.mono_upm,
            Typeface::Display => self.display_upm,
        }
    }

    /// The advance of a character in font units.
    fn advance_units(&self, face: Typeface, character: char) -> Result<f64> {
        let parsed = self.face(face);
        let glyph = parsed
            .glyph_index(character)
            .with_context(|| format!("the {} font has no glyph for {character:?}", face.id()))?;
        parsed
            .glyph_hor_advance(glyph)
            .map(f64::from)
            .with_context(|| {
                format!(
                    "the {} font carries no advance for {character:?}",
                    face.id()
                )
            })
    }

    /// Width of one line in user units, including optional tracking in em
    /// units.
    pub(crate) fn width(
        &self,
        face: Typeface,
        text: &str,
        size: f64,
        tracking: f64,
    ) -> Result<f64> {
        let scale = size / self.upm(face);
        let mut width = 0.0;
        for character in text.chars() {
            width += self.advance_units(face, character)?;
            width += tracking * self.upm(face);
        }
        Ok(width * scale)
    }

    /// The exact ink box of one line, measured from the glyph outlines.
    ///
    /// A space has an advance but no outline. A line made only of spaces has no
    /// ink, so its box is a point at the baseline; no card draws such a line.
    pub(crate) fn line_box(
        &self,
        face: Typeface,
        text: &str,
        x: f64,
        baseline: f64,
        size: f64,
        tracking: f64,
    ) -> Result<TextBox> {
        let parsed = self.face(face);
        let scale = size / self.upm(face);
        let mut pen = 0.0f64;
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for character in text.chars() {
            let glyph = parsed.glyph_index(character).with_context(|| {
                format!("the {} font has no glyph for {character:?}", face.id())
            })?;
            if let Some(bounds) = parsed.glyph_bounding_box(glyph) {
                min_x = min_x.min(pen + f64::from(bounds.x_min));
                max_x = max_x.max(pen + f64::from(bounds.x_max));
                min_y = min_y.min(f64::from(bounds.y_min));
                max_y = max_y.max(f64::from(bounds.y_max));
            }
            pen += self.advance_units(face, character)? + tracking * self.upm(face);
        }
        if !min_x.is_finite() || !max_x.is_finite() {
            return Ok(TextBox {
                x,
                y: baseline,
                width: 0.0,
                height: 0.0,
            });
        }
        Ok(TextBox {
            x: x + min_x * scale,
            y: baseline - max_y * scale,
            width: (max_x - min_x) * scale,
            height: (max_y - min_y) * scale,
        })
    }

    fn glyph(&self, face: Typeface, character: char) -> Option<GlyphId> {
        self.face(face).glyph_index(character)
    }

    fn outline(&self, face: Typeface, glyph: GlyphId) -> Option<String> {
        let mut builder = PathBuilder::default();
        self.face(face).outline_glyph(glyph, &mut builder)?;
        if builder.d.is_empty() {
            None
        } else {
            Some(builder.d)
        }
    }

    /// True when the face can draw the character without a fallback.
    pub(crate) fn has_outline(&self, face: Typeface, character: char) -> bool {
        self.glyph(face, character)
            .and_then(|glyph| self.outline(face, glyph))
            .is_some()
    }
}

/// Glyph outlines collected per face, so the two definitions of `7` cannot
/// collide.
pub(crate) type UsedGlyphs = BTreeMap<Typeface, BTreeSet<char>>;

/// Markup for one line of text, drawn as glyph outlines. Glyphs that the face
/// cannot draw are an error: a silent fallback would change the picture.
// One text run is described by its content, placement and style; splitting
// those into a struct would only move the same fields to the call sites.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text_markup(
    font: &Font,
    face: Typeface,
    text: &str,
    x: f64,
    baseline: f64,
    size: f64,
    fill: &str,
    tracking: f64,
    used: &mut UsedGlyphs,
) -> Result<String> {
    let scale = size / font.upm(face);
    let box_ = font.line_box(face, text, x, baseline, size, tracking)?;
    let mut out = String::new();
    let _ = write!(
        out,
        "<g class=\"cg-run\" data-bbox=\"{} {} {} {}\" \
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
        if font.has_outline(face, character) {
            used.entry(face).or_default().insert(character);
            let _ = write!(
                out,
                "<use href=\"#{}{}\"",
                face.glyph_prefix(),
                character as u32
            );
            if pen != 0.0 {
                let _ = write!(out, " x=\"{}\"", number(pen));
            }
            out.push_str("/>");
        } else if !character.is_whitespace() {
            bail!(
                "the {} font cannot draw {character:?} in `{text}`; refusing to fall back",
                face.id()
            );
        }
        pen += font.advance_units(face, character)? + tracking * font.upm(face);
    }
    out.push_str("</g></g>");
    Ok(out)
}

/// Path definitions for every collected character, one namespace per face.
pub(crate) fn glyph_defs(font: &Font, used: &UsedGlyphs) -> String {
    let mut out = String::new();
    for (face, characters) in used {
        for character in characters {
            let Some(glyph) = font.glyph(*face, *character) else {
                continue;
            };
            let Some(path) = font.outline(*face, glyph) else {
                continue;
            };
            let _ = write!(
                out,
                "<path id=\"{}{}\" d=\"{path}\"/>",
                face.glyph_prefix(),
                *character as u32
            );
        }
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
    fn both_faces_load_and_report_their_own_em_size() {
        let font = Font::load().expect("embedded fonts");
        assert!(font.upm(Typeface::Mono) >= 1000.0);
        assert!(font.upm(Typeface::Display) >= 1000.0);
        // The mono face is monospaced; the display face is not, so measuring
        // every character with the mono advance would misplace the serif.
        let narrow = font.width(Typeface::Display, "i", 10.0, 0.0).unwrap();
        let wide = font.width(Typeface::Display, "W", 10.0, 0.0).unwrap();
        assert!((narrow - wide).abs() > 0.1);
    }

    #[test]
    fn glyph_outlines_are_non_empty_for_letters_and_digits() {
        let font = Font::load().expect("embedded fonts");
        for face in [Typeface::Mono, Typeface::Display] {
            for character in "ABCxyz0189-./:+,".chars() {
                assert!(
                    font.has_outline(face, character),
                    "{} has no outline for {character}",
                    face.id()
                );
            }
        }
    }

    #[test]
    fn every_character_of_the_displayed_lines_exists_in_its_assigned_face() {
        let font = Font::load().expect("embedded fonts");
        let lines = [
            (Typeface::Display, "The Climb"),
            (Typeface::Display, "+78,662"),
            (Typeface::Display, "-5,773"),
            (Typeface::Mono, "COMMIT HISTORY / PUBLIC REPOSITORY"),
            (Typeface::Mono, "as of f94fc6a"),
            (Typeface::Mono, "22 Oct 2026"),
            (Typeface::Mono, "busiest day - 8 commits - 12 Oct"),
            (Typeface::Mono, "Finals, IIT Jammu - 17 Dec"),
        ];
        for (face, text) in lines {
            for character in text.chars().filter(|character| *character != ' ') {
                assert!(
                    font.has_outline(face, character),
                    "{} cannot draw {character:?} from {text:?}",
                    face.id()
                );
            }
        }
    }

    #[test]
    fn an_undrawable_character_is_an_error_not_a_silent_fallback() {
        let font = Font::load().expect("embedded fonts");
        let mut used = UsedGlyphs::new();
        let error = text_markup(
            &font,
            Typeface::Mono,
            "ok \u{e000}",
            0.0,
            10.0,
            10.0,
            "#fff",
            0.0,
            &mut used,
        )
        .expect_err("an arrow has no glyph");
        // The face has no outline for the arrow, so the line is refused rather
        // than drawn with a substitute glyph.
        assert!(error.to_string().contains("no glyph"), "{error}");
    }

    #[test]
    fn a_space_advances_but_defines_no_glyph() {
        let font = Font::load().expect("embedded fonts");
        let mut used = UsedGlyphs::new();
        let markup = text_markup(
            &font,
            Typeface::Mono,
            "a b",
            0.0,
            10.0,
            10.0,
            "#fff",
            0.0,
            &mut used,
        )
        .expect("text");
        assert!(!used.values().flatten().any(|character| *character == ' '));
        assert_eq!(markup.matches("<use ").count(), 2);
        assert!(
            font.width(Typeface::Mono, "a b", 10.0, 0.0).unwrap()
                > font.width(Typeface::Mono, "ab", 10.0, 0.0).unwrap()
        );
    }

    #[test]
    fn text_markup_never_emits_a_text_element() {
        let font = Font::load().expect("embedded fonts");
        let mut used = UsedGlyphs::new();
        let markup = text_markup(
            &font,
            Typeface::Mono,
            "THE CLIMB",
            0.0,
            10.0,
            12.0,
            "#fff",
            0.0,
            &mut used,
        )
        .expect("text");
        assert!(!markup.contains("<text"));
        assert_eq!(markup.matches("<use ").count(), "THECLIMB".chars().count());
    }

    #[test]
    fn ink_boxes_follow_the_outlines_rather_than_the_em_box() {
        let font = Font::load().expect("embedded fonts");
        // A digit has no descender: its ink box ends at the baseline, while
        // the old cap-plus-descender box over-reported the height by a third.
        let digit = font
            .line_box(Typeface::Display, "71", 0.0, 100.0, 40.0, 0.0)
            .expect("box");
        assert!((digit.bottom() - 100.0).abs() < 1e-6);
        assert!(digit.height > 20.0 && digit.height < 32.0);
        // A lowercase descender extends below the baseline.
        let descender = font
            .line_box(Typeface::Mono, "gyp", 0.0, 100.0, 100.0, 0.0)
            .expect("box");
        assert!(descender.bottom() > 103.0);
    }

    #[test]
    fn one_line_is_one_placement_transform_with_pen_offsets() {
        let font = Font::load().expect("embedded fonts");
        let mut used = UsedGlyphs::new();
        let markup = text_markup(
            &font,
            Typeface::Mono,
            "abc",
            4.0,
            20.0,
            10.0,
            "#fff",
            0.0,
            &mut used,
        )
        .expect("text");
        assert_eq!(markup.matches("transform=").count(), 1);
        assert_eq!(markup.matches("width=").count(), 0);
        assert_eq!(markup.matches(" x=").count(), 2);
        assert_eq!(used.get(&Typeface::Mono).map(BTreeSet::len), Some(3));
    }

    #[test]
    fn glyph_definitions_are_unique_sorted_and_namespaced() {
        let font = Font::load().expect("embedded fonts");
        let mut used = UsedGlyphs::new();
        let markup = text_markup(
            &font,
            Typeface::Display,
            "7",
            0.0,
            10.0,
            10.0,
            "#fff",
            0.0,
            &mut used,
        )
        .expect("text");
        assert!(markup.contains("href=\"#d55\""));
        let defs = glyph_defs(&font, &used);
        assert!(defs.contains("id=\"d55\""));
        assert!(!defs.contains("id=\"m55\""));
        assert_eq!(defs.matches("<path id=").count(), 1);
    }

    #[test]
    fn glyph_paths_are_relative_and_reuse_their_command_letter() {
        let font = Font::load().expect("embedded fonts");
        let mut used = UsedGlyphs::new();
        for character in "aeo8".chars() {
            used.entry(Typeface::Display).or_default().insert(character);
        }
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
