//! Ladder geometry and drawing for the Ladder tab (phases 24.1, 24.2).
//!
//! The MTL ladder for `N` leaves is the sequence of maximal perfect binary
//! subtrees ("rungs") that tiles `0..N`; each rung is drawn as its own small
//! tree, roots at different heights, so the decomposition is visible at a
//! glance. The selected leaf's authentication path is *inside its own rung* —
//! which is the whole point of MTL: the expensive signature is paid once for
//! the ladder, and each message carries only its short in-rung sibling path.
//!
//! All geometry ([`build`], [`project`], [`leaf_x`]) is pure and unit-tested;
//! rendering only plots what the geometry computed. No result number is
//! hardcoded here — leaf counts and the selected leaf come from the model in
//! `climb-analytical` and from the caller's key handling.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use climb_analytical::rungs;

use crate::theme::{ColorDepth, Glyphs, Theme};

/// Smallest leaf count the Ladder tab can show.
pub const MIN_LEAVES: u64 = 1;
/// Largest leaf count the Ladder tab can show.
pub const MAX_LEAVES: u64 = 32;

/// Normalised vertical coordinate of the leaf row (bottom of the tree).
const LEAF_Y: f64 = 0.30;
/// Normalised vertical coordinate of the rung marker bars.
const RUNG_BAR_Y: f64 = 0.08;

/// A node in the ladder tree geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Height above the leaves (`0` is a leaf).
    pub level: u32,
    /// The leaf index for a leaf node; `None` for an internal node.
    pub leaf: Option<u64>,
    /// Normalised horizontal coordinate in `[0, 1]`, left to right.
    pub x: f64,
    /// Normalised vertical coordinate in `[0, 1]`, bottom to top.
    pub y: f64,
}

/// An edge between two nodes, by index into [`Layout::nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// The parent (higher-level) node.
    pub parent: usize,
    /// The child (lower-level) node.
    pub child: usize,
}

/// The visual role of a node; drives its glyph and style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
    /// A leaf that is neither selected nor a path sibling.
    Leaf,
    /// An internal node off the selected leaf's path.
    Internal,
    /// The currently selected leaf.
    Selected,
    /// A sibling hash on the selected leaf's authentication path.
    Sibling,
    /// An internal node on the selected leaf's authentication path.
    OnPath,
}

/// The visual role of an edge; drives its style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeRole {
    /// An edge inside a rung but off the selected path.
    Rung,
    /// An edge on the selected leaf's authentication path.
    Path,
    /// Any other edge.
    Plain,
}

/// The complete pure geometry of one ladder rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Number of leaves (messages) in the batch.
    pub leaf_count: u64,
    /// The selected leaf index.
    pub selected: u64,
    /// Height of the tallest rung (levels above the leaves).
    pub max_level: u32,
    /// Every node, in build order.
    pub nodes: Vec<Node>,
    /// Every edge, in build order.
    pub edges: Vec<Edge>,
    /// Node roles, parallel to [`Layout::nodes`].
    pub node_roles: Vec<NodeRole>,
    /// Edge roles, parallel to [`Layout::edges`].
    pub edge_roles: Vec<EdgeRole>,
    /// Inclusive leaf span of every rung, left to right.
    pub rung_spans: Vec<(u64, u64)>,
    /// Inclusive leaf span of the rung containing the selected leaf.
    pub selected_rung: (u64, u64),
    /// Node indices on the selected leaf's authentication path (leaf → rung root).
    pub auth_path: Vec<usize>,
    /// Node indices that are path siblings.
    pub siblings: Vec<usize>,
}

/// Normalised horizontal coordinate of leaf `index` in a ladder of `leaf_count`.
///
/// Each leaf sits at the centre of its equal slice, so `N` leaves span the full
/// width without touching the edges.
#[must_use]
pub fn leaf_x(index: u64, leaf_count: u64) -> f64 {
    if leaf_count == 0 {
        return 0.5;
    }
    (index as f64 + 0.5) / leaf_count as f64
}

/// Normalised vertical coordinate of a node at `level` in a tree of `max_level`.
fn level_y(level: u32, max_level: u32) -> f64 {
    if max_level == 0 {
        return LEAF_Y + 0.45;
    }
    LEAF_Y + (1.0 - LEAF_Y) * (f64::from(level) / f64::from(max_level))
}

/// Build the pure geometry for `leaf_count` leaves with `selected` highlighted.
///
/// `leaf_count` is clamped to at least one leaf and `selected` to the valid
/// leaf range, so the function is total.
#[must_use]
pub fn build(leaf_count: u64, selected: u64) -> Layout {
    let n = leaf_count.max(1);
    let selected = selected.min(n - 1);
    let spans = rungs(n);
    let max_level = spans
        .iter()
        .map(|r| r.size().trailing_zeros())
        .max()
        .unwrap_or(0);

    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut node_roles: Vec<NodeRole> = Vec::new();
    let mut edge_roles: Vec<EdgeRole> = Vec::new();
    let mut auth_path: Vec<usize> = Vec::new();
    let mut siblings: Vec<usize> = Vec::new();
    let mut rung_spans: Vec<(u64, u64)> = Vec::new();
    let mut selected_rung: (u64, u64) = (0, 0);

    for rung in &spans {
        rung_spans.push((rung.left, rung.right));
        let size = rung.size();
        let k = size.trailing_zeros();
        let mut levels: Vec<Vec<usize>> = Vec::with_capacity(k as usize + 1);

        let mut leaf_level = Vec::with_capacity(size as usize);
        for j in 0..size {
            let index = nodes.len();
            nodes.push(Node {
                level: 0,
                leaf: Some(rung.left + j),
                x: leaf_x(rung.left + j, n),
                y: level_y(0, max_level),
            });
            node_roles.push(NodeRole::Leaf);
            leaf_level.push(index);
        }
        levels.push(leaf_level);

        for t in 1..=k {
            let below = levels[(t - 1) as usize].clone();
            let mut level_nodes = Vec::with_capacity((size >> t) as usize);
            for p in 0..(size >> t) {
                let c0 = below[(2 * p) as usize];
                let c1 = below[(2 * p + 1) as usize];
                let index = nodes.len();
                let x = (nodes[c0].x + nodes[c1].x) / 2.0;
                nodes.push(Node {
                    level: t,
                    leaf: None,
                    x,
                    y: level_y(t, max_level),
                });
                node_roles.push(NodeRole::Internal);
                edges.push(Edge {
                    parent: index,
                    child: c0,
                });
                edge_roles.push(EdgeRole::Rung);
                edges.push(Edge {
                    parent: index,
                    child: c1,
                });
                edge_roles.push(EdgeRole::Rung);
                level_nodes.push(index);
            }
            levels.push(level_nodes);
        }

        if rung.contains(selected) {
            selected_rung = (rung.left, rung.right);
            let li = selected - rung.left;
            for t in 0..=k {
                let pos = (li >> t) as usize;
                let index = levels[t as usize][pos];
                auth_path.push(index);
                if t == 0 {
                    node_roles[index] = NodeRole::Selected;
                } else {
                    node_roles[index] = NodeRole::OnPath;
                    let sib_pos = ((li >> (t - 1)) ^ 1) as usize;
                    let sib = levels[(t - 1) as usize][sib_pos];
                    node_roles[sib] = NodeRole::Sibling;
                    siblings.push(sib);
                }
            }
            for t in 1..=k {
                let parent = levels[t as usize][(li >> t) as usize];
                let child = levels[(t - 1) as usize][(li >> (t - 1)) as usize];
                if let Some(edge_index) = edges
                    .iter()
                    .position(|edge| edge.parent == parent && edge.child == child)
                {
                    edge_roles[edge_index] = EdgeRole::Path;
                }
            }
        }
    }

    Layout {
        leaf_count: n,
        selected,
        max_level,
        nodes,
        edges,
        node_roles,
        edge_roles,
        rung_spans,
        selected_rung,
        auth_path,
        siblings,
    }
}

/// Map a normalised layout coordinate to a cell inside `area`.
///
/// `x` runs left to right and `y` bottom to top, both in `[0, 1]` and clamped.
/// A zero-sized area collapses to its origin. Pure; the whole drawing pipeline
/// uses this one projection so the canvas and the tests cannot disagree.
#[must_use]
pub fn project(x: f64, y: f64, area: Rect) -> (u16, u16) {
    if area.width == 0 || area.height == 0 {
        return (area.x, area.y);
    }
    let x = x.clamp(0.0, 1.0);
    let y = y.clamp(0.0, 1.0);
    let col = area.x + (x * f64::from(area.width - 1)).round() as u16;
    let row = area.y + ((1.0 - y) * f64::from(area.height - 1)).round() as u16;
    (col, row)
}

/// Draw the ladder tree (Unicode glyphs) inside a bordered block.
pub struct LadderView<'a> {
    /// The geometry to draw.
    pub layout: &'a Layout,
    /// The active theme.
    pub theme: &'a Theme,
    /// The active glyph set.
    pub glyphs: &'a Glyphs,
}

impl LadderView<'_> {
    fn node_glyph(&self, role: NodeRole, is_leaf: bool) -> (char, Style) {
        let theme = self.theme;
        match role {
            NodeRole::Selected => (
                '●',
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            ),
            NodeRole::Sibling => (
                '#',
                Style::new().fg(theme.warn).add_modifier(Modifier::BOLD),
            ),
            NodeRole::OnPath => ('*', Style::new().fg(theme.accent)),
            NodeRole::Leaf if is_leaf => ('•', Style::new().fg(theme.fg)),
            _ => ('○', Style::new().fg(theme.fg)),
        }
    }

    fn render_grid(&self, inner: Rect, buf: &mut Buffer) {
        let bar_row = inner.y + inner.height.saturating_sub(1);
        let tree = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.saturating_sub(1),
        };
        if tree.width == 0 || tree.height == 0 {
            return;
        }

        for (edge, role) in self.layout.edges.iter().zip(self.layout.edge_roles.iter()) {
            let from = project(
                self.layout.nodes[edge.child].x,
                self.layout.nodes[edge.child].y,
                tree,
            );
            let to = project(
                self.layout.nodes[edge.parent].x,
                self.layout.nodes[edge.parent].y,
                tree,
            );
            let style = match role {
                EdgeRole::Path => Style::new().fg(self.theme.accent),
                EdgeRole::Rung => Style::new().fg(self.theme.accent2),
                EdgeRole::Plain => Style::new().fg(self.theme.muted),
            };
            draw_line(buf, from, to, style, '·');
        }

        for (left, right) in &self.layout.rung_spans {
            let (c0, _) = project(leaf_x(*left, self.layout.leaf_count), RUNG_BAR_Y, tree);
            let (c1, _) = project(leaf_x(*right, self.layout.leaf_count), RUNG_BAR_Y, tree);
            let (lo, hi) = if c0 <= c1 { (c0, c1) } else { (c1, c0) };
            let style = Style::new().fg(if self.layout.selected_rung == (*left, *right) {
                self.theme.accent
            } else {
                self.theme.accent2
            });
            for col in lo..=hi {
                set_cell(buf, col, bar_row, '=', style);
            }
        }

        for (index, node) in self.layout.nodes.iter().enumerate() {
            let (col, row) = project(node.x, node.y, tree);
            let (glyph, style) =
                self.node_glyph(self.layout.node_roles[index], node.leaf.is_some());
            set_cell(buf, col, row, glyph, style);
        }
    }

    fn render_text(&self, inner: Rect, buf: &mut Buffer) {
        let ascii = self.theme.ascii;
        let sep = if ascii { "-" } else { "·" };
        let layout = self.layout;
        let mut lines = vec![Line::from(Span::styled(
            format!(
                " N={} {sep} {} rungs {sep} {} levels {sep} leaf {} selected",
                layout.leaf_count,
                layout.rung_spans.len(),
                layout.max_level,
                layout.selected
            ),
            Style::new().fg(self.theme.muted),
        ))];

        for (left, right) in &layout.rung_spans {
            let size = right - left + 1;
            let depth = size.trailing_zeros();
            let is_selected = layout.selected_rung == (*left, *right);
            let glyph = if is_selected {
                self.glyphs.bar_filled
            } else {
                self.glyphs.bar_empty
            };
            let bar: String = std::iter::repeat_n(glyph, size.min(24) as usize).collect();
            let pointer = if is_selected {
                self.glyphs.pointer.to_string()
            } else {
                " ".to_string()
            };
            let mark = if is_selected {
                format!("  {pointer} selected leaf {}", layout.selected)
            } else {
                String::new()
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" [{left}..{right}] "),
                    Style::new().fg(if is_selected {
                        self.theme.accent
                    } else {
                        self.theme.accent2
                    }),
                ),
                Span::styled(
                    bar,
                    Style::new().fg(if is_selected {
                        self.theme.accent
                    } else {
                        self.theme.accent2
                    }),
                ),
                Span::styled(
                    format!("  depth {depth}{mark}"),
                    Style::new().fg(self.theme.muted),
                ),
            ]));
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

impl Widget for LadderView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered()
            .border_set(self.glyphs.border)
            .border_style(Style::new().fg(self.theme.border))
            .title(Span::styled(
                " Ladder geometry ",
                Style::new()
                    .fg(self.theme.accent2)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.width < 4 || inner.height < 3 {
            return;
        }
        if self.theme.ascii || self.theme.depth == ColorDepth::Mono {
            self.render_text(inner, buf);
        } else {
            self.render_grid(inner, buf);
        }
    }
}

/// Plot a straight line of `glyph` between two cells (Bresenham's algorithm).
fn draw_line(buf: &mut Buffer, from: (u16, u16), to: (u16, u16), style: Style, glyph: char) {
    let (x0, y0) = (i32::from(from.0), i32::from(from.1));
    let (x1, y1) = (i32::from(to.0), i32::from(to.1));
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        if (0..=i32::from(u16::MAX)).contains(&x) && (0..=i32::from(u16::MAX)).contains(&y) {
            set_cell(buf, x as u16, y as u16, glyph, style);
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Write one glyph at a cell, ignoring out-of-bounds positions.
fn set_cell(buf: &mut Buffer, col: u16, row: u16, glyph: char, style: Style) {
    if let Some(cell) = buf.cell_mut((col, row)) {
        cell.set_char(glyph);
        cell.set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::theme::{ColorDepth, ThemeId};

    fn render_text(
        layout: &Layout,
        width: u16,
        height: u16,
        theme_id: ThemeId,
        depth: ColorDepth,
        ascii: bool,
    ) -> String {
        let theme = Theme::new(theme_id, depth, ascii);
        let glyphs = theme.glyphs();
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("in-memory terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    LadderView {
                        layout,
                        theme: &theme,
                        glyphs: &glyphs,
                    },
                    frame.area(),
                );
            })
            .expect("rendering never fails on TestBackend");
        let buffer = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                if let Some(cell) = buffer.cell((x, y)) {
                    out.push_str(cell.symbol());
                }
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn project_maps_the_unit_square_to_cell_corners() {
        let area = Rect::new(0, 0, 11, 6);
        assert_eq!(project(0.0, 0.0, area), (0, 5));
        assert_eq!(project(1.0, 1.0, area), (10, 0));
        assert_eq!(project(0.5, 0.5, area), (5, 3));
        let offset = Rect::new(4, 3, 11, 6);
        assert_eq!(project(0.0, 0.0, offset), (4, 8));
        assert_eq!(project(1.0, 1.0, offset), (14, 3));
    }

    #[test]
    fn project_clamps_and_survives_a_zero_area() {
        let area = Rect::new(2, 7, 4, 4);
        assert_eq!(project(-3.0, 9.0, area), (2, 7));
        assert_eq!(project(9.0, -3.0, area), (5, 10));
        let empty = Rect::new(2, 7, 0, 0);
        assert_eq!(project(0.5, 0.5, empty), (2, 7));
    }

    #[test]
    fn layout_has_one_leaf_node_per_message() {
        for n in 1..=32u64 {
            let layout = build(n, 0);
            assert_eq!(layout.leaf_count, n);
            assert_eq!(
                layout
                    .nodes
                    .iter()
                    .filter(|node| node.leaf.is_some())
                    .count(),
                n as usize
            );
            assert_eq!(layout.nodes.len(), 2 * n as usize - layout.rung_spans.len());
        }
    }

    #[test]
    fn layout_edges_reference_existing_nodes() {
        for n in 1..=32u64 {
            let layout = build(n, n - 1);
            for edge in &layout.edges {
                assert!(edge.parent < layout.nodes.len());
                assert!(edge.child < layout.nodes.len());
                assert!(
                    layout.nodes[edge.parent].level > layout.nodes[edge.child].level,
                    "parent must sit above its child for N={n}"
                );
            }
        }
    }

    #[test]
    fn layout_rungs_match_the_closed_form_model() {
        let layout = build(13, 5);
        assert_eq!(layout.rung_spans, vec![(0, 7), (8, 11), (12, 12)]);
        assert_eq!(layout.selected_rung, (0, 7));
        assert_eq!(layout.max_level, 3);
        let layout = build(8, 0);
        assert_eq!(layout.rung_spans, vec![(0, 7)]);
    }

    #[test]
    fn auth_path_length_matches_the_model_for_every_leaf() {
        for n in 1..=32u64 {
            for leaf in 0..n {
                let layout = build(n, leaf);
                let expected = climb_analytical::auth_path_len(n, leaf)
                    .expect("leaf is in range by construction");
                assert_eq!(
                    layout.auth_path.len() - 1,
                    expected,
                    "path steps for N={n} leaf={leaf}"
                );
                assert_eq!(layout.siblings.len(), expected);
                assert_eq!(
                    layout.node_roles[layout.auth_path[0]],
                    NodeRole::Selected,
                    "the first path node is the selected leaf for N={n} leaf={leaf}"
                );
                for &index in &layout.auth_path[1..] {
                    assert_eq!(layout.node_roles[index], NodeRole::OnPath);
                }
                for &index in &layout.siblings {
                    assert_eq!(layout.node_roles[index], NodeRole::Sibling);
                }
            }
        }
    }

    #[test]
    fn every_auth_path_node_lies_inside_the_selected_rung() {
        let layout = build(13, 10);
        assert_eq!(layout.selected_rung, (8, 11));
        for &index in &layout.auth_path {
            let leaf = layout.nodes[index].leaf;
            if let Some(leaf) = leaf {
                assert!((8..=11).contains(&leaf), "path leaf {leaf} left the rung");
            }
        }
    }

    #[test]
    fn renders_a_grid_with_path_and_rung_markers() {
        let layout = build(13, 5);
        let out = render_text(&layout, 80, 24, ThemeId::Dark, ColorDepth::TrueColor, false);
        assert!(out.contains("Ladder geometry"), "title:\n{out}");
        assert!(out.contains('●'), "selected-leaf glyph:\n{out}");
        assert!(out.contains('#'), "sibling glyph:\n{out}");
        assert!(out.contains('·'), "edge glyph:\n{out}");
        assert!(out.contains('='), "rung bars:\n{out}");
    }

    #[test]
    fn renders_the_text_tree_when_ascii() {
        let layout = build(13, 5);
        let out = render_text(&layout, 80, 24, ThemeId::Dark, ColorDepth::TrueColor, true);
        assert!(out.contains("rungs"), "text-tree header:\n{out}");
        assert!(out.contains("[0..7]"), "rung span:\n{out}");
        assert!(out.contains("selected leaf 5"), "marker:\n{out}");
        assert!(out.contains('>'), "ascii pointer:\n{out}");
        assert!(
            !out.contains('●'),
            "ascii must not use unicode nodes:\n{out}"
        );
    }

    #[test]
    fn renders_the_text_tree_in_mono_colour() {
        let layout = build(8, 3);
        let out = render_text(&layout, 80, 24, ThemeId::Dark, ColorDepth::Mono, false);
        assert!(out.contains("[0..7]"), "mono text tree:\n{out}");
    }

    #[test]
    fn single_leaf_ladder_renders() {
        let layout = build(1, 0);
        assert_eq!(layout.max_level, 0);
        assert_eq!(layout.auth_path, vec![0]);
        assert!(layout.siblings.is_empty());
        let out = render_text(&layout, 80, 24, ThemeId::Dark, ColorDepth::TrueColor, false);
        assert!(out.contains('●'), "single selected leaf:\n{out}");
    }

    #[test]
    fn thirty_two_leaves_render_without_panicking() {
        let layout = build(MAX_LEAVES, 31);
        let out = render_text(&layout, 80, 24, ThemeId::Dark, ColorDepth::TrueColor, false);
        assert!(out.contains("Ladder geometry"), "L={MAX_LEAVES}:\n{out}");
        let tall = render_text(
            &layout,
            120,
            40,
            ThemeId::Dark,
            ColorDepth::TrueColor,
            false,
        );
        assert!(tall.contains('●'));
    }

    #[test]
    fn zero_leaf_count_is_clamped_to_one() {
        let layout = build(0, 0);
        assert_eq!(layout.leaf_count, 1);
        assert_eq!(layout.selected, 0);
    }
}
