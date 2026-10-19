//! Grasshopper-style align widget: two icon clusters on a multi-selection
//! (left + bottom — the other two sides duplicate the same actions). The
//! group bounding-box outline is the selection indicator — this module does
//! not draw a second frame, and hover does not ghost the result.
//!
//! Syntax (target rects) is renderer-free in this file's math helpers.
//! Shared shell chrome owns the two T-shaped clusters and cached arrow glyphs.
//! Hit-testing is screen-space (eight fixed squares). Mutations go through
//! [`SlateApp::patch_nodes`] (Art. VI). See `docs/keymap/contracts/align-widget.md`.

use super::board::{BoardTool, BoardXf};
use super::board_snap;
use super::SlateApp;
use atlas_commands::CommandId;
use atlas_shell::theme::Palette;
use atlas_shell::{icons::Icon, selection_tools::alignment};
use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, Vec2};
use slate_doc::scene::{NodeKind, WorldRect};
use slate_doc::NodeId;

#[cfg(test)]
pub use alignment::{bottom_outset_px, FRAME_OUTSET_PX, ICON_GAP_PX, ICON_PX};
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

    fn icon(self) -> Icon {
        match self {
            Self::Align(BoardAlign::Left) => Icon::AlignLeft,
            Self::Align(BoardAlign::CenterH) => Icon::AlignCenterH,
            Self::Align(BoardAlign::Right) => Icon::AlignRight,
            Self::Align(BoardAlign::Top) => Icon::AlignTop,
            Self::Align(BoardAlign::CenterV) => Icon::AlignCenterV,
            Self::Align(BoardAlign::Bottom) => Icon::AlignBottom,
            Self::Distribute(DistributeAxis::Horizontal) => Icon::DistributeHorizontal,
            Self::Distribute(DistributeAxis::Vertical) => Icon::DistributeVertical,
        }
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
    chrome: alignment::AlignmentLayout,
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

fn layout_for(screen_bbox: Rect, alignable: usize, zoom: f32) -> AlignLayout {
    let chrome = alignment::layout(screen_bbox, zoom);
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
    let hits = std::array::from_fn(|i| {
        let (action, rect) = if i < 4 {
            (h_actions[i], chrome.bottom[i])
        } else {
            (v_actions[i - 4], chrome.left[i - 4])
        };
        AlignHit {
            action,
            rect,
            enabled: !action.needs_three() || dist_on,
        }
    });
    AlignLayout { chrome, hits }
}

fn hit_test(screen: Pos2, layout: &AlignLayout) -> Option<AlignAction> {
    layout.hits.iter().find_map(|h| {
        (h.enabled && h.rect.expand(alignment::HIT_PAD_PX).contains(screen)).then_some(h.action)
    })
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
        let mut layout = layout_for(xf.rect_w2s(bounds), members.len(), xf.z);
        // The current property owner publishes its live screen-space hit boxes
        // before board input. Move the whole cluster, never individual buttons.
        let mut occupied = self.shape_properties.chrome_hits.clone();
        occupied.sort_by(|a, b| a.bottom().total_cmp(&b.bottom()));
        for occupied in &occupied {
            if layout
                .chrome
                .bottom
                .iter()
                .any(|r| r.expand(alignment::HIT_PAD_PX).intersects(*occupied))
            {
                let top = layout
                    .chrome
                    .bottom
                    .iter()
                    .map(Rect::top)
                    .fold(f32::INFINITY, f32::min);
                let delta = Vec2::new(
                    0.0,
                    occupied.bottom() + alignment::ICON_GAP_PX + alignment::HIT_PAD_PX - top,
                );
                layout
                    .chrome
                    .bottom
                    .iter_mut()
                    .for_each(|r| *r = r.translate(delta));
            }
        }
        // The outward wire reach is intentionally generous. Keep the left
        // cluster outside that reach on the selected hosts that offer ports.
        let wire_left = members
            .iter()
            .filter_map(|(id, _, _)| self.doc().scene.node(*id))
            .filter(|n| {
                self.wire_host(n).offers_ports() && super::board_wire::wire_ports_fit(n.rect, xf.z)
            })
            .flat_map(|n| self.wire_host(n).ports())
            .map(|port| xf.w2s(Pos2::from(port.point)))
            .filter(|point| {
                layout.chrome.left.iter().any(|r| {
                    point.y >= r.top() - super::board_wire::grip_hit_out_radius(xf.z)
                        && point.y <= r.bottom() + super::board_wire::grip_hit_out_radius(xf.z)
                })
            })
            .map(|point| point.x - super::board_wire::grip_hit_out_radius(xf.z))
            .fold(f32::INFINITY, f32::min);
        let right = layout
            .chrome
            .left
            .iter()
            .map(Rect::right)
            .fold(f32::NEG_INFINITY, f32::max);
        let dx = (wire_left - alignment::ICON_GAP_PX - alignment::HIT_PAD_PX - right).min(0.0);
        layout
            .chrome
            .left
            .iter_mut()
            .for_each(|r| *r = r.translate(Vec2::new(dx, 0.0)));
        for (i, hit) in layout.hits.iter_mut().enumerate() {
            hit.rect = if i < 4 {
                layout.chrome.bottom[i]
            } else {
                layout.chrome.left[i - 4]
            };
        }
        Some(layout)
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

        for (i, hit) in layout.hits.iter().enumerate() {
            let hovered = hover == Some(hit.action) && hit.enabled;
            alignment::paint_button(
                painter,
                hit.rect,
                hit.action.icon(),
                hit.enabled,
                hovered,
                palette,
                select,
            );
            if hovered {
                alignment::paint_label(
                    painter,
                    if i < 4 {
                        layout.chrome.bottom
                    } else {
                        layout.chrome.left
                    },
                    i < 4,
                    hit.action.label(),
                    palette,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
