//! Load a secondary editor while its verified parent remains hosted.
use super::*;

pub(in crate::app::application_connector) struct LaunchRequest {
    pub ticket: LaunchToken,
    pub command: String,
}

impl ChildSurface {
    pub(in crate::app::application_connector) fn admit_launch(
        &mut self,
        tx: &Sender<worker::Action>,
        parent_ready: bool,
    ) {
        if !parent_ready {
            return;
        }
        let Some(request) = self.waiting_launch.take() else {
            return;
        };
        match tx.try_send(worker::Action::DiscoverSurfaces {
            launch: Some(request),
        }) {
            Ok(()) => self.status = "Loading native application window…".into(),
            Err(crossbeam_channel::TrySendError::Full(worker::Action::DiscoverSurfaces {
                launch,
            })) => {
                self.waiting_launch = launch;
            }
            Err(_) => {
                self.launch.cancel();
                self.launch_node = None;
                self.status = "Application worker is disconnected".into();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{application_connector::native::test_support, tests::Harness};
    use atlas_connector::{
        CommandDescriptor, CommandEffect, ProcessOwnership, SourceMode, UndoOwner,
    };

    fn fixture(tag: &str) -> (Harness, NodeId, test_support::Peer) {
        let mut h = Harness::new(tag);
        h.app.leave_home();
        h.app.ensure_work_tab();
        let (parent, peer) = test_support::install(&mut h, SourceMode::Linked);
        test_support::add_command(
            &mut h,
            parent,
            CommandDescriptor {
                id: "org.slate.rhino.grasshopper_show".into(),
                title: "Grasshopper".into(),
                effect: CommandEffect::EphemeralView,
                undo: UndoOwner::NativeApplication,
                cancellable: false,
                agent_allowed: false,
                parameters: serde_json::json!({"type":"object"}),
            },
        );
        h.app.portal_enter_interactive(parent);
        (h, parent, peer)
    }
    fn descriptor() -> SurfaceDescriptor {
        SurfaceDescriptor {
            surface_id: "grasshopper".into(),
            mode: SurfaceMode::NativeFullUi,
            process_ownership: ProcessOwnership::LaunchedDedicated,
            native_handle: Some("1234".into()),
            can_detach: true,
        }
    }
    #[test]
    fn unresolved_companion_discovery_is_cached_per_worker_without_losing_queue_retry() {
        let (mut h, parent, peer) = fixture("surface-negative-discovery");
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        let reference =
            ApplicationSurface::new(parent, "grasshopper", "org.slate.rhino.grasshopper_show")
                .unwrap();
        let node = h.app.doc().scene.node_preparation().build_node(
            slate_doc::WorldRect::new(0.0, 0.0, 960.0, 600.0),
            NodeKind::Portal(reference.portal("Grasshopper")),
        );
        h.app.add_nodes(vec![node]);
        h.app.contents_blur(); // An unfocused persisted companion caused the live polling leak.
        h.frame();
        assert_eq!(peer.background_discoveries(), 1);
        let tab = h.app.tab().id;
        let key = (tab, parent, "grasshopper".into());
        for _ in 0..32 {
            h.app.connectors.children.get_mut(&key).unwrap().last_query =
                Some(Instant::now() - Duration::from_secs(120));
            h.frame();
            assert_eq!(
                peer.background_discoveries(),
                0,
                "elapsed time must not rediscover a missing editor"
            );
        }
        let new_worker = atlas_agent::request_id();
        h.app
            .connectors
            .sessions
            .get_mut(&(tab, parent))
            .unwrap()
            .binding
            .as_mut()
            .unwrap()
            .worker = new_worker.clone();
        h.frame();
        assert_eq!(
            peer.background_discoveries(),
            1,
            "new source worker gets one fresh discovery"
        );
        assert_eq!(
            h.app.connectors.children[&key].worker.as_deref(),
            Some(new_worker.as_str())
        );

        let tx = h.app.connectors.sessions[&(tab, parent)].tx.clone();
        h.app.connectors.children.get_mut(&key).unwrap().last_query = None;
        for _ in 0..8 {
            assert!(tx
                .try_send(worker::Action::DiscoverSurfaces { launch: None })
                .is_ok());
        }
        h.frame();
        assert!(
            h.app.connectors.children[&key].last_query.is_none(),
            "a full queue did not admit discovery"
        );
        assert_eq!(peer.background_discoveries(), 8);
        h.frame();
        assert_eq!(
            peer.background_discoveries(),
            1,
            "unadmitted discovery retries once queue space is available"
        );

        h.app
            .open_application_surface(parent, "grasshopper", "org.slate.rhino.grasshopper_show");
        h.app
            .connectors
            .children
            .get_mut(&key)
            .unwrap()
            .admit_launch(&tx, true);
        assert_eq!(
            peer.next_surface_launch()
                .expect("explicit open bypasses negative cache")
                .command,
            "org.slate.rhino.grasshopper_show"
        );
    }

    #[test]
    fn parent_stays_focused_until_the_requested_editor_descriptor_completes() {
        let (mut h, parent, peer) = fixture("surface-staged-handoff");
        h.app
            .open_application_surface(parent, "grasshopper", "org.slate.rhino.grasshopper_show");
        assert_eq!(h.app.contents_focused(), Some(parent));
        assert!(h.app.connectors.sessions[&(h.app.tab().id, parent)].focused);
        assert!(
            peer.next_surface_launch().is_none(),
            "intent alone cannot launch the editor"
        );
        let tab = h.app.tab().id;
        let tx = h.app.connectors.sessions[&(tab, parent)].tx.clone();
        let child = h
            .app
            .connectors
            .children
            .get_mut(&(tab, parent, "grasshopper".into()))
            .unwrap();
        child.admit_launch(&tx, false);
        assert!(peer.next_surface_launch().is_none());
        child.admit_launch(&tx, true);
        let request = peer
            .next_surface_launch()
            .expect("one cold launch after actual hosting readiness");
        let tab = h.app.tab().id;
        h.app.complete_application_surface_launch(
            tab,
            parent,
            42,
            request.ticket,
            Ok(vec![descriptor()]),
        );
        let child = &h.app.connectors.children[&(tab, parent, "grasshopper".into())];
        assert_eq!(h.app.contents_focused(), child.node);
        assert!(!h.app.connectors.sessions[&(tab, parent)].focused);
        assert!(
            peer.next_surface_launch().is_none(),
            "handoff must not replay LoadEditor"
        );
    }
    #[test]
    fn leaving_parent_cancels_handoff_but_keeps_the_returned_descriptor() {
        let (mut h, parent, peer) = fixture("surface-cancel-handoff");
        h.app
            .open_application_surface(parent, "grasshopper", "org.slate.rhino.grasshopper_show");
        let tab = h.app.tab().id;
        let tx = h.app.connectors.sessions[&(tab, parent)].tx.clone();
        h.app
            .connectors
            .children
            .get_mut(&(tab, parent, "grasshopper".into()))
            .unwrap()
            .admit_launch(&tx, true);
        let request = peer.next_surface_launch().unwrap();
        h.app.contents_blur();
        let tab = h.app.tab().id;
        h.app.complete_application_surface_launch(
            tab,
            parent,
            42,
            request.ticket,
            Ok(vec![descriptor()]),
        );
        assert_eq!(h.app.contents_focused(), None);
        assert!(
            h.app.connectors.children[&(tab, parent, "grasshopper".into())]
                .descriptor
                .is_some()
        );
    }
}

impl SlateApp {
    pub(crate) fn open_application_surface(
        &mut self,
        parent: NodeId,
        surface: &str,
        command: &str,
    ) {
        let Some(reference) = ApplicationSurface::new(parent, surface, command) else {
            return;
        };
        if self.native_binding(parent).is_err() {
            self.toast("Activate this native model first.");
            return;
        }
        let existing = self.doc().scene.nodes.iter().find(|n| matches!(&n.kind, NodeKind::Portal(p) if ApplicationSurface::from_portal(p).as_ref() == Some(&reference))).map(|n| n.id);
        let node = if let Some(id) = existing {
            id
        } else {
            let Some(host) = self.doc().scene.node(parent) else {
                return;
            };
            let rect = slate_doc::WorldRect {
                x: host.rect.x + host.rect.w + 80.0,
                y: host.rect.y,
                w: 960.0,
                h: 600.0,
            };
            let rect = self.clear_spawn_rect(rect);
            let portal = self
                .doc_mut()
                .scene
                .build_node(rect, NodeKind::Portal(reference.portal(surface)));
            let id = portal.id;
            let wire = self.provenance_wire(parent, &portal, true);
            self.add_nodes(vec![portal, wire]);
            id
        };
        self.portal_enter_interactive(node);
    }

    /// Run before the shared contents-blur path. Only terminal success may hand off.
    pub(crate) fn stage_application_surface_open(&mut self, node: NodeId) -> bool {
        let Some(reference) = self.application_surface_ref(node) else {
            return true;
        };
        let Ok(binding) = self.native_binding(reference.parent) else {
            self.toast("Activate the source model before opening its window.");
            return true;
        };
        if !binding.commands.iter().any(|c| {
            c.id == reference.open_command
                && c.effect == atlas_connector::CommandEffect::EphemeralView
        }) {
            self.toast("This model did not offer the requested window command.");
            return true;
        }
        let tab = self.tab().id;
        let key = (tab, reference.parent, reference.surface.clone());
        if let Some(child) = self.connectors.children.get_mut(&key) {
            if std::mem::take(&mut child.handoff_ready)
                && child.worker.as_deref() == Some(binding.worker.as_str())
                && child.descriptor.is_some()
            {
                return false;
            }
            if child.launch.is_pending() {
                return true;
            }
        }
        if self.contents_focused() != Some(reference.parent) {
            self.portal_enter_interactive(reference.parent);
        }
        let child = self.connectors.children.entry(key).or_default();
        child.node = Some(node);
        child.worker = Some(binding.worker.clone());
        child.launch_node = Some((node, reference.open_command.clone()));
        child.last_query = Some(Instant::now());
        child.status = "Preparing readable source window before loading editor…".into();
        let ticket = child.launch.begin(&reference.surface, binding.worker);
        child.waiting_launch = Some(LaunchRequest {
            ticket,
            command: reference.open_command,
        });
        true
    }

    pub(crate) fn application_surface_enter(&mut self, node: NodeId) {
        let Some(reference) = self.application_surface_ref(node) else {
            return;
        };
        let key = (self.tab().id, reference.parent, reference.surface);
        let child = self.connectors.children.entry(key).or_default();
        child.node = Some(node);
        child.host.begin_intent();
        child.status = "Native application window".into();
    }

    pub(in crate::app::application_connector) fn complete_application_surface_launch(
        &mut self,
        tab: u64,
        parent: NodeId,
        pid: u32,
        ticket: LaunchToken,
        result: Result<Vec<SurfaceDescriptor>, String>,
    ) {
        if let Ok(surfaces) = &result {
            self.register_application_surfaces(tab, parent, pid, surfaces.clone());
        }
        let key = (tab, parent, ticket.surface.clone());
        let target = self
            .connectors
            .children
            .get(&key)
            .and_then(|c| c.launch_node.clone());
        let target_valid = target.as_ref().is_some_and(|(node, command)| {
            self.doc().scene.node(*node).is_some_and(|n| !n.hidden)
                && self.application_surface_ref(*node).is_some_and(|r| {
                    r.parent == parent && r.surface == ticket.surface && r.open_command == *command
                })
        });
        let session = self.connectors.sessions.get(&(tab, parent));
        let worker = session
            .and_then(|s| s.binding.as_ref())
            .map(|b| b.worker.as_str())
            .unwrap_or("");
        let parent_focused = !self.at_home
            && self.tab().id == tab
            && target_valid
            && self.contents_focused() == Some(parent)
            && session.is_some_and(|s| s.focused && s.enabled && !s.ended);
        let Some(child) = self.connectors.children.get_mut(&key) else {
            return;
        };
        let current = child.launch.complete(&ticket, worker, parent_focused);
        if !child.launch.is_pending() {
            child.launch_node = None;
        }
        if let Err(error) = result {
            if current {
                child.status = format!("Application load: {error}");
            }
            return;
        }
        if !current || child.worker.as_deref() != Some(worker) || child.descriptor.is_none() {
            return;
        }
        let Some((node, _)) = target else { return };
        child.node = Some(node);
        child.handoff_ready = true;
        self.portal_enter_interactive(node);
    }
}
