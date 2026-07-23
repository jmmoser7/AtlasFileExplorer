//! Board color state + the expressive ink tools (keymap wave 2b, cluster A):
//! Brush (B), Eraser (E), Eyedropper (I + spring-loaded Alt from Brush),
//! Sticky note (N), fg/bg color state (D reset / X swap), and the shared
//! `[`/`]` width stepping.
//!
//! See `docs/keymap/specs/brush-color.md`. All stroke creation/removal goes
//! through the journal (Art. VI); color state is tool state (never journaled).

use super::board::{to_rgba, BoardTool, MIN_DRAW};
use super::{board_path, SlateApp};
use eframe::egui::{self, Color32, Pos2, Stroke as EStroke};
use slate_doc::scene::{
    FontChoice, NodeKind, Rgba, ShapeKind, ShapeNode, Stroke, StrokeCap, StrokeJoin, TextAlign,
    TextNode, WidthProfile, WorldRect,
};
use slate_doc::NodeId;
use vector_ink::kurbo::BezPath;

/// Sticky note preset: fixed size (Miro S), soft yellow harmonious with the
/// tag amber, dark ink, default text size (no autosize in P1 — overflow
/// clips, matching the artifact's `overflow:hidden`).
pub const STICKY_SIZE: f32 = 200.0;
pub const STICKY_GAP: f32 = 24.0;
pub const STICKY_FILL: Rgba = Rgba([0xF4, 0xE3, 0x8C, 0xFF]);
pub const STICKY_INK: Rgba = Rgba([0x26, 0x28, 0x2C, 0xFF]);

/// Brush taper preset: slight thinning toward the stroke end (expressive
/// default; the precise Pen family stays uniform).
const BRUSH_TAPER: WidthProfile = WidthProfile::Taper {
    start: 1.0,
    end: 0.7,
};

/// The shared foreground/background color pair consumed by Brush strokes,
/// wires, and the sticky/eyedropper flow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoardColors {
    pub fg: Rgba,
    pub bg: Rgba,
}

impl BoardColors {
    /// Theme-aware defaults: fg = ink, bg = canvas paper.
    pub fn theme_default(dark_mode: bool) -> BoardColors {
        let palette = atlas_shell::theme::Palette::for_mode(dark_mode);
        BoardColors {
            fg: to_rgba(palette.ink),
            bg: to_rgba(palette.bg),
        }
    }

    /// Startup resolution: persisted values win, else theme defaults.
    pub fn from_settings(settings: &super::settings::SlateSettings, dark_mode: bool) -> Self {
        let d = Self::theme_default(dark_mode);
        BoardColors {
            fg: settings.board_fg.map(Rgba).unwrap_or(d.fg),
            bg: settings.board_bg.map(Rgba).unwrap_or(d.bg),
        }
    }
}

/// Photoshop's `[`/`]` width stepping table, in **screen pixels**:
/// `<10 px → ±1 · 10–50 → ±5 · 50–100 → ±10 · >100 → ±25`. Stepping down at
/// a tier boundary uses the lower tier so up/down are inverses
/// (10 → 9, 50 → 45). Result clamps at 1 px.
pub fn step_width_px(px: f32, up: bool) -> f32 {
    fn tier(p: f32) -> f32 {
        if p < 10.0 {
            1.0
        } else if p < 50.0 {
            5.0
        } else if p < 100.0 {
            10.0
        } else {
            25.0
        }
    }
    let px = px.max(0.0);
    if up {
        px + tier(px)
    } else {
        (px - tier((px - 0.01).max(0.0))).max(1.0)
    }
}

impl SlateApp {
    // ---------- color state ----------

    /// `D` — reset fg/bg to the theme defaults.
    pub(crate) fn reset_board_colors(&mut self) {
        self.board_colors = BoardColors::theme_default(self.dark_mode);
        self.save_board_colors();
    }

    /// `X` — swap fg ⇄ bg.
    pub(crate) fn swap_board_colors(&mut self) {
        std::mem::swap(&mut self.board_colors.fg, &mut self.board_colors.bg);
        self.save_board_colors();
    }

    pub(crate) fn save_board_colors(&mut self) {
        self.settings.board_fg = Some(self.board_colors.fg.0);
        self.settings.board_bg = Some(self.board_colors.bg.0);
        self.settings.save();
    }

    /// `[` / `]` — step the active width (eraser while the Eraser tool is
    /// armed, brush otherwise) through the PS tier table in screen px,
    /// converted by the current zoom. Returns (new world width, is_eraser).
    pub(crate) fn step_active_width(&mut self, up: bool) -> (f32, bool) {
        let z = self.tab().cam.z.max(f32::EPSILON);
        let eraser = self.board_tool == BoardTool::Eraser;
        let w = if eraser {
            self.eraser_width
        } else {
            self.brush_width
        };
        let px = step_width_px(w * z, up);
        let new_w = (px / z).clamp(
            super::settings::STROKE_WIDTH_MIN,
            super::settings::STROKE_WIDTH_MAX,
        );
        if eraser {
            self.eraser_width = new_w;
            self.settings.eraser_width = new_w;
        } else {
            self.brush_width = new_w;
            self.settings.brush_width = new_w;
        }
        self.settings.save();
        (new_w, eraser)
    }

    // ---------- brush (B) ----------

    fn brush_stroke(&self) -> Stroke {
        Stroke {
            width: self.brush_width,
            color: self.board_colors.fg,
            dash: slate_doc::scene::Dash::Solid,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            profile: BRUSH_TAPER,
        }
    }

    /// Commit a brush path node (freehand fit or straight chain segment).
    /// One stroke = one journaled Add; the Brush tool stays armed and the
    /// chain end updates for Shift+click straight segments.
    fn commit_brush_bez(&mut self, bez: &BezPath, end: Pos2) {
        let (rect, data) = board_path::bezpath_to_path_data(bez, false);
        if data.is_empty() {
            return;
        }
        let stroke = self.brush_stroke();
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke,
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: Some(data),
            }),
        );
        self.add_nodes(vec![node]);
        self.brush_chain = Some(end);
        self.push_history(
            atlas_commands::CommandId("board.tool.brush"),
            Some("stroke".into()),
        );
    }

    /// Freehand brush release: same fitter as the Pen, expressive defaults.
    pub(crate) fn finish_freehand_brush(&mut self, points: Vec<Pos2>) {
        if points.len() < 2 {
            return;
        }
        let tol = 1.0 / self.tab().cam.z.max(0.05);
        let flat: Vec<[f32; 2]> = points.iter().map(|p| [p.x, p.y]).collect();
        let bez = vector_ink::fit_polyline(&flat, tol);
        let end = *points.last().expect("len >= 2");
        self.commit_brush_bez(&bez, end);
    }

    /// Shift+click while Brush is armed: straight segment from the last
    /// stroke end (PS convention). No-op (chain seed only) without one.
    pub(crate) fn brush_straight_click(&mut self, world: Pos2) {
        match self.brush_chain {
            Some(from) if (world - from).length() > 0.5 => {
                let mut bez = BezPath::new();
                bez.move_to((from.x as f64, from.y as f64));
                bez.line_to((world.x as f64, world.y as f64));
                self.commit_brush_bez(&bez, world);
            }
            _ => self.brush_chain = Some(world),
        }
    }

    // ---------- eraser (E) ----------

    /// Path/Line stroke nodes under the eraser circle at `world`
    /// (pick radius = eraser width / 2). Images, text, frames, and
    /// connectors are never erasable; hidden/locked strokes are skipped.
    pub(crate) fn eraser_hits_at(&self, world: Pos2) -> Vec<NodeId> {
        let zoom = self.tab().cam.z;
        let slop = (self.eraser_width * 0.5).max(1.0);
        let mut hits = Vec::new();
        for n in &self.doc().scene.nodes {
            if n.hidden || n.locked {
                continue;
            }
            let NodeKind::Shape(s) = &n.kind else {
                continue;
            };
            let hit = match s.shape {
                ShapeKind::Path => {
                    let Some(path) = s.path.as_ref() else {
                        continue;
                    };
                    if path.is_empty() {
                        continue;
                    }
                    let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
                    let style = board_path::stroke_style_world(&s.stroke, zoom);
                    vector_ink::hit_stroke(&bez, &style, [world.x, world.y], slop)
                }
                ShapeKind::Line => {
                    let (a, b) = line_endpoints(n.rect, s.flip, n.rotation_deg);
                    dist_point_segment(world, a, b) <= slop + s.stroke.width.max(1.0) * 0.5
                }
                _ => false,
            };
            if hit {
                hits.push(n.id);
            }
        }
        hits
    }

    /// Eraser release: one journal group of Removes.
    pub(crate) fn finish_erase(&mut self, touched: Vec<NodeId>) {
        if touched.is_empty() {
            return;
        }
        let n = touched.len();
        self.delete_board_nodes(&touched);
        self.push_history(
            atlas_commands::CommandId("board.tool.eraser"),
            Some(format!("{n} stroke(s)")),
        );
    }

    // ---------- eyedropper (I / Alt while Brush) ----------

    /// The most salient color of the topmost node under the cursor:
    /// shape/path stroke → fill → text color → sticky fill → frame fill.
    /// Image nodes only yield their border stroke (raster sampling is P2).
    pub(crate) fn eyedropper_sample_at(&self, world: Pos2) -> Option<Rgba> {
        let id = board_path::board_pick_node_ex(
            &self.doc().scene,
            world.x,
            world.y,
            self.tab().cam.z,
            true, // locked nodes paint normally — they sample normally
        )?;
        let node = self.doc().scene.node(id)?;
        match &node.kind {
            NodeKind::Shape(s) => {
                if !s.stroke.is_none() {
                    Some(s.stroke.color)
                } else {
                    s.fill.filter(|f| f.0[3] > 0)
                }
            }
            NodeKind::Text(t) => {
                if t.color.0[3] > 0 {
                    Some(t.color)
                } else {
                    t.fill.filter(|f| f.0[3] > 0)
                }
            }
            NodeKind::Frame(f) => Some(f.fill),
            NodeKind::Image(img) => (!img.stroke.is_none()).then_some(img.stroke.color),
            NodeKind::Connector(c) => Some(c.stroke.color),
        }
    }

    /// Eyedropper click: sample into fg (bg with `to_bg`). Tool state only —
    /// never journaled.
    pub(crate) fn eyedropper_click(&mut self, world: Pos2, to_bg: bool) {
        let Some(c) = self.eyedropper_sample_at(world) else {
            return;
        };
        if to_bg {
            self.board_colors.bg = c;
        } else {
            self.board_colors.fg = c;
        }
        self.save_board_colors();
    }

    /// Whether the eyedropper is live this frame: the tool itself, or
    /// spring-loaded Alt while Brush is armed (Alt-drag duplicate is a
    /// Select-tool gesture and is untouched).
    pub(crate) fn eyedropper_active(&self) -> bool {
        self.board_tool == BoardTool::Eyedropper
            || (self.board_tool == BoardTool::Brush && self.alt_down)
    }

    // ---------- sticky note (N) ----------

    /// Click-to-place sticky: a Text-node preset (fill = sticky yellow),
    /// caret enters immediately (Miro flow). The tool stays armed.
    pub(crate) fn place_sticky_at(&mut self, world: Pos2) {
        let rect = WorldRect::new(
            world.x - STICKY_SIZE * 0.5,
            world.y - STICKY_SIZE * 0.5,
            STICKY_SIZE.max(MIN_DRAW),
            STICKY_SIZE.max(MIN_DRAW),
        );
        let id = self.insert_sticky(rect);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.text_edit = Some((id, String::new()));
        self.push_history(
            atlas_commands::CommandId("board.tool.sticky"),
            Some("placed".into()),
        );
    }

    fn insert_sticky(&mut self, rect: WorldRect) -> NodeId {
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Text(TextNode {
                text: String::new(),
                family: FontChoice::Sans,
                size: 24.0,
                color: STICKY_INK,
                align: TextAlign::Left,
                fill: Some(STICKY_FILL),
            }),
        );
        let id = node.id;
        self.add_nodes(vec![node]);
        id
    }

    /// Tab / Shift+Tab while editing a sticky: spawn a sibling to the right
    /// (left with Shift), same size/fill, 24-unit gap, caret moves there.
    pub(crate) fn spawn_adjacent_sticky(&mut self, from: NodeId, dir: f32) {
        let Some(src) = self.doc().scene.node(from).cloned() else {
            return;
        };
        let NodeKind::Text(t) = &src.kind else {
            return;
        };
        let fill = t.fill;
        let (family, size, color, align) = (t.family, t.size, t.color, t.align);
        let rect = src.rect.translated(dir * (src.rect.w + STICKY_GAP), 0.0);
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Text(TextNode {
                text: String::new(),
                family,
                size,
                color,
                align,
                fill,
            }),
        );
        let id = node.id;
        self.add_nodes(vec![node]);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.text_edit = Some((id, String::new()));
        self.push_history(
            atlas_commands::CommandId("board.tool.sticky"),
            Some(
                if dir >= 0.0 {
                    "tab-spawn right"
                } else {
                    "tab-spawn left"
                }
                .into(),
            ),
        );
    }

    // ---------- cursor feedback ----------

    /// Width circle at the pointer while Brush/Eraser is armed: solid core
    /// at the stroke radius + fainter feather ring (InkMesh contract).
    pub(crate) fn paint_width_cursor(&self, painter: &egui::Painter, pointer: Pos2) {
        let z = self.tab().cam.z;
        let w = if self.board_tool == BoardTool::Eraser {
            self.eraser_width
        } else {
            self.brush_width
        };
        let r = (w * 0.5 * z).max(1.5);
        let ink = if self.board_tool == BoardTool::Eraser {
            Color32::from_gray(160)
        } else {
            super::board::rgba32(self.board_colors.fg)
        };
        painter.circle_stroke(pointer, r, EStroke::new(1.4, ink));
        painter.circle_stroke(
            pointer,
            r + 2.0,
            EStroke::new(1.0, ink.gamma_multiply(0.35)),
        );
    }

    /// Eyedropper sampling ring: outer half = hovered candidate color,
    /// inner = current fg (PS sampling-ring adaptation).
    pub(crate) fn paint_eyedropper_cursor(
        &self,
        painter: &egui::Painter,
        pointer: Pos2,
        world: Pos2,
    ) {
        let candidate = self.eyedropper_sample_at(world);
        let fg = super::board::rgba32(self.board_colors.fg);
        if let Some(c) = candidate {
            painter.circle_stroke(pointer, 11.0, EStroke::new(4.0, super::board::rgba32(c)));
        } else {
            painter.circle_stroke(pointer, 11.0, EStroke::new(1.2, Color32::from_gray(150)));
        }
        painter.circle_filled(pointer, 5.0, fg);
        painter.circle_stroke(
            pointer,
            5.5,
            EStroke::new(1.0, Color32::from_black_alpha(90)),
        );
    }
}

/// World endpoints of a Line shape node (same convention as the painter:
/// `flip` = ↗ diagonal, else ↘), rotated with the node.
fn line_endpoints(rect: WorldRect, flip: bool, rotation_deg: f32) -> (Pos2, Pos2) {
    let (a, b) = if flip {
        (
            Pos2::new(rect.x, rect.y + rect.h),
            Pos2::new(rect.x + rect.w, rect.y),
        )
    } else {
        (
            Pos2::new(rect.x, rect.y),
            Pos2::new(rect.x + rect.w, rect.y + rect.h),
        )
    };
    if rotation_deg.abs() < 0.01 {
        return (a, b);
    }
    let (cx, cy) = rect.center();
    let rot = |p: Pos2| {
        let rad = rotation_deg.to_radians();
        let (sin, cos) = rad.sin_cos();
        let (dx, dy) = (p.x - cx, p.y - cy);
        Pos2::new(cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    };
    (rot(a), rot(b))
}

fn dist_point_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 <= f32::EPSILON {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_stepping_tiers_match_photoshop_table() {
        // <10 px → ±1
        assert_eq!(step_width_px(5.0, true), 6.0);
        assert_eq!(step_width_px(5.0, false), 4.0);
        // 10–50 → ±5
        assert_eq!(step_width_px(20.0, true), 25.0);
        assert_eq!(step_width_px(20.0, false), 15.0);
        // 50–100 → ±10
        assert_eq!(step_width_px(60.0, true), 70.0);
        assert_eq!(step_width_px(60.0, false), 50.0);
        // >100 → ±25
        assert_eq!(step_width_px(120.0, true), 145.0);
        assert_eq!(step_width_px(120.0, false), 95.0);
    }

    #[test]
    fn width_stepping_is_reversible_at_tier_boundaries() {
        // Down from a boundary uses the lower tier so up undoes down.
        assert_eq!(step_width_px(10.0, false), 9.0);
        assert_eq!(step_width_px(9.0, true), 10.0);
        assert_eq!(step_width_px(50.0, false), 45.0);
        assert_eq!(step_width_px(45.0, true), 50.0);
        assert_eq!(step_width_px(100.0, false), 90.0);
        assert_eq!(step_width_px(90.0, true), 100.0);
    }

    #[test]
    fn width_stepping_clamps_at_one_pixel() {
        assert_eq!(step_width_px(1.0, false), 1.0);
        assert_eq!(step_width_px(0.5, false), 1.0);
    }

    #[test]
    fn point_segment_distance() {
        let a = Pos2::new(0.0, 0.0);
        let b = Pos2::new(10.0, 0.0);
        assert!((dist_point_segment(Pos2::new(5.0, 3.0), a, b) - 3.0).abs() < 1e-5);
        assert!((dist_point_segment(Pos2::new(-4.0, 0.0), a, b) - 4.0).abs() < 1e-5);
    }
}
