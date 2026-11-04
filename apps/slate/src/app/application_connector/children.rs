//! Secondary application windows use the same portal, focus and native-surface owners.
use super::*;
use crate::app::board::BoardXf;
use slate_doc::application_surface::ApplicationSurface;
use slate_doc::scene::{Node, NodeKind, PortalKind, PortalNode};
use std::time::Instant;
mod launch;
mod launch_state;
pub(super) use launch::LaunchRequest;
pub(super) use launch_state::LaunchToken;

pub(super) struct ChildSurface {
    node: Option<NodeId>,
    descriptor: Option<SurfaceDescriptor>,
    process_id: u32,
    worker: Option<String>,
    native: Option<NativeSurface>,
    last_query: Option<Instant>,
    status: String,
    host: hosting::HostPresentation,
    launch: launch_state::LaunchState,
    handoff_ready: bool,
    launch_node: Option<(NodeId, String)>,
    waiting_launch: Option<LaunchRequest>,
}
impl Default for ChildSurface {
    fn default() -> Self {
        Self {
            node: None,
            descriptor: None,
            process_id: 0,
            worker: None,
            native: None,
            last_query: None,
            status: "Double-click to open application window".into(),
            host: Default::default(),
            launch: Default::default(),
            handoff_ready: false,
            launch_node: None,
            waiting_launch: None,
        }
    }
}
impl SlateApp {
    fn application_surface_ref(&self, node: NodeId) -> Option<ApplicationSurface> {
        let NodeKind::Portal(portal) = &self.doc().scene.node(node)?.kind else {
            return None;
        };
        ApplicationSurface::from_portal(portal)
    }
    pub(crate) fn application_surfaces_blur(&mut self) -> bool {
        let mut changed = false;
        for child in self.connectors.children.values_mut() {
            child.launch.cancel();
            child.handoff_ready = false;
            child.launch_node = None;
            child.waiting_launch = None;
            if let Some(mut surface) = child.native.take() {
                let _ = surface.park();
                let _ = surface.hide();
                let _ = surface.detach_hidden();
                changed = true;
            }
            child.host.parked();
        }
        changed
    }
    pub(super) fn register_application_surfaces(
        &mut self,
        tab: u64,
        parent: NodeId,
        pid: u32,
        surfaces: Vec<SurfaceDescriptor>,
    ) {
        if self.at_home || self.tab().id != tab {
            return;
        }
        let nodes: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| {
                let NodeKind::Portal(p) = &n.kind else {
                    return None;
                };
                let reference = ApplicationSurface::from_portal(p)?;
                (reference.parent == parent).then_some((n.id, reference))
            })
            .collect();
        for (node, reference) in nodes {
            if let Some(surface) = surfaces
                .iter()
                .find(|s| s.surface_id == reference.surface && s.native_handle.is_some())
            {
                let child = self
                    .connectors
                    .children
                    .entry((tab, parent, reference.surface))
                    .or_default();
                if child
                    .descriptor
                    .as_ref()
                    .and_then(|d| d.native_handle.as_ref())
                    != surface.native_handle.as_ref()
                {
                    if let Some(mut old) = child.native.take() {
                        let _ = old.detach_hidden();
                    }
                }
                child.node = Some(node);
                child.descriptor = Some(surface.clone());
                child.process_id = pid;
                child.worker = self
                    .connectors
                    .sessions
                    .get(&(tab, parent))
                    .and_then(|s| s.binding.as_ref())
                    .map(|b| b.worker.clone());
                child.status = "Native application window · Double-click to enter".into();
            } else if let Some(child) =
                self.connectors
                    .children
                    .get_mut(&(tab, parent, reference.surface))
            {
                if let Some(mut old) = child.native.take() {
                    let _ = old.detach_hidden();
                }
                child.descriptor = None;
                child.status = "Window closed · Double-click to open".into();
            }
        }
    }
    pub(super) fn maintain_application_surfaces(&mut self) {
        let active = (!self.at_home).then(|| self.tab().id);
        let focused = self.contents_focused();
        let live: std::collections::HashSet<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden)
            .map(|n| n.id)
            .collect();
        for ((tab, parent, _), child) in &mut self.connectors.children {
            let session = self.connectors.sessions.get(&(*tab, *parent));
            let epoch = session
                .and_then(|s| s.binding.as_ref())
                .map(|b| b.worker.clone());
            let source_alive = session.is_some_and(|s| !s.ended && s.enabled);
            if !source_alive || (child.worker.is_some() && child.worker != epoch) {
                child.launch.cancel();
                child.handoff_ready = false;
                child.launch_node = None;
                child.waiting_launch = None;
                if let Some(mut surface) = child.native.take() {
                    let _ = surface.detach();
                }
                child.descriptor = None;
                child.worker = None;
                child.last_query = None;
            }
            let visible = active == Some(*tab)
                && source_alive
                && child
                    .node
                    .is_some_and(|id| live.contains(&id) && focused == Some(id));
            if active != Some(*tab) || focused != Some(*parent) {
                child.launch.cancel();
                child.launch_node = None;
                child.waiting_launch = None;
            }
            if !visible {
                if let Some(mut surface) = child.native.take() {
                    let _ = surface.park();
                    let _ = surface.hide();
                    let _ = surface.detach_hidden();
                }
                child.host.parked();
            }
            if active == Some(*tab) && child.node.is_some_and(|n| !live.contains(&n)) {
                child.node = None;
            }
        }
    }
    pub(super) fn complete_application_surface_focus(
        &mut self,
        tab: u64,
        parent: NodeId,
        surface: &str,
        request: hosting::FocusRequest,
        result: Result<(), String>,
    ) {
        if let Some(child) = self
            .connectors
            .children
            .get_mut(&(tab, parent, surface.to_owned()))
        {
            if child.host.complete(request, result.is_ok()) {
                if let Err(error) = result {
                    child.status = format!("Application window focus: {error}");
                }
            }
        }
    }
    pub(crate) fn paint_application_surface(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let rect = xf.rect_w2s(node.rect);
        let layout = crate::app::board_portal_chrome::layout_for_portal(
            PortalKind::Application,
            rect,
            self.portal_chrome_collapsed(node.id),
            self.portal_is_maximized(node.id),
            self.node_resolved_corner(node),
            xf.z,
        );
        let fill = self.portal_frame_fill_color(portal);
        self.paint_portal_frame_fill(painter, &layout, fill);
        let Some(reference) = ApplicationSurface::from_portal(portal) else {
            return;
        };
        if self.doc().scene.node(node.id).is_none() {
            return;
        }
        let tab = self.tab().id;
        let key = (tab, reference.parent, reference.surface.clone());
        let focused = self.contents_focused() == Some(node.id);
        let live_worker = self
            .connectors
            .sessions
            .get(&(tab, reference.parent))
            .filter(|s| s.enabled && !s.ended)
            .and_then(|s| s.binding.as_ref())
            .map(|b| b.worker.clone());
        let child = self.connectors.children.entry(key).or_default();
        child.node = Some(node.id);
        let source_ready = live_worker.is_some();
        let trusted_surface = source_ready && child.worker == live_worker;
        let descriptor_valid = child
            .descriptor
            .as_ref()
            .is_some_and(|descriptor| descriptor.native_handle.is_some())
            && child
                .native
                .as_ref()
                .is_none_or(|surface| surface.handle().is_ok());
        let should_query = source_ready
            && !descriptor_valid
            && !child.launch.is_pending()
            && child.last_query.is_none();
        if should_query {
            if let Some(parent) = self.connectors.sessions.get(&(tab, reference.parent)) {
                if parent
                    .tx
                    .try_send(worker::Action::DiscoverSurfaces { launch: None })
                    .is_ok()
                {
                    // Cache negative discovery too. A new worker, explicit
                    // open intent or known HWND invalidation permits refresh.
                    child.last_query = Some(Instant::now());
                    child.worker = live_worker.clone();
                }
            }
        }
        let body = layout
            .body
            .intersect(self.canvas_rect)
            .intersect(painter.clip_rect());
        let readable = child.host.update_readable(layout.body, body);
        if focused && trusted_surface && child.native.is_none() && body.is_positive() {
            if let Some(handle) = child
                .descriptor
                .as_ref()
                .and_then(|s| s.native_handle.as_deref())
                .and_then(|h| h.parse::<isize>().ok())
            {
                match NativeSurface::attach_full_ui(handle, child.process_id, self.frame_hwnd) {
                    Ok(surface) => child.native = Some(surface),
                    Err(error) => {
                        if matches!(
                            error,
                            atlas_connector_runtime::surface::SurfaceError::InvalidWindow
                                | atlas_connector_runtime::surface::SurfaceError::IdentityChanged
                                | atlas_connector_runtime::surface::SurfaceError::WrongProcess { .. }
                        ) {
                            child.descriptor = None;
                            child.last_query = None;
                        }
                        child.status = error.to_string();
                    }
                }
            }
        }
        let mut placed = false;
        let host_active = child
            .native
            .as_ref()
            .is_some_and(|surface| surface.host_is_foreground().unwrap_or(false));
        if let Some(surface) = &child.native {
            if focused && trusted_surface && readable && host_active {
                let scale = ui.ctx().pixels_per_point();
                let physical = |r: Rect| SurfaceRect {
                    x: (r.min.x * scale).round() as i32,
                    y: (r.min.y * scale).round() as i32,
                    width: (r.width() * scale).round().max(1.0) as i32,
                    height: (r.height() * scale).round().max(1.0) as i32,
                };
                match surface.place_clipped_ready(physical(layout.body), physical(body)) {
                    Ok(ready) => placed = ready,
                    Err(error) => {
                        child.status = error.to_string();
                        let _ = surface.park();
                    }
                }
            } else {
                let _ = surface.park();
            }
        }
        if placed {
            child.host.revealed();
            if let Some(request) = child.host.request(true, false) {
                if let Some(parent) = self.connectors.sessions.get(&(tab, reference.parent)) {
                    if parent
                        .tx
                        .try_send(worker::Action::FocusChild {
                            request,
                            surface: reference.surface.clone(),
                            command: reference.open_command.clone(),
                        })
                        .is_ok()
                    {
                        child.host.queued(request);
                    }
                }
            }
        } else if readable && host_active {
            child.host.take_revealed();
            child.host.suspend_focus();
        } else {
            child.host.parked();
        }
        let label = if readable {
            child.status.clone()
        } else {
            "Application preview · Enlarge or maximize to enter".into()
        };
        atlas_shell::canvas_text::text(
            painter,
            body.center(),
            egui::Align2::CENTER_CENTER,
            &label,
            egui::FontId::proportional(atlas_shell::canvas_scale::px(15.0, layout.zoom)),
            self.palette().sub,
        );
        self.paint_portal_shell_finish(
            ui,
            painter,
            &layout,
            node.id,
            portal,
            None,
            self.palette().border,
            focused,
            layout.zoom,
        );
    }
}
