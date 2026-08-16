//! Grasshopper-style align widget: two icon clusters on a multi-selection
//! (left + bottom — the other two sides duplicate the same actions). The
//! group bounding-box outline is the selection indicator — this module does
//! not draw a second frame, and hover does not ghost the result.
//!
//! Syntax (target rects) is renderer-free in this file's math helpers.
//! Painting and hit-testing are screen-space chrome (Art. II — no tessellation
//! cache; the widget is eight fixed squares). Mutations go through
//! [`SlateApp::patch_nodes`] (Art. VI). See `docs/keymap/contracts/align-widget.md`.

use super::board::{BoardTool, BoardXf};
use super::board_snap;
use super::SlateApp;
use atlas_commands::CommandId;
use atlas_shell::theme::Palette;
use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, Stroke as EStroke, StrokeKind, Vec2};
use slate_doc::scene::{NodeKind, WorldRect};
use slate_doc::NodeId;

/// Screen-px size of one icon button.
pub const ICON_PX: f32 = 14.0;
/// Gap between the four icons in a cluster.
pub const ICON_GAP_PX: f32 = 2.0;
/// How far outside the group box the icon clusters sit.
/// Must clear the edge-handle hit (`HANDLE_PX` + pad) so a click never
/// starts a resize.
pub const FRAME_OUTSET_PX: f32 = 20.0;
/// Extra hit slop around each icon (screen px).
pub const HIT_PAD_PX: f32 = 2.0;
/// Disabled (distribute with fewer than 3) icon alpha.
pub const DISABLED_ALPHA: f32 = 0.32;
pub const MIN_ALIGN: usize = 2;
pub const MIN_DISTRIBUTE: usize = 3;

/// Align selected board objects relative to their shared bounding box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardAlign {
    Left,
    CenterH,
    Right,
    Top,
    CenterV,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistributeAxis {
    Horizontal,
    Vertical,
}

/// One clickable action on the widget. Align needs 2+; distribute needs 3+.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignAction {
    Align(BoardAlign),
    Distribute(DistributeAxis),
}

impl AlignAction {
    pub fn command_id(self) -> CommandId {
        CommandId(match self {
            AlignAction::Align(BoardAlign::Left) => "board.align.left",
            AlignAction::Align(BoardAlign::CenterH) => "board.align.center_h",
            AlignAction::Align(BoardAlign::Right) => "board.align.right",
            AlignAction::Align(BoardAlign::Top) => "board.align.top",
            AlignAction::Align(BoardAlign::CenterV) => "board.align.middle_v",
            AlignAction::Align(BoardAlign::Bottom) => "board.align.bottom",
            AlignAction::Distribute(DistributeAxis::Horizontal) => "board.distribute.horizontal",
            AlignAction::Distribute(DistributeAxis::Vertical) => "board.distribute.vertical",
        })
    }

    pub fn from_command_id(id: &str) -> Option<Self> {
        Some(match id {
            "board.align.left" => AlignAction::Align(BoardAlign::Left),
            "board.align.center_h" => AlignAction::Align(BoardAlign::CenterH),
            "board.align.right" => AlignAction::Align(BoardAlign::Right),
            "board.align.top" => AlignAction::Align(BoardAlign::Top),
            "board.align.middle_v" => AlignAction::Align(BoardAlign::CenterV),
            "board.align.bottom" => AlignAction::Align(BoardAlign::Bottom),
            "board.distribute.horizontal" => AlignAction::Distribute(DistributeAxis::Horizontal),
            "board.distribute.vertical" => AlignAction::Distribute(DistributeAxis::Vertical),
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            AlignAction::Align(BoardAlign::Left) => "Align left",
            AlignAction::Align(BoardAlign::CenterH) => "Align center",
            AlignAction::Align(BoardAlign::Right) => "Align right",
            AlignAction::Align(BoardAlign::Top) => "Align top",
            AlignAction::Align(BoardAlign::CenterV) => "Align middle",
            AlignAction::Align(BoardAlign::Bottom) => "Align bottom",
            AlignAction::Distribute(DistributeAxis::Horizontal) => "Distribute horizontally",
            AlignAction::Distribute(DistributeAxis::Vertical) => "Distribute vertically",
        }
    }

    fn needs_three(self) -> bool {
        matches!(self, AlignAction::Distribute(_))
    }
}

#[derive(Clone, Copy)]
struct AlignHit {
    action: AlignAction,
    rect: Rect,
    enabled: bool,
}

/// Screen-space layout of the 8 icon buttons (left + bottom, outset).
struct AlignLayout {
    frame: Rect,
    hits: [AlignHit; 8],
}

/// Move `rect` so the chosen edge/center coincides with `bounds`.
pub fn aligned_rect(rect: WorldRect, bounds: WorldRect, align: BoardAlign) -> WorldRect {
    let mut r = rect;
    match align {
        BoardAlign::Left => r.x = bounds.x,
        BoardAlign::CenterH => r.x = bounds.x + (bounds.w - r.w) * 0.5,
        BoardAlign::Right => r.x = bounds.x + bounds.w - r.w,
        BoardAlign::Top => r.y = bounds.y,
        BoardAlign::CenterV => r.y = bounds.y + (bounds.h - r.h) * 0.5,
        BoardAlign::Bottom => r.y = bounds.y + bounds.h - r.h,
    }
    r
}

/// Even gaps between first and last along `axis`. First and last keep their
/// leading edge; middles are rewritten. Fewer than 3 items: identity.
pub fn distributed_rects(
    rects: &[(NodeId, WorldRect)],
    axis: DistributeAxis,
) -> Vec<(NodeId, WorldRect)> {
    if rects.len() < MIN_DISTRIBUTE {
        return rects.to_vec();
    }
    let mut ordered = rects.to_vec();
    ordered.sort_by(|a, b| match axis {
        DistributeAxis::Horizontal => {
            a.1.x
                .partial_cmp(&b.1.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
        DistributeAxis::Vertical => {
            a.1.y
                .partial_cmp(&b.1.y)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    });
    let first = ordered[0].1;
    let last = ordered[ordered.len() - 1].1;
    match axis {
        DistributeAxis::Horizontal => {
            let widths: f32 = ordered.iter().map(|(_, r)| r.w).sum();
            let span = (last.x + last.w) - first.x;
            let gap = (span - widths) / (ordered.len() as f32 - 1.0);
            let mut x = first.x;
            for (_, r) in &mut ordered {
                r.x = x;
                x += r.w + gap;
            }
        }
        DistributeAxis::Vertical => {
            let heights: f32 = ordered.iter().map(|(_, r)| r.h).sum();
            let span = (last.y + last.h) - first.y;
            let gap = (span - heights) / (ordered.len() as f32 - 1.0);
            let mut y = first.y;
            for (_, r) in &mut ordered {
                r.y = y;
                y += r.h + gap;
            }
        }
    }
    ordered
}

/// Target rects for a commit. `locked` ids keep their current rect even
/// when the action would move them.
pub fn preview_rects(
    rects: &[(NodeId, WorldRect)],
    locked: &[NodeId],
    action: AlignAction,
) -> Vec<(NodeId, WorldRect)> {
    let Some(bounds) = board_snap::union_rect(&rects.iter().map(|(_, r)| *r).collect::<Vec<_>>())
    else {
        return rects.to_vec();
    };
    let targets = match action {
        AlignAction::Align(align) => rects
            .iter()
            .map(|(id, r)| (*id, aligned_rect(*r, bounds, align)))
            .collect(),
        AlignAction::Distribute(axis) => distributed_rects(rects, axis),
    };
    targets
        .into_iter()
        .map(|(id, r)| {
            if locked.contains(&id) {
                let current = rects
                    .iter()
                    .find(|(i, _)| *i == id)
                    .map(|(_, r)| *r)
                    .unwrap_or(r);
                (id, current)
            } else {
                (id, r)
            }
        })
        .collect()
}

fn cluster_len() -> f32 {
    4.0 * ICON_PX + 3.0 * ICON_GAP_PX
}

fn icon_center(cluster_origin: Pos2, along: Vec2, index: usize) -> Pos2 {
    cluster_origin + along * (index as f32 * (ICON_PX + ICON_GAP_PX) + ICON_PX * 0.5)
}

fn layout_for(screen_bbox: Rect, alignable: usize) -> AlignLayout {
    let frame = screen_bbox.expand(FRAME_OUTSET_PX);
    let dist_on = alignable >= MIN_DISTRIBUTE;
    let h_actions = [
        AlignAction::Align(BoardAlign::Left),
        AlignAction::Align(BoardAlign::CenterH),
        AlignAction::Align(BoardAlign::Right),
        AlignAction::Distribute(DistributeAxis::Horizontal),
    ];
    let v_actions = [
        AlignAction::Align(BoardAlign::Top),
        AlignAction::Align(BoardAlign::CenterV),
        AlignAction::Align(BoardAlign::Bottom),
        AlignAction::Distribute(DistributeAxis::Vertical),
    ];
    let half = cluster_len() * 0.5;
    let mut hits = [AlignHit {
        action: h_actions[0],
        rect: Rect::NOTHING,
        enabled: false,
    }; 8];
    let mut i = 0;
    // Bottom: horizontal cluster, left → right.
    {
        let origin = Pos2::new(frame.center().x - half, frame.bottom());
        let along = Vec2::new(1.0, 0.0);
        for (k, action) in h_actions.into_iter().enumerate() {
            let c = icon_center(origin, along, k);
            hits[i] = AlignHit {
                action,
                rect: Rect::from_center_size(c, Vec2::splat(ICON_PX)),
                enabled: !action.needs_three() || dist_on,
            };
            i += 1;
        }
    }
    // Left: vertical cluster, top → bottom.
    {
        let origin = Pos2::new(frame.left(), frame.center().y - half);
        let along = Vec2::new(0.0, 1.0);
        for (k, action) in v_actions.into_iter().enumerate() {
            let c = icon_center(origin, along, k);
            hits[i] = AlignHit {
                action,
                rect: Rect::from_center_size(c, Vec2::splat(ICON_PX)),
                enabled: !action.needs_three() || dist_on,
            };
            i += 1;
        }
    }
    debug_assert_eq!(i, 8);
    AlignLayout { frame, hits }
}

fn hit_test(screen: Pos2, layout: &AlignLayout) -> Option<AlignAction> {
    layout
        .hits
        .iter()
        .find_map(|h| (h.enabled && h.rect.expand(HIT_PAD_PX).contains(screen)).then_some(h.action))
}

impl SlateApp {
    fn alignable_selection(&self) -> Vec<(NodeId, WorldRect, bool)> {
        let mut out = Vec::new();
        for id in &self.board_sel {
            let Some(n) = self.doc().scene.node(*id) else {
                continue;
            };
            if n.hidden || matches!(n.kind, NodeKind::Connector(_)) {
                continue;
            }
            out.push((*id, n.rect, n.locked));
        }
        out
    }

    fn align_widget_active(&self) -> bool {
        self.board_tool == BoardTool::Select
            && self.board_drag.is_none()
            && self.board_crop.is_none()
            && self.presenting.is_none()
            && self.alignable_selection().len() >= MIN_ALIGN
    }

    fn align_layout(&self, xf: &BoardXf) -> Option<AlignLayout> {
        if !self.align_widget_active() {
            return None;
        }
        let members = self.alignable_selection();
        let bounds = self.board_group_bounds()?;
        Some(layout_for(xf.rect_w2s(bounds), members.len()))
    }

    /// Screen hit on an enabled icon. `None` when the widget is hidden.
    pub(crate) fn align_action_at(&self, screen: Pos2) -> Option<AlignAction> {
        let xf = self.board_xf();
        let layout = self.align_layout(&xf)?;
        hit_test(screen, &layout)
    }

    /// Hover cursor + stored action for icon invert / label. Call before
    /// transform chrome so the pointing hand wins over resize arrows.
    pub(crate) fn hover_align_widget(
        &mut self,
        pointer: Option<Pos2>,
        ctx: &egui::Context,
    ) -> bool {
        self.board_align_hover = None;
        if !self.align_widget_active() {
            return false;
        }
        let Some(p) = pointer else {
            return false;
        };
        let Some(action) = self.align_action_at(p) else {
            return false;
        };
        self.board_align_hover = Some(action);
        ctx.set_cursor_icon(CursorIcon::PointingHand);
        true
    }

    /// Press on an icon commits immediately (Grasshopper) and swallows the
    /// rest of the press so it cannot start a move or clear the selection.
    pub(crate) fn try_align_press(&mut self, ctx: &egui::Context, screen: Pos2) -> bool {
        let Some(action) = self.align_action_at(screen) else {
            return false;
        };
        self.board_align_eat_press = true;
        self.dispatch(ctx, action.command_id(), Some("align widget".into()));
        true
    }

    pub(crate) fn apply_align_action(&mut self, action: AlignAction) -> bool {
        match action {
            AlignAction::Align(align) => self.align_board_selection(align),
            AlignAction::Distribute(axis) => self.distribute_board_selection(axis),
        }
    }

    /// Journaled align. Returns whether any node moved.
    pub(crate) fn align_board_selection(&mut self, align: BoardAlign) -> bool {
        self.commit_align_action(AlignAction::Align(align))
    }

    /// Journaled distribute. Returns whether any node moved.
    pub(crate) fn distribute_board_selection(&mut self, axis: DistributeAxis) -> bool {
        self.commit_align_action(AlignAction::Distribute(axis))
    }

    fn commit_align_action(&mut self, action: AlignAction) -> bool {
        let members = self.alignable_selection();
        if members.len() < MIN_ALIGN {
            return false;
        }
        if action.needs_three() && members.len() < MIN_DISTRIBUTE {
            return false;
        }
        let rects: Vec<(NodeId, WorldRect)> = members.iter().map(|(id, r, _)| (*id, *r)).collect();
        let locked: Vec<NodeId> = members
            .iter()
            .filter(|(_, _, locked)| *locked)
            .map(|(id, _, _)| *id)
            .collect();
        let targets = preview_rects(&rects, &locked, action);
        let ids: Vec<NodeId> = targets.iter().map(|(id, _)| *id).collect();
        let before: Vec<WorldRect> = rects.iter().map(|(_, r)| *r).collect();
        self.patch_nodes(&ids, |n| {
            if let Some((_, r)) = targets.iter().find(|(id, _)| *id == n.id) {
                n.rect = *r;
            }
        });
        ids.iter()
            .zip(before.iter())
            .any(|(id, b)| self.doc().scene.node(*id).is_some_and(|n| n.rect != *b))
    }

    pub(crate) fn paint_align_widget(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        palette: &Palette,
        select: Color32,
    ) {
        let Some(layout) = self.align_layout(xf) else {
            return;
        };
        let hover = self.board_align_hover;

        for hit in &layout.hits {
            let hovered = hover == Some(hit.action) && hit.enabled;
            paint_icon_button(
                painter,
                hit.rect,
                hit.action,
                hit.enabled,
                hovered,
                palette,
                select,
            );
            if hovered {
                let (anchor, pos) = label_anchor(layout.frame, hit.rect);
                painter.text(
                    pos,
                    anchor,
                    hit.action.label(),
                    egui::FontId::proportional(10.0),
                    palette.ink.gamma_multiply(0.85),
                );
            }
        }
    }
}

fn paint_icon_button(
    painter: &egui::Painter,
    rect: Rect,
    action: AlignAction,
    enabled: bool,
    hovered: bool,
    palette: &Palette,
    select: Color32,
) {
    let alpha = if enabled { 1.0 } else { DISABLED_ALPHA };
    let fill = if hovered {
        select.gamma_multiply(alpha)
    } else {
        palette.window.gamma_multiply(0.92 * alpha)
    };
    let stroke = if hovered {
        select.gamma_multiply(alpha)
    } else {
        select.gamma_multiply(0.55 * alpha)
    };
    let glyph = if hovered {
        palette.window.gamma_multiply(alpha)
    } else {
        select.gamma_multiply(0.95 * alpha)
    };
    painter.rect_filled(rect, 2.0, fill);
    painter.rect_stroke(rect, 2.0, EStroke::new(1.0_f32, stroke), StrokeKind::Inside);
    paint_glyph(painter, rect.shrink(2.4), action, glyph);
}

fn paint_glyph(painter: &egui::Painter, r: Rect, action: AlignAction, color: Color32) {
    let s = EStroke::new(1.15_f32, color);
    match action {
        AlignAction::Align(BoardAlign::Left) => {
            painter.line_segment([pos(r, 0.12, 0.12), pos(r, 0.12, 0.88)], s);
            painter.rect_filled(bar(r, 0.22, 0.22, 0.78, 0.42), 0.5, color);
            painter.rect_filled(bar(r, 0.22, 0.58, 0.62, 0.78), 0.5, color);
        }
        AlignAction::Align(BoardAlign::Right) => {
            painter.line_segment([pos(r, 0.88, 0.12), pos(r, 0.88, 0.88)], s);
            painter.rect_filled(bar(r, 0.22, 0.22, 0.78, 0.42), 0.5, color);
            painter.rect_filled(bar(r, 0.38, 0.58, 0.78, 0.78), 0.5, color);
        }
        AlignAction::Align(BoardAlign::CenterH) => {
            painter.line_segment([pos(r, 0.50, 0.10), pos(r, 0.50, 0.90)], s);
            painter.rect_filled(bar(r, 0.22, 0.22, 0.78, 0.42), 0.5, color);
            painter.rect_filled(bar(r, 0.32, 0.58, 0.68, 0.78), 0.5, color);
        }
        AlignAction::Align(BoardAlign::Top) => {
            painter.line_segment([pos(r, 0.12, 0.12), pos(r, 0.88, 0.12)], s);
            painter.rect_filled(bar(r, 0.22, 0.22, 0.42, 0.78), 0.5, color);
            painter.rect_filled(bar(r, 0.58, 0.22, 0.78, 0.62), 0.5, color);
        }
        AlignAction::Align(BoardAlign::Bottom) => {
            painter.line_segment([pos(r, 0.12, 0.88), pos(r, 0.88, 0.88)], s);
            painter.rect_filled(bar(r, 0.22, 0.22, 0.42, 0.78), 0.5, color);
            painter.rect_filled(bar(r, 0.58, 0.38, 0.78, 0.78), 0.5, color);
        }
        AlignAction::Align(BoardAlign::CenterV) => {
            painter.line_segment([pos(r, 0.10, 0.50), pos(r, 0.90, 0.50)], s);
            painter.rect_filled(bar(r, 0.22, 0.22, 0.42, 0.78), 0.5, color);
            painter.rect_filled(bar(r, 0.58, 0.32, 0.78, 0.68), 0.5, color);
        }
        AlignAction::Distribute(DistributeAxis::Horizontal) => {
            for t in [0.18, 0.50, 0.82] {
                painter.rect_filled(bar(r, t - 0.08, 0.22, t + 0.08, 0.78), 0.5, color);
            }
        }
        AlignAction::Distribute(DistributeAxis::Vertical) => {
            for t in [0.18, 0.50, 0.82] {
                painter.rect_filled(bar(r, 0.22, t - 0.08, 0.78, t + 0.08), 0.5, color);
            }
        }
    }
}

fn label_anchor(frame: Rect, icon: Rect) -> (egui::Align2, Pos2) {
    let c = icon.center();
    let pad = 6.0;
    if (c.y - frame.top()).abs() < 1.0 {
        (
            egui::Align2::CENTER_BOTTOM,
            Pos2::new(c.x, icon.top() - pad),
        )
    } else if (c.y - frame.bottom()).abs() < 1.0 {
        (
            egui::Align2::CENTER_TOP,
            Pos2::new(c.x, icon.bottom() + pad),
        )
    } else if (c.x - frame.left()).abs() < 1.0 {
        (
            egui::Align2::RIGHT_CENTER,
            Pos2::new(icon.left() - pad, c.y),
        )
    } else {
        (
            egui::Align2::LEFT_CENTER,
            Pos2::new(icon.right() + pad, c.y),
        )
    }
}

fn pos(r: Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(r.min.x + r.width() * x, r.min.y + r.height() * y)
}

fn bar(r: Rect, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    Rect::from_min_max(pos(r, x0, y0), pos(r, x1, y1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> WorldRect {
        WorldRect::new(x, y, w, h)
    }

    fn id(n: u64) -> NodeId {
        NodeId(n)
    }

    #[test]
    fn align_left_shares_the_left_datum() {
        let bounds = r(10.0, 0.0, 100.0, 50.0);
        let a = aligned_rect(r(20.0, 0.0, 10.0, 10.0), bounds, BoardAlign::Left);
        assert_eq!(a.x, 10.0);
        assert_eq!(a.y, 0.0);
    }

    #[test]
    fn align_center_h_centers_in_the_union() {
        let bounds = r(0.0, 0.0, 100.0, 40.0);
        let a = aligned_rect(r(0.0, 0.0, 20.0, 10.0), bounds, BoardAlign::CenterH);
        assert!((a.x - 40.0).abs() < 1e-4);
    }

    #[test]
    fn distribute_h_keeps_ends_and_even_gaps() {
        let ids = [
            (id(1), r(0.0, 0.0, 10.0, 10.0)),
            (id(2), r(20.0, 5.0, 10.0, 10.0)),
            (id(3), r(90.0, 0.0, 10.0, 10.0)),
        ];
        let out = distributed_rects(&ids, DistributeAxis::Horizontal);
        assert!((out[0].1.x - 0.0).abs() < 1e-4);
        assert!((out[2].1.x - 90.0).abs() < 1e-4);
        let g0 = out[1].1.x - (out[0].1.x + out[0].1.w);
        let g1 = out[2].1.x - (out[1].1.x + out[1].1.w);
        assert!((g0 - g1).abs() < 1e-3);
    }

    #[test]
    fn distribute_is_identity_for_two() {
        let ids = [
            (id(1), r(0.0, 0.0, 10.0, 10.0)),
            (id(2), r(40.0, 0.0, 10.0, 10.0)),
        ];
        let out = distributed_rects(&ids, DistributeAxis::Horizontal);
        assert_eq!(out[0].1.x, 0.0);
        assert_eq!(out[1].1.x, 40.0);
    }

    #[test]
    fn locked_ids_do_not_move_in_preview() {
        let ids = [
            (id(1), r(0.0, 0.0, 10.0, 10.0)),
            (id(2), r(40.0, 20.0, 10.0, 10.0)),
        ];
        let out = preview_rects(&ids, &[id(2)], AlignAction::Align(BoardAlign::Left));
        assert!((out[0].1.x - 0.0).abs() < 1e-4);
        assert!((out[1].1.x - 40.0).abs() < 1e-4);
    }

    #[test]
    fn bottom_and_left_clusters_sit_outside_the_group_box() {
        let bbox = Rect::from_min_max(Pos2::new(100.0, 80.0), Pos2::new(300.0, 200.0));
        let layout = layout_for(bbox, 3);
        let bottom_y = bbox.bottom() + FRAME_OUTSET_PX;
        let left_x = bbox.left() - FRAME_OUTSET_PX;
        let bottom: Vec<_> = layout
            .hits
            .iter()
            .filter(|h| (h.rect.center().y - bottom_y).abs() < 0.5)
            .collect();
        let left: Vec<_> = layout
            .hits
            .iter()
            .filter(|h| (h.rect.center().x - left_x).abs() < 0.5)
            .collect();
        assert_eq!(bottom.len(), 4);
        assert_eq!(left.len(), 4);
        assert_eq!(bottom[0].action, AlignAction::Align(BoardAlign::Left));
        assert_eq!(
            bottom[3].action,
            AlignAction::Distribute(DistributeAxis::Horizontal)
        );
        assert_eq!(left[0].action, AlignAction::Align(BoardAlign::Top));
        assert!(bottom[3].enabled);
        let top_y = bbox.top() - FRAME_OUTSET_PX;
        let right_x = bbox.right() + FRAME_OUTSET_PX;
        assert!(layout
            .hits
            .iter()
            .all(|h| (h.rect.center().y - top_y).abs() > 0.5));
        assert!(layout
            .hits
            .iter()
            .all(|h| (h.rect.center().x - right_x).abs() > 0.5));
    }

    #[test]
    fn distribute_icon_disabled_for_two() {
        let bbox = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(80.0, 80.0));
        let layout = layout_for(bbox, 2);
        let dist = layout
            .hits
            .iter()
            .filter(|h| matches!(h.action, AlignAction::Distribute(_)))
            .collect::<Vec<_>>();
        assert!(dist.iter().all(|h| !h.enabled));
        assert!(dist
            .iter()
            .all(|h| hit_test(h.rect.center(), &layout).is_none()));
    }

    #[test]
    fn icon_hit_does_not_overlap_the_inner_bbox() {
        let bbox = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(200.0, 100.0));
        let layout = layout_for(bbox, 3);
        for h in &layout.hits {
            assert!(
                !bbox.contains(h.rect.center()),
                "icon {:?} sits inside the selection box",
                h.action
            );
        }
        // Mid-top of the inner box is the N resize handle — must miss the widget.
        assert_eq!(hit_test(Pos2::new(100.0, 0.0), &layout), None);
    }
}
