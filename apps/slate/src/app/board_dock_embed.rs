//! Canvas-embedded copies of a dock palette (`NodeKind::DockStrip`).
//!
//! Each drop is a journaled node. Contents are derived from the dock recipe
//! plus the `visible` snapshot taken at drop. Baseline dock chrome is
//! untouched — these copies never pin, unpin, or hide the floating strip.

use super::board::{BoardDrag, BoardXf};
use super::ui::tools::{activate_flyout_id, flyout_tool_visual};
use super::SlateApp;
use atlas_shell::canvas_scale;
use atlas_shell::canvas_text;
use atlas_shell::dock::{self, paint_dock_icon};
use eframe::egui::{self, Align2, FontId, Pos2, Stroke};
use slate_doc::scene::{DockStripNode, Node, NodeKind, WorldRect};
use slate_doc::NodeId;

/// Fillet shared by paint, hover glow, and selection chrome (P1.dock-strip).
pub(crate) fn dock_strip_corner_radius(z: f32) -> f32 {
    canvas_scale::px(6.0, z)
}

impl SlateApp {
    /// Place a copy of `palette_id` at the view center. Unlimited; each
    /// call journals a new node.
    pub(crate) fn drop_dock_strip(&mut self, ctx: &egui::Context, palette_id: &str) {
        if self.doc().view.active_view != slate_doc::ViewKind::Board {
            return;
        }
        let visible = dock::last_strip_tools(ctx, palette_id);
        let n = visible.len().max(1) as f32;
        let mut tokens = atlas_shell::tokens::current().dock;
        tokens.normalize();
        let icon_px = dock::flyout_icon_size(&tokens);
        let gap_px = 4.0;
        let pad_px = 8.0;
        let z = self.tab().cam.z.max(0.05);
        let w = (n * icon_px + (n - 1.0) * gap_px + pad_px * 2.0) / z;
        let h = (icon_px + pad_px * 2.0) / z;
        let center = self.board_xf().s2w(self.canvas_rect.center());
        let rect = WorldRect::new(center.x - w * 0.5, center.y - h * 0.5, w, h);
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::DockStrip(DockStripNode {
                palette_id: palette_id.to_owned(),
                visible,
            }),
        );
        self.add_nodes(vec![node]);
    }

    /// Pointing-hand on an icon. Never steals the press — click arms, and
    /// click-hold-drag moves the whole node (P1.dock-strip).
    pub(crate) fn dock_embed_frame(
        &mut self,
        ctx: &egui::Context,
        xf: &BoardXf,
        pointer: Option<Pos2>,
        blocked: bool,
    ) -> bool {
        if blocked {
            return false;
        }
        let Some(screen) = pointer else {
            return false;
        };
        let world = xf.s2w(screen);
        if self.dock_embed_tool_at(world.x, world.y).is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        false
    }

    pub(crate) fn dock_embed_node_at(&self, wx: f32, wy: f32) -> Option<NodeId> {
        for n in self.doc().scene.nodes.iter().rev() {
            if n.hidden {
                continue;
            }
            if !matches!(n.kind, NodeKind::DockStrip(_)) {
                continue;
            }
            if n.rect.contains(wx, wy) {
                return Some(n.id);
            }
        }
        None
    }

    pub(crate) fn dock_embed_tool_at(&self, wx: f32, wy: f32) -> Option<(NodeId, String)> {
        for n in self.doc().scene.nodes.iter().rev() {
            if n.hidden {
                continue;
            }
            let NodeKind::DockStrip(strip) = &n.kind else {
                continue;
            };
            if !n.rect.contains(wx, wy) {
                continue;
            }
            let tools = strip_tools(strip);
            let slots = icon_slots(n.rect, tools.len());
            for (id, slot) in tools.iter().zip(slots.iter()) {
                if slot.contains(wx, wy) {
                    return Some((n.id, (*id).to_owned()));
                }
            }
            return None;
        }
        None
    }

    /// Click (no drag) on a canvas copy: an icon arms the command; padding
    /// selects the node. Create tools do not place on this click.
    pub(crate) fn try_dock_embed_click(&mut self, ctx: &egui::Context, world: Pos2) -> bool {
        if let Some((_, tool)) = self.dock_embed_tool_at(world.x, world.y) {
            activate_flyout_id(self, ctx, &tool);
            return true;
        }
        if let Some(id) = self.dock_embed_node_at(world.x, world.y) {
            self.board_sel.clear();
            self.board_sel.insert(id);
            return true;
        }
        false
    }

    /// Press-drag anywhere on the node — including an icon — moves it.
    /// Edge resize still wins via [`Self::begin_transform_drag`].
    pub(crate) fn begin_dock_strip_drag(&mut self, screen: Pos2, world: Pos2) -> Option<BoardDrag> {
        let id = self.dock_embed_node_at(world.x, world.y)?;
        if let Some(drag) = self.begin_transform_drag(screen, world) {
            return Some(drag);
        }
        if !self.board_sel.contains(&id) {
            self.board_sel.clear();
            self.board_sel.insert(id);
        }
        let n = self.doc().scene.node(id)?.clone();
        Some(BoardDrag::Move {
            ids: vec![id],
            before: vec![n],
            start_world: world,
            dup: false,
        })
    }
}

pub(crate) fn paint_dock_strip(
    ui: &egui::Ui,
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    strip: &DockStripNode,
) {
    let srect = xf.rect_w2s(node.rect);
    let z = xf.z;
    let palette = {
        // Theme ink for the card — matches dock squircles, not a new chrome.
        let tokens = atlas_shell::tokens::current();
        if ui.visuals().dark_mode {
            tokens.dock.dark
        } else {
            tokens.dock.light
        }
    };
    let radius = dock_strip_corner_radius(z);
    painter.rect_filled(
        srect,
        radius,
        palette.popover_fill_color().gamma_multiply(0.72),
    );
    painter.rect_stroke(
        srect,
        radius,
        Stroke::new(
            canvas_scale::px(1.0, z).max(0.6),
            palette.border_color().gamma_multiply(0.7),
        ),
        egui::StrokeKind::Inside,
    );

    let tools = strip_tools(strip);
    if tools.is_empty() {
        let title_px = canvas_scale::px(12.0, z);
        if canvas_text::legible(title_px) {
            canvas_text::text(
                painter,
                srect.center(),
                Align2::CENTER_CENTER,
                &strip.palette_id,
                FontId::proportional(title_px),
                palette.text_color(),
            );
        }
        return;
    }
    let slots = icon_slots(node.rect, tools.len());
    let pointer = ui.ctx().pointer_latest_pos().map(|p| xf.s2w(p));
    for (id, slot) in tools.iter().zip(slots.iter()) {
        let icon_s = xf.rect_w2s(*slot);
        let hovered = pointer.is_some_and(|p| slot.contains(p.x, p.y));
        let fill = if hovered {
            palette.icon_hover_color().gamma_multiply(0.35)
        } else {
            palette.icon_fill_color()
        };
        atlas_shell::dock::paint_squircle(
            painter,
            icon_s.shrink(0.5),
            fill,
            Stroke::new(1.0_f32, palette.border_color()),
            4.0,
        );
        let (label, icon) = flyout_tool_visual(id);
        paint_dock_icon(
            painter,
            icon_s.shrink((icon_s.width() * 0.20).max(1.0)),
            icon,
            palette.text_color(),
        );
        if hovered {
            let name_px = canvas_scale::px(11.0, z);
            if canvas_text::legible(name_px) {
                canvas_text::text(
                    painter,
                    Pos2::new(icon_s.center().x, icon_s.top() - canvas_scale::px(4.0, z)),
                    Align2::CENTER_BOTTOM,
                    label,
                    FontId::proportional(name_px),
                    palette.text_color(),
                );
            }
        }
    }
}

fn strip_tools(strip: &DockStripNode) -> Vec<&str> {
    if strip.visible.is_empty() {
        Vec::new()
    } else {
        strip.visible.iter().map(String::as_str).collect()
    }
}

fn icon_slots(rect: WorldRect, count: usize) -> Vec<WorldRect> {
    let n = count.max(1);
    let pad = rect.h * 0.16;
    let gap = rect.h * 0.10;
    let inner_w = (rect.w - pad * 2.0).max(1.0);
    let icon = ((inner_w - gap * (n.saturating_sub(1) as f32)) / n as f32)
        .min(rect.h - pad * 2.0)
        .max(0.5);
    let used = icon * n as f32 + gap * (n.saturating_sub(1) as f32);
    let x0 = rect.x + (rect.w - used) * 0.5;
    let y = rect.y + (rect.h - icon) * 0.5;
    (0..count)
        .map(|i| WorldRect::new(x0 + i as f32 * (icon + gap), y, icon, icon))
        .collect()
}
