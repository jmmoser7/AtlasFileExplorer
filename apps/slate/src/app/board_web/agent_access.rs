//! Narrow collaborator adapters over the existing WebHost and page-read owner.
use super::*;
use slate_doc::scene::WorldRect;

impl SlateApp {
    /// Change only the local grant for this portal's remote origin.
    /// Revocation also closes the page so the denied origin stops running.
    pub(crate) fn set_web_portal_consent(&mut self, id: NodeId, allow: bool) -> Option<String> {
        let node = self.doc().scene.node(id)?;
        let NodeKind::Portal(portal) = &node.kind else {
            return None;
        };
        let origin = web_origin(&portal.source.as_ref()?.locator)?;
        if allow {
            self.web.grant_consent(origin.clone());
        } else {
            self.web.revoke_consent(&origin);
            self.release_web_host(id);
        }
        Some(origin)
    }

    /// Human command palette action for revoking the selected Web portal.
    pub(crate) fn web_revoke_selected_origin(&mut self) -> bool {
        let Some(id) = self.selected_web_portal() else {
            return false;
        };
        match self.set_web_portal_consent(id, false) {
            Some(origin) => {
                self.toast(format!("{origin} blocked for this workbook"));
                true
            }
            None => {
                self.toast("Local pages have no website permission to revoke");
                false
            }
        }
    }

    pub(crate) fn web_consent_command(&mut self, command: &str) -> bool {
        match command {
            "portal.web.allow_origin" => self.web_allow_selected_origin(),
            "portal.web.revoke_origin" => self.web_revoke_selected_origin(),
            _ => false,
        }
    }

    pub(crate) fn web_source_command(&mut self, detail: Option<&str>) -> bool {
        match detail.map(str::trim).filter(|source| !source.is_empty()) {
            Some(source) => self.web_bind_source_for_selection(source),
            None => self.web_pick_source_for_selection(),
        }
    }

    pub(crate) fn build_bound_web(&self, rect: WorldRect, path: &Path) -> Node {
        let locator = web_source_locator(self.tab().path.as_deref(), path);
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Web portal");
        let portal = PortalNode::bound_web(title, locator);
        let mut prepared = self.doc().scene.node_preparation();
        prepared.build_node(rect, NodeKind::Portal(portal))
    }

    pub(crate) fn agent_web_text(&mut self, id: NodeId) -> Result<Option<String>, String> {
        self.require_agent_web(id)?;
        self.read_web_text(id)
    }

    pub(crate) fn agent_web_capture(
        &mut self,
        id: NodeId,
        dir: &Path,
    ) -> Result<Option<PathBuf>, String> {
        self.require_agent_web(id)?;
        self.capture_web_page(id, dir)
    }

    pub(crate) fn agent_web_navigate(&mut self, id: NodeId, target: &str) -> Result<(), String> {
        self.require_agent_web(id)?;
        let origin = web_origin(target).ok_or("Navigation requires a web URL")?;
        if !self.web.has_consent(&origin) {
            return Err(web_consent_denial(id, &origin));
        }
        if !self.web.host.navigate(id, target) {
            return Err("The portal has no live page to navigate".into());
        }
        Ok(())
    }

    pub(crate) fn agent_web_check_scope(&self, id: NodeId) -> Result<(), String> {
        let portal = self.require_agent_web(id)?;
        if !portal.web_ref().interactive_allowed {
            return Err("Web portal interaction is disabled for this page".into());
        }
        let source = portal.source.as_ref().ok_or("Web portal has no source")?;
        if let Some(origin) = web_origin(&source.locator) {
            if !self.web.has_consent(&origin) {
                return Err(web_consent_denial(id, &origin));
            }
        }
        if let Some(origin) = self
            .web
            .host
            .current_url(id)
            .and_then(|url| web_origin(&url))
        {
            if !self.web.has_consent(&origin) {
                return Err(web_consent_denial(id, &origin));
            }
        }
        Ok(())
    }

    pub(crate) fn agent_web_needs_file_grant(&self, id: NodeId) -> Result<bool, String> {
        let portal = self.require_agent_web(id)?;
        Ok(portal
            .source
            .as_ref()
            .is_none_or(|source| web_origin(&source.locator).is_none()))
    }

    pub(crate) fn agent_web_contact(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        x: f32,
        y: f32,
    ) -> Result<(f64, f64), String> {
        let node = self
            .doc()
            .scene
            .node(id)
            .ok_or("Web portal no longer exists")?;
        let NodeKind::Portal(portal) = &node.kind else {
            return Err("Target is not a Web portal".into());
        };
        if portal.kind != PortalKind::Web {
            return Err("Target is not a Web portal".into());
        }
        let web = portal.web_ref();
        let maximized = self.portal_chrome.maximized == Some(id);
        let css_rect = self.web_layout_world(id, node.rect, ctx);
        let (width, height) = css_size(&web, css_rect);
        if x < 0.0 || y < 0.0 || x >= width as f32 || y >= height as f32 {
            return Err("Input point is outside the page".into());
        }
        let xf = self.board_xf();
        let frame = if maximized {
            ctx.screen_rect()
        } else {
            xf.rect_w2s(node.rect)
        };
        let mut layout = super::super::board_portal_chrome::layout_portal_chrome(
            frame,
            self.portal_chrome_collapsed(id),
            maximized,
            self.node_resolved_corner(node),
            xf.z,
        );
        layout.retract_when_idle(ctx, id, self.web.focused == Some(id));
        // The Web body and human input use the unrotated screen frame; only
        // the board hit shape is rotated, so do not rotate this contact point.
        let point = super::input::screen_point_for_css(&layout, width as f32, height as f32, x, y);
        if !super::page_owns_point(&layout, point) {
            return Err("Input point is covered by Web portal chrome".into());
        }
        let world = xf.s2w(point);
        if self.board_pick_node_with_locked(world.x, world.y, true) != Some(id) {
            return Err("Input point is covered by another board object".into());
        }
        Ok((f64::from(world.x), f64::from(world.y)))
    }

    pub(crate) fn agent_web_input(&mut self, id: NodeId, input: WebInput) -> Result<(), String> {
        let portal = self.require_agent_web(id)?;
        let source = portal.source.as_ref().ok_or("Web portal has no source")?;
        if let Some(origin) = web_origin(&source.locator) {
            if !self.web.has_consent(&origin) {
                return Err(web_consent_denial(id, &origin));
            }
        }
        if self.web.focused != Some(id)
            || !self
                .web
                .agent_focus_lease
                .as_ref()
                .is_some_and(|(owner, _)| *owner == id)
            || !self.web.host.available()
        {
            return Err("The agent no longer owns focus on this live Web portal".into());
        }
        self.web.host.send_input(id, input);
        Ok(())
    }

    /// Acquire the existing Web input slot without taking board contents or
    /// OS focus. A human-owned contents slot always wins.
    pub(crate) fn agent_web_focus(&mut self, id: NodeId, request: &str) -> Result<(), String> {
        self.agent_web_check_scope(id)?;
        if let Some(owner) = self.portals.contents {
            return Err(format!("Portal contents are in human use ({})", owner.0));
        }
        if !self.web.host.available() {
            return Err("The WebView2 runtime is unavailable on this machine".into());
        }
        match self.web.agent_focus_lease.as_ref() {
            Some((owner, held_request)) if *owner == id && held_request == request => {}
            Some(_) => return Err("Another agent request owns Web portal focus".into()),
            None if self.web.focused.is_some() => {
                return Err("A human already owns Web input focus".into());
            }
            None => {
                self.web.agent_focus_lease = Some((id, request.to_owned()));
                self.web.focused = Some(id);
                self.web.reset_page_input();
                if let Some(view) = self.web.views.get_mut(&id) {
                    view.last_focus = Some(Instant::now());
                }
            }
        }
        Ok(())
    }

    /// Release only the lease owned by this request. A human takeover clears
    /// the lease first and is therefore never blurred here.
    pub(crate) fn agent_web_release_focus(&mut self, request: &str) -> bool {
        let Some((id, owner)) = self.web.agent_focus_lease.as_ref() else {
            return false;
        };
        if owner != request {
            return false;
        }
        let id = *id;
        self.web.agent_focus_lease = None;
        if self.web.focused == Some(id) && self.portals.contents.is_none() {
            self.web.focused = None;
            self.web.reset_page_input();
            self.web.host.release_keyboard();
        }
        true
    }

    fn require_agent_web(&self, id: NodeId) -> Result<PortalNode, String> {
        let node = self
            .doc()
            .scene
            .node(id)
            .ok_or("Web portal is no longer on this board")?;
        match &node.kind {
            NodeKind::Portal(portal) if portal.kind == PortalKind::Web => Ok(portal.clone()),
            _ => Err("Target is not a Web portal".into()),
        }
    }
}

fn web_consent_denial(id: NodeId, origin: &str) -> String {
    format!(
        "Web portal {id:?} needs human consent for {origin}. Click Allow site on that portal's tab; provider Full access is a separate grant and does not allow this page."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{only_portal, web_board, with_fake_host};

    #[test]
    fn agent_web_denial_points_to_human_portal_consent_without_granting_it() {
        let mut h = web_board("agent_web_consent_denial");
        let _host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
        let (id, _) = only_portal(&h);
        // Pasting a URL is itself human consent. Model a reopened/unapproved
        // portal explicitly before checking the agent denial path.
        assert_eq!(
            h.app.set_web_portal_consent(id, false).as_deref(),
            Some("https://example.com")
        );
        assert!(!h.app.web.has_consent("https://example.com"));

        let error = h.app.agent_web_check_scope(id).unwrap_err();

        assert!(error.contains("https://example.com"));
        assert!(error.contains(&format!("{id:?}")));
        assert!(error.contains("human consent"));
        assert!(error.contains("Full access is a separate grant"));
        assert!(error.contains("Click Allow site on that portal's tab"));
        assert!(!h.app.web.has_consent("https://example.com"));
    }

    #[test]
    fn selected_portal_consent_commands_allow_and_revoke_without_scene_mutations() {
        let mut h = web_board("agent_web_consent_controls");
        let _host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
        let (id, _) = only_portal(&h);
        h.app.board_sel.insert(id);
        let journal_len = h.app.tab().journal.undo_depth();

        assert!(h.app.web_allow_selected_origin());
        assert!(h.app.web.has_consent("https://example.com"));
        assert!(h.app.web_revoke_selected_origin());
        assert!(!h.app.web.has_consent("https://example.com"));
        assert_eq!(h.app.tab().journal.undo_depth(), journal_len);
        assert!(matches!(
            h.app.doc().scene.node(id).map(|node| &node.kind),
            Some(NodeKind::Portal(_))
        ));
    }

    #[test]
    fn agent_focus_is_visible_cancelable_and_yields_to_human_contents_focus() {
        let mut h = web_board("agent_web_focus_lease");
        let host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
        let (id, _) = only_portal(&h);
        h.app.web_allow_origin(id);
        h.frame();

        h.app.agent_web_focus(id, "request-a").unwrap();
        assert_eq!(h.app.web.focused, Some(id));
        assert_eq!(
            h.app.portals.contents, None,
            "agent focus does not enter contents"
        );
        assert_eq!(
            h.app.web.agent_focus_lease,
            Some((id, "request-a".to_owned()))
        );
        h.app.agent_web_input(id, WebInput::Text('x')).unwrap();
        assert!(host
            .inputs()
            .iter()
            .any(|event| matches!(event, WebInput::Text('x'))));

        // A human entering the same portal takes the existing focus path and
        // clears the agent's ownership marker.
        h.app.web_focus(id);
        assert_eq!(h.app.portals.contents, Some(id));
        assert_eq!(h.app.web.agent_focus_lease, None);
        assert!(!h.app.agent_web_release_focus("request-a"));
        assert_eq!(
            h.app.web.focused,
            Some(id),
            "release cannot blur human focus"
        );
        assert!(h.app.agent_web_input(id, WebInput::Text('y')).is_err());
    }

    #[test]
    fn agent_focus_does_not_displace_human_or_other_request() {
        let mut h = web_board("agent_web_focus_busy");
        let _host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
        let (id, _) = only_portal(&h);
        h.app.web_allow_origin(id);
        h.frame();

        h.app.web_focus(id);
        assert!(h.app.agent_web_focus(id, "request-a").is_err());
        h.app.web_blur();
        h.app.agent_web_focus(id, "request-a").unwrap();
        assert!(h.app.agent_web_focus(id, "request-b").is_err());
        assert!(h.app.agent_web_release_focus("request-a"));
        assert_eq!(h.app.web.focused, None);
    }

    #[test]
    fn agent_contact_matches_human_css_mapping_on_rotated_portal() {
        let mut h = web_board("agent_web_contact_rotated");
        let host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
        let (id, _) = only_portal(&h);
        h.app.web_allow_origin(id);
        h.frame();
        h.app.web_focus(id);
        h.app.doc_mut().scene.node_mut(id).unwrap().rotation_deg = 37.0;

        let node = h.app.doc().scene.node(id).unwrap();
        let web = match &node.kind {
            NodeKind::Portal(portal) => portal.web_ref(),
            _ => unreachable!(),
        };
        let (width, height) = css_size(&web, node.rect);
        let xf = h.app.board_xf();
        let layout = super::super::super::board_portal_chrome::layout_portal_chrome(
            xf.rect_w2s(node.rect),
            h.app.portal_chrome_collapsed(id),
            false,
            h.app.node_resolved_corner(node),
            xf.z,
        );
        let css = egui::vec2(width as f32 * 0.35, height as f32 * 0.72);
        let screen = super::super::input::screen_point_for_css(
            &layout,
            width as f32,
            height as f32,
            css.x,
            css.y,
        );
        assert!(super::page_owns_point(&layout, screen));
        let expected = xf.s2w(screen);
        assert_eq!(
            h.app
                .board_pick_node_with_locked(expected.x, expected.y, true),
            Some(id)
        );

        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(screen)));
        assert!(host.inputs().iter().any(|event| matches!(
            event,
            WebInput::Move { x, y, .. }
                if (*x - css.x).abs() < 0.1 && (*y - css.y).abs() < 0.1
        )));
        let actual = h.app.agent_web_contact(&h.ctx, id, css.x, css.y).unwrap();
        assert!((actual.0 - f64::from(expected.x)).abs() < 0.01);
        assert!((actual.1 - f64::from(expected.y)).abs() < 0.01);
    }

    #[test]
    fn agent_contact_rejects_the_overlaid_identity_tab() {
        let mut h = web_board("agent_web_contact_chrome");
        let _host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
        let (id, _) = only_portal(&h);
        h.app.web_allow_origin(id);
        h.frame();

        let node = h.app.doc().scene.node(id).unwrap();
        let web = match &node.kind {
            NodeKind::Portal(portal) => portal.web_ref(),
            _ => unreachable!(),
        };
        let (width, height) = css_size(&web, node.rect);
        let xf = h.app.board_xf();
        let layout = super::super::super::board_portal_chrome::layout_portal_chrome(
            xf.rect_w2s(node.rect),
            h.app.portal_chrome_collapsed(id),
            false,
            h.app.node_resolved_corner(node),
            xf.z,
        );
        let bar = layout.bar.expect("Web portal identity tab").center();
        let (css_x, css_y) =
            super::super::input::css_point_for_screen(&layout, width as f32, height as f32, bar);
        assert!(!super::page_owns_point(&layout, bar));
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(bar)));
        let error = h
            .app
            .agent_web_contact(&h.ctx, id, css_x, css_y)
            .unwrap_err();
        assert!(error.contains("covered by Web portal chrome"));
    }

    #[test]
    fn agent_focus_uses_existing_admission_for_offscreen_page_without_panning() {
        let mut h = web_board("agent_web_focus_offscreen");
        let host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
        let (id, _) = only_portal(&h);
        h.app.web_allow_origin(id);
        h.frame();
        h.app.doc_mut().scene.node_mut(id).unwrap().rect.x = 100_000.0;
        h.frame();
        // Visibility recorded by the preceding board paint feeds the next
        // admission pass. Let that cached on-screen observation expire before
        // asserting that the portal is genuinely offscreen.
        h.frame();
        assert!(!h.app.web.is_live(id));
        let camera = h.app.tab().cam;

        h.app.agent_web_focus(id, "request-a").unwrap();
        h.frame();

        assert!(h.app.web.is_live(id));
        assert!(host.admit_targets().iter().any(|(portal, _)| *portal == id));
        assert_eq!(h.app.tab().cam.offset, camera.offset);
        assert_eq!(h.app.tab().cam.z, camera.z);
    }
}
