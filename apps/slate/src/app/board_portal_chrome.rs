//! Shared portal chrome: identity tab (opt-in by kind), maximize overlay,
//! and corner clip.
//!
//! Identity tab is **P1.portal.chrome** — web embed has one; agent portal
//! does not. Maximize and tab-fold are derived view-state and are never
//! journaled (Art. VI.3). On the canvas, tab height and type follow **P0.9**.
//! Tab painting lives in `atlas-shell::tabs`.

use std::collections::HashSet;

use atlas_shell::tabs::{self, PortalTabAction, PortalTabModel};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, StrokeKind};
use slate_doc::scene::{NodeId, NodeKind, PortalKind, PortalNode, WorldRect};

use super::board::{self, rgba32, BoardXf};
use super::SlateApp;

/// Derived chrome for every portal on the board.
#[derive(Default)]
pub struct PortalChrome {
    /// Portal filling the window. `None` = ordinary board view.
    pub maximized: Option<NodeId>,
    /// Portals whose identity tab is folded away.
    pub chrome_collapsed: HashSet<NodeId>,
}

/// Screen-space layout of one portal's frame, tab, and contents.
#[derive(Debug, Clone, Copy)]
pub struct PortalChromeLayout {
    pub frame: Rect,
    pub radius: f32,
    pub bar: Option<Rect>,
    pub reveal: Option<Rect>,
    /// Maximize hit. On a tab bar this is the window-control slot at the
    /// right; otherwise it floats in the frame's upper-right.
    pub maximize: Rect,
    pub body: Rect,
    pub page: Rect,
}

impl PortalChromeLayout {
    pub fn pointer_on_chrome(&self, p: Pos2) -> bool {
        self.bar.is_some_and(|r| r.contains(p))
            || self.reveal.is_some_and(|r| r.contains(p))
            || self.maximize.contains(p)
    }
}

pub fn portal_frame_tokens() -> atlas_shell::tokens::PortalFrameTokens {
    atlas_shell::tokens::current().portal_frame
}

pub fn tab_bar_height() -> f32 {
    let tokens = atlas_shell::tokens::current();
    tokens.topbar.height * tokens.portal_frame.tab_height_scale
}

/// Identity tab is web-only (P1.portal.chrome). Every portal still gets
/// the maximize icon.
pub fn uses_identity_tab(kind: PortalKind) -> bool {
    matches!(kind, PortalKind::Web)
}

fn maximize_in_bar(bar: Rect, zoom: f32, radius: f32) -> Rect {
    let size = portal_frame_tokens().chrome_button_px * zoom;
    let inset = radius.max(2.0 * zoom);
    let w = size.min(bar.width() * 0.35).max(bar.height());
    let right = (bar.right() - inset).max(bar.left() + w);
    Rect::from_min_max(
        Pos2::new(right - w, bar.top()),
        Pos2::new(right, bar.bottom()),
    )
}

fn maximize_floating(frame: Rect, zoom: f32, radius: f32) -> Rect {
    let size = portal_frame_tokens().chrome_button_px * zoom;
    let inset = radius.max(6.0 * zoom);
    if frame.width() < size + inset * 2.0 || frame.height() < size + inset * 2.0 {
        return Rect::from_min_max(frame.right_top(), frame.right_top());
    }
    Rect::from_min_max(
        Pos2::new(frame.right() - inset - size, frame.top() + inset),
        Pos2::new(frame.right() - inset, frame.top() + inset + size),
    )
}

/// Lay out a portal of `kind`: identity tab only when the kind opted in.
pub fn layout_for_portal(
    kind: PortalKind,
    frame: Rect,
    collapsed: bool,
    maximized: bool,
    zoom: f32,
) -> PortalChromeLayout {
    if uses_identity_tab(kind) {
        layout_portal_chrome(frame, collapsed, maximized, zoom)
    } else {
        layout_portal_frame(frame, maximized, zoom)
    }
}

/// Frame + body, no identity tab — agent portals, and inner paints that
/// already reserved a tab above.
pub fn layout_portal_frame(frame: Rect, maximized: bool, zoom: f32) -> PortalChromeLayout {
    let tokens = portal_frame_tokens();
    let z = if maximized { 1.0 } else { zoom.max(0.01) };
    let radius = if maximized {
        0.0
    } else {
        tokens.corner_radius * z
    };
    let border = if maximized {
        0.0
    } else {
        tokens.border_hit_px * z
    };
    let page = Rect::from_min_max(
        Pos2::new(frame.left() + border, frame.top() + border),
        Pos2::new(frame.right() - border, frame.bottom() - border),
    );
    PortalChromeLayout {
        frame,
        radius,
        bar: None,
        reveal: None,
        maximize: maximize_floating(frame, z, radius),
        body: frame,
        page,
    }
}

/// Contents fill `frame` with no identity tab — presentation and inner
/// maximize paints that already reserved the tab above.
pub fn layout_portal_contents_only(frame: Rect) -> PortalChromeLayout {
    PortalChromeLayout {
        frame,
        radius: 0.0,
        bar: None,
        reveal: None,
        maximize: Rect::NOTHING,
        body: frame,
        page: frame,
    }
}

/// Lay out the identity tab and the contents body inside `frame`.
///
/// `zoom` is the board camera. On the canvas the tab, fillet, and border
/// track it (P0.9). Maximized, the tab is window chrome and stays
/// screen-sized (`zoom` is ignored).
pub fn layout_portal_chrome(
    frame: Rect,
    collapsed: bool,
    maximized: bool,
    zoom: f32,
) -> PortalChromeLayout {
    let tokens = portal_frame_tokens();
    let z = if maximized { 1.0 } else { zoom.max(0.01) };
    let radius = if maximized {
        0.0
    } else {
        tokens.corner_radius * z
    };
    let tab_h = (tab_bar_height() * z).min(frame.height());
    let border = if maximized {
        0.0
    } else {
        tokens.border_hit_px * z
    };

    if collapsed {
        let reveal_h = (tokens.reveal_strip_px * z).min(frame.height() * 0.35);
        let reveal = Rect::from_min_max(
            frame.left_top(),
            Pos2::new(frame.right(), frame.top() + reveal_h),
        );
        let body = frame;
        let page = Rect::from_min_max(
            Pos2::new(frame.left() + border, frame.top() + reveal_h.max(border)),
            Pos2::new(frame.right() - border, frame.bottom() - border),
        );
        return PortalChromeLayout {
            frame,
            radius,
            bar: None,
            reveal: Some(reveal),
            maximize: maximize_floating(frame, z, radius),
            body,
            page,
        };
    }

    let bar = Rect::from_min_max(
        frame.left_top(),
        Pos2::new(frame.right(), frame.top() + tab_h),
    );
    let body = if frame.height() <= tab_h + 4.0 {
        Rect::from_min_max(bar.left_bottom(), bar.left_bottom())
    } else {
        Rect::from_min_max(Pos2::new(frame.left(), bar.bottom()), frame.right_bottom())
    };
    let page = Rect::from_min_max(
        Pos2::new(body.left() + border, body.top()),
        Pos2::new(body.right() - border, body.bottom() - border),
    );
    PortalChromeLayout {
        frame,
        radius,
        bar: Some(bar),
        reveal: None,
        maximize: maximize_in_bar(bar, z, radius),
        body,
        page,
    }
}

impl SlateApp {
    pub fn portal_chrome_collapsed(&self, id: NodeId) -> bool {
        self.portal_chrome.chrome_collapsed.contains(&id)
    }

    pub fn portal_is_maximized(&self, id: NodeId) -> bool {
        self.portal_chrome.maximized == Some(id)
    }

    /// True when the pointer is on a portal's maximize hit — resize chrome
    /// must not steal that corner.
    pub(crate) fn pointer_on_portal_maximize(&self, p: Pos2, xf: &BoardXf) -> bool {
        let world = xf.s2w(p);
        let Some(id) =
            super::board_path::board_pick_node(&self.doc().scene, world.x, world.y, xf.z)
        else {
            return false;
        };
        let Some(node) = self.doc().scene.node(id) else {
            return false;
        };
        let NodeKind::Portal(portal) = &node.kind else {
            return false;
        };
        let layout = layout_for_portal(
            portal.kind,
            xf.rect_w2s(node.rect),
            self.portal_chrome_collapsed(id),
            false,
            xf.z,
        );
        layout.maximize.contains(p)
    }

    /// Press on a portal's maximize square. Toggles maximize and reports
    /// whether the press was consumed.
    pub(crate) fn try_portal_maximize_press(&mut self, p: Pos2, xf: &BoardXf) -> bool {
        if !self.pointer_on_portal_maximize(p, xf) {
            return false;
        }
        let world = xf.s2w(p);
        let Some(id) =
            super::board_path::board_pick_node(&self.doc().scene, world.x, world.y, xf.z)
        else {
            return false;
        };
        self.portal_toggle_maximize(id)
    }

    /// Fill the window with this portal. Derived; the node rect is untouched.
    pub fn portal_maximize(&mut self, id: NodeId) -> bool {
        if !self.doc().scene.node(id).is_some_and(|n| n.is_portal()) {
            return false;
        }
        self.portal_chrome.maximized = Some(id);
        self.board_sel = std::iter::once(id).collect();
        if matches!(
            self.doc().scene.node(id).map(|n| &n.kind),
            Some(NodeKind::Portal(p)) if p.kind == PortalKind::Web
        ) {
            self.web_focus(id);
        }
        true
    }

    pub fn portal_restore(&mut self) -> bool {
        self.portal_chrome.maximized.take().is_some()
    }

    pub fn portal_toggle_maximize(&mut self, id: NodeId) -> bool {
        if self.portal_chrome.maximized == Some(id) {
            self.portal_restore()
        } else {
            self.portal_maximize(id)
        }
    }

    pub fn portal_toggle_chrome(&mut self, id: NodeId) -> bool {
        let kind = self.doc().scene.node(id).and_then(|n| match &n.kind {
            NodeKind::Portal(p) => Some(p.kind),
            _ => None,
        });
        let Some(kind) = kind else {
            return false;
        };
        if !uses_identity_tab(kind) {
            return false;
        }
        if !self.portal_chrome.chrome_collapsed.remove(&id) {
            self.portal_chrome.chrome_collapsed.insert(id);
        }
        true
    }

    /// Probe / alias: maximize a web portal.
    pub fn web_maximize(&mut self, id: NodeId) {
        let _ = self.portal_maximize(id);
    }

    pub fn web_restore(&mut self) {
        let _ = self.portal_restore();
    }

    fn selected_portal(&self) -> Option<NodeId> {
        if self.board_sel.len() != 1 {
            return None;
        }
        let id = *self.board_sel.iter().next()?;
        self.doc()
            .scene
            .node(id)
            .is_some_and(|n| n.is_portal())
            .then_some(id)
    }

    pub(crate) fn portal_maximize_selected(&mut self) -> bool {
        let id = self
            .portal_chrome
            .maximized
            .or(self.web.focused)
            .or_else(|| self.selected_portal());
        let Some(id) = id else {
            return false;
        };
        self.portal_toggle_maximize(id)
    }

    pub(crate) fn portal_toggle_chrome_selected(&mut self) -> bool {
        let id = self
            .portal_chrome
            .maximized
            .or(self.web.focused)
            .or_else(|| self.selected_portal());
        let Some(id) = id else {
            return false;
        };
        self.portal_toggle_chrome(id)
    }

    pub(crate) fn drop_stale_portal_chrome(&mut self) {
        if self
            .portal_chrome
            .maximized
            .is_some_and(|id| self.doc().scene.node(id).is_none())
        {
            self.portal_chrome.maximized = None;
        }
        let stale: Vec<NodeId> = self
            .portal_chrome
            .chrome_collapsed
            .iter()
            .copied()
            .filter(|&id| !self.doc().scene.node(id).is_some_and(|n| n.is_portal()))
            .collect();
        for id in stale {
            self.portal_chrome.chrome_collapsed.remove(&id);
        }
    }

    /// Apply a tab-bar click. Returns whether the pointer was consumed.
    pub(crate) fn apply_portal_tab_action(
        &mut self,
        id: NodeId,
        action: PortalTabAction,
        at: Option<Pos2>,
    ) -> bool {
        match action {
            PortalTabAction::ToggleMaximize => {
                // Canvas / overlay already committed this on press so the
                // board gesture cannot steal the corner. Ignore the later
                // egui clicked() or we would toggle twice and appear dead.
                if !self.board_align_eat_press {
                    self.portal_toggle_maximize(id);
                }
            }
            PortalTabAction::Collapse => {
                self.portal_chrome.chrome_collapsed.insert(id);
            }
            PortalTabAction::Context => {
                if let Some(p) = at {
                    self.board_menu = Some((id, p));
                }
            }
        }
        true
    }

    fn portal_tab_model<'a>(
        &self,
        id: NodeId,
        portal: &'a PortalNode,
        visiting: Option<&'a str>,
    ) -> PortalTabModel<'a> {
        let title = visiting.unwrap_or(portal.title.as_str());
        let title = if title.is_empty() {
            match portal.kind {
                PortalKind::Web => "Web portal",
                PortalKind::RepoLens => "Repository",
                PortalKind::StatusBoard => "Status",
                PortalKind::Agent => "Agent",
            }
        } else {
            title
        };
        PortalTabModel {
            title,
            tooltip: title,
            live: self.web.is_live(id),
            maximized: self.portal_is_maximized(id),
        }
    }

    /// Paint the identity tab (web) or the folded-tab reveal, and the
    /// maximize icon that every portal carries.
    pub(crate) fn paint_portal_identity_chrome(
        &mut self,
        ui: &egui::Ui,
        layout: &PortalChromeLayout,
        id: NodeId,
        portal: &PortalNode,
        visiting: Option<&str>,
    ) {
        let palette = self.palette();
        if let Some(bar) = layout.bar {
            let model = self.portal_tab_model(id, portal, visiting);
            if let Some(action) = tabs::portal_tab_bar(
                ui,
                &palette,
                bar,
                layout.maximize,
                layout.radius,
                id.0,
                &model,
            ) {
                self.apply_portal_tab_action(id, action, ui.ctx().pointer_latest_pos());
            }
            return;
        }
        if let Some(reveal) = layout.reveal {
            if tabs::portal_reveal_hint(ui, &palette, reveal, id.0) {
                self.portal_chrome.chrome_collapsed.remove(&id);
            }
        }
        if tabs::portal_maximize_button(
            ui,
            &palette,
            layout.maximize,
            id.0,
            self.portal_is_maximized(id),
        ) {
            self.portal_toggle_maximize(id);
        }
    }

    /// Fill + stroke of the portal frame. Contents paint between these two.
    pub(crate) fn paint_portal_frame_fill(
        &self,
        painter: &egui::Painter,
        layout: &PortalChromeLayout,
        fill: Color32,
        border: Color32,
        focused: bool,
    ) {
        painter.rect(
            layout.frame,
            layout.radius,
            fill,
            Stroke::NONE,
            StrokeKind::Inside,
        );
        let _ = (border, focused);
    }

    pub(crate) fn paint_portal_frame_stroke(
        &self,
        painter: &egui::Painter,
        layout: &PortalChromeLayout,
        border: Color32,
        focused: bool,
        zoom: f32,
    ) {
        let width = if focused { 2.0_f32 } else { 1.0_f32 } * zoom.max(0.01);
        painter.rect_stroke(
            layout.frame,
            layout.radius,
            Stroke::new(width, border),
            StrokeKind::Outside,
        );
    }

    /// Full-window overlay: the portal is the sole interface (P1.portal.maximize).
    pub fn paint_maximized_portal(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.portal_chrome.maximized else {
            return;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            self.portal_chrome.maximized = None;
            return;
        };
        let NodeKind::Portal(portal) = node.kind.clone() else {
            self.portal_chrome.maximized = None;
            return;
        };

        let screen = ui.max_rect();
        self.canvas_rect = screen;
        let collapsed = self.portal_chrome_collapsed(id);
        let layout = layout_for_portal(portal.kind, screen, collapsed, true, 1.0);
        let pointer = ui.ctx().pointer_latest_pos();
        let restore = ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
            && pointer.is_some_and(|p| layout.maximize.contains(p));
        if restore {
            self.board_align_eat_press = true;
        }
        let painter = ui.painter_at(screen);
        let palette = self.palette();
        let fill = rgba32(portal.fill);
        painter.rect_filled(screen, 0.0, fill);

        match portal.kind {
            PortalKind::Web => {
                self.paint_web_portal_in_rect(ui, &painter, &node, &portal, &layout, 1.0);
            }
            PortalKind::RepoLens | PortalKind::StatusBoard => {
                let xf = fit_xf(node.rect, layout.body);
                let clipped = painter.with_clip_rect(layout.body.intersect(painter.clip_rect()));
                self.paint_portal_node(ui, &clipped, &xf, &node, &portal, false);
            }
            PortalKind::Agent => {
                let xf = fit_xf(node.rect, layout.body);
                self.paint_agent_portal(ui, &painter, &xf, &node, &portal);
            }
        }

        let visiting = self.web_visiting_label(id, &portal);
        self.paint_portal_identity_chrome(ui, &layout, id, &portal, visiting.as_deref());
        self.paint_portal_frame_stroke(
            &painter,
            &layout,
            palette.border_strong,
            self.web.focused == Some(id),
            1.0,
        );

        if portal.kind == PortalKind::Web {
            self.web_input_in_layout(ui, id, &node, &portal, &layout);
        }

        if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary)) {
            if let Some(p) = pointer {
                if layout.frame.contains(p) {
                    self.board_menu = Some((id, p));
                }
            }
        }
        if restore {
            let _ = self.portal_restore();
        }
        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            self.board_align_eat_press = false;
        }
    }
}

fn fit_xf(world: WorldRect, dest: Rect) -> BoardXf {
    let z = (dest.width() / world.w.max(1.0)).min(dest.height() / world.h.max(1.0));
    BoardXf {
        center: dest.center(),
        offset: eframe::egui::vec2(world.x + world.w * 0.5, world.y + world.h * 0.5),
        z,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{pos2, Rect};

    #[test]
    fn every_portal_kind_uses_the_same_fillet() {
        let frame = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));
        let expected = portal_frame_tokens().corner_radius;
        for kind in [
            PortalKind::Web,
            PortalKind::Agent,
            PortalKind::StatusBoard,
            PortalKind::RepoLens,
        ] {
            let layout = layout_for_portal(kind, frame, false, false, 1.0);
            assert!(
                (layout.radius - expected).abs() < 1e-4,
                "{kind:?} fillet {} != shared {expected}",
                layout.radius
            );
        }
    }

    #[test]
    fn collapsed_layout_exposes_a_reveal_strip_and_full_body() {
        let frame = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));
        let layout = layout_portal_chrome(frame, true, false, 1.0);
        assert!(layout.bar.is_none());
        assert!(layout.reveal.is_some());
        assert_eq!(layout.body, frame);
        assert!(layout.radius > 0.0);
    }

    #[test]
    fn expanded_layout_reserves_a_tab_bar() {
        let frame = Rect::from_min_max(pos2(10.0, 20.0), pos2(410.0, 320.0));
        let layout = layout_portal_chrome(frame, false, false, 1.0);
        let bar = layout.bar.expect("tab bar");
        let expected = tab_bar_height();
        assert!(
            (bar.height() - expected).abs() < 0.05,
            "web tab {} != slim height {expected}",
            bar.height()
        );
        let dashboard = atlas_shell::tokens::current().topbar.height;
        assert!(
            bar.height() < dashboard * 0.5,
            "web tab {} should be slimmer than the dashboard bar {dashboard}",
            bar.height()
        );
        assert!(layout.body.top() >= bar.bottom() - 0.5);
        assert!(layout.reveal.is_none());
    }

    #[test]
    fn on_canvas_the_tab_bar_tracks_zoom() {
        let frame = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));
        let full = layout_portal_chrome(frame, false, false, 1.0);
        let half = layout_portal_chrome(frame, false, false, 0.5);
        let bar_full = full.bar.expect("tab");
        let bar_half = half.bar.expect("tab");
        assert!((bar_half.height() - bar_full.height() * 0.5).abs() < 0.05);
        assert!((half.radius - full.radius * 0.5).abs() < 0.05);
    }

    #[test]
    fn only_web_portals_have_an_identity_tab() {
        assert!(uses_identity_tab(PortalKind::Web));
        assert!(!uses_identity_tab(PortalKind::Agent));
        assert!(!uses_identity_tab(PortalKind::RepoLens));
        assert!(!uses_identity_tab(PortalKind::StatusBoard));
        let frame = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));
        let layout = layout_for_portal(PortalKind::Agent, frame, false, false, 1.0);
        assert!(layout.bar.is_none());
        assert!(layout.reveal.is_none());
        assert_eq!(layout.body, frame);
    }

    #[test]
    fn maximize_icon_sits_upper_right_without_a_tab() {
        let frame = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));
        let layout = layout_for_portal(PortalKind::StatusBoard, frame, false, false, 1.0);
        assert!(layout.bar.is_none());
        assert!(layout.maximize.right() <= frame.right() + 0.01);
        assert!(layout.maximize.top() >= frame.top() - 0.01);
        assert!(layout.maximize.center().x > frame.center().x);
        assert!(layout.maximize.center().y < frame.center().y);
    }

    #[test]
    fn web_tab_hosts_maximize_in_the_window_slot() {
        let frame = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));
        let layout = layout_for_portal(PortalKind::Web, frame, false, false, 1.0);
        let bar = layout.bar.expect("web tab");
        assert!(
            layout.maximize.right() <= bar.right() - layout.radius + 0.5,
            "maximize must sit inside the fillet, not on the square corner"
        );
        assert!((layout.maximize.top() - bar.top()).abs() < 0.5);
        assert!(layout.maximize.center().x > bar.center().x);
        assert!(bar.contains(layout.maximize.center()));
    }

    #[test]
    fn maximized_layout_has_no_fillet() {
        let frame = Rect::from_min_max(pos2(0.0, 0.0), pos2(1920.0, 1080.0));
        let layout = layout_portal_chrome(frame, false, true, 1.0);
        assert_eq!(layout.radius, 0.0);
        assert_eq!(layout.frame, frame);
    }

    #[test]
    fn content_outline_stays_inside_the_fillet() {
        let frame = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 120.0));
        let r = 8.0;
        for p in board::rounded_rect_outline(frame, r) {
            assert!(frame.contains(p), "outline point {p:?} left the frame");
        }
        let corner = pos2(0.0, 120.0);
        let inset = pos2(r, 120.0 - r);
        let outline = board::rounded_rect_outline(frame, r);
        assert!(
            !point_in_convex(&outline, corner),
            "the square corner must sit outside the fillet"
        );
        assert!(
            point_in_convex(&outline, inset),
            "the fillet center must stay inside"
        );
        let overhangs = board::fillet_overhangs(frame, r);
        assert!(
            overhangs.iter().any(|poly| poly.first() == Some(&corner)),
            "the square corner must be the outer vertex of a fillet mask"
        );
        assert!(
            !overhangs
                .iter()
                .any(|poly| poly.iter().any(|p| (*p - inset).length() < 0.5)),
            "the fillet center is content, not a mask vertex"
        );

        let page = Rect::from_min_max(pos2(6.0, 20.0), pos2(194.0, 114.0));
        let content = board::portal_content_outline(frame, page, r);
        assert!(
            content.iter().all(|p| page.contains(*p)),
            "content mesh must stay inside the UV rect, got {content:?}"
        );
        let bottom_corner = pos2(page.left(), page.bottom());
        assert!(
            !point_in_convex(&content, bottom_corner),
            "the inset page corner must still be clipped"
        );
    }
}

fn point_in_convex(poly: &[Pos2], p: Pos2) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut sign = 0.0f32;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cross.abs() < 1e-3 {
            continue;
        }
        let s = cross.signum();
        if sign == 0.0 {
            sign = s;
        } else if s != sign {
            return false;
        }
    }
    true
}
