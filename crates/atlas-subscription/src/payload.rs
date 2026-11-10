//! The actual Text bytes and a content-free receipt at their stdin boundary.
use crate::{process::Process, Provider};
use atlas_agent::AgentRequest;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(crate) struct TextPayload {
    pub(crate) text: String,
    pub(crate) progress: crate::progress::Progress,
    receipt_path: PathBuf,
    metadata: Value,
    pub(crate) images: Vec<crate::vision::ImageContent>,
}

impl TextPayload {
    pub(crate) fn new(request: &AgentRequest, provider: Provider, dir: &Path) -> Self {
        let text = if request.artifact.is_some() {
            request.input_text_in(Some(dir))
        } else if request
            .collaborator
            .as_ref()
            .is_some_and(|link| link.is_model_task())
        {
            let aliases = crate::host_tools::tool_aliases(
                request.collaborator.as_ref().unwrap(),
                &request.id,
            );
            let guide =
                atlas_agent::collaborator::board_link_guide(request, &dir.to_string_lossy());
            let aliased = guide
                .replace("slate_board_read", &aliases[0])
                .replace("slate_board_command", &aliases[1]);
            let mut text = request.input_text_in(Some(dir));
            let start = request.prompt_with_history().len();
            text.replace_range(start..start + guide.len(), &aliased);
            text
        } else {
            request.inline_text_output_prompt()
        };
        let projected = request.prompt_with_history();
        let sources: Vec<_> = crate::vision::input_items(request)
            .filter(|item| !item.text.is_empty()).map(|item| json!({
                "node":item.node,"slot":item.slot,"utf8_bytes":item.text.len(),"sha256":hash(item.text.as_bytes()),
                "current_prompt_occurrences":request.prompt.matches(item.text.as_str()).count(),
                "projected_input_occurrences":projected.matches(item.text.as_str()).count(),
                "rendered_input_occurrences":text.matches(item.text.as_str()).count(),
            })).collect();
        let direct_source = request
            .text_source
            .as_ref()
            .filter(|source| !source.is_empty())
            .map(|source| {
                json!({
                    "utf8_bytes":source.len(),"sha256":hash(source.as_bytes()),
                    "current_prompt_occurrences":request.prompt.matches(source.as_str()).count(),
                    "projected_input_occurrences":projected.matches(source.as_str()).count(),
                })
            });
        Self {
            progress: crate::progress::Progress::new(dir, &request.id),
            receipt_path: dir.join(format!(
                "subscription-dispatch-{}.json",
                hash(request.id.as_bytes())
            )),
            metadata: json!({"version":1,"request":request.id,"provider":provider.id(),
                "selected_model":request.model,"selected_model_parameters":request.model_parameters,"effective_model":null,"history_turn_count":request.history.len(),
                "content_types":["text"],"text_prompt_utf8_bytes":text.len(),"text_prompt_sha256":hash(text.as_bytes()),
                "source_metadata":sources,"direct_source_metadata":direct_source,
                "occurrence_scope":"decoded prompt/history projection; rendered_input_occurrences counts literal full rendered text (JSON quoting may escape source characters)",
                "write_status":"not_submitted"}),
            text,
            images: Vec::new(),
        }
    }

    pub(crate) fn cursor_wire(&self, session: &str) -> String {
        let mut content = vec![json!({"type":"text","text":self.text})];
        content.extend(self.images.iter().map(|image| image.cursor()));
        format!(
            "{}\n",
            json!({"jsonrpc":"2.0","id":5,"method":"session/prompt","params":{"sessionId":session,"prompt":content}})
        )
    }
    pub(crate) fn capture_images(
        &mut self,
        request: &AgentRequest,
        guard: Option<crate::vision::CloudGuard>,
    ) -> Result<(), String> {
        self.images = crate::vision::load(request, guard)?;
        self.metadata["image_metadata"] = json!(self
            .images
            .iter()
            .map(|image| image.receipt())
            .collect::<Vec<_>>());
        if !self.images.is_empty() {
            self.metadata["content_types"] = json!(["text", "image"]);
        }
        Ok(())
    }
    pub(crate) fn claude_wire(&self) -> String {
        let mut content = vec![json!({"type":"text","text":self.text})];
        content.extend(self.images.iter().map(|image| image.claude()));
        format!(
            "{}\n",
            json!({"type":"user","message":{"role":"user","content":content},"parent_tool_use_id":null})
        )
    }

    /// Persist prepared metadata before a paid request can begin, then qualify
    /// successful write+flush. No raw source, prompt, keys or thinking is stored.
    pub(crate) fn send(
        &mut self,
        process: &mut Process,
        wire: &str,
        format: &str,
    ) -> Result<(), String> {
        self.metadata["wire_format"] = json!(format);
        self.metadata["wire_utf8_bytes"] = json!(wire.len());
        self.metadata["wire_sha256"] = json!(hash(wire.as_bytes()));
        self.metadata["write_status"] = json!("prepared_before_stdin");
        self.persist()?;
        self.progress
            .note("user_prompt_before_stdin", "", format, None);
        if let Err(error) = process.write(wire) {
            self.progress
                .note("user_prompt_stdin_failed", "", format, None);
            self.metadata["write_status"] = json!("stdin_write_failed");
            let _ = self.persist();
            return Err(error);
        }
        self.progress
            .note("user_prompt_stdin_flushed", "", format, None);
        self.metadata["write_status"] = json!("stdin_written_and_flushed");
        self.persist()
    }

    pub(crate) fn observe_model(&mut self, model: &str, evidence: &str) {
        if model.is_empty() || model.len() > 256 || model.contains(['\n', '\r']) {
            return;
        }
        self.metadata["effective_model"] = json!(model);
        self.metadata["effective_model_evidence"] = json!(evidence);
        let _ = self.persist();
    }
    pub(crate) fn observe_host_ready(&mut self, tools: Option<&atlas_agent::helper::HelperLaunch>) {
        if let Some(tools) = tools {
            let identity = match tools {
                atlas_agent::helper::HelperLaunch::Model(model) => {
                    json!({"kind":"model","session":model.session,"deadline_unix_ms":model.deadline_unix_ms})
                }
                atlas_agent::helper::HelperLaunch::Artifact(artifact) => {
                    json!({"kind":"artifact","identity":artifact.identity,"deadline_unix_ms":artifact.deadline_unix_ms})
                }
            };
            self.metadata["host_tools"] = json!({"request":tools.request_id(),"scope":identity,"initialized":crate::tool_client::initialized(tools),"tools_listed":crate::tool_client::ready(tools),"tool_names":crate::host_tools::helper_tool_aliases(tools)});
        }
    }
    pub(crate) fn observe_mcp_status(&mut self, status: &str) {
        self.metadata["vendor_mcp_slate_status"] = json!(status);
        let _ = self.persist();
    }

    pub(crate) fn terminal(&mut self, completed: bool) {
        self.metadata["terminal_result"] = json!(if completed { "completed" } else { "failed" });
        let _ = self.persist();
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.receipt_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!("Could not create subscription dispatch receipt folder: {error}")
            })?;
        }
        let bytes = serde_json::to_vec_pretty(&self.metadata).map_err(|error| error.to_string())?;
        let temporary = self.receipt_path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes)
            .and_then(|_| std::fs::rename(&temporary, &self.receipt_path))
            .map_err(|error| format!("Could not retain subscription dispatch metadata: {error}"))
    }
}

pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}
