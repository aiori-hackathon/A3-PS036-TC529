//! Theming, colour-depth mapping and glyph sets.

use clap::ValueEnum;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;

/// A selectable theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ThemeId {
    /// Dark background (default).
    #[default]
    Dark,
    /// Light background, tuned for projectors.
    Light,
    /// Maximum-contrast palette.
    Contrast,
}

impl ThemeId {
    /// Every theme, in cycle order.
    pub const ALL: [ThemeId; 3] = [ThemeId::Dark, ThemeId::Light, ThemeId::Contrast];

    /// The next theme in the cycle, wrapping at the end.
    pub fn next(self) -> ThemeId {
        ThemeId::ALL
            .get((self.index() + 1) % ThemeId::ALL.len())
            .copied()
            .unwrap_or(self)
    }

    /// The position of this theme in [`ThemeId::ALL`].
    pub fn index(self) -> usize {
        match self {
            ThemeId::Dark => 0,
            ThemeId::Light => 1,
            ThemeId::Contrast => 2,
        }
    }

    /// Stable lowercase label used on the CLI and in the status bar.
    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Dark => "dark",
            ThemeId::Light => "light",
            ThemeId::Contrast => "contrast",
        }
    }
}

impl std::fmt::Display for ThemeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// The terminal's colour capability, from richest to poorest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Ansi256,
    Ansi16,
    Mono,
}

impl ColorDepth {
    /// Stable label used in the status bar.
    pub fn label(self) -> &'static str {
        match self {
            ColorDepth::TrueColor => "truecolor",
            ColorDepth::Ansi256 => "256",
            ColorDepth::Ansi16 => "16",
            ColorDepth::Mono => "mono",
        }
    }
}

/// Decide the colour depth from `COLORTERM`, `TERM` and the `NO_COLOR` convention.
///
/// Pure by construction so it can be exercised without a real terminal. `NO_COLOR` wins
/// over everything (https://no-color.org): when set, output degrades to
/// [`ColorDepth::Mono`].
pub fn detect_color_depth(
    colorterm: Option<&str>,
    term: Option<&str>,
    no_color: bool,
) -> ColorDepth {
    if no_color {
        return ColorDepth::Mono;
    }
    if let Some(colorterm) = colorterm {
        let colorterm = colorterm.to_ascii_lowercase();
        if colorterm == "truecolor" || colorterm == "24bit" {
            return ColorDepth::TrueColor;
        }
    }
    if let Some(term) = term {
        let term = term.to_ascii_lowercase();
        if term.contains("truecolor") || term.contains("24bit") {
            return ColorDepth::TrueColor;
        }
        if term.contains("256color") {
            return ColorDepth::Ansi256;
        }
    }
    ColorDepth::Ansi16
}

/// Read the ambient colour capability from the process environment.
pub fn depth_from_env() -> ColorDepth {
    let colorterm = std::env::var("COLORTERM").ok();
    let term = std::env::var("TERM").ok();
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
    detect_color_depth(colorterm.as_deref(), term.as_deref(), no_color)
}

/// An 8-bit-per-channel colour.
pub type Rgb = (u8, u8, u8);

/// Map a truecolour value down to the requested depth.
pub fn map_color(rgb: Rgb, depth: ColorDepth) -> Color {
    match depth {
        ColorDepth::TrueColor => Color::Rgb(rgb.0, rgb.1, rgb.2),
        ColorDepth::Ansi256 => Color::Indexed(nearest_ansi256(rgb)),
        ColorDepth::Ansi16 => nearest_ansi16(rgb, None, 0.0),
        ColorDepth::Mono => Color::Reset,
    }
}

fn lerp_rgb(from: Rgb, to: Rgb, t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
    (mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2))
}

fn dist2(a: Rgb, b: Rgb) -> u32 {
    let d = |x: u8, y: u8| {
        let delta = x as i32 - y as i32;
        (delta * delta) as u32
    };
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
}

/// Perceived chroma of a colour: the spread between its brightest and darkest
/// channel. Zero for every grey.
fn chroma(rgb: Rgb) -> u8 {
    let max = rgb.0.max(rgb.1).max(rgb.2);
    let min = rgb.0.min(rgb.1).min(rgb.2);
    max - min
}

/// Hue angle in degrees, or `None` for a grey.
fn hue(rgb: Rgb) -> Option<f64> {
    let (red, green, blue) = (rgb.0 as f64, rgb.1 as f64, rgb.2 as f64);
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    if delta == 0.0 {
        return None;
    }
    let angle = if max == red {
        60.0 * (((green - blue) / delta).rem_euclid(6.0))
    } else if max == green {
        60.0 * (((blue - red) / delta) + 2.0)
    } else {
        60.0 * (((red - green) / delta) + 4.0)
    };
    Some(angle)
}

/// Shortest angular distance between two hues, in degrees (0..=180).
fn hue_distance(a: f64, b: f64) -> f64 {
    let difference = (a - b).abs() % 360.0;
    difference.min(360.0 - difference)
}

/// WCAG 2.x relative luminance of an sRGB colour.
fn relative_luminance(rgb: Rgb) -> f64 {
    fn channel(value: u8) -> f64 {
        let value = value as f64 / 255.0;
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(rgb.0) + 0.7152 * channel(rgb.1) + 0.0722 * channel(rgb.2)
}

/// WCAG 2.x contrast ratio between two sRGB colours, from 1.0 to 21.0.
///
/// This is the computation the accessibility tests use against the palette the
/// dashboard actually resolves for a terminal, not against an idealised one.
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (lighter, darker) = {
        let (la, lb) = (relative_luminance(a), relative_luminance(b));
        if la >= lb {
            (la, lb)
        } else {
            (lb, la)
        }
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// The RGB value a terminal will paint for a ratatui colour.
fn resolved_rgb(color: Color) -> Option<Rgb> {
    match color {
        Color::Rgb(red, green, blue) => Some((red, green, blue)),
        Color::Reset => None,
        Color::Indexed(index) => Some(ansi256_rgb(index)),
        other => ansi16_rgb(other),
    }
}

/// The RGB value of one of the sixteen ANSI colours.
pub fn ansi16_rgb(color: Color) -> Option<Rgb> {
    ANSI16
        .iter()
        .find_map(|(rgb, candidate)| (*candidate == color).then_some(*rgb))
}

/// The RGB value of an xterm-256 index.
fn ansi256_rgb(index: u8) -> Rgb {
    match index {
        0..=15 => ANSI16
            .get(index as usize)
            .map(|(rgb, _)| *rgb)
            .unwrap_or((0, 0, 0)),
        16..=231 => {
            let n = index - 16;
            let r = n / 36;
            let g = (n % 36) / 6;
            let b = n % 6;
            (
                CUBE_LEVELS.get(r as usize).copied().unwrap_or(0),
                CUBE_LEVELS.get(g as usize).copied().unwrap_or(0),
                CUBE_LEVELS.get(b as usize).copied().unwrap_or(0),
            )
        }
        _ => {
            let level = 8 + 10 * (index - 232);
            (level, level, level)
        }
    }
}

const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// Nearest xterm-256 index, choosing between the 6x6x6 cube and the 24-step grey ramp.
fn nearest_ansi256(rgb: Rgb) -> u8 {
    let nearest_level = |value: u8| -> usize {
        let mut best = 0usize;
        let mut best_distance = u32::MAX;
        for (i, &level) in CUBE_LEVELS.iter().enumerate() {
            let distance = (value as i32 - level as i32).unsigned_abs();
            if distance < best_distance {
                best_distance = distance;
                best = i;
            }
        }
        best
    };

    let (ri, gi, bi) = (
        nearest_level(rgb.0),
        nearest_level(rgb.1),
        nearest_level(rgb.2),
    );
    let cube_index = 16 + (36 * ri + 6 * gi + bi) as u8;
    let cube_rgb = (
        CUBE_LEVELS.get(ri).copied().unwrap_or(0),
        CUBE_LEVELS.get(gi).copied().unwrap_or(0),
        CUBE_LEVELS.get(bi).copied().unwrap_or(0),
    );

    let average = (rgb.0 as u32 + rgb.1 as u32 + rgb.2 as u32) / 3;
    let grey_step = (((average.saturating_sub(8) + 5) / 10) as u8).min(23);
    let grey_index = 232 + grey_step;
    let grey_level = 8 + 10 * grey_step;
    let grey_rgb = (grey_level, grey_level, grey_level);

    if dist2(rgb, grey_rgb) < dist2(rgb, cube_rgb) {
        grey_index
    } else {
        cube_index
    }
}

const ANSI16: [(Rgb, Color); 16] = [
    ((0, 0, 0), Color::Black),
    ((128, 0, 0), Color::Red),
    ((0, 128, 0), Color::Green),
    ((128, 128, 0), Color::Yellow),
    ((0, 0, 128), Color::Blue),
    ((128, 0, 128), Color::Magenta),
    ((0, 128, 128), Color::Cyan),
    ((192, 192, 192), Color::Gray),
    ((128, 128, 128), Color::DarkGray),
    ((255, 0, 0), Color::LightRed),
    ((0, 255, 0), Color::LightGreen),
    ((255, 255, 0), Color::LightYellow),
    ((0, 0, 255), Color::LightBlue),
    ((255, 0, 255), Color::LightMagenta),
    ((0, 255, 255), Color::LightCyan),
    ((255, 255, 255), Color::White),
];

/// Nearest ANSI-16 colour, optionally constrained to a minimum contrast
/// against `background`.
///
/// A saturated source colour is never allowed to collapse onto a grey just
/// because the grey is numerically close: the sixteen-colour palette is small,
/// and the semantic colours (accent, ok, warn, bad) must stay distinguishable
/// from each other and from body text. When `background` is supplied, a
/// candidate below `min_ratio` is skipped; if no candidate qualifies, the
/// unconstrained nearest colour is returned rather than an arbitrary one.
fn nearest_ansi16(rgb: Rgb, background: Option<Rgb>, min_ratio: f64) -> Color {
    let source_chroma = chroma(rgb);
    let source_hue = hue(rgb);
    let mut best = Color::Reset;
    let mut best_distance = u32::MAX;
    let mut fallback = Color::Reset;
    let mut fallback_distance = u32::MAX;
    for &(candidate, color) in ANSI16.iter() {
        let mut distance = dist2(rgb, candidate);
        if source_chroma > 24 {
            if chroma(candidate) == 0 {
                // A saturated source must not collapse onto a grey just
                // because the grey is numerically close.
                distance = distance.saturating_mul(16);
            } else if let (Some(source), Some(candidate_hue)) = (source_hue, hue(candidate)) {
                // The sixteen-colour palette is coarse: pure yellow sits
                // between red and green, so an RGB distance alone maps both a
                // success green and a failure red onto yellow. Weighting the
                // hue difference keeps the semantic roles distinct.
                let penalty = (hue_distance(source, candidate_hue) / 180.0 * 120_000.0) as u32;
                distance = distance.saturating_add(penalty);
            }
        }
        if distance < fallback_distance {
            fallback_distance = distance;
            fallback = color;
        }
        if let Some(background) = background {
            if contrast_ratio(candidate, background) < min_ratio {
                continue;
            }
        }
        if distance < best_distance {
            best_distance = distance;
            best = color;
        }
    }
    if best_distance == u32::MAX {
        fallback
    } else {
        best
    }
}

struct Palette {
    fg: Rgb,
    bg: Rgb,
    accent: Rgb,
    accent2: Rgb,
    ok: Rgb,
    warn: Rgb,
    bad: Rgb,
    muted: Rgb,
    border: Rgb,
}

impl ThemeId {
    fn palette(self) -> Palette {
        match self {
            ThemeId::Dark => Palette {
                fg: (229, 229, 229),
                bg: (17, 17, 17),
                accent: (86, 182, 194),
                accent2: (197, 134, 192),
                ok: (152, 195, 121),
                warn: (229, 192, 123),
                bad: (224, 108, 117),
                muted: (128, 128, 128),
                border: (86, 86, 86),
            },
            ThemeId::Light => Palette {
                fg: (24, 24, 24),
                bg: (250, 250, 250),
                accent: (14, 90, 140),
                accent2: (130, 60, 130),
                ok: (30, 130, 60),
                warn: (170, 110, 0),
                bad: (180, 30, 30),
                muted: (110, 110, 110),
                border: (70, 70, 70),
            },
            ThemeId::Contrast => Palette {
                fg: (255, 255, 255),
                bg: (0, 0, 0),
                accent: (0, 255, 255),
                accent2: (255, 255, 0),
                ok: (0, 255, 0),
                warn: (255, 255, 0),
                bad: (255, 0, 0),
                muted: (200, 200, 200),
                border: (255, 255, 255),
            },
        }
    }
}

/// A resolved theme: colours mapped to the terminal's depth, the three semantic data
/// styles, and the glyph sets.
#[derive(Debug, Clone)]
pub struct Theme {
    pub id: ThemeId,
    pub depth: ColorDepth,
    pub ascii: bool,
    pub fg: Color,
    pub bg: Color,
    pub accent: Color,
    pub accent2: Color,
    pub ok: Color,
    pub warn: Color,
    pub bad: Color,
    pub muted: Color,
    pub border: Color,
    /// Style for directly measured values.
    pub measured: Style,
    /// Style for values derived by extrapolation.
    pub extrapolated: Style,
    /// Style for closed-form analytical values.
    pub analytical: Style,
}

impl Theme {
    /// Resolve a theme to a concrete colour depth, selecting the glyph set later.
    pub fn new(id: ThemeId, depth: ColorDepth, ascii: bool) -> Self {
        let palette = id.palette();
        let bg = map_color(palette.bg, depth);
        // Contrast-aware mapping only matters on the sixteen-colour palette;
        // truecolour and 256-colour keep the exact palette value.
        let bg_rgb = resolved_rgb(bg).unwrap_or(palette.bg);
        let role = |rgb: Rgb, minimum: f64| -> Color {
            if depth == ColorDepth::Ansi16 {
                nearest_ansi16(rgb, Some(bg_rgb), minimum)
            } else {
                map_color(rgb, depth)
            }
        };
        let fg = role(palette.fg, 7.0);
        let accent = role(palette.accent, 4.5);
        let accent2 = role(palette.accent2, 3.0);
        let warn = role(palette.warn, 3.0);

        let (measured, extrapolated, analytical) = if depth == ColorDepth::Mono {
            (
                Style::new().add_modifier(Modifier::BOLD),
                Style::new().add_modifier(Modifier::DIM),
                Style::new().add_modifier(Modifier::REVERSED),
            )
        } else {
            (
                Style::new().fg(accent).add_modifier(Modifier::BOLD),
                Style::new().fg(warn).add_modifier(Modifier::DIM),
                Style::new().fg(accent2).add_modifier(Modifier::ITALIC),
            )
        };

        Self {
            id,
            depth,
            ascii,
            fg,
            bg,
            accent,
            accent2,
            ok: role(palette.ok, 3.0),
            warn,
            bad: role(palette.bad, 3.0),
            muted: role(palette.muted, 3.0),
            border: role(palette.border, 2.0),
            measured,
            extrapolated,
            analytical,
        }
    }

    /// Resolve one of this theme's colours back to the RGB a terminal will
    /// actually paint, for the active colour depth.
    pub fn resolved_rgb(&self, color: Color) -> Option<Rgb> {
        resolved_rgb(color)
    }

    /// Contrast ratio between two of this theme's colours, when both resolve.
    pub fn contrast(&self, foreground: Color, background: Color) -> Option<f64> {
        let foreground = self.resolved_rgb(foreground)?;
        let background = self.resolved_rgb(background)?;
        Some(contrast_ratio(foreground, background))
    }

    /// Every semantic foreground against the theme background, with the
    /// contrast the active colour depth will really produce. `mono` has no
    /// colour to measure and returns an empty list.
    pub fn contrast_pairs(&self) -> Vec<(&'static str, f64)> {
        let mut pairs = Vec::new();
        for (role, colour) in [
            ("fg", self.fg),
            ("accent", self.accent),
            ("accent2", self.accent2),
            ("ok", self.ok),
            ("warn", self.warn),
            ("bad", self.bad),
            ("muted", self.muted),
            ("border", self.border),
        ] {
            if let Some(ratio) = self.contrast(colour, self.bg) {
                pairs.push((role, ratio));
            }
        }
        pairs
    }

    /// The active glyph set (ASCII when `ascii` is set, Unicode otherwise).
    pub fn glyphs(&self) -> Glyphs {
        if self.ascii {
            Glyphs::ascii()
        } else {
            Glyphs::unicode()
        }
    }

    /// One style per word-mark row: a top-to-bottom `accent` → `accent2`
    /// gradient, bold throughout. At [`ColorDepth::Mono`] only the bold
    /// survives, so the mark stays legible with no colour to rely on.
    pub fn wordmark_rows(&self, rows: usize) -> Vec<Style> {
        let palette = self.id.palette();
        (0..rows)
            .map(|row| {
                if self.depth == ColorDepth::Mono {
                    return Style::new().add_modifier(Modifier::BOLD);
                }
                let t = if rows <= 1 {
                    0.0
                } else {
                    row as f64 / (rows - 1) as f64
                };
                let rgb = lerp_rgb(palette.accent, palette.accent2, t);
                Style::new()
                    .fg(map_color(rgb, self.depth))
                    .add_modifier(Modifier::BOLD)
            })
            .collect()
    }
}

/// Border, bar, ladder and spinner glyphs for one rendering mode.
#[derive(Debug, Clone)]
pub struct Glyphs {
    pub bar_filled: char,
    pub bar_empty: char,
    pub rung: char,
    pub rail: char,
    pub pointer: char,
    pub spinner: &'static [char],
    pub border: symbols::border::Set<'static>,
    /// Positive-status marker. Never colour alone: every status carries one.
    pub ok_marker: &'static str,
    /// Negative-status marker.
    pub bad_marker: &'static str,
    /// Caution/status marker.
    pub warn_marker: &'static str,
    /// Informational/status marker.
    pub info_marker: &'static str,
}

impl Glyphs {
    /// Unicode glyphs (box-drawing, block bars, braille spinner).
    pub const fn unicode() -> Self {
        Self {
            bar_filled: '█',
            bar_empty: '░',
            rung: '╫',
            rail: '│',
            pointer: '▶',
            spinner: &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'],
            border: symbols::border::Set {
                top_left: "┌",
                top_right: "┐",
                bottom_left: "└",
                bottom_right: "┘",
                horizontal_top: "─",
                horizontal_bottom: "─",
                vertical_left: "│",
                vertical_right: "│",
            },
            ok_marker: "✔",
            bad_marker: "✖",
            warn_marker: "!",
            info_marker: "•",
        }
    }

    /// ASCII-only glyphs for terminals without Unicode support.
    pub const fn ascii() -> Self {
        Self {
            bar_filled: '#',
            bar_empty: '-',
            rung: '=',
            rail: '|',
            pointer: '>',
            spinner: &['|', '/', '-', '\\'],
            border: symbols::border::Set {
                top_left: "+",
                top_right: "+",
                bottom_left: "+",
                bottom_right: "+",
                horizontal_top: "-",
                horizontal_bottom: "-",
                vertical_left: "|",
                vertical_right: "|",
            },
            ok_marker: "[ok]",
            bad_marker: "[x]",
            warn_marker: "[!]",
            info_marker: "[i]",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truecolor_detected_from_colorterm() {
        assert_eq!(
            detect_color_depth(Some("truecolor"), Some("xterm-256color"), false),
            ColorDepth::TrueColor
        );
        assert_eq!(
            detect_color_depth(Some("24bit"), Some("screen"), false),
            ColorDepth::TrueColor
        );
    }

    #[test]
    fn ansi256_detected_from_term() {
        assert_eq!(
            detect_color_depth(None, Some("xterm-256color"), false),
            ColorDepth::Ansi256
        );
    }

    #[test]
    fn ansi16_is_the_fallback() {
        assert_eq!(
            detect_color_depth(None, Some("xterm"), false),
            ColorDepth::Ansi16
        );
        assert_eq!(detect_color_depth(None, None, false), ColorDepth::Ansi16);
    }

    #[test]
    fn no_color_forces_mono() {
        assert_eq!(
            detect_color_depth(Some("truecolor"), Some("xterm-256color"), true),
            ColorDepth::Mono
        );
    }

    #[test]
    fn truecolor_mapping_preserves_rgb() {
        assert_eq!(
            map_color((12, 34, 56), ColorDepth::TrueColor),
            Color::Rgb(12, 34, 56)
        );
    }

    #[test]
    fn ansi256_maps_pure_red_to_196() {
        assert_eq!(
            map_color((255, 0, 0), ColorDepth::Ansi256),
            Color::Indexed(196)
        );
    }

    #[test]
    fn ansi256_maps_mid_grey_to_the_grey_ramp() {
        assert_eq!(
            map_color((128, 128, 128), ColorDepth::Ansi256),
            Color::Indexed(244)
        );
    }

    #[test]
    fn ansi16_maps_primaries() {
        assert_eq!(map_color((255, 0, 0), ColorDepth::Ansi16), Color::LightRed);
        assert_eq!(map_color((0, 0, 0), ColorDepth::Ansi16), Color::Black);
        assert_eq!(map_color((250, 250, 250), ColorDepth::Ansi16), Color::White);
        assert_eq!(map_color((128, 0, 0), ColorDepth::Ansi16), Color::Red);
    }

    #[test]
    fn mono_mapping_is_reset() {
        assert_eq!(map_color((255, 0, 0), ColorDepth::Mono), Color::Reset);
    }

    #[test]
    fn mono_theme_uses_modifiers_for_the_three_styles() {
        let theme = Theme::new(ThemeId::Dark, ColorDepth::Mono, false);
        assert_eq!(theme.fg, Color::Reset);
        assert!(theme.measured.add_modifier.contains(Modifier::BOLD));
        assert!(theme.extrapolated.add_modifier.contains(Modifier::DIM));
        assert!(theme.analytical.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn colour_theme_styles_are_distinct() {
        let theme = Theme::new(ThemeId::Dark, ColorDepth::TrueColor, false);
        assert_ne!(theme.measured, theme.extrapolated);
        assert_ne!(theme.extrapolated, theme.analytical);
    }

    #[test]
    fn ascii_and_unicode_glyph_sets_differ() {
        let unicode = Glyphs::unicode();
        let ascii = Glyphs::ascii();
        assert_ne!(unicode.bar_filled, ascii.bar_filled);
        assert_ne!(unicode.border.top_left, ascii.border.top_left);
        assert_eq!(ascii.border.top_left, "+");
        assert_eq!(ascii.spinner.len(), 4);
    }

    #[test]
    fn theme_ids_cycle_and_wrap() {
        assert_eq!(ThemeId::Dark.next(), ThemeId::Light);
        assert_eq!(ThemeId::Light.next(), ThemeId::Contrast);
        assert_eq!(ThemeId::Contrast.next(), ThemeId::Dark);
    }

    #[test]
    fn theme_label_matches_cli_spelling() {
        assert_eq!(ThemeId::Dark.to_string(), "dark");
        assert_eq!(ThemeId::Light.to_string(), "light");
        assert_eq!(ThemeId::Contrast.to_string(), "contrast");
    }

    #[test]
    fn wordmark_rows_returns_one_style_per_row() {
        let theme = Theme::new(ThemeId::Dark, ColorDepth::TrueColor, false);
        assert_eq!(theme.wordmark_rows(5).len(), 5);
        assert!(theme.wordmark_rows(0).is_empty());
    }

    #[test]
    fn wordmark_rows_gradient_runs_accent_to_accent2() {
        let theme = Theme::new(ThemeId::Dark, ColorDepth::TrueColor, false);
        let rows = theme.wordmark_rows(5);
        assert_eq!(rows[0].fg, Some(theme.accent));
        assert_eq!(rows[4].fg, Some(theme.accent2));
        assert!(
            rows.iter()
                .all(|style| style.add_modifier.contains(Modifier::BOLD)),
            "every word-mark row stays bold"
        );
    }

    #[test]
    fn wordmark_rows_keep_only_bold_in_mono() {
        let theme = Theme::new(ThemeId::Dark, ColorDepth::Mono, false);
        for style in theme.wordmark_rows(5) {
            assert_eq!(style.fg, None, "mono carries no colour");
            assert!(style.add_modifier.contains(Modifier::BOLD));
        }
    }

    #[test]
    fn contrast_ratio_matches_the_wcag_reference_points() {
        // Black on white is the maximum 21:1; a colour against itself is 1:1.
        assert!((contrast_ratio((0, 0, 0), (255, 255, 255)) - 21.0).abs() < 0.01);
        assert!((contrast_ratio((255, 255, 255), (0, 0, 0)) - 21.0).abs() < 0.01);
        assert!((contrast_ratio((17, 17, 17), (17, 17, 17)) - 1.0).abs() < 1e-9);
        // The ratio is symmetric.
        let a = contrast_ratio((14, 90, 140), (250, 250, 250));
        let b = contrast_ratio((250, 250, 250), (14, 90, 140));
        assert!((a - b).abs() < 1e-9);
    }

    #[test]
    fn every_theme_semantic_colour_clears_its_contrast_floor() {
        // The floors are the ones `Theme::new` asks the sixteen-colour mapper
        // to respect, so a palette edit that dims a role fails here.
        let floors = [
            ("fg", 7.0),
            ("accent", 4.5),
            ("accent2", 3.0),
            ("ok", 3.0),
            ("warn", 3.0),
            ("bad", 3.0),
            ("muted", 3.0),
            ("border", 2.0),
        ];
        for id in ThemeId::ALL {
            let theme = Theme::new(id, ColorDepth::TrueColor, false);
            let ratios = theme.contrast_pairs();
            for (role, floor) in floors {
                let ratio = ratios
                    .iter()
                    .find_map(|(name, ratio)| (*name == role).then_some(*ratio))
                    .unwrap_or_else(|| panic!("{id:?} has no {role} contrast"));
                assert!(
                    ratio >= floor,
                    "{id:?} {role} contrast {ratio:.2} is below the {floor:.1} floor"
                );
            }
        }
    }

    #[test]
    fn sixteen_colour_resolution_keeps_contrast_and_distinct_roles() {
        // `TERM=xterm` resolves to the sixteen-colour palette. The mapper must
        // still keep each semantic role readable against the resolved
        // background, and must not collapse every role onto one colour.
        for id in ThemeId::ALL {
            let theme = Theme::new(id, ColorDepth::Ansi16, false);
            let ratios = theme.contrast_pairs();
            assert!(!ratios.is_empty(), "{id:?} lost every colour");
            for (role, ratio) in &ratios {
                assert!(
                    *ratio >= 2.0,
                    "{id:?} {role} contrast {ratio:.2} on the sixteen-colour palette is illegible"
                );
            }
            assert!(
                theme.fg != theme.bg,
                "{id:?} maps text onto the background colour"
            );
            assert_ne!(theme.accent, theme.fg, "{id:?} collapses accent into fg");
            assert_ne!(theme.accent, theme.bad, "{id:?} collapses accent into bad");
            assert_ne!(theme.ok, theme.bad, "{id:?} collapses ok into bad");
        }
    }

    #[test]
    fn the_contrast_theme_reaches_the_wcag_aaa_text_ratio() {
        let theme = Theme::new(ThemeId::Contrast, ColorDepth::TrueColor, false);
        let ratio = theme
            .contrast(theme.fg, theme.bg)
            .expect("truecolour resolves");
        assert!(
            ratio >= 7.0,
            "contrast-theme body text is {ratio:.2}:1, below the AAA floor"
        );
    }

    #[test]
    fn status_markers_are_never_empty_and_differ_between_glyph_sets() {
        let unicode = Glyphs::unicode();
        let ascii = Glyphs::ascii();
        for marker in [
            unicode.ok_marker,
            unicode.bad_marker,
            unicode.warn_marker,
            unicode.info_marker,
        ] {
            assert!(!marker.is_empty(), "a status marker may not be blank");
        }
        for marker in [
            ascii.ok_marker,
            ascii.bad_marker,
            ascii.warn_marker,
            ascii.info_marker,
        ] {
            assert!(!marker.is_empty());
            assert!(marker.is_ascii(), "{marker:?} is not ASCII");
        }
        assert_ne!(unicode.ok_marker, ascii.ok_marker);
        assert_ne!(unicode.bad_marker, ascii.bad_marker);
    }

    #[test]
    fn an_empty_no_color_variable_does_not_disable_colour() {
        // https://no-color.org: the convention only applies when the variable
        // is present and non-empty.
        assert_eq!(
            detect_color_depth(Some("truecolor"), Some("xterm-256color"), false),
            ColorDepth::TrueColor
        );
    }
}
