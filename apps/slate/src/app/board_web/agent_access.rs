//! Narrow collaborator adapters over the existing WebHost and page-read owner.
use super::*;
use slate_doc::scene::WorldRect;

impl SlateApp {
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
            return Err("That origin has not been allowed for this workbook; ask the human to allow it first".into());
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
                return Err("The Web portal origin has not been allowed for this workbook".into());
            }
        }
        if let Some(origin) = self
            .web
            .host
            .current_url(id)
            .and_then(|url| web_origin(&url))
        {
            if !self.web.has_consent(&origin) {
                return Err("The current page origin is not allowed for this workbook".into());
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
        let (width, height) = css_size(&web, node.rect);
        if x < 0.0 || y < 0.0 || x > width as f32 || y > height as f32 {
            return Err("Input point is outside the page".into());
        }
        let point = node.rect.rotate_point(
            [
                node.rect.x + node.rect.w * x / width as f32,
                node.rect.y + node.rect.h * y / height as f32,
            ],
            node.rotation_deg,
        );
        Ok((f64::from(point[0]), f64::from(point[1])))
    }

    pub(crate) fn agent_web_input(&mut self, id: NodeId, input: WebInput) -> Result<(), String> {
        let portal = self.require_agent_web(id)?;
        let source = portal.source.as_ref().ok_or("Web portal has no source")?;
        if let Some(origin) = web_origin(&source.locator) {
            if !self.web.has_consent(&origin) {
                return Err("The portal origin is no longer allowed".into());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{only_portal, web_board, with_fake_host};

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
