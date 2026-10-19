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
        if !self.web_consent_pending(portal) {
            layout.retract_when_idle(ui.ctx(), node.id, self.web.focused == Some(node.id));
        }
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
        self.paint_web_consent_control(ui, &layout, node.id, portal);
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

    /// A compact consent action in the Web identity tab keeps the human's
    /// origin decision on the portal that needs it (D35).
    pub(crate) fn paint_web_consent_control(
        &mut self,
        ui: &egui::Ui,
        layout: &PortalChromeLayout,
        id: NodeId,
        portal: &PortalNode,
    ) {
        let Some(rect) = Self::web_consent_control_rect(layout) else {
            return;
        };
        let Some(origin) = portal
            .source
            .as_ref()
            .and_then(|source| web_origin(&source.locator))
        else {
            return;
        };
        let allowed = self.web.has_consent(&origin);
        let all_sites = self.web.allows_all_websites();
        let label = if all_sites {
            "All sites on"
        } else if allowed {
            "Revoke site"
        } else {
            "Allow site"
        };
        let mut control_ui = egui::Ui::new(
            ui.ctx().clone(),
            ui.id().with(("web_consent", id.0)),
            egui::UiBuilder::new()
                .layer_id(ui.layer_id())
                .max_rect(rect),
        );
        control_ui.set_style(ui.style().clone());
        control_ui.set_clip_rect(ui.clip_rect());
        let ui = &mut control_ui;
        ui.spacing_mut().button_padding *= layout.zoom;
        ui.spacing_mut().interact_size *= layout.zoom;
        let response = ui
            .put(
                rect,
                egui::Button::new(egui::RichText::new(label).size(11.0 * layout.zoom)),
            )
            .on_hover_text(if all_sites {
                "All websites are allowed for this user across all Slate workbooks on this device. Click to change Website access settings.".into()
            } else { format!(
                "{} {origin} for this workbook. This is separate from agent Full access.",
                if allowed { "Block" } else { "Allow" }
            ) });
        if response.clicked() {
            if all_sites {
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.web.access_settings"),
                    None,
                );
                return;
            }
            self.board_sel.clear();
            self.board_sel.insert(id);
            let command = if allowed {
                "portal.web.revoke_origin"
            } else {
                "portal.web.allow_origin"
            };
            self.dispatch(ui.ctx(), atlas_commands::CommandId(command), None);
        }
    }

    pub(crate) fn web_consent_pending(&self, portal: &PortalNode) -> bool {
        portal
            .source
            .as_ref()
            .and_then(|source| web_origin(&source.locator))
            .is_some_and(|origin| !self.web.has_consent(&origin))
    }

    fn web_consent_control_rect(layout: &PortalChromeLayout) -> Option<Rect> {
        let bar = layout.bar?;
        let size = egui::vec2(
            (76.0 * layout.zoom).min(bar.width() * 0.24),
            (bar.height() - 4.0 * layout.zoom).max(0.0),
        );
        Some(Rect::from_min_size(
            egui::pos2(
                layout.maximize.left() - size.x - 4.0 * layout.zoom,
                bar.top() + 2.0 * layout.zoom,
            ),
            size,
        ))
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
                "Click Allow site on this tab, or choose Allow all websites in Agent settings → Website access. Website access is separate from agent Full access."
                    .into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{only_portal, web_board, with_fake_host};

    #[test]
    fn portal_tab_control_allows_and_revokes_only_after_human_clicks() {
        let mut h = web_board("web_portal_local_consent");
        let _host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
        let (id, _) = only_portal(&h);
        h.app.set_web_portal_consent(id, false);
        h.app.board_sel.insert(id);
        h.frame_with(|_| {});
        let node = h.app.doc().scene.node(id).unwrap();
        let xf = h.app.board_xf();
        let layout = super::super::super::board_portal_chrome::layout_portal_chrome(
            xf.rect_w2s(node.rect),
            false,
            false,
            h.app.node_resolved_corner(node),
            xf.z,
        );
        let point = SlateApp::web_consent_control_rect(&layout)
            .unwrap()
            .center();
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(point)));

        for allowed in [true, false] {
            for pressed in [true, false] {
                h.frame_with(|i| {
                    i.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    })
                });
            }
            assert_eq!(h.app.web.has_consent("https://example.com"), allowed);
        }
    }

    #[test]
    fn maximized_denied_web_portal_keeps_its_consent_control_available() {
        let mut h = web_board("web_maximized_pending_consent");
        let _host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
        let (id, _) = only_portal(&h);
        h.app.set_web_portal_consent(id, false);
        h.app.web_maximize(id);
        h.frame_with(|_| {});

        let layout = super::super::super::board_portal_chrome::layout_for_portal(
            PortalKind::Web,
            h.app.canvas_rect,
            false,
            true,
            slate_doc::scene::Corner::Square,
            1.0,
        );
        let point = SlateApp::web_consent_control_rect(&layout)
            .expect("maximized web chrome has a consent slot")
            .center();
        for pressed in [true, false] {
            h.frame_with(|i| {
                i.events.push(egui::Event::PointerMoved(point));
                i.events.push(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            });
        }
        assert!(h.app.web.has_consent("https://example.com"));
    }

    #[test]
    fn global_site_control_opens_settings_without_revoking_permission() {
        let mut h = web_board("web_global_policy_control");
        let _host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
        let (id, _) = only_portal(&h);
        h.app.set_global_web_consent(true);
        h.app.web_maximize(id);
        h.frame();
        let layout = super::super::super::board_portal_chrome::layout_for_portal(
            PortalKind::Web,
            h.app.canvas_rect,
            false,
            true,
            slate_doc::scene::Corner::Square,
            1.0,
        );
        let point = SlateApp::web_consent_control_rect(&layout)
            .unwrap()
            .center();
        for pressed in [true, false] {
            h.frame_with(|i| {
                i.events.push(egui::Event::PointerMoved(point));
                i.events.push(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            });
        }
        assert!(h.app.web.allows_all_websites());
        assert!(h
            .app
            .web
            .has_consent("https://previously-unvisited.example"));
        assert!(h.app.tab().chrome.advanced_open);
        assert!(
            !h.app.portal_is_maximized(id),
            "settings must be visible after restoring the portal"
        );
    }
}
