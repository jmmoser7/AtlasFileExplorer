//! Typed dispatch beneath the single MCP lifecycle.
use super::*;
use atlas_agent::artifact_tools::{transport::PendingArtifactCalls, ArtifactIdentity};

pub(super) enum Route {
    Model {
        session: BoardLinkSession,
        request: String,
        workspace: PathBuf,
    },
    Artifact(ArtifactIdentity),
}
pub(super) struct Dispatch {
    pub token: Value,
    pub result: Value,
    pub audit: delivery::Call,
    pub ticket: Option<String>,
}
#[derive(Default)]
pub(super) struct Pending {
    model: PendingCalls,
    artifact: PendingArtifactCalls,
}
impl Route {
    pub fn server_name(&self) -> &'static str {
        match self {
            Self::Model { .. } => "slate-model-task",
            Self::Artifact(_) => "slate-artifact-task",
        }
    }
    pub fn request(&self) -> &str {
        match self {
            Self::Model { request, .. } => request,
            Self::Artifact(identity) => &identity.request_id,
        }
    }
    pub fn current(&self, dir: &Path, guard: CloudGuard) -> bool {
        match self {
            Self::Model {
                session, request, ..
            } => transport::binding_matches(dir, session, request, Some(guard)),
            Self::Artifact(identity) => {
                atlas_agent::artifact_tools::transport::binding_matches(dir, identity, Some(guard))
            }
        }
    }
    pub fn aliases(&self) -> Vec<String> {
        match self {
            Self::Model {
                session, request, ..
            } => tool_aliases(session, request).to_vec(),
            Self::Artifact(identity) => artifact::aliases(identity),
        }
    }
    pub fn schemas(&self) -> Value {
        match self {
            Self::Model {
                session, request, ..
            } => request_tool_schemas(session, request),
            Self::Artifact(identity) => artifact::schemas(identity),
        }
    }
    pub fn ready(&self) -> Value {
        let mut value = match self {
            Self::Model {
                session, request, ..
            } => transport::ready_value(session, request),
            Self::Artifact(identity) => {
                json!({"identity":identity,"request_id":identity.request_id,"initialized":true})
            }
        };
        value["tool_names"] = json!(self.aliases());
        value
    }
    pub fn instructions(&self) -> String {
        let names = self.aliases();
        match self {
            Self::Model {..} => format!("Explicit model task {}. Read frozen context with {} and invoke registered native commands with {}. Inspect the native command schema, then invoke the closed generator once. Completion requires the real native model artifact.",self.request(),names[0],names[1]),
            Self::Artifact(_) => format!("Explicit Artifact task {}. Read run context with {}, read exact offered inputs with {}, then write the complete self-contained index.html and matching return.json with {}. No other filesystem or native tools are offered. Existing host publication validates the result.",self.request(),names[0],names[1],names[2]),
        }
    }
}
impl Pending {
    pub fn start(
        &mut self,
        route: &Route,
        dir: &Path,
        token: Value,
        name: &str,
        arguments: &Value,
    ) -> Result<(), String> {
        match route {
            Route::Model {
                session, request, ..
            } => self.model.start(
                token,
                dir.to_path_buf(),
                session.clone(),
                request.clone(),
                parse_task_operation(name, arguments, session, request)?,
            ),
            Route::Artifact(identity) => self.artifact.start(
                token,
                dir.to_path_buf(),
                identity.clone(),
                artifact::parse(name, arguments, identity)?,
            ),
        }
    }
    pub fn poll(&mut self, route: &Route, cancel: &AtomicBool, guard: CloudGuard) -> Vec<Dispatch> {
        match route {
            Route::Model { workspace, .. } => self
                .model
                .poll(cancel, Some(guard))
                .into_iter()
                .map(|done| {
                    let content = reply_content(
                        workspace,
                        guard,
                        &done.call.operation,
                        done.reply.success,
                        &done.reply.result,
                    );
                    let ticket = done.reply.result["native_ticket"]
                        .as_str()
                        .map(str::to_owned);
                    Dispatch {
                        token: done.token,
                        result: tool_result(content, done.reply.success),
                        audit: delivery::Call::model(&done.call),
                        ticket,
                    }
                })
                .collect(),
            Route::Artifact(_) => self
                .artifact
                .poll(cancel, Some(guard))
                .into_iter()
                .map(|done| {
                    let content = artifact::content(&done.reply.result, done.reply.success, guard);
                    let canonical = match done.call.operation {
                        atlas_agent::artifact_tools::ArtifactOperation::ReadContext => {
                            "artifact_read_context"
                        }
                        atlas_agent::artifact_tools::ArtifactOperation::ReadFile { .. } => {
                            "artifact_read_file"
                        }
                        atlas_agent::artifact_tools::ArtifactOperation::WriteFile { .. } => {
                            "artifact_write_file"
                        }
                    };
                    Dispatch {
                        token: done.token,
                        result: tool_result(content, done.reply.success),
                        audit: delivery::Call {
                            request: done.call.identity.request_id.clone(),
                            binding: json!(done.call.identity),
                            id: done.call.id,
                            canonical,
                        },
                        ticket: None,
                    }
                })
                .collect(),
        }
    }
}
fn tool_result(content: Result<Vec<Value>, String>, success: bool) -> Value {
    match content {
        Ok(content) => json!({"content":content,"isError":!success}),
        Err(error) => json!({"content":[{"type":"text","text":error}],"isError":true}),
    }
}
