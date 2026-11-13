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

    /// Whole-board inspection is reserved for explicit commands and tests.
    #[cfg(test)]
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

    pub(super) fn web_pump_portals(&self) -> Vec<(NodeId, PortalNode, slate_doc::WorldRect)> {
        let mut demanded = self.web.painted.clone();
        let view = (!self.at_home
            && self.presenting.is_none()
            && self.doc().view.active_view == slate_doc::ViewKind::Board)
            .then(|| self.board_paint_view(self.canvas_rect).normalized());
        demanded.extend(self.web.host_open.iter().copied());
        demanded.extend(self.web.focused);
        demanded.extend(self.portal_chrome.maximized);
        demanded.extend(self.web.agent_focus_lease.as_ref().map(|(id, _)| *id));
        demanded.extend(self.web.text_pending.iter().copied());
        if self.board_sel.len() == 1 {
            demanded.extend(self.board_sel.iter().copied());
        }
        self.web
            .placements
            .iter()
            .filter_map(|&(id, bounds)| {
                let in_view = view
                    .zip(bounds)
                    .is_some_and(|(view, bounds)| bounds.intersects(&view));
                if !in_view && !demanded.contains(&id) {
                    return None;
                }
                let (node, portal) = self.web_node(id)?;
                Some((
                    id,
                    self.effective_web_portal(id, portal).into_owned(),
                    node.rect,
                ))
            })
            .collect()
    }

    pub(super) fn prune_web_views(&mut self) {
        let revision = (self.tab().id, self.scene_gen, self.doc().scene.scene_gen());
        if self.web.scene_revision == Some(revision) {
            return;
        }
        self.web.scene_revision = Some(revision);
        self.web.placements = self
            .web_nodes()
            .map(|(node, _)| (node.id, (!node.hidden).then(|| slate_doc::node_aabb(node))))
            .collect();
        let authored: HashSet<_> = self.web.placements.iter().map(|(id, _)| *id).collect();
        let dropped: Vec<_> = self
            .web
            .views
            .keys()
            .copied()
            .filter(|id| !authored.contains(id))
            .collect();
        for id in dropped {
            if self.web.host_open.contains(&id) {
                self.web.host.evict(id);
                self.web.note_host(id, false);
            }
            self.web.views.remove(&id);
            self.web.text_pending.remove(&id);
        }
        if self.web.focused.is_some_and(|id| !authored.contains(&id)) {
            self.web_blur();
        }
    }

    fn web_node(&self, id: NodeId) -> Option<(&Node, &PortalNode)> {
        let node = self.doc().scene.node(id)?;
        let NodeKind::Portal(portal) = &node.kind else {
            return None;
        };
        (portal.kind == PortalKind::Web).then_some((node, portal))
    }

    pub(super) fn refresh_web_view_for_node(&mut self, id: NodeId) {
        if let Some((_, portal)) = self.web_node(id) {
            let portal = self.effective_web_portal(id, portal).into_owned();
            self.refresh_web_view(id, &portal);
        }
    }

    pub(super) fn refresh_web_view(&mut self, id: NodeId, portal: &PortalNode) -> bool {
        let key = view_key(portal, &portal.web_ref());
        if self.web.views.get(&id).is_some_and(|view| view.key == key) {
            return false;
        }
        if self.web.focused == Some(id) {
            self.web.reset_page_input();
        }
        if self.web.host_open.contains(&id) {
            self.remember_web_visit(id);
            self.web.host.evict(id);
            self.web.note_host(id, false);
        }
        let locator = portal
            .source
            .as_ref()
            .map_or("", |source| source.locator.as_str());
        let mut view = WebView::new(key);
        view.still_key = still_key_for(locator);
        if let Some(old) = self.web.views.remove(&id) {
            view.width_px = old.width_px;
            view.height_px = old.height_px;
            view.area_px = old.area_px;
            view.anchor_px = old.anchor_px;
            view.on_screen = old.on_screen;
            view.was_on_screen = old.was_on_screen;
            // A viewport change keeps the current source's last good image.
            if old.still_key == view.still_key {
                view.resume_url = old.resume_url;
                view.poster = old.poster;
                view.poster_at = old.poster_at;
                view.poster_wall = old.poster_wall;
                view.poster_bytes = old.poster_bytes;
                view.poster_size = old.poster_size;
                view.poster_fresh = old.poster_fresh;
                view.cached_text = old.cached_text;
                view.cached_text_at = old.cached_text_at;
                view.text_miss = old.text_miss;
            }
        }
        self.web.views.insert(id, view);
        true
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
        self.paint_artifact_result_strip(ui, node.id, layout.frame, xf.z);
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
        if self.refresh_web_view(node.id, portal) {
            ui.ctx().request_repaint();
        }
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
        if self.note_web_geometry(
            node.id,
            layout.frame,
            ui.clip_rect(),
            ui.ctx().pixels_per_point(),
        ) {
            ui.ctx().request_repaint();
        }

        self.paint_portal_frame_fill(painter, layout, fade(rgba32(portal.fill)));

        let body = layout.body;
        if body.width() < 2.0 || body.height() < 2.0 {
            return;
        }
        let clip = body.intersect(layout.frame);
        let artifact = slate_doc::scene::artifact(node).is_some();
        let working = self.artifact_generation_working(node.id);
        let generating = artifact && (working.is_some() || self.generations_waiting(node.id) > 0);
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
        } else if !generating || !matches!(state, WebState::Unbound) {
            let clipped = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
            self.paint_web_empty_state(&clipped, clip, node.id, &state, alpha, zoom);
        }
        if artifact {
            self.paint_run_progress_with_state(
                ui,
                &painter.with_clip_rect(clip.intersect(painter.clip_rect())),
                node.id,
                clip,
                zoom,
                working.unwrap_or("Building dashboard"),
                false,
                working.is_some(),
            );
        }
    }

    /// Only a blocked page needs an inline Allow action. Existing revocation
    /// and policy commands live in the Web context menu and Website access.
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
        if self.web.has_consent(&origin) {
            return;
        }
        let label = "Allow site";
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
            .on_hover_text(format!(
                "Allow {origin} for this workbook. This is separate from agent Full access."
            ));
        if response.clicked() {
            self.board_sel.clear();
            self.board_sel.insert(id);
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("portal.web.allow_origin"),
                None,
            );
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
            WebState::Unbound if self.is_artifact_node(id) && self.agent_failure_reason(id).is_some() => (
                "Artifact generation failed".to_string(),
                self.agent_failure_reason(id).unwrap().to_owned(),
            ),
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
mod tests;
