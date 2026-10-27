//! Generic native-application adapter. Protocol and native ownership stay in shared crates.
use super::SlateApp;
use atlas_connector::{SurfaceDescriptor, SurfaceGeometry, SurfaceMode};
use atlas_connector_runtime::surface::{NativeSurface, SurfaceRect};
use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Color32, Rect, TextureHandle};
use slate_doc::NodeId;
use std::{collections::HashMap, path::PathBuf, time::Duration};
mod actions;
mod children;
mod hosting;
pub(crate) mod native;
mod presentation;
mod worker;

#[derive(Default)]
pub(crate) struct ConnectorRuntime {
    sessions: HashMap<(u64, NodeId), Session>,
    children: HashMap<(u64, NodeId, String), children::ChildSurface>,
    uncertain: Vec<native::Completion>,
    retired: Vec<native::TicketEntry>,
    recent: std::collections::VecDeque<native::Completion>,
}
struct Session {
    enabled: bool,
    pending: usize,
    ended: bool,
    failure: Option<String>,
    path: PathBuf,
    tx: Sender<worker::Action>,
    rx: Receiver<worker::Event>,
    surface: Option<NativeSurface>,
    descriptor: Option<SurfaceDescriptor>,
    process_id: u32,
    binding: Option<native::BindingSnapshot>,
    tickets: HashMap<String, native::TicketEntry>,
    status: String,
    preview_error_status: Option<String>,
    focused: bool,
    last_geometry: Option<SurfaceGeometry>,
    host: hosting::HostPresentation,
    poster: Option<TextureHandle>,
    input_captures: HashMap<PathBuf, InputCapture>,
    invocation_receipt: Option<serde_json::Value>,
}

enum InputCapture {
    Pending,
    Complete(Result<PathBuf, String>),
}

impl SlateApp {
    pub(crate) fn application_connector_eligible(&self, id: NodeId) -> bool {
        self.model_node_info(id).is_some_and(|info| {
            info.path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("3dm"))
        })
    }
    pub(crate) fn application_connector_active(&self, id: NodeId) -> bool {
        !self.at_home
            && self
                .connectors
                .sessions
                .get(&(self.tab().id, id))
                .is_some_and(|s| s.enabled)
    }
    pub(crate) fn connector_activate(&mut self, id: NodeId) {
        self.activate_connector_mode(id, None);
    }
    pub(crate) fn connector_activate_draft(
        &mut self,
        id: NodeId,
        grant_id: String,
        agent_id: String,
        commands: Vec<String>,
    ) {
        self.activate_connector_mode(
            id,
            Some(worker::DraftGrant {
                grant_id,
                agent_id,
                commands,
            }),
        );
    }
    fn activate_connector_mode(&mut self, id: NodeId, draft: Option<worker::DraftGrant>) {
        if !self.application_connector_eligible(id) {
            self.toast("Choose a Rhino .3dm model first.");
            return;
        }
        let key = (self.tab().id, id);
        let Some(info) = self.model_node_info(id) else {
            return;
        };
        if self
            .connectors
            .sessions
            .get(&key)
            .is_some_and(|session| session.path != info.path || session.ended)
        {
            self.retire_connector_session(key);
        }
        if let Some(session) = self.connectors.sessions.get_mut(&key) {
            session.enabled = true;
            self.lock_model(id);
            self.portal_enter_interactive(id);
            return;
        }
        if self
            .doc()
            .scene
            .node(id)
            .is_none_or(|n| n.rotation_deg.abs() > 0.001)
        {
            self.toast("Reset the model frame rotation before entering Rhino.");
            return;
        }
        // Installation health is distinct from launching. Loading a workbook never reaches here.
        let Some(offer) = self
            .ai
            .packs
            .application_offers()
            .into_iter()
            .find(|o| o.supports_extension("3dm"))
        else {
            if self.ai.packs.in_flight() {
                self.toast("Checking Rhino connector availability. Try again when Connections finishes checking.");
                return;
            }
            self.toast("Rhino connector is unavailable. Install the Slate Rhino.Inside expansion pack, then refresh Connections.");
            return;
        };
        let (tx, rx) = worker::start(
            info.path.clone(),
            offer.id.0,
            offer.descriptor.registration,
            draft,
        );
        self.connectors.sessions.insert(
            key,
            Session {
                enabled: true,
                pending: 0,
                ended: false,
                failure: None,
                path: info.path,
                tx,
                rx,
                surface: None,
                descriptor: None,
                process_id: 0,
                binding: None,
                tickets: HashMap::new(),
                status: "Starting Rhino.Inside…".into(),
                preview_error_status: None,
                focused: false,
                last_geometry: None,
                host: Default::default(),
                poster: None,
                input_captures: HashMap::new(),
                invocation_receipt: None,
            },
        );
        self.lock_model(id);
        self.portal_enter_interactive(id);
    }
    pub(crate) fn connector_deactivate(&mut self, id: NodeId) {
        let tab = self.tab().id;
        self.connector_leave_contents();
        if let Some(s) = self.connectors.sessions.get_mut(&(tab, id)) {
            s.enabled = false;
        }
        self.portals.contents = None;
        self.toast("Slate 3D preview active. Rhino keeps its document open.");
    }
    pub(crate) fn connector_enter_contents(&mut self, id: NodeId) {
        let tab = self.tab().id;
        if let Some(s) = self.connectors.sessions.get_mut(&(tab, id)) {
            s.focused = true;
            s.host.begin_intent();
        }
    }
    pub(crate) fn connector_leave_contents(&mut self) -> bool {
        let mut changed = false;
        for s in self.connectors.sessions.values_mut().filter(|s| s.focused) {
            changed = true;
            s.focused = false;
            s.last_geometry = None;
            if let Some(surface) = &s.surface {
                let _ = surface.park();
            }
            s.queue_hidden_release();
        }
        changed
    }
    pub(crate) fn maintain_application_connectors(&mut self, ctx: &egui::Context) {
        let active_tab = (!self.at_home).then(|| self.tab().id);
        let stale: Vec<_> = self
            .connectors
            .sessions
            .keys()
            .copied()
            .filter(|key| {
                let path = self.tabs.iter().find(|t| t.id == key.0).and_then(|tab| {
                    let node = tab.doc.scene.node(key.1)?;
                    let slate_doc::scene::NodeKind::Image(image) = &node.kind else {
                        return None;
                    };
                    let item = tab.doc.item(image.item)?;
                    Some(super::board_portal::resolve_source(
                        tab.path.as_deref(),
                        &item.path.to_string_lossy(),
                    ))
                });
                path.as_ref() != Some(&self.connectors.sessions[key].path)
            })
            .collect();
        for key in stale {
            self.retire_connector_session(key);
        }
        let keys: Vec<_> = self.connectors.sessions.keys().copied().collect();
        let mut captures = Vec::new();
        let mut notices = Vec::new();
        let mut surface_updates = Vec::new();
        let mut child_focus = Vec::new();
        let mut child_launches = Vec::new();
        for key in keys {
            let valid = active_tab == Some(key.0)
                && self
                    .model_node_info(key.1)
                    .is_some_and(|i| self.connectors.sessions[&key].path == i.path);
            let session = self.connectors.sessions.get_mut(&key).unwrap();
            if !valid && session.focused {
                session.focused = false;
                session.last_geometry = None;
                if let Some(surface) = &session.surface {
                    let _ = surface.park();
                }
                session.queue_hidden_release();
            }
            session.retry_hidden_release();
            for _ in 0..8 {
                let Ok(event) = session.rx.try_recv() else {
                    break;
                };
                match event {
                    worker::Event::FocusApplied { request, result } => {
                        let success = result.is_ok();
                        let decision =
                            session
                                .host
                                .complete_for_release(request, success, session.focused);
                        if let Some(release) = decision {
                            if release {
                                if let Err(error) = session.release_hidden_surface() {
                                    session.status = format!("Rhino window parking: {error}");
                                }
                                ctx.request_repaint();
                            } else if let Err(error) = result {
                                session.status = format!("Rhino focus: {error}");
                            }
                        }
                    }
                    worker::Event::ChildFocusApplied {
                        surface,
                        request,
                        result,
                    } => child_focus.push((key.0, key.1, surface, request, result)),
                    worker::Event::Surfaces(surfaces) => {
                        surface_updates.push((key.0, key.1, session.process_id, surfaces))
                    }
                    worker::Event::SurfaceLaunchComplete { ticket, result } => {
                        child_launches.push((key.0, key.1, session.process_id, ticket, result))
                    }
                    worker::Event::PreviewUnavailable(error) => {
                        let status = format!("Rhino parked; preview unavailable: {error}");
                        session.preview_error_status = Some(status.clone());
                        session.status = status;
                    }
                    worker::Event::InvocationResult(receipt) => {
                        session.invocation_receipt = Some(receipt)
                    }
                    worker::Event::Ended => {
                        session.ended = true;
                        session.pending = 0;
                        session.descriptor = None;
                        session.focused = false;
                        if let Some(mut surface) = session.surface.take() {
                            let _ = surface.hide();
                            let _ = surface.detach();
                        }
                        if session.failure.is_none() {
                            session.failure = Some("Rhino connection ended. Switch to Slate 3D and activate Rhino to reconnect.".into());
                        }
                        session.status = session.failure.clone().unwrap_or_default();
                    }
                    worker::Event::Ready {
                        surface,
                        process_id,
                        binding,
                    } => {
                        session.process_id = process_id;
                        session.binding = Some(binding);
                        session.descriptor = Some(surface.clone());
                        session.status = "Rhino.Inside".into();
                        // Prepare an authenticated poster while hidden. Presentation
                        // alone may reveal/focus after actual placement is ready.
                        session.request_native_focus(false, true);
                    }
                    worker::Event::Status(message) => {
                        session.failure = Some(message.clone());
                        session.status = message.clone();
                        notices.push(message);
                    }
                    worker::Event::CommandComplete => {
                        session.pending = session.pending.saturating_sub(1)
                    }
                    worker::Event::InputCapture { directory, result } => {
                        if session.input_captures.contains_key(&directory) {
                            session
                                .input_captures
                                .insert(directory, InputCapture::Complete(result));
                        }
                    }
                    worker::Event::Binding(binding) => {
                        if session.binding.as_ref().is_none_or(|old| {
                            old.worker == binding.worker && old.observation <= binding.observation
                        }) {
                            session.binding = Some(binding);
                        }
                    }
                    worker::Event::Capture {
                        file,
                        size,
                        canvas,
                        author,
                    } => {
                        if valid {
                            captures.push((key.1, file, size, canvas, author));
                        }
                    }
                    worker::Event::Preview {
                        rgba,
                        width,
                        height,
                    } => {
                        if !valid || session.ended {
                            continue;
                        }
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [width as usize, height as usize],
                            &rgba,
                        );
                        if let Some(texture) = &mut session.poster {
                            texture.set(image, egui::TextureOptions::LINEAR);
                        } else {
                            session.poster = Some(ctx.load_texture(
                                format!("connector-{}-{}", key.0, key.1 .0),
                                image,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                        // A poster proves capture only, never native UI focus.
                        // Clear only the exact diagnostic still owned by preview.
                        if session.failure.is_none()
                            && session.preview_error_status.take().as_deref()
                                == Some(session.status.as_str())
                        {
                            session.status = "Rhino.Inside".into();
                        }
                    }
                }
            }
        }
        for (tab, parent, pid, surfaces) in surface_updates {
            self.register_application_surfaces(tab, parent, pid, surfaces);
        }
        for (tab, parent, surface, request, result) in child_focus {
            self.complete_application_surface_focus(tab, parent, &surface, request, result);
        }
        for (tab, parent, pid, ticket, result) in child_launches {
            self.complete_application_surface_launch(tab, parent, pid, ticket, result);
        }
        self.maintain_application_surfaces();
        for (node, path, size, canvas, author) in captures {
            self.finish_external_portal_capture(node, path, size, canvas, author);
        }
        for message in notices {
            self.toast(message);
        }
        self.maintain_native_receipts();
        if !self.connectors.sessions.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        for entry in self.tickets.values() {
            entry.lease.revoke();
        }
        // Restore parenting before dropping the last request sender asks Preserve detach.
        if let Some(mut surface) = self.surface.take() {
            let _ = surface.detach();
        }
    }
}

#[cfg(test)]
mod tests;
