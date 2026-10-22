//! Narrow collaborator adapters over the existing WebHost and page-read owner.
use super::*;
use slate_doc::scene::WorldRect;

impl SlateApp {
    /// Change the local grant for this portal; revocation also closes the page.
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
        if self.web.allows_all_websites() {
            self.web_consent_command("portal.web.access_settings");
            self.toast("All websites are allowed. Turn off Allow all websites to use individual site permissions.");
            return true;
        }
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

    /// Diagnostic page identity only; navigation keeps the host's original URL.
    pub(crate) fn agent_web_current_url(&self, id: NodeId) -> Option<String> {
        let mut page = url::Url::parse(&self.web.host.current_url(id)?).ok()?;
        if !matches!(page.scheme(), "http" | "https" | "file") {
            return None;
        }
        if !page.username().is_empty() {
            page.set_username("").ok()?;
        }
        if page.password().is_some() {
            page.set_password(None).ok()?;
        }
        page.set_query(None);
        page.set_fragment(None);
        Some(page.into())
    }

    pub(crate) fn agent_web_capture(
        &mut self,
        id: NodeId,
        dir: &Path,
        capture_id: &str,
    ) -> Result<Option<PathBuf>, String> {
        self.require_agent_web(id)?;
        self.capture_web_page(id, dir, Some(capture_id))
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
        self.require_maximized_web_input_owner(id)?;
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
            if maximized { 1.0 } else { xf.z },
        );
        layout.retract_when_idle(ctx, id, self.web.focused == Some(id));
        // Page input uses the unrotated screen frame, not the rotated hit shape.
        let point = super::input::screen_point_for_css(&layout, width as f32, height as f32, x, y);
        if !super::page_owns_point(&layout, point) {
            return Err("Input point is covered by Web portal chrome".into());
        }
        let contact_xf = if maximized {
            super::super::board_portal_chrome::fit_xf(node.rect, layout.body)
        } else {
            xf
        };
        let world = contact_xf.s2w(point);
        if !maximized && self.board_pick_node_with_locked(world.x, world.y, true) != Some(id) {
            return Err("Input point is covered by another board object".into());
        }
        Ok((f64::from(world.x), f64::from(world.y)))
    }

    pub(crate) fn agent_web_input(&mut self, id: NodeId, input: WebInput) -> Result<(), String> {
        self.require_maximized_web_input_owner(id)?;
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

    /// Acquire Web input without taking board contents or OS focus.
    pub(crate) fn agent_web_focus(&mut self, id: NodeId, request: &str) -> Result<(), String> {
        self.require_maximized_web_input_owner(id)?;
        self.agent_web_check_scope(id)?;
        if let Some(owner) = self.portals.contents {
            return Err(format!("Portal contents are in human use ({})", owner.0));
        }
        if !self.web.host.available() {
            return Err("The WebView2 runtime is unavailable on this machine".into());
        }
        match self.web.agent_focus_lease.as_ref() {
            Some((owner, held_request)) if *owner == id && held_request == request => {}
            Some((_, held_request)) if held_request == request => {
                self.web.host.release_keyboard();
                self.web.reset_page_input();
                self.web.focused = Some(id);
                self.web.agent_focus_lease = Some((id, request.to_owned()));
                if let Some(view) = self.web.views.get_mut(&id) {
                    view.last_focus = Some(Instant::now());
                }
            }
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

    /// Release only this request's lease; human takeover clears it first.
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

    fn require_maximized_web_input_owner(&self, id: NodeId) -> Result<(), String> {
        match self.portal_chrome.maximized {
            Some(owner) if owner != id => Err("Another portal is maximized; restore the board or maximize this Web portal before sending page input".into()),
            _ => Ok(()),
        }
    }
}

fn web_consent_denial(id: NodeId, origin: &str) -> String {
    format!(
        "Web portal {id:?} needs human consent for {origin}. Click Allow site on that portal's tab, or choose Allow all websites in Agent settings → Website access. Provider Full access is a separate grant and does not allow this page."
    )
}

#[cfg(test)]
#[path = "agent_access_tests.rs"]
mod tests;
