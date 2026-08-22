//! Canvas-embedded copies of a dock palette (`NodeKind::DockStrip`).
//!
//! Paint, measure, and hit-test go through `atlas_shell::dock` — the same
//! fieldset strip the docked flyout uses. The node is a contain-scaled
//! poster of that strip (P0.9): extra bounds are margin, never a reflow.
//! Baseline dock chrome is untouched; copies never pin, unpin, or hide it.

use super::board::{BoardDrag, BoardXf};
use super::ui::tools::{activate_flyout_id, palette_strip_items, palette_title};
use super::SlateApp;
use atlas_shell::dock::{self, DockSide, IconStripLayout};
use atlas_shell::tokens::DockTokens;
use eframe::egui::{self, Pos2, Rect};
use slate_doc::scene::{DockStripNode, Node, NodeKind};
use slate_doc::NodeId;

/// Wide enough that a canvas poster never wraps the way a window dock does.
const CANVAS_STRIP_BUDGET: f32 = 16_384.0;

fn dock_tokens() -> DockTokens {
    let mut tokens = atlas_shell::tokens::current().dock;
    tokens.normalize();
    tokens
}

fn measure_canvas_strip(
    ctx: &egui::Context,
    items: &[atlas_shell::dock::FlyoutItem<'_>],
    tokens: &DockTokens,
) -> IconStripLayout {
    dock::measure_icon_strip(
        ctx,
        items,
        tokens,
        DockSide::BottomCenter,
        CANVAS_STRIP_BUDGET,
    )
}

impl SlateApp {
    /// Place a copy of `palette_id` at the view center. Unlimited; each
    /// call journals a new node sized to the docked strip's intrinsic card.
    pub(crate) fn drop_dock_strip(&mut self, ctx: &egui::Context, palette_id: &str) {
        if self.doc().view.active_view != slate_doc::ViewKind::Board {
            return;
        }
        let mut visible = dock::last_strip_tools(ctx, palette_id);
        let items = palette_strip_items(
            self,
            palette_id,
            if visible.is_empty() {
                &[]
            } else {
                &visible
            },
        );
        if visible.is_empty() {
            visible = items.iter().map(|it| it.id.to_string()).collect();
        }
        let tokens = dock_tokens();
        let layout = measure_canvas_strip(ctx, &items, &tokens);
        let card = dock::icon_strip_card_size(&layout, &tokens);
        let z = self.tab().cam.z.max(0.05);
        let w = (card.x / z).max(8.0);
        let h = (card.y / z).max(8.0);
        let center = self.board_xf().s2w(self.canvas_rect.center());
        let rect = slate_doc::scene::WorldRect::new(center.x - w * 0.5, center.y - h * 0.5, w, h);
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
        if self.dock_embed_tool_at(ctx, xf, screen).is_some() {
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

    pub(crate) fn dock_embed_tool_at(
        &self,
        ctx: &egui::Context,
        xf: &BoardXf,
        screen: Pos2,
    ) -> Option<(NodeId, String)> {
        let tokens = dock_tokens();
        let world = xf.s2w(screen);
        for n in self.doc().scene.nodes.iter().rev() {
            if n.hidden {
                continue;
            }
            let NodeKind::DockStrip(strip) = &n.kind else {
                continue;
            };
            if !n.rect.contains(world.x, world.y) {
                continue;
            }
            let items = palette_strip_items(self, &strip.palette_id, &strip.visible);
            let layout = measure_canvas_strip(ctx, &items, &tokens);
            let dest = xf.rect_w2s(n.rect);
            if let Some(id) = dock::icon_strip_hit(dest, &layout, &tokens, screen) {
                return Some((n.id, id.to_owned()));
            }
            return None;
        }
        None
    }

    /// Painted card + fillet in screen space (selection / hover must match).
    pub(crate) fn dock_strip_screen_card(
        &self,
        ctx: &egui::Context,
        xf: &BoardXf,
        node: &Node,
        strip: &DockStripNode,
    ) -> (Rect, f32) {
        let dest = xf.rect_w2s(node.rect);
        let tokens = dock_tokens();
        let items = palette_strip_items(self, &strip.palette_id, &strip.visible);
        if items.is_empty() {
            return (dest, tokens.popover_corner_radius);
        }
        let layout = measure_canvas_strip(ctx, &items, &tokens);
        (
            dock::icon_strip_card_rect(dest, &layout, &tokens),
            dock::icon_strip_card_radius(dest, &layout, &tokens),
        )
    }

    /// World-space center of the first tool slot — tests and diagnostics.
    pub(crate) fn dock_embed_first_tool_world(
        &self,
        ctx: &egui::Context,
        xf: &BoardXf,
        node: &Node,
    ) -> Option<Pos2> {
        let NodeKind::DockStrip(strip) = &node.kind else {
            return None;
        };
        let tokens = dock_tokens();
        let items = palette_strip_items(self, &strip.palette_id, &strip.visible);
        let layout = measure_canvas_strip(ctx, &items, &tokens);
        let id = layout.first_slot_id()?;
        let dest = xf.rect_w2s(node.rect);
        let slot = dock::icon_strip_slot_rect(dest, &layout, &tokens, id)?;
        Some(xf.s2w(slot.center()))
    }

    /// Click (no drag) on a canvas copy: an icon arms the command; padding
    /// selects the node. Create tools do not place on this click.
    pub(crate) fn try_dock_embed_click(&mut self, ctx: &egui::Context, world: Pos2) -> bool {
        let xf = self.board_xf();
        let screen = xf.w2s(world);
        if let Some((_, tool)) = self.dock_embed_tool_at(ctx, &xf, screen) {
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
    xf: &BoardXf,
    node: &Node,
    strip: &DockStripNode,
    app: &SlateApp,
) {
    let dest = xf.rect_w2s(node.rect);
    let tokens = dock_tokens();
    let items = palette_strip_items(app, &strip.palette_id, &strip.visible);
    let layout = measure_canvas_strip(ui.ctx(), &items, &tokens);
    let hovered = ui
        .ctx()
        .pointer_latest_pos()
        .and_then(|p| dock::icon_strip_hit(dest, &layout, &tokens, p));
    dock::paint_icon_strip_card(
        ui,
        dest,
        palette_title(&strip.palette_id),
        &items,
        &layout,
        &tokens,
        hovered,
    );
}
