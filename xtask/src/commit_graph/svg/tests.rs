//! Contract tests for the rendered artwork. They read the artifact the way a
//! consumer does, from its structure and data attributes, so a rendering change
//! cannot quietly break the layout rules.

use super::super::text::{Font, TextBox, Typeface};
use super::super::{CommitGraph, CommitGraphConfig, StyleConfig};
use super::*;
use roxmltree::{Document, Node};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const WIDE: &str = "commit-graph.svg";
const NARROW: &str = "commit-graph-narrow.svg";
const COMPACT: &str = "commit-graph-compact.svg";
const COMPACT_NARROW: &str = "commit-graph-compact-narrow.svg";
const ALL: [&str; 4] = [WIDE, NARROW, COMPACT, COMPACT_NARROW];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the workspace")
        .to_path_buf()
}

fn committed(name: &str) -> String {
    let path = workspace_root().join("docs/media").join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn committed_graph() -> CommitGraph {
    let text = committed("commit-graph.json");
    serde_json::from_str(&text).expect("the committed document is valid")
}

fn parse(text: &str) -> Document<'_> {
    Document::parse(text).expect("the artifact is well-formed XML")
}

fn bbox(node: Node<'_, '_>) -> (f64, f64, f64, f64) {
    let value = node.attribute("data-bbox").expect("a data-bbox");
    let parts: Vec<f64> = value
        .split_whitespace()
        .map(|part| part.parse().expect("a numeric bbox component"))
        .collect();
    assert_eq!(parts.len(), 4, "bbox `{value}` has four components");
    (parts[0], parts[1], parts[2], parts[3])
}

struct TextGroup {
    name: String,
    font: String,
    size: f64,
    fill: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl TextGroup {
    fn right(&self) -> f64 {
        self.x + self.width
    }

    fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

fn text_groups(document: &Document<'_>) -> Vec<TextGroup> {
    document
        .descendants()
        .filter(|node| node.has_tag_name("g") && node.attribute("class") == Some("cg-text-group"))
        .map(|node| {
            let (x, y, width, height) = bbox(node);
            TextGroup {
                name: node.attribute("data-name").expect("a group name").into(),
                font: node.attribute("data-font").expect("a face").into(),
                size: node
                    .attribute("data-size")
                    .expect("a size")
                    .parse()
                    .expect("a numeric size"),
                fill: node.attribute("fill").expect("a fill").into(),
                x,
                y,
                width,
                height,
            }
        })
        .collect()
}

fn text_runs(document: &Document<'_>) -> Vec<(f64, f64, f64, f64)> {
    document
        .descendants()
        .filter(|node| node.has_tag_name("g") && node.attribute("class") == Some("cg-run"))
        .map(bbox)
        .collect()
}

/// The number of text runs inside the named semantic group.
fn runs_in_group(document: &Document<'_>, name: &str) -> usize {
    document
        .descendants()
        .filter(|node| node.attribute("data-name") == Some(name))
        .map(|group| {
            group
                .descendants()
                .filter(|node| node.attribute("class") == Some("cg-run"))
                .count()
        })
        .sum()
}

fn display_width(name: &str) -> f64 {
    match name {
        WIDE | COMPACT => 840.0,
        _ => 390.0,
    }
}

fn nominal_width(name: &str) -> f64 {
    match name {
        WIDE | COMPACT => 840.0,
        _ => 400.0,
    }
}

fn channels(hex: &str) -> (f64, f64, f64) {
    assert_eq!(hex.len(), 7, "`{hex}` is a six-digit hex colour");
    let channel = |index: usize| {
        u8::from_str_radix(&hex[index..index + 2], 16)
            .expect("a hex channel")
            .into()
    };
    (channel(1), channel(3), channel(5))
}

fn linearise(value: f64) -> f64 {
    let value = value / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(hex: &str) -> f64 {
    let (red, green, blue) = channels(hex);
    0.2126 * linearise(red) + 0.7152 * linearise(green) + 0.0722 * linearise(blue)
}

fn contrast(foreground: &str, background: &str) -> f64 {
    let first = luminance(foreground);
    let second = luminance(background);
    let (lighter, darker) = if first >= second {
        (first, second)
    } else {
        (second, first)
    };
    (lighter + 0.05) / (darker + 0.05)
}

fn max_animation_delay(svg: &str) -> u64 {
    let mut maximum = 0;
    let mut rest = svg;
    while let Some(index) = rest.find("animation-delay:") {
        rest = &rest[index + "animation-delay:".len()..];
        let digits: String = rest
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect();
        if let Ok(value) = digits.parse::<u64>() {
            maximum = maximum.max(value);
        }
    }
    maximum
}

fn style() -> StyleConfig {
    StyleConfig::default()
}

#[test]
fn the_four_artifacts_have_the_expected_viewports() {
    let expected = [
        (WIDE, 840.0, 540.0),
        (NARROW, 400.0, 640.0),
        (COMPACT, 840.0, 156.0),
        (COMPACT_NARROW, 400.0, 200.0),
    ];
    for (name, width, height) in expected {
        let svg = committed(name);
        let document = parse(&svg);
        let root = document.root_element();
        assert_eq!(root.attribute("width"), Some(format!("{width}").as_str()));
        assert_eq!(root.attribute("height"), Some(format!("{height}").as_str()));
        assert_eq!(
            root.attribute("viewBox"),
            Some(format!("0 0 {width} {height}").as_str())
        );
        assert_eq!(root.attribute("role"), Some("img"));
        assert!(document
            .descendants()
            .any(|node| node.has_tag_name("title")));
        assert!(document.descendants().any(|node| node.has_tag_name("desc")));
    }
}

#[test]
fn card_group_counts_stay_inside_the_caps() {
    for (name, cap) in [(WIDE, 16usize), (NARROW, 12)] {
        let svg = committed(name);
        let count = text_groups(&parse(&svg)).len();
        assert!(count <= cap, "{name} has {count} text groups, cap {cap}");
    }
}

#[test]
fn strip_group_counts_stay_inside_the_caps() {
    for name in [COMPACT, COMPACT_NARROW] {
        let svg = committed(name);
        let count = text_groups(&parse(&svg)).len();
        assert!(count <= 8, "{name} has {count} text groups, cap 8");
    }
}

#[test]
fn text_sizes_clear_the_readability_floor_at_the_display_width() {
    for name in ALL {
        let svg = committed(name);
        let scale = display_width(name) / nominal_width(name);
        for group in text_groups(&parse(&svg)) {
            let effective = group.size * scale;
            let floor = match group.name.as_str() {
                "title" => 32.0,
                "commit-total" => 52.0,
                "kpi-values" => 28.0,
                _ => 11.0,
            };
            assert!(
                effective + 1e-9 >= floor,
                "{name}/{} is {effective:.2} px at the display width, floor {floor}",
                group.name
            );
        }
    }
}

#[test]
fn no_text_run_overlaps_another_run() {
    for name in ALL {
        let svg = committed(name);
        let runs = text_runs(&parse(&svg));
        for (index, first) in runs.iter().enumerate() {
            for second in runs.iter().skip(index + 1) {
                let overlaps = first.0 < second.0 + second.2
                    && second.0 < first.0 + first.2
                    && first.1 < second.1 + second.3
                    && second.1 < first.1 + first.3;
                assert!(!overlaps, "{name}: run {first:?} overlaps run {second:?}");
            }
        }
        assert!(!runs.is_empty(), "{name} has no text runs");
    }
}

#[test]
fn overlap_detection_is_not_vacuous() {
    let a = TextBox {
        x: 10.0,
        y: 10.0,
        width: 20.0,
        height: 10.0,
    };
    let b = TextBox {
        x: 25.0,
        y: 15.0,
        width: 20.0,
        height: 10.0,
    };
    let c = TextBox {
        x: 40.0,
        y: 10.0,
        width: 20.0,
        height: 10.0,
    };
    assert!(a.intersects(&b), "the predicate must see a real overlap");
    assert!(!a.intersects(&c), "separated boxes are not overlaps");
    assert_eq!(a.gap(&c), 10.0);
}

#[test]
fn text_ink_stays_inside_the_canvas_and_its_margins() {
    for name in ALL {
        let svg = committed(name);
        let document = parse(&svg);
        let root = document.root_element();
        let width: f64 = root
            .attribute("width")
            .expect("width")
            .parse()
            .expect("a number");
        let height: f64 = root
            .attribute("height")
            .expect("height")
            .parse()
            .expect("a number");
        let margin = if name.contains("narrow") { 24.0 } else { 32.0 };
        for (x, y, w, h) in text_runs(&document) {
            assert!(
                x >= margin - 1.5,
                "{name}: text starts at {x}, inside the {margin} px margin"
            );
            assert!(
                x + w <= width - margin + 1.5,
                "{name}: text ends at {}, past the right margin",
                x + w
            );
            assert!(y >= 0.0, "{name}: text starts above the canvas");
            assert!(y + h <= height, "{name}: text ends below the canvas");
        }
    }
}

#[test]
fn text_group_boxes_stay_inside_the_canvas() {
    for name in ALL {
        let svg = committed(name);
        let document = parse(&svg);
        let root = document.root_element();
        let width: f64 = root
            .attribute("width")
            .expect("a width")
            .parse()
            .expect("a number");
        let height: f64 = root
            .attribute("height")
            .expect("a height")
            .parse()
            .expect("a number");
        for group in text_groups(&document) {
            assert!(
                group.x >= 0.0 && group.right() <= width,
                "{name}/{} spans {}..{} in a {width} px canvas",
                group.name,
                group.x,
                group.right()
            );
            assert!(
                group.y >= 0.0 && group.bottom() <= height,
                "{name}/{} spans {}..{} in a {height} px canvas",
                group.name,
                group.y,
                group.bottom()
            );
        }
    }
}

#[test]
fn the_palette_clears_the_contrast_floor() {
    let background = "#0E1217";
    for (label, colour, floor) in [
        ("ink", "#E6EDF3", 12.0),
        ("muted", "#8B98A5", 4.5),
        ("teal text", "#7FB5B3", 4.5),
        ("teal", "#5C8E8D", 4.5),
    ] {
        let ratio = contrast(colour, background);
        assert!(
            ratio + 1e-9 >= floor,
            "{label} {colour} is {ratio:.2}:1 on {background}, floor {floor}:1"
        );
    }
}

#[test]
fn orange_never_paints_a_glyph() {
    let allowed: BTreeSet<&str> = ["#E6EDF3", "#8B98A5", "#7FB5B3"].into_iter().collect();
    for name in ALL {
        let svg = committed(name);
        let document = parse(&svg);
        for group in text_groups(&document) {
            assert!(
                allowed.contains(group.fill.as_str()),
                "{name}/{} is painted {}; orange is a data ink, never a text ink",
                group.name,
                group.fill
            );
        }
        for node in document.descendants() {
            if node.has_tag_name("g") && node.attribute("class") == Some("cg-run") {
                let fill = node
                    .children()
                    .find(|child| child.has_tag_name("g"))
                    .and_then(|child| child.attribute("fill"))
                    .expect("a run fill");
                assert!(
                    allowed.contains(fill),
                    "{name}: a run is painted {fill}, not a text ink"
                );
            }
        }
    }
}

#[test]
fn text_uses_at_most_two_faces() {
    for name in ALL {
        let svg = committed(name);
        let faces: BTreeSet<String> = text_groups(&parse(&svg))
            .into_iter()
            .map(|group| group.font)
            .collect();
        assert!(
            faces.len() <= 2,
            "{name} uses {} faces: {faces:?}",
            faces.len()
        );
        for face in &faces {
            assert!(
                face == "mono" || face == "display",
                "{name} names an unknown face {face}"
            );
        }
    }
}

#[test]
fn the_artwork_carries_no_filters_and_at_most_two_gradients() {
    for name in ALL {
        let svg = committed(name);
        assert_eq!(svg.matches("<filter").count(), 0, "{name} carries a filter");
        assert!(
            svg.matches("<linearGradient").count() <= 2,
            "{name} carries more than two gradients"
        );
        assert_eq!(svg.matches("<radialGradient").count(), 0);
    }
}

#[test]
fn the_gradient_fills_an_area_and_never_a_glyph_or_stroke() {
    for name in [WIDE, NARROW] {
        let svg = committed(name);
        let document = parse(&svg);
        for node in document.descendants() {
            if let Some(fill) = node.attribute("fill") {
                if fill.contains("url(") {
                    assert!(
                        node.has_tag_name("path"),
                        "{name}: a {} carries the gradient fill",
                        node.tag_name().name()
                    );
                    let in_text = node
                        .ancestors()
                        .any(|ancestor| ancestor.attribute("class") == Some("cg-text-group"));
                    assert!(!in_text, "{name}: the gradient reaches a text group");
                }
            }
            if let Some(stroke) = node.attribute("stroke") {
                assert!(
                    !stroke.contains("url("),
                    "{name}: a gradient paints a stroke"
                );
            }
        }
        assert!(
            svg.contains("fill=\"url(#cg-fill)\""),
            "{name} has no area fill"
        );
    }
}

#[test]
fn the_wide_staircase_keeps_a_seventy_thirty_split() {
    for name in [WIDE, NARROW] {
        let svg = committed(name);
        let document = parse(&svg);
        let plot = document
            .descendants()
            .find_map(|node| node.attribute("data-plot"))
            .expect("a plot rectangle");
        let values: Vec<f64> = plot
            .split_whitespace()
            .take(4)
            .map(|value| value.parse().expect("a numeric plot component"))
            .collect();
        let (left, top, data_right, bottom) = (values[0], values[1], values[2], values[3]);
        let right = if name == WIDE { 808.0 } else { 376.0 };
        let ratio = (data_right - left) / (right - left);
        if name == WIDE {
            assert!(
                (ratio - 0.70).abs() <= 0.01,
                "{name}: the data region is {ratio:.3} of the chart, not 70/30"
            );
            assert!(left >= 68.0, "{name}: the chart starts at {left}");
        } else {
            // The narrow card has no runway: the staircase uses the whole width.
            assert!(
                (ratio - 1.0).abs() <= 1e-9,
                "{name}: the data region is {ratio:.3} of the chart, not the full width"
            );
            assert!(left >= 24.0, "{name}: the chart starts at {left}");
        }
        assert!(data_right <= right, "{name}: data ends past the chart");
        for (label, value) in [
            ("left", left),
            ("top", top),
            ("data right", data_right),
            ("bottom", bottom),
        ] {
            assert!(
                (value / 8.0).fract().abs() < 1e-9,
                "{name}: plot {label} {value} is off the 8 px grid"
            );
        }
        assert!(
            top >= 200.0 && bottom <= 372.0,
            "{name}: plot y range {top}..{bottom}"
        );
    }
}

#[test]
fn the_day_bars_match_the_document() {
    let graph = committed_graph();
    for (name, baseline) in [(WIDE, 464.0), (NARROW, 512.0)] {
        let svg = committed(name);
        let document = parse(&svg);
        let plot = document
            .descendants()
            .find_map(|node| node.attribute("data-plot"))
            .expect("a plot rectangle");
        let bounds: Vec<f64> = plot
            .split_whitespace()
            .take(3)
            .map(|value| value.parse().expect("a numeric plot component"))
            .collect();
        let (plot_left, plot_right) = (bounds[0], bounds[2]);
        let bars: Vec<(String, u64, f64, f64, f64, f64)> = document
            .descendants()
            .filter(|node| node.has_tag_name("rect") && node.attribute("data-date").is_some())
            .map(|node| {
                (
                    node.attribute("data-date").expect("a date").to_string(),
                    node.attribute("data-commits")
                        .expect("a count")
                        .parse()
                        .expect("a number"),
                    node.attribute("x").expect("x").parse().expect("a number"),
                    node.attribute("y").expect("y").parse().expect("a number"),
                    node.attribute("width")
                        .expect("w")
                        .parse()
                        .expect("a number"),
                    node.attribute("height")
                        .expect("h")
                        .parse()
                        .expect("a number"),
                )
            })
            .collect();
        let active: Vec<_> = graph.per_day.iter().filter(|day| day.commits > 0).collect();
        assert_eq!(bars.len(), active.len(), "{name}: one bar per active day");
        for (date, commits, x, y, width, height) in bars {
            let day = graph
                .per_day
                .iter()
                .find(|day| day.date == date)
                .unwrap_or_else(|| panic!("{name}: unknown bar date {date}"));
            assert_eq!(commits, day.commits, "{name}: {date} commit count");
            assert!(
                x >= plot_left && x + width <= plot_right,
                "{name}: bar {date} is off the plot"
            );
            assert!(
                (y + height - baseline).abs() < 1e-6,
                "{name}: bar {date} does not sit on the {baseline} baseline"
            );
            assert!(height >= 3.0, "{name}: bar {date} is invisible");
        }
    }
}

#[test]
fn the_milestones_match_the_document() {
    let graph = committed_graph();
    for name in [WIDE, NARROW] {
        let svg = committed(name);
        let document = parse(&svg);
        let markers: Vec<(String, String)> = document
            .descendants()
            .filter(|node| node.has_tag_name("circle") && node.attribute("data-date").is_some())
            .map(|node| {
                (
                    node.attribute("data-date").expect("a date").to_string(),
                    node.attribute("data-status").expect("a status").to_string(),
                )
            })
            .collect();
        assert_eq!(
            markers.len(),
            graph.milestones.len(),
            "{name}: one marker per milestone"
        );
        for milestone in &graph.milestones {
            let found = markers
                .iter()
                .find(|(date, _)| date == &milestone.date)
                .unwrap_or_else(|| panic!("{name}: no marker for {}", milestone.date));
            assert_eq!(
                found.1,
                milestone.status.as_str(),
                "{name}: status {}",
                milestone.date
            );
        }
    }
}

#[test]
fn the_kpi_block_has_rules_not_boxes() {
    let svg = committed(WIDE);
    let document = parse(&svg);
    let rules = document
        .descendants()
        .filter(|node| {
            node.has_tag_name("line")
                && node.attribute("y1").is_some_and(|value| {
                    value
                        .parse::<f64>()
                        .is_ok_and(|y| (100.0..200.0).contains(&y))
                })
        })
        .count();
    assert_eq!(rules, 3, "the wide KPI block draws three column rules");
    let boxes = document
        .descendants()
        .filter(|node| node.has_tag_name("rect"))
        .filter(|node| {
            let y = node
                .attribute("y")
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(f64::NAN);
            (100.0..200.0).contains(&y)
        })
        .count();
    assert_eq!(boxes, 0, "the KPI block must not draw boxes");

    let narrow_text = committed(NARROW);
    let narrow = parse(&narrow_text);
    let boxes = narrow
        .descendants()
        .filter(|node| node.has_tag_name("rect"))
        .filter(|node| {
            let y = node
                .attribute("y")
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(f64::NAN);
            (100.0..240.0).contains(&y)
        })
        .count();
    assert_eq!(boxes, 0, "the narrow KPI block must not draw boxes");
}

#[test]
fn the_desc_repeats_the_document_totals() {
    let graph = committed_graph();
    let expected = [
        ("commits", graph.totals.commits.to_string()),
        ("refresh_commits", graph.totals.refresh_commits.to_string()),
        ("active_days", graph.totals.active_days.to_string()),
        ("longest_streak", graph.totals.longest_streak.to_string()),
        ("current_streak", graph.totals.current_streak.to_string()),
        ("lines_added", graph.totals.lines_added.to_string()),
        ("lines_removed", graph.totals.lines_removed.to_string()),
        ("files_touched", graph.totals.files_touched.to_string()),
        (
            "as_of",
            graph.as_of_sha[..7.min(graph.as_of_sha.len())].to_string(),
        ),
        ("date", graph.as_of_date.clone()),
        ("authors", graph.per_author.len().to_string()),
        ("first_commit", graph.totals.first_commit_date.clone()),
        ("last_commit", graph.totals.last_commit_date.clone()),
        ("busiest_day", graph.totals.busiest_day.date.clone()),
        (
            "busiest_commits",
            graph.totals.busiest_day.commits.to_string(),
        ),
        (
            "streak_start",
            graph.totals.longest_streak_range.start.clone(),
        ),
        ("streak_end", graph.totals.longest_streak_range.end.clone()),
    ];
    for name in ALL {
        let svg = committed(name);
        let document = parse(&svg);
        let desc = document
            .descendants()
            .find(|node| node.has_tag_name("desc"))
            .and_then(|node| node.text())
            .expect("a desc");
        let tokens: BTreeMap<&str, &str> = desc
            .split_whitespace()
            .filter_map(|token| token.split_once('='))
            .collect();
        for (key, value) in &expected {
            assert_eq!(
                tokens.get(*key),
                Some(&value.as_str()),
                "{name}: desc {key}"
            );
        }
        for milestone in &graph.milestones {
            let needle = format!(
                "{} {} {}",
                milestone.date,
                milestone.label,
                milestone.status.as_str()
            );
            assert!(
                desc.contains(&needle),
                "{name}: desc omits milestone `{needle}`"
            );
        }
        for author in &graph.per_author {
            let needle = format!("{}={}", author.display, author.commits);
            assert!(desc.contains(&needle), "{name}: desc omits `{needle}`");
        }
    }
}

#[test]
fn the_committed_files_stay_inside_their_budgets() {
    let (card_budget, strip_budget) = (
        super::super::CARD_BUDGET_BYTES,
        super::super::STRIP_BUDGET_BYTES,
    );
    assert_eq!(card_budget, 60 * 1024);
    assert_eq!(strip_budget, 25 * 1024);
    for name in ALL {
        let bytes = committed(name).len();
        let budget = if name.contains("compact") {
            strip_budget
        } else {
            card_budget
        };
        assert!(bytes <= budget, "{name} is {bytes} B, budget {budget} B");
    }
}

#[test]
fn every_glyph_reference_is_defined_and_available_in_its_face() {
    let font = Font::load().expect("the embedded fonts");
    for name in ALL {
        let svg = committed(name);
        let document = parse(&svg);
        let definitions: BTreeSet<String> = document
            .descendants()
            .filter(|node| node.has_tag_name("path"))
            .filter_map(|node| node.attribute("id").map(str::to_string))
            .collect();
        let mut references = BTreeSet::new();
        for node in document
            .descendants()
            .filter(|node| node.has_tag_name("use"))
        {
            let href = node.attribute("href").expect("a glyph href");
            let id = href.strip_prefix('#').expect("an internal glyph reference");
            assert!(
                definitions.contains(id),
                "{name}: {id} is used but never defined"
            );
            references.insert(id.to_string());
            let (face, digits) = if let Some(digits) = id.strip_prefix('m') {
                (Typeface::Mono, digits)
            } else if let Some(digits) = id.strip_prefix('d') {
                (Typeface::Display, digits)
            } else {
                panic!("{name}: {id} has no face prefix");
            };
            let code = digits.parse::<u32>().expect("a codepoint");
            let character = char::from_u32(code).expect("a valid codepoint");
            assert!(
                font.has_outline(face, character),
                "{name}: {} cannot draw {character:?}",
                face.id()
            );
        }
        assert_eq!(
            definitions, references,
            "{name}: every definition is referenced and every reference is defined"
        );
    }
}

#[test]
fn motion_draws_the_line_and_staggers_the_bars_under_the_cap() {
    for name in [WIDE, NARROW] {
        let svg = committed(name);
        assert!(
            svg.contains(".cg-stair{animation:cg-draw 1.6s"),
            "{name}: stair motion"
        );
        assert!(
            svg.contains("prefers-reduced-motion"),
            "{name}: reduced motion"
        );
        assert!(
            svg.contains("animation:none"),
            "{name}: reduced motion disables it"
        );
        assert_eq!(
            svg.matches("infinite").count(),
            1,
            "{name}: only the now ring may loop"
        );
        assert!(svg.contains(".cg-now-ring"), "{name}: now ring");
        let delay = max_animation_delay(&svg);
        assert!(delay <= 800, "{name}: bar stagger reaches {delay} ms");
        assert!(
            svg.contains("class=\"cg-stair"),
            "{name}: the staircase is animated"
        );
    }
}

#[test]
fn the_final_state_is_complete_without_animation() {
    for name in ALL {
        let svg = committed(name);
        assert!(
            !svg.contains("<animate"),
            "{name}: carries declarative animation"
        );
        assert!(!svg.contains("<set"), "{name}: carries a declarative set");
        if name == WIDE || name == NARROW {
            assert!(
                svg.contains("stroke-dashoffset=\"0\""),
                "{name}: the staircase does not start complete"
            );
            let document = parse(&svg);
            for node in document.descendants() {
                if node
                    .ancestors()
                    .any(|ancestor| ancestor.has_tag_name("defs"))
                {
                    continue;
                }
                for attribute in ["opacity", "fill-opacity", "stroke-opacity"] {
                    if let Some(value) = node.attribute(attribute) {
                        let opacity: f64 = value.parse().expect("a numeric opacity");
                        assert!(
                            opacity > 0.0,
                            "{name}: <{}> is invisible at {attribute}=\"0\"",
                            node.tag_name().name()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_refresh_note_appears_only_when_refreshes_exist() {
    let graph = committed_graph();
    let palette = Palette::default();
    assert_eq!(
        graph.totals.refresh_commits, 0,
        "the committed history has no refreshes"
    );
    let quiet = render_full(&graph, &palette, &style()).expect("a card");
    assert_eq!(
        runs_in_group(&parse(&quiet), "footer-source"),
        1,
        "zero refreshes prints nothing"
    );

    let mut refreshed = graph.clone();
    refreshed.totals.refresh_commits = 3;
    for (label, svg) in [
        (
            "full",
            render_full(&refreshed, &palette, &style()).expect("a card"),
        ),
        (
            "narrow",
            render_narrow(&refreshed, &palette, &style()).expect("a card"),
        ),
    ] {
        assert_eq!(
            runs_in_group(&parse(&svg), "footer-source"),
            2,
            "{label}: the refresh note is missing"
        );
        let desc = parse(&svg)
            .descendants()
            .find(|node| node.has_tag_name("desc"))
            .and_then(|node| node.text())
            .expect("a desc")
            .to_string();
        assert!(
            desc.contains("refresh_commits=3"),
            "{label}: the refresh count is missing from the description"
        );
    }
}

#[test]
fn the_style_knobs_change_the_artwork() {
    let graph = committed_graph();
    let palette = Palette::default();
    let base = render_full(&graph, &palette, &style()).expect("a card");

    let mut no_flags = style();
    no_flags.show_flags = false;
    let without_flags = render_full(&graph, &palette, &no_flags).expect("a card");
    assert!(!without_flags.contains("dm-milestone"));
    assert_ne!(base, without_flags);

    let mut no_bars = style();
    no_bars.show_bars = false;
    let without_bars = render_full(&graph, &palette, &no_bars).expect("a card");
    assert!(!without_bars.contains("dm dm-bar cg-bar"));
    assert_ne!(base, without_bars);

    let mut mono = style();
    mono.display_font = "JetBrains Mono".to_string();
    let mono_card = render_full(&graph, &palette, &mono).expect("a card");
    let document = parse(&mono_card);
    let title = text_groups(&document)
        .into_iter()
        .find(|group| group.name == "title")
        .expect("a title");
    assert_eq!(title.font, "mono");

    let mut accent = style();
    accent.accent = Some("#123456".to_string());
    let accented = Palette::load(&workspace_root(), &accent).expect("a palette");
    let accented_card = render_full(&graph, &accented, &accent).expect("a card");
    assert!(accented_card.contains("#123456"));
    assert!(!text_groups(&parse(&accented_card))
        .iter()
        .any(|group| group.fill == "#123456"));

    let mut retitled = style();
    retitled.title = "Another Climb".to_string();
    let retitled_card = render_full(&graph, &palette, &retitled).expect("a card");
    assert_ne!(base, retitled_card);
}

#[test]
fn narrow_is_a_distinct_layout() {
    let graph = committed_graph();
    let wide = text_groups(&parse(&committed(WIDE)));
    let narrow = text_groups(&parse(&committed(NARROW)));
    let wide_names: BTreeSet<&str> = wide.iter().map(|group| group.name.as_str()).collect();
    let narrow_names: BTreeSet<&str> = narrow.iter().map(|group| group.name.as_str()).collect();
    assert!(wide_names.contains("milestone-1"));
    assert!(!wide_names.contains("milestones"));
    assert!(narrow_names.contains("milestones"));
    assert!(narrow_names.contains("milestone-numbers"));
    assert!(!narrow_names.contains("milestone-1"));

    let wide_values = wide
        .iter()
        .find(|group| group.name == "kpi-values")
        .expect("wide KPI")
        .size;
    let narrow_values = narrow
        .iter()
        .find(|group| group.name == "kpi-values")
        .expect("narrow KPI")
        .size;
    assert_eq!(wide_values, 40.0);
    assert_eq!(narrow_values, 32.0);
    assert_eq!(graph.milestones.len(), 4);
}

#[test]
fn the_copy_carries_no_emoji_or_shouting() {
    for name in ALL {
        let svg = committed(name);
        let document = parse(&svg);
        let text = document
            .descendants()
            .find(|node| node.has_tag_name("desc"))
            .and_then(|node| node.text())
            .expect("a desc");
        assert!(
            text.is_ascii(),
            "{name}: desc carries a non-ASCII character"
        );
        assert!(!text.contains('!'), "{name}: desc shouts");
        let title = document
            .descendants()
            .find(|node| node.has_tag_name("title"))
            .and_then(|node| node.text())
            .expect("a title");
        assert!(
            title.is_ascii(),
            "{name}: title carries a non-ASCII character"
        );
        for node in document
            .descendants()
            .filter(|node| node.has_tag_name("use"))
        {
            let href = node.attribute("href").expect("a glyph href");
            let digits = href.trim_start_matches('#').trim_start_matches(['m', 'd']);
            let code = digits.parse::<u32>().expect("a codepoint");
            assert!(
                !(0x1F000..=0x1FAFF).contains(&code) && !(0x2600..=0x27BF).contains(&code),
                "{name}: an emoji glyph ({code:#x}) reached the artwork"
            );
        }
    }
}

#[test]
fn structural_geometry_sits_on_the_eight_pixel_grid() {
    let wide = Layout::wide();
    assert_eq!(wide.margin % 8.0, 0.0);
    let kpi = wide.kpi.as_ref().expect("a KPI block");
    assert_eq!(kpi.origin_x % 8.0, 0.0);
    assert_eq!((kpi.col_width + 8.0) % 8.0, 0.0);
    let chart = wide.chart.as_ref().expect("a chart");
    assert_eq!(chart.left % 8.0, 0.0);
    assert_eq!(chart.top % 8.0, 0.0);
    assert_eq!(chart.data_right % 8.0, 0.0);
    assert_eq!(chart.bottom % 8.0, 0.0);
    assert_eq!(wide.bars.as_ref().expect("bars").baseline % 8.0, 0.0);
    assert_eq!(wide.footer.as_ref().expect("a footer").baseline % 8.0, 0.0);

    let narrow = Layout::narrow();
    assert_eq!(narrow.margin % 8.0, 0.0);
    let chart = narrow.chart.as_ref().expect("a chart");
    assert_eq!(chart.left % 8.0, 0.0);
    assert_eq!(chart.top % 8.0, 0.0);
    assert_eq!(chart.data_right % 8.0, 0.0);
    assert_eq!(chart.bottom % 8.0, 0.0);
    assert_eq!(narrow.bars.as_ref().expect("bars").baseline % 8.0, 0.0);
}

#[test]
fn no_external_reference_is_embedded() {
    for name in ALL {
        let svg = committed(name);
        let document = parse(&svg);
        for tag in ["script", "image", "foreignObject", "text"] {
            assert!(
                !document.descendants().any(|node| node.has_tag_name(tag)),
                "{name} carries a <{tag}> element"
            );
        }
        assert!(!svg.contains("xlink:href"), "{name}: carries an xlink");
        assert!(!svg.contains("@import"), "{name}: imports a stylesheet");
        let remaining = svg.replace("http://www.w3.org/2000/svg", "");
        assert!(
            !remaining.contains("http"),
            "{name}: carries an external URL"
        );
    }
}

#[test]
fn the_axis_rounds_up_and_covers_every_commit() {
    assert_eq!(nice_max(74), 80);
    assert_eq!(nice_max(100), 100);
    assert_eq!(nice_max(1), 1);
    assert_eq!(nice_max(0), 1);
    assert_eq!(nice_max(5), 6);
    assert_eq!(nice_max(1234), 1500);
    for value in 0..500 {
        assert!(nice_max(value) >= value, "value {value}");
        assert!(nice_max(value) <= (value * 2).max(2), "value {value}");
    }
}

#[test]
fn thousands_separator_groups_from_the_right() {
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(1000), "1,000");
    assert_eq!(thousands(1234567), "1,234,567");
}

#[test]
fn a_missing_brand_file_falls_back_to_the_sampled_palette() {
    let palette = Palette::load(Path::new("/definitely/not/a/repo"), &style()).expect("a palette");
    assert_eq!(palette.orange, "#C55E33");
    assert_eq!(palette.teal, "#5C8E8D");
}

#[test]
fn the_config_defaults_keep_staleness_bounded() {
    let config: CommitGraphConfig =
        toml::from_str("timezone = \"Asia/Kolkata\"\nbudget_kb = 50\n").expect("a config");
    assert_eq!(config.staleness_commits, 20);
    assert_eq!(config.staleness_days, 7);
    assert_eq!(
        config.source_url,
        "https://github.com/aiori-hackathon/A3-PS036-TC529.git"
    );
    assert_eq!(config.source_ref, "main");
    assert_eq!(config.source_cache, "target/commit-graph-source.git");
    assert_eq!(
        config.repo_web_url,
        "https://github.com/aiori-hackathon/A3-PS036-TC529"
    );
}
