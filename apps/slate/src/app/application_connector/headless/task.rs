//! Explicit generator Run: transient task authority, never a Board participant.
use super::*;
use crate::app::application_connector::native::{AgentIdentity, ModelTaskPermit};
use atlas_agent::collaborator::{BoardLinkSession, CollaboratorGrant};
use atlas_agent::{AgentRequest, InputSnapshot};
use std::{
    collections::{BTreeMap, HashSet},
    time::Instant,
};
mod calls;
mod capture;
mod inputs;
#[cfg(test)]
mod tests;
mod witness;

const COMMANDS: [&str; 3] = [
    "board.model.connector.inspect",
    "board.model.connector.capture",
    "board.model.connector.invoke",
];

#[derive(Default)]
pub(super) struct ModelTasks {
    links: atlas_ai::board_link::BoardLinks,
    staging: HashMap<(u64, NodeId), Staging>,
    active: HashMap<(u64, NodeId), ModelTaskAuthority>,
}
struct Target {
    provider: String,
    session: String,
    model: Option<String>,
    parameters: BTreeMap<String, String>,
    path: PathBuf,
}
struct ModelTaskAuthority {
    node: NodeId,
    doc: u64,
    permit: ModelTaskPermit,
    target: Target,
    source_inputs: InputSnapshot,
    source_witness: witness::Witness,
    provider_dir: PathBuf,
    provider_terminal: bool,
    inputs: InputSnapshot,
    workspace: PathBuf,
    dir: PathBuf,
    grant: CollaboratorGrant,
    seen: HashSet<String>,
    next_sequence: u64,
    mutations: u8,
    pending: Option<calls::Pending>,
    capture: Option<capture::Pending>,
    accepted_path: Option<PathBuf>,
    model_accepted: bool,
    failure: Option<String>,
}
impl Drop for ModelTaskAuthority {
    fn drop(&mut self) {
        self.permit.lease.revoke();
        if let Some(pending) = &self.pending {
            pending.ticket.lease.revoke();
        }
    }
}
struct Staging {
    authority: ModelTaskAuthority,
    request: AgentRequest,
    prep: crate::app::board_agent::TaskInputPreparation,
    copies: Option<Receiver<Result<InputSnapshot, String>>>,
    authored: String,
    model_sources: Vec<(u64, PathBuf)>,
}

impl ModelTaskAuthority {
    fn validate(&self, app: &SlateApp, submitted: bool) -> Result<(), String> {
        let tab = app
            .tabs
            .iter()
            .find(|tab| tab.id == self.doc)
            .ok_or("Model task workbook was closed")?;
        if let Some(reason) = &self.failure {
            return Err(reason.clone());
        }
        if Instant::now() >= self.permit.expires {
            return Err(format!(
                "Model task expired after {} seconds. Existing model output was retained.",
                self.permit.budget_seconds
            ));
        }
        if tab.read_only || !self.permit.live() {
            return Err("Model task ended, expired, or belongs to another workbook".into());
        }
        let node = tab
            .doc
            .scene
            .node(self.node)
            .ok_or("Model task target was removed")?;
        let agent = slate_doc::agent_chat::agent(node).ok_or("Model task agent was removed")?;
        if node.locked
            || agent.provider != self.target.provider
            || !SlateApp::scoped_model_harness(&agent.provider)
            || agent.session != self.target.session
            || agent.model != self.target.model
            || agent.model_parameters != self.target.parameters
            || app
                .model_node_info_in(self.doc, self.node)
                .is_none_or(|info| info.path != self.target.path)
            || witness::Witness::capture(app, self.doc, self.node, &self.source_inputs)?
                != self.source_witness
            || (app.tab().id == self.doc
                && app.agent_input_snapshot(self.node)? != self.source_inputs)
            || (submitted
                && app
                    .agent_runtime_in(self.doc)
                    .and_then(|runtime| runtime.request_of(self.node))
                    != Some(self.permit.identity.request.as_str()))
        {
            return Err("Model task target, model, request, or declared sources changed".into());
        }
        let binding = app.native_binding_in(self.doc, self.node)?;
        if binding.key() != self.permit.binding.key()
            || binding.binding.mode != atlas_connector::SourceMode::IsolatedDraft
            || (self.pending.is_none()
                && (binding.binding.source.scope() != self.permit.binding.binding.source.scope()
                    || binding.observation != self.permit.binding.observation))
        {
            return Err("Model task native binding or source witness changed".into());
        }
        Ok(())
    }
}

impl SlateApp {
    pub(crate) fn retain_model_task_session_sources(&self, dirs: &mut HashSet<PathBuf>) {
        dirs.extend(
            self.connectors
                .headless
                .tasks
                .active
                .values()
                .map(|task| task.provider_dir.clone()),
        );
    }

    pub(crate) fn model_task_is_running(&self, node: NodeId) -> bool {
        let key = (self.tab().id, node);
        self.connectors.headless.tasks.staging.contains_key(&key)
            || self.connectors.headless.tasks.active.contains_key(&key)
            || self.connectors.headless.setup_sends.contains_key(&key)
    }

    pub(crate) fn queue_model_task_request(
        &mut self,
        node: NodeId,
        workspace: PathBuf,
        source_inputs: InputSnapshot,
        authored: String,
        mut request: AgentRequest,
    ) -> Result<(), String> {
        let key = (self.tab().id, node);
        if self.model_task_is_running(node) {
            return Err("This model task is already running".into());
        }
        let binding = self.native_binding(node)?;
        if binding.binding.mode != atlas_connector::SourceMode::IsolatedDraft
            || self.connectors.sessions[&key]
                .descriptor
                .as_ref()
                .is_none_or(|d| d.mode != SurfaceMode::Headless)
        {
            return Err("Model generation requires this target's isolated headless binding".into());
        }
        let agent = self
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::agent_chat::agent)
            .ok_or("Model agent is missing")?;
        if request.model != agent.model || request.model_parameters != agent.model_parameters {
            return Err("The captured Model choice changed before admission".into());
        }
        if request.model_parameters.get("effort").map(String::as_str) == Some("max") {
            self.agents
                .catalog(atlas_ai::agent::model_catalog(&agent.provider))
                .iter()
                .find(|model| {
                    request
                        .model
                        .as_deref()
                        .map_or(model.is_default, |id| id == model.id)
                })
                .ok_or("Refresh this Model's catalog before requesting MAX effort")?
                .validate_parameters(&request.model_parameters)?;
        }
        let root = self
            .agent_link_dir(node, &workspace)
            .ok_or("Model task link folder is missing")?;
        let session = BoardLinkSession {
            session_id: format!("model-task-{}", request.id),
            grant_id: format!("task-{}", request.id),
            workspace_id: format!("model-{}-{}", self.tab().id, node.0),
        };
        let grant = CollaboratorGrant {
            id: session.grant_id.clone(),
            workspace_id: session.workspace_id.clone(),
            readable_board: false,
            readable_nodes: std::iter::once(node.0.to_string())
                .chain(source_inputs.wired.iter().map(|item| item.node.to_string()))
                .collect(),
            readable_sources: Vec::new(),
            commands: COMMANDS.iter().map(|s| s.to_string()).collect(),
            conversation_ids: Vec::new(),
            mutation_autonomy: false,
            helper_limit: 0,
            max_actions: 32,
        };
        grant.validate().map_err(str::to_owned)?;
        session.validate().map_err(str::to_owned)?;
        request.collaborator = Some(session.clone());
        let budget_seconds = atlas_agent::collaborator::model_task_budget_seconds(&request);
        let admitted = Instant::now();
        let deadline_unix_ms =
            atlas_agent::collaborator::host_tools::deadline_after(budget_seconds)?;
        let authority = ModelTaskAuthority {
            node,
            doc: self.tab().id,
            permit: ModelTaskPermit {
                doc: self.tab().id,
                node,
                identity: AgentIdentity {
                    agent: agent.session.clone(),
                    membership: session.clone(),
                    request: request.id.clone(),
                    call: String::new(),
                    epoch: binding.observation,
                },
                binding,
                lease: Lease::new(),
                expires: admitted + Duration::from_secs(budget_seconds),
                deadline_unix_ms,
                budget_seconds,
            },
            target: Target {
                provider: agent.provider.clone(),
                session: agent.session.clone(),
                model: agent.model.clone(),
                parameters: agent.model_parameters.clone(),
                path: self.model_node_info(node).unwrap().path,
            },
            source_witness: witness::Witness::capture(self, self.tab().id, node, &source_inputs)?,
            source_inputs,
            provider_dir: root.clone(),
            provider_terminal: false,
            inputs: InputSnapshot::default(),
            workspace,
            dir: session.transport_dir(&root),
            grant,
            seen: HashSet::new(),
            next_sequence: 0,
            mutations: 0,
            pending: None,
            capture: None,
            accepted_path: None,
            model_accepted: false,
            failure: None,
        };
        request.collaborator = Some(session);
        request.inputs.context.clear(); // Only explicitly declared wires enter a generator task.
        let prep = self.begin_agent_input_preparation(request.inputs.clone())?;
        let model_sources = authority
            .source_inputs
            .wired
            .iter()
            .filter_map(|item| {
                let info = self.model_node_info(NodeId(item.node))?;
                item.images
                    .iter()
                    .any(|path| std::path::Path::new(path) == info.path)
                    .then_some((item.node, info.path))
            })
            .collect();
        self.connectors.headless.tasks.staging.insert(
            key,
            Staging {
                authority,
                request,
                prep,
                copies: None,
                authored,
                model_sources,
            },
        );
        Ok(())
    }

    pub(crate) fn cancel_model_tasks_for_doc(&mut self, doc: u64) {
        let tasks = &self.connectors.headless.tasks;
        let keys: Vec<_> = tasks
            .active
            .keys()
            .chain(tasks.staging.keys())
            .filter(|key| key.0 == doc)
            .copied()
            .collect();
        for key in keys {
            self.cancel_model_task(key);
        }
    }
    pub(in crate::app::application_connector) fn cancel_model_task(&mut self, key: (u64, NodeId)) {
        let tasks = &mut self.connectors.headless.tasks;
        if let Some(task) = tasks.staging.remove(&key) {
            tasks.links.revoke(&task.authority.dir);
        }
        if let Some(task) = tasks.active.remove(&key) {
            tasks.links.revoke(&task.dir);
        }
    }

    pub(super) fn pump_model_tasks(&mut self, ctx: &egui::Context) {
        let staging = std::mem::take(&mut self.connectors.headless.tasks.staging);
        for (key, mut staged) in staging {
            if !self.tabs.iter().any(|tab| tab.id == key.0) {
                continue;
            }
            if self.at_home || self.tab().id != key.0 {
                self.connectors.headless.tasks.staging.insert(key, staged);
                continue;
            }
            let result = (|| -> Result<bool, String> {
                staged.authority.validate(self, false)?;
                if !self.poll_model_task_inputs(key.1, &mut staged.prep, &mut staged.request)? {
                    return Ok(false);
                }
                if staged.copies.is_none() {
                    staged.copies = Some(inputs::freeze(
                        &staged.authority.dir,
                        staged.request.inputs.clone(),
                        staged.model_sources.clone(),
                        staged.authority.permit.clone(),
                    ));
                    return Ok(false);
                }
                match staged.copies.as_ref().unwrap().try_recv() {
                    Ok(result) => staged.request.inputs = result?,
                    Err(crossbeam_channel::TryRecvError::Empty) => return Ok(false),
                    Err(_) => return Err("Model task input preparation worker closed".into()),
                }
                staged.authority.validate(self, false)?;
                if staged.request.prompt.trim().is_empty() {
                    return Err(
                        "Write a prompt or connect a text instruction before generating a model"
                            .into(),
                    );
                }
                staged.authority.inputs = staged.request.inputs.clone();
                self.connectors
                    .headless
                    .tasks
                    .links
                    .set_session(
                        &staged.authority.dir,
                        staged.authority.permit.identity.membership.clone(),
                        staged.authority.grant.clone(),
                        staged.request.id.clone(),
                    )
                    .map_err(str::to_owned)?;
                if !self.dispatch_model_task_request(
                    key.1,
                    staged.request.clone(),
                    &staged.authority.workspace,
                    &staged.authored,
                    staged.authority.permit.deadline_unix_ms,
                ) {
                    return Err("Model task could not be submitted".into());
                }
                Ok(true)
            })();
            match result {
                Ok(true) => {
                    self.connectors
                        .headless
                        .tasks
                        .active
                        .insert(key, staged.authority);
                }
                Ok(false) => {
                    self.connectors.headless.tasks.staging.insert(key, staged);
                }
                Err(error) => {
                    self.connectors
                        .headless
                        .tasks
                        .links
                        .revoke(&staged.authority.dir);
                    self.fail_agent_await(key.1, error);
                }
            }
        }
        let active = std::mem::take(&mut self.connectors.headless.tasks.active);
        for (key, mut task) in active {
            if !self.tabs.iter().any(|tab| tab.id == key.0) {
                self.connectors.headless.tasks.links.revoke(&task.dir);
                continue;
            }

            if let Some(path) = &task.accepted_path {
                if let Ok(binding) = self.native_binding_in(key.0, key.1) {
                    if binding.key() == task.permit.binding.key()
                        && binding.binding.source.scope()
                            == task.permit.binding.binding.source.scope()
                        && binding
                            .binding
                            .source
                            .locator
                            .absolute
                            .as_deref()
                            .map(std::path::Path::new)
                            == Some(path.as_path())
                    {
                        task.permit.binding = binding;
                        task.accepted_path = None;
                    }
                }
            }
            let session = self.poll_model_task_session_in(key.0, key.1, &task.provider_dir);
            let terminal_error = session.as_ref().and_then(|session| match &session.status {
                atlas_agent::AgentStatus::Error(reason) => Some(reason.clone()),
                _ => None,
            });
            let terminal = session.is_some_and(|session| {
                session.request == task.permit.identity.request
                    && session.provider == task.target.provider
                    && matches!(
                        session.status,
                        atlas_agent::AgentStatus::Idle | atlas_agent::AgentStatus::Error(_)
                    )
            });
            task.provider_terminal |= terminal;
            if task.provider_terminal && task.pending.is_none() && task.capture.is_none() {
                if task.failure.is_some() || !task.model_accepted {
                    self.fail_agent_await_in(key.0, key.1, task.failure.clone().or(terminal_error).unwrap_or_else(|| "Model generation ended without a verified native model artifact; prior output retained".into()));
                }
                self.connectors.headless.tasks.links.revoke(&task.dir);
                continue;
            }
            if let Err(error) = task.validate(self, true) {
                self.connectors.headless.tasks.links.revoke(&task.dir);
                self.stop_model_task_provider_in(
                    key.0,
                    key.1,
                    &task.permit.identity.request,
                    &task.target.session,
                    &error,
                );
                self.fail_agent_await_in(key.0, key.1, error);
                continue;
            }
            calls::pump(self, &mut task);
            self.connectors.headless.tasks.active.insert(key, task);
        }
        if !self.connectors.headless.tasks.staging.is_empty()
            || !self.connectors.headless.tasks.active.is_empty()
        {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        let live = self
            .connectors
            .headless
            .tasks
            .staging
            .values()
            .map(|staged| staged.authority.dir.clone())
            .chain(
                self.connectors
                    .headless
                    .tasks
                    .active
                    .values()
                    .map(|task| task.dir.clone()),
            )
            .collect();
        self.connectors.headless.tasks.links.retain(&live);
    }
}
