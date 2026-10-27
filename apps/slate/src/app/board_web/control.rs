//! Typed page controls are an input adapter, not a second command authority.
use super::*;
use crate::app::application_connector::native::{
    BindingKey, Lease, NativeOperation, NativeOutcome, NativeResult, NativeTicket,
};
use atlas_agent::control::{self as protocol, Control, Intent, Manifest};
use serde_json::{json, Value};

/// The v1 native leaf capability registry. New actions extend this owner after
/// their parameter/source authority is reviewed; a manifest cannot opt itself in.
const NATIVE_CONTROLS: &[&str] = &[
    "org.slate.rhino.buildings",
    "org.slate.rhino.draft_building_floors",
];

#[derive(Default)]
pub(super) struct Controls {
    slots: HashMap<NodeId, Slot>,
    next_tick: Option<Instant>,
}
struct PendingControl {
    target: NodeId,
    key: BindingKey,
    ticket: NativeTicket,
    lease: Lease,
    intent: Intent,
}
struct Slot {
    source: String,
    epoch: u64,
    manifest: Option<Manifest>,
    invalidated: bool,
    model: Option<(NodeId, BindingKey)>,
    sequence: u64,
    next_poll: Instant,
    pending: Option<PendingControl>,
    state: Value,
}

#[cfg(test)]
mod tests;
impl Slot {
    fn new(source: String, epoch: u64) -> Self {
        Self {
            source,
            epoch,
            manifest: None,
            invalidated: false,
            model: None,
            sequence: 0,
            next_poll: Instant::now(),
            pending: None,
            state: json!({"version":1,"ready":false,"connection_generation":0,"message":"Connect a model wire","replies":[]}),
        }
    }
    fn reply(&mut self, intent: &Intent, status: &str, result: Value, message: &str) {
        let replies = self.state["replies"].as_array_mut().unwrap();
        if replies.len() >= 16 {
            replies.remove(0);
        }
        replies.push(json!({"control_id":intent.control_id,"sequence":intent.sequence,"status":status,"result":result,"message":message}));
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        if let Some(pending) = &self.pending {
            pending.lease.revoke();
        }
    }
}

fn local_key(path: &Path) -> Option<String> {
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(
        path.to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase(),
    )
}

impl SlateApp {
    pub(crate) fn dashboard_control_wire(
        &self,
        a: &slate_doc::scene::ConnectorEnd,
        b: &slate_doc::scene::ConnectorEnd,
    ) -> Option<slate_doc::agent_inputs::WireBinding> {
        slate_doc::agent_inputs::infer_control_binding(
            &self.doc().scene,
            a,
            b,
            &|id| {
                self.control_source(id).ok()?;
                let manifest = self.web.controls.slots.get(&id)?.manifest.as_ref()?;
                let mut inputs: Vec<_> =
                    manifest.controls.iter().map(|c| c.input.clone()).collect();
                inputs.sort();
                inputs.dedup();
                (inputs.len() == 1).then(|| inputs.remove(0))
            },
            &|id| self.model_node_info(id).is_some(),
        )
    }
    fn control_source(&self, id: NodeId) -> Result<String, String> {
        if self.tab().read_only {
            return Err("This workbook is read-only".into());
        }
        let root = self
            .tab()
            .path
            .as_deref()
            .and_then(Path::parent)
            .ok_or("Save the workbook before using controls")?;
        let node = self
            .doc()
            .scene
            .node(id)
            .ok_or("Control portal disappeared")?;
        let NodeKind::Portal(portal) = &node.kind else {
            return Err("Not a Web portal".into());
        };
        if portal.kind != PortalKind::Web || !portal.web_ref().interactive_allowed {
            return Err("Web interaction is disabled".into());
        }
        let locator = &portal
            .source
            .as_ref()
            .ok_or("Missing control source")?
            .locator;
        let is_dir = !locator.to_ascii_lowercase().ends_with(".html")
            && !locator.to_ascii_lowercase().ends_with(".htm");
        let kind =
            classify_web_locator(locator, is_dir).map_err(|_| "Unsupported control source")?;
        if kind == WebSourceKind::Remote {
            return Err("Remote pages cannot issue controls".into());
        }
        let mut source = resolve_web_source(self.tab().path.as_deref(), locator);
        if kind == WebSourceKind::LocalDir {
            source.push(&portal.web_ref().entry);
        }
        let source = local_key(&source).ok_or("Control source traversal is not allowed")?;
        let root = local_key(root).ok_or("Invalid workbook root")?;
        if !source.starts_with(&(root.trim_end_matches('/').to_string() + "/")) {
            return Err("Controls must be inside this workbook folder".into());
        }
        let current = self
            .web
            .host
            .current_url(id)
            .ok_or("Control page is still loading")?;
        let current = url::Url::parse(&current)
            .ok()
            .and_then(|u| u.to_file_path().ok())
            .unwrap_or_else(|| PathBuf::from(current));
        if local_key(&current).as_deref() != Some(source.as_str()) {
            return Err("Control page navigated away from its local source".into());
        }
        Ok(source)
    }

    /// The page names a logical input; only an actual board wire chooses a node.
    fn control_target(&self, page: NodeId, input: &str) -> Result<NodeId, String> {
        use slate_doc::agent_inputs::{endpoint_node, InputKind};
        let mut targets = HashSet::new();
        for node in &self.doc().scene.nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            let Some(binding) = &conn.binding else {
                continue;
            };
            if binding.kind != InputKind::Control
                || endpoint_node(binding.target_end(conn)) != Some(page)
            {
                continue;
            }
            let Some(other) = endpoint_node(binding.source_end(conn)) else {
                continue;
            };
            let slot = binding.slot.as_deref().unwrap_or("");
            if slot == input && self.model_node_info(other).is_some() {
                targets.insert(other);
            }
        }
        if targets.len() != 1 {
            return Err("Connect exactly one model wire for this input".into());
        }
        Ok(*targets.iter().next().unwrap())
    }

    fn submit_control(
        &mut self,
        page: NodeId,
        control: &Control,
        intent: &Intent,
    ) -> Result<PendingControl, String> {
        self.control_source(page)?;
        let arguments = control
            .arguments_for(&intent.value)
            .map_err(str::to_owned)?;
        Self::validate_targeted_arguments(&control.command, &arguments)?;
        let target = self.control_target(page, &control.input)?;
        let binding = self.native_binding(target)?;
        // New capabilities extend this adapter registry; page markup cannot do so.
        if control.command != "board.model.connector.invoke" {
            return Err("Unsupported declared control command".into());
        }
        let command = arguments["command_id"]
            .as_str()
            .ok_or("Missing declared native command")?;
        if !NATIVE_CONTROLS.contains(&command) {
            return Err("This native command is not registered for declared controls".into());
        }
        let descriptor = binding
            .commands
            .iter()
            .find(|d| d.id == command)
            .ok_or("Command is not advertised by the connected model")?;
        use atlas_connector::CommandEffect;
        if !matches!(
            descriptor.effect,
            CommandEffect::Inspect | CommandEffect::DraftMutation
        ) {
            return Err("Controls allow inspections and isolated draft parameters only".into());
        }
        if descriptor.effect == CommandEffect::DraftMutation {
            if binding.binding.mode != atlas_connector::SourceMode::IsolatedDraft {
                return Err("Open an isolated model copy before changing its parameters".into());
            }
            if self.web.focused != Some(page)
                || self.web.agent_focus_lease.is_some()
                || !self
                    .web
                    .last_page_input
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
            {
                return Err("Change this control directly in the focused page; agents use their scoped board commands".into());
            }
        }
        let operation = NativeOperation::Invoke {
            command: command.into(),
            parameters: arguments["parameters"].clone(),
        };
        let lease = Lease::new();
        let ticket = self
            .native_submit_control(target, &binding, operation, lease.clone())
            .map_err(|outcome| match outcome {
                NativeOutcome::NotDispatched { reason, .. } => reason,
                _ => "The model rejected this control before dispatch".to_string(),
            })?;
        Ok(PendingControl {
            target,
            key: binding.key(),
            ticket,
            lease,
            intent: intent.clone(),
        })
    }

    fn finish_control(&mut self, page: NodeId, slot: &mut Slot) {
        let Some(pending) = slot.pending.take() else {
            return;
        };
        let valid = !slot.invalidated
            && self.control_source(page).is_ok()
            && slot
                .manifest
                .as_ref()
                .and_then(|m| {
                    m.controls
                        .iter()
                        .find(|c| c.id == pending.intent.control_id)
                })
                .is_some_and(|c| self.control_target(page, &c.input).ok() == Some(pending.target))
            && self.native_source_matches(pending.target, &pending.key);
        if !valid {
            pending.lease.revoke();
        }
        match self.native_poll(pending.target, &pending.ticket) {
            Ok(None) => slot.pending = Some(pending),
            Ok(Some(completion)) => {
                if !valid {
                    self.native_abandon(completion);
                    slot.reply(
                        &pending.intent,
                        "rejected",
                        Value::Null,
                        "The control wire or source changed",
                    );
                    return;
                }
                match &completion.outcome {
                    NativeOutcome::Completed {
                        result: NativeResult::Value(value),
                    } if pending.lease.publish() => {
                        slot.reply(
                            &pending.intent,
                            "completed",
                            value.clone(),
                            "Model confirmed",
                        );
                    }
                    _ => slot.reply(
                        &pending.intent,
                        "rejected",
                        Value::Null,
                        "The model did not confirm this change",
                    ),
                }
            }
            Err(error) => slot.reply(&pending.intent, "rejected", Value::Null, &error),
        }
    }

    pub(super) fn pump_dashboard_controls(&mut self, ctx: &egui::Context) {
        if self
            .web
            .controls
            .next_tick
            .is_some_and(|next| Instant::now() < next)
        {
            return;
        }
        self.web.controls.next_tick = Some(Instant::now() + Duration::from_millis(125));
        let ids: Vec<_> = self.web.host_open.iter().copied().collect();
        self.web.controls.slots.retain(|id, _| ids.contains(id));
        for id in ids {
            let Ok(source) = self.control_source(id) else {
                if self.web.controls.slots.remove(&id).is_some() {
                    self.web.host.control_state(id,&json!({"version":1,"ready":false,"message":"Local control source unavailable","replies":[]}));
                }
                continue;
            };
            let epoch = self.web.host.control_epoch(id);
            let mut slot = self
                .web
                .controls
                .slots
                .remove(&id)
                .filter(|s| s.source == source && s.epoch == epoch)
                .unwrap_or_else(|| Slot::new(source, epoch));
            let before = slot.state.clone();
            // Observe withdrawals and changed declarations before a queued native
            // result can be published. None means no reply yet, never revocation.
            let packet = match self.web.host.take_controls(id) {
                None => None,
                Some(Ok(text)) => match protocol::parse_packet(&text) {
                    Ok(packet)
                        if slot
                            .manifest
                            .as_ref()
                            .is_none_or(|m| !slot.invalidated && m == &packet.manifest) =>
                    {
                        // A first valid declaration can arrive after initial page boot.
                        if slot.manifest.is_none() {
                            slot.invalidated = false;
                        }
                        slot.manifest = Some(packet.manifest.clone());
                        Some(packet)
                    }
                    _ => {
                        slot.invalidated = true;
                        None
                    }
                },
                Some(Err(_)) => {
                    slot.invalidated = true;
                    None
                }
            };
            self.finish_control(id, &mut slot);
            if let Some(packet) = packet {
                for intent in packet.intents {
                    if intent.sequence <= slot.sequence {
                        continue;
                    }
                    slot.sequence = intent.sequence;
                    let Some(control) = packet
                        .manifest
                        .controls
                        .iter()
                        .find(|c| c.id == intent.control_id)
                    else {
                        slot.reply(&intent, "rejected", Value::Null, "Unknown declared control");
                        continue;
                    };
                    if slot.pending.is_some() {
                        slot.reply(
                            &intent,
                            "busy",
                            Value::Null,
                            "Wait for the current model change",
                        );
                        continue;
                    }
                    match self.submit_control(id, control, &intent) {
                        Ok(pending) => slot.pending = Some(pending),
                        Err(error) => slot.reply(&intent, "rejected", Value::Null, &error),
                    }
                }
            }
            let model = slot
                .manifest
                .as_ref()
                .and_then(|m| m.controls.first())
                .and_then(|c| self.control_target(id, &c.input).ok())
                .and_then(|target| {
                    self.native_binding(target)
                        .ok()
                        .map(|binding| (target, binding.key()))
                });
            if slot.model != model {
                slot.model = model;
                slot.state["connection_generation"] =
                    json!(slot.state["connection_generation"].as_u64().unwrap_or(0) + 1);
            }
            let ready = !slot.invalidated
                && slot.manifest.as_ref().is_some_and(|m| {
                    m.controls.iter().all(|c| {
                        self.control_target(id, &c.input)
                            .ok()
                            .is_some_and(|target| self.native_binding(target).is_ok())
                    })
                });
            slot.state["ready"] = json!(ready);
            slot.state["busy"] = json!(slot.pending.is_some());
            slot.state["message"] = json!(if slot.invalidated {
                "Control declaration changed; reload the page"
            } else if ready {
                "Connected model"
            } else {
                "Activate the model connected by its board wire"
            });
            if slot.state != before {
                self.web.host.control_state(id, &slot.state);
            }
            if Instant::now() >= slot.next_poll {
                self.web.host.request_controls(id);
                slot.next_poll = Instant::now() + Duration::from_millis(125);
            }
            self.web.controls.slots.insert(id, slot);
        }
        if !self.web.controls.slots.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(125));
        }
    }
}
