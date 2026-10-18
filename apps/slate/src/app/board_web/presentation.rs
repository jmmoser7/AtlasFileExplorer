//! One Web-host presentation, including derived artifact previews.
use super::*;

impl SlateApp {
    /// Pending generation previews change the browser input, never the scene.
    pub(super) fn effective_web_portal<'a>(
        &self,
        id: NodeId,
        portal: &'a PortalNode,
    ) -> std::borrow::Cow<'a, PortalNode> {
        match self.artifact_preview_source(id) {
            Some(source) if portal.source.as_ref() != Some(&source) => {
                let mut preview = portal.clone();
                preview.source = Some(source);
                std::borrow::Cow::Owned(preview)
            }
            _ => std::borrow::Cow::Borrowed(portal),
        }
    }

    /// Every web portal on the board, with the node rect each one occupies.
    pub(super) fn web_portals(&self) -> Vec<(NodeId, PortalNode, slate_doc::scene::WorldRect)> {
        self.web_nodes()
            .map(|(node, portal)| {
                (
                    node.id,
                    self.effective_web_portal(node.id, portal).into_owned(),
                    node.rect,
                )
            })
            .collect()
    }

    pub(super) fn authored_web_portals(
        &self,
    ) -> Vec<(NodeId, PortalNode, slate_doc::scene::WorldRect)> {
        self.web_nodes()
            .map(|(node, portal)| (node.id, portal.clone(), node.rect))
            .collect()
    }

    fn web_nodes(&self) -> impl Iterator<Item = (&Node, &PortalNode)> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter_map(|node| match &node.kind {
                NodeKind::Portal(portal) if portal.kind == PortalKind::Web => Some((node, portal)),
                _ => None,
            })
    }

    /// `portal.web.open_external` — hand the locator to the system browser,
    /// which is the honest place for a page Slate is only framing.
    pub(crate) fn web_open_external(&mut self) -> bool {
        let Some(id) = self.selected_web_portal() else {
            return false;
        };
        self.web_open_external_for(id)
    }

    pub(crate) fn web_open_external_for(&mut self, id: NodeId) -> bool {
        let Some(Node {
            kind: NodeKind::Portal(portal),
            ..
        }) = self.doc().scene.node(id)
        else {
            return false;
        };
        let Some(locator) = self
            .effective_web_portal(id, portal)
            .source
            .as_ref()
            .map(|source| source.locator.clone())
        else {
            return false;
        };
        let workbook = self.tab().path.clone();
        let target = if web_origin(&locator).is_some() {
            locator
        } else {
            resolve_web_source(workbook.as_deref(), &locator)
                .to_string_lossy()
                .into_owned()
        };
        match open_externally(&target) {
            Ok(()) => true,
            Err(e) => {
                self.toast(format!("Could not open {target}: {e}"));
                false
            }
        }
    }

    pub(crate) fn paint_web_portal(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let effective = self.effective_web_portal(node.id, portal);
        let portal = effective.as_ref();
        let srect = xf.rect_w2s(node.rect);
        self.note_web_geometry(node.id, srect, ui.clip_rect(), ui.ctx().pixels_per_point());
        let collapsed = self.portal_chrome_collapsed(node.id);
        let mut layout = layout_portal_chrome(
            srect,
            collapsed,
            false,
            self.node_resolved_corner(node),
            xf.z,
        );
        layout.retract_when_idle(ui.ctx(), node.id, self.web.focused == Some(node.id));
        self.paint_web_portal_in_rect(ui, painter, node, portal, &layout, xf.z);
        let visiting = self.web_visiting_label(node.id, portal);
        let focused = self.web.focused == Some(node.id);
        let alpha = node.opacity.clamp(0.0, 1.0);
        let palette = self.palette();
        let border = if focused {
            palette.select
        } else {
            atlas_shell::theme::Palette::alpha(palette.sub, 150)
        };
        self.paint_portal_shell_finish(
            ui,
            painter,
            &layout,
            node.id,
            portal,
            visiting.as_deref(),
            border.gamma_multiply(alpha),
            focused,
            xf.z,
        );
    }

    pub(crate) fn paint_web_portal_in_rect(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        node: &Node,
        portal: &PortalNode,
        layout: &PortalChromeLayout,
        zoom: f32,
    ) {
        let effective = self.effective_web_portal(node.id, portal);
        let portal = effective.as_ref();
        let alpha = node.opacity.clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(alpha);
        let state = self.web.state(node.id);
        let css_rect = self.web_layout_world(node.id, node.rect, ui.ctx());
        let (css_width, _) = css_size(&portal.web_ref(), css_rect);
        let chrome_height = if self.portal_is_maximized(node.id) {
            atlas_shell::tokens::current().topbar.height
        } else {
            super::super::board_portal_chrome::tab_bar_height() * zoom
        };
        let scrollbar_css = atlas_shell::tabs::portal_scrollbar_width(chrome_height)
            * css_width as f32
            / layout.body.width().max(1.0);
        let scrollbar_color = self.palette().sub;
        self.web.host.set_scrollbars(
            node.id,
            layout.bar.is_some() || layout.reveal.is_some(),
            scrollbar_css,
            scrollbar_color,
        );
        self.note_web_geometry(
            node.id,
            layout.frame,
            ui.clip_rect(),
            ui.ctx().pixels_per_point(),
        );

        self.paint_portal_frame_fill(painter, layout, fade(rgba32(portal.fill)));

        let body = layout.body;
        if body.width() < 2.0 || body.height() < 2.0 {
            return;
        }
        let clip = body.intersect(layout.frame);
        let poster = self.web.views.get(&node.id).and_then(|v| v.poster.clone());
        if let Some(tex) = poster {
            // A resource-saving still frame is the page, not a disabled slide.
            let stale = matches!(state, WebState::Missing { .. } | WebState::Failed { .. });
            let tint =
                Color32::WHITE.gamma_multiply(if stale { STALE_ALPHA * alpha } else { alpha });
            let outline = board::portal_content_outline(layout.frame, clip, layout.corner, zoom);
            if !outline.is_empty() {
                board::textured_polygon(
                    painter,
                    &tex,
                    &outline,
                    clip,
                    slate_doc::scene::Crop::full(),
                    tint,
                );
            }
        } else {
            let clipped = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
            self.paint_web_empty_state(&clipped, clip, node.id, &state, alpha, zoom);
        }
    }

    /// Every failure says itself in the frame rather than blanking it (D30).
    fn paint_web_empty_state(
        &self,
        painter: &egui::Painter,
        body: Rect,
        id: NodeId,
        state: &WebState,
        alpha: f32,
        zoom: f32,
    ) {
        let fade = |c: Color32| c.gamma_multiply(alpha);
        let (headline, detail) = match state {
            WebState::Unbound if self.is_artifact_node(id) => (
                "Interactive artifact".to_string(),
                "Describe your dashboard in the generator below.".to_string(),
            ),
            WebState::Unbound => (
                "Choose page or file…".to_string(),
                "URL, an .html file, or a folder with index.html".to_string(),
            ),
            WebState::Blocked { origin } => (
                format!("{origin} is not permitted yet"),
                "Allow this origin in the Portal inspector".into(),
            ),
            WebState::Missing { locator } => ("Source is missing".to_string(), locator.clone()),
            WebState::Failed { reason } => ("Page could not load".to_string(), reason.clone()),
            WebState::Refused { reason } => ("Refused".to_string(), reason.clone()),
            WebState::NoRuntime => (
                "No WebView2 runtime".to_string(),
                "Install the Microsoft Edge WebView2 Evergreen runtime".into(),
            ),
            WebState::TooSmall => ("Zoom in to run this page".to_string(), String::new()),
            WebState::Budgeted => (
                "Waiting for a live slot".to_string(),
                format!("{LIVE_POOL} pages run at once"),
            ),
            WebState::Loading => ("Loading…".to_string(), String::new()),
            _ => ("Resolving…".to_string(), String::new()),
        };
        let head = atlas_shell::canvas_scale::px(15.0, zoom);
        if atlas_shell::canvas_text::legible(head) {
            atlas_shell::canvas_text::text(
                painter,
                body.center() - egui::vec2(0.0, atlas_shell::canvas_scale::px(10.0, zoom)),
                Align2::CENTER_CENTER,
                headline,
                FontId::proportional(head),
                fade(self.palette().ink.gamma_multiply(0.85)),
            );
        }
        if !detail.is_empty() {
            let sub = atlas_shell::canvas_scale::px(12.0, zoom);
            if atlas_shell::canvas_text::legible(sub) {
                atlas_shell::canvas_text::text(
                    painter,
                    body.center() + egui::vec2(0.0, atlas_shell::canvas_scale::px(12.0, zoom)),
                    Align2::CENTER_CENTER,
                    detail,
                    FontId::proportional(sub),
                    fade(self.palette().sub),
                );
            }
        }
    }
}
