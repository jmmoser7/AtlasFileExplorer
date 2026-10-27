//! Trusted in-memory peer for real BoardCall admission and completion tests.
use super::*;
use crate::app::application_connector::Session as ConnectorSession;
use crate::app::tests::Harness;
use atlas_connector::*;
use slate_doc::scene::{ImageNode, NodeKind, WorldRect};

pub(crate) struct Peer {
    actions: Receiver<worker::Action>,
    pub binding: BindingSnapshot,
}

/// Add one advertised capability through the fixture's own private session owner.
pub(crate) fn add_command(h: &mut Harness, node: NodeId, command: CommandDescriptor) {
    let key = (h.app.tab().id, node);
    h.app
        .connectors
        .sessions
        .get_mut(&key)
        .unwrap()
        .binding
        .as_mut()
        .unwrap()
        .commands
        .push(command);
}
impl Peer {
    pub(in crate::app::application_connector) fn background_discoveries(&self) -> usize {
        self.actions
            .try_iter()
            .filter(|action| matches!(action, worker::Action::DiscoverSurfaces { launch: None }))
            .count()
    }

    pub(in crate::app::application_connector) fn next_surface_launch(
        &self,
    ) -> Option<super::super::children::LaunchRequest> {
        while let Ok(action) = self.actions.try_recv() {
            if let worker::Action::DiscoverSurfaces {
                launch: Some(request),
            } = action
            {
                return Some(request);
            }
        }
        None
    }
    pub fn next(&self) -> Option<NativeRequest> {
        while let Ok(action) = self.actions.try_recv() {
            if let worker::Action::Native(request) = action {
                return Some(request);
            }
        }
        None
    }
    pub fn complete(&self, request: NativeRequest, outcome: NativeOutcome, revision: &str) {
        assert!(request.lease.claim());
        self.finish_admitted(request, outcome, revision);
    }
    pub fn finish_admitted(&self, request: NativeRequest, outcome: NativeOutcome, revision: &str) {
        assert!(request.lease.dispatched());
        request.lease.complete();
        let mut binding = self.binding.binding.clone();
        binding.source.revision = revision.into();
        let actor = match &request.caller {
            NativeCaller::Agent { identity, .. } => Some(identity.clone()),
            NativeCaller::Human => None,
        };
        request
            .reply
            .try_send(Completion {
                ticket: request.ticket,
                key: request.key,
                binding_after: Some(binding),
                observation: self.binding.observation + 1,
                outcome,
                actor,
            })
            .ok()
            .expect("one terminal completion");
    }
}

pub(crate) fn install(h: &mut Harness, mode: SourceMode) -> (NodeId, Peer) {
    let path = h.base.join("native-api.3dm");
    let item = h
        .app
        .doc_mut()
        .add_item(path.clone(), "native-api.3dm", 1, 0, "native-api-fixture");
    let node = h.app.doc().scene.node_preparation().build_node(
        WorldRect::new(200.0, 160.0, 640.0, 480.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    let descriptor = |id: &str, effect| CommandDescriptor {
        id: id.into(),
        title: id.into(),
        effect,
        undo: UndoOwner::NativeApplication,
        cancellable: false,
        agent_allowed: true,
        parameters: serde_json::json!({"type":"object","properties":{"x":{"type":"number"}},"additionalProperties":false}),
    };
    let snapshot = BindingSnapshot {
        worker: atlas_agent::request_id(),
        observation: 1,
        source_root: h.base.clone(),
        binding: SourceBinding {
            source: SourceRef {
                source_id: format!("source-{}", id.0),
                revision: "selected-revision".into(),
                locator: SourceLocator {
                    root_id: None,
                    relative: None,
                    absolute: Some(path.to_string_lossy().into()),
                },
            },
            document_id: "native-document".into(),
            mode,
            dirty: false,
        },
        commands: vec![
            descriptor("org.slate.rhino.inspect", CommandEffect::Inspect),
            descriptor("org.slate.rhino.draft_box", CommandEffect::DraftMutation),
            descriptor(
                "org.slate.rhino.source_save",
                CommandEffect::SourceWriteBack,
            ),
        ],
    };
    let (tx, actions) = crossbeam_channel::bounded(8);
    let (_events, rx) = crossbeam_channel::bounded(16);
    h.app.connectors.sessions.insert(
        (h.app.tab().id, id),
        ConnectorSession {
            enabled: true,
            pending: 0,
            ended: false,
            failure: None,
            path,
            tx,
            rx,
            surface: None,
            descriptor: None,
            process_id: 0,
            binding: Some(snapshot.clone()),
            tickets: HashMap::new(),
            status: "bound test peer".into(),
            preview_error_status: None,
            focused: false,
            last_geometry: None,
            host: Default::default(),
            poster: None,
            input_captures: HashMap::new(),
            invocation_receipt: None,
        },
    );
    (
        id,
        Peer {
            actions,
            binding: snapshot,
        },
    )
}
