//! The browser backend rasterises glyphs on demand, but a glyph the configured
//! fonts do not carry would still draw as the fallback space. This test renders
//! every tab at the minimum desktop size and fails with the exact code points
//! that fall outside the documented coverage set.
//!
//! The set is deliberately a set of Unicode blocks, not the literal glyphs of
//! one layout: a new bar, border or status marker in any of these blocks is
//! expected to render, while an emoji or a private-use character is not.

use std::collections::{BTreeMap, BTreeSet};

use climb_tui::theme::ColorDepth;
use climb_tui::{runtime, ui, App, Cli, Glyphs, Tab};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

const WIDTH: u16 = 120;
const HEIGHT: u16 = 40;

/// Unicode blocks the dynamic atlas is expected to cover: printable ASCII,
/// Latin-1 punctuation, Greek (the sigma in summary labels), general
/// punctuation, arrows, math operators, box drawing, block elements,
/// geometric shapes, the symbol/dingbat blocks (check and cross markers) and
/// the braille spinner block.
const ATLAS_RANGES: &[(u32, u32, &str)] = &[
    (0x0020, 0x007E, "printable ASCII"),
    (0x00A0, 0x00FF, "Latin-1"),
    (0x0370, 0x03FF, "Greek"),
    (0x2000, 0x206F, "general punctuation"),
    (0x2190, 0x21FF, "arrows"),
    (0x2200, 0x22FF, "math operators"),
    (0x2500, 0x257F, "box drawing"),
    (0x2580, 0x259F, "block elements"),
    (0x25A0, 0x25FF, "geometric shapes"),
    (0x2600, 0x27BF, "misc symbols and dingbats"),
    (0x2800, 0x28FF, "braille"),
];

fn in_atlas(character: char) -> bool {
    let point = character as u32;
    ATLAS_RANGES
        .iter()
        .any(|&(start, end, _)| (start..=end).contains(&point))
}

fn embedded_app() -> App {
    let mut app = App::from_cli(&Cli::viewer(), ColorDepth::TrueColor);
    // The same loader the browser uses before the fetch settles.
    app.apply_reload(runtime::load_all_embedded());
    app.set_status(None);
    app
}

/// Every code point drawn by any tab, with the tabs that drew it.
fn glyphs_outside_atlas() -> BTreeMap<char, BTreeSet<&'static str>> {
    let mut outside: BTreeMap<char, BTreeSet<&'static str>> = BTreeMap::new();
    let mut app = embedded_app();
    // Render every spinner frame, not just the one a single tick happens to
    // show, so the whole braille set is exercised.
    let spinner_frames = Glyphs::unicode().spinner.len();
    for tab in Tab::ALL {
        app.tab = tab;
        let backend = TestBackend::new(WIDTH, HEIGHT);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        for tick in 0..spinner_frames {
            app.tick = tick;
            terminal
                .draw(|frame| ui::render(frame, &app))
                .expect("rendering never fails on the in-memory backend");
            let buffer = terminal.backend().buffer();
            for y in 0..buffer.area.height {
                for x in 0..buffer.area.width {
                    let Some(cell) = buffer.cell((x, y)) else {
                        continue;
                    };
                    for character in cell.symbol().chars() {
                        if !in_atlas(character) {
                            outside.entry(character).or_default().insert(tab.title());
                        }
                    }
                }
            }
        }
    }
    outside
}

#[test]
fn every_glyph_the_dashboard_draws_is_in_the_atlas_set() {
    let outside = glyphs_outside_atlas();
    let report = outside
        .iter()
        .map(|(character, tabs)| {
            let tabs: Vec<&str> = tabs.iter().copied().collect();
            format!(
                "U+{:04X} {:?} drawn on {}",
                u32::from(*character),
                character,
                tabs.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        outside.is_empty(),
        "{} glyph(s) outside the atlas set at {WIDTH}x{HEIGHT}:\n{report}",
        outside.len()
    );
}

#[test]
fn the_atlas_set_rejects_glyphs_the_fonts_are_not_known_to_carry() {
    assert!(in_atlas('⠋'), "a braille spinner frame is covered");
    assert!(in_atlas('╫'), "a box-drawing glyph is covered");
    assert!(!in_atlas('😀'), "an emoji is outside the set");
    assert!(
        !in_atlas('\u{E000}'),
        "a private-use character is outside the set"
    );
}

#[test]
fn the_atlas_set_covers_every_theme_glyph_and_spinner_frame() {
    let glyphs = Glyphs::unicode();
    let singles = [
        glyphs.bar_filled,
        glyphs.bar_empty,
        glyphs.rung,
        glyphs.rail,
        glyphs.pointer,
    ];
    for character in singles {
        assert!(
            in_atlas(character),
            "U+{:04X} {character:?} must be in the atlas set",
            u32::from(character)
        );
    }
    for character in glyphs.spinner {
        assert!(
            in_atlas(*character),
            "spinner frame U+{:04X} {character:?} must be in the atlas set",
            u32::from(*character)
        );
    }
    for marker in [
        glyphs.ok_marker,
        glyphs.bad_marker,
        glyphs.warn_marker,
        glyphs.info_marker,
    ] {
        for character in marker.chars() {
            assert!(
                in_atlas(character),
                "status marker U+{:04X} {character:?} must be in the atlas set",
                u32::from(character)
            );
        }
    }
}
