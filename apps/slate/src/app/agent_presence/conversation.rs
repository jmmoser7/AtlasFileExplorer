//! Durable visible conversation, independent of live presence and vendor sessions.
use super::*;

pub(super) fn conversation_messages(
    conversation: &Conversation,
) -> Vec<chrome::ConversationMessage<'_>> {
    conversation
        .turns
        .iter()
        .map(|turn| chrome::ConversationMessage {
            id: &turn.id,
            text: &turn.text,
            outgoing: turn.role == "user",
            system: turn.role == "system",
            status: &turn.status,
            attachment_count: conversation
                .message_images
                .get(&turn.request)
                .map_or(0, Vec::len),
        })
        .collect()
}
mod changes;
pub(super) use changes::ConversationChange;
use changes::ConversationRuntime;
#[cfg(test)]
mod delta_tests;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HumanProfile {
    pub id: String,
    pub name: String,
    /// Machine-local image locator; never embedded in a workbook or exported.
    pub photo: Option<PathBuf>,
}
impl Default for HumanProfile {
    fn default() -> Self {
        Self {
            id: if cfg!(test) {
                "test-local-human".into()
            } else {
                atlas_agent::request_id()
            },
            name: "You".into(),
            photo: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Conversation {
    #[serde(skip)]
    pub(super) runtime: ConversationRuntime,
    #[serde(default)]
    pub turns: Vec<ConversationTurn>,
    #[serde(default)]
    pub draft: String,
    #[serde(default)]
    pub bindings: Vec<ProviderBinding>,
    #[serde(default)]
    pub choices: std::collections::BTreeMap<String, ModelChoice>,
    #[serde(default)]
    pub artifacts: Vec<ConversationArtifact>,
    #[serde(default)]
    pub schedule: Option<atlas_ai::schedule::AgentSchedule>,
    /// Explicit human attachments; machine-local locators, never implicit board context.
    #[serde(default)]
    pub draft_images: Vec<PathBuf>,
    #[serde(default)]
    pub message_images: std::collections::BTreeMap<String, Vec<PathBuf>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ConversationArtifact {
    pub request: String,
    pub provider: String,
    pub dir: PathBuf,
    pub artifact: atlas_agent::AgentArtifact,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ModelChoice {
    pub model: Option<String>,
    #[serde(default)]
    pub parameters: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ConversationTurn {
    #[serde(default)]
    pub board_origin: Option<BoardMessageOrigin>,
    #[serde(default)]
    pub id: String,
    pub role: String,
    pub text: String,
    pub author: String,
    pub provider: Option<String>,
    pub request: String,
    pub at: u64,
    pub status: String,
}
/// Durable provenance only; this never restores a grant or execution job.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct BoardMessageOrigin {
    pub tab: u64,
    pub workbook: Option<PathBuf>,
    pub node: slate_doc::NodeId,
    pub text_hash: u64,
    pub recipient: String,
    pub human: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProviderBinding {
    pub provider: String,
    pub session: String,
    pub dir: PathBuf,
    pub through: usize,
}
impl Conversation {
    pub(super) fn replay(&self) -> Vec<atlas_agent::AgentTurn> {
        self.turns
            .iter()
            .filter(|t| matches!(t.role.as_str(), "user" | "assistant"))
            .filter(|t| {
                !t.text.is_empty()
                    && !matches!(
                        t.status.as_str(),
                        "queued" | "dispatching" | "not-sent" | "steering" | "unconfirmed"
                    )
            })
            .map(|t| atlas_agent::AgentTurn {
                role: t.role.clone(),
                text: t.text.clone(),
                at: t.at,
            })
            .collect()
    }
    pub(super) fn push(
        &mut self,
        role: &str,
        text: &str,
        author: &str,
        provider: Option<&str>,
        request: &str,
        status: &str,
    ) {
        self.append_turn(ConversationTurn {
            board_origin: None,
            id: atlas_agent::request_id(),
            role: role.into(),
            text: text.into(),
            author: author.into(),
            provider: provider.map(str::to_owned),
            request: request.into(),
            at: atlas_ai::context::now_secs(),
            status: status.into(),
        });
    }
    pub(super) fn reply_start(&self, a: &Collaborator, snapshot: &AgentSession) -> Option<usize> {
        if snapshot.request != a.request || a.request.is_empty() {
            return None;
        }
        let prompt = self
            .request_index(&a.request)
            .and_then(|index| self.turns.get(index))
            .filter(|t| t.role == "user")
            .map(|t| t.text.as_str())?;
        let mut matches = snapshot
            .turns
            .iter()
            .enumerate()
            .skip(a.reply_from)
            .filter(|(_, t)| t.role == "user" && t.text == prompt);
        // Native steering may repeat the original prompt verbatim. Its first
        // request-scoped user row remains the response boundary.
        let start = if a.profile.provider == "codex" {
            matches.next()
        } else {
            matches.next_back()
        };
        start.map(|(index, _)| index + 1)
    }
    /// Import only visible turns after the current submitted prompt, beyond the
    /// captured provider prefix. A new request ID alone is insufficient: native
    /// adapters publish Thinking before they append the new user turn.
    pub(super) fn absorb(&mut self, a: &Collaborator, snapshot: &AgentSession) -> bool {
        let Some(start) = self.reply_start(a, snapshot) else {
            return false;
        };
        let terminal = matches!(snapshot.status, atlas_agent::AgentStatus::Idle);
        let mut changed = false;
        for artifact in snapshot
            .artifacts
            .iter()
            .filter(|artifact| artifact.turn >= start)
        {
            let next = ConversationArtifact {
                request: a.request.clone(),
                provider: a.profile.provider.clone(),
                dir: a.dir.clone(),
                artifact: artifact.clone(),
            };
            changed |= self.set_artifact(next);
        }
        if terminal {
            for index in self.request_indices(&a.request).to_vec() {
                if self.turns[index].status != "complete" {
                    self.set_turn_status(index, "complete");
                    changed = true;
                }
            }
        }
        if let Some(index) = self.bindings.iter().position(|b| b.dir == a.dir) {
            let mut b = self.bindings[index].clone();
            if !snapshot.conversation.is_empty() && b.session != snapshot.conversation {
                b.session = snapshot.conversation.clone();
                self.set_binding(index, b);
                changed = true;
            }
        }
        let visible: Vec<_> = snapshot.turns[start..]
            .iter()
            .filter(|t| matches!(t.role.as_str(), "user" | "assistant") && !t.text.is_empty())
            .collect();
        let old_indices: Vec<_> = self
            .request_indices(&a.request)
            .iter()
            .copied()
            .filter(|index| self.turns[*index].role == "assistant")
            .collect();
        let old: Vec<_> = old_indices
            .iter()
            .map(|index| (&self.turns[*index].text, &self.turns[*index].status))
            .collect();
        let status = if terminal { "complete" } else { "streaming" };
        if visible.iter().all(|t| t.role == "assistant")
            && old.len() == visible.len()
            && old
                .iter()
                .zip(&visible)
                .all(|((text, state), t)| *text == &t.text && state.as_str() == status)
        {
            return self.acknowledge(&a.dir) || changed;
        }
        let old_ids: Vec<_> = old_indices
            .iter()
            .map(|index| self.turns[*index].id.clone())
            .collect();
        let first = self
            .request_index(&a.request)
            .map_or(self.turns.len(), |i| i + 1);
        let end = self.turns.len();
        let mut remaining: Vec<_> = self.turns[first..end]
            .iter()
            .filter(|turn| turn.request != a.request || turn.role != "assistant")
            .cloned()
            .collect();
        let mut replacement = Vec::new();
        let mut answer_index = 0;
        for turn in visible {
            if turn.role == "user" {
                if let Some(index) = remaining.iter().position(|row| {
                    row.role == "user"
                        && row.text == turn.text
                        && matches!(row.status.as_str(), "steering" | "complete" | "unconfirmed")
                }) {
                    let mut row = remaining.remove(index);
                    row.status = "complete".into();
                    replacement.push(row);
                }
                continue;
            }
            replacement.push(ConversationTurn {
                board_origin: None,
                id: old_ids
                    .get(answer_index)
                    .cloned()
                    .unwrap_or_else(atlas_agent::request_id),
                role: "assistant".into(),
                text: turn.text.clone(),
                author: a.profile.name.clone(),
                provider: Some(a.profile.provider.clone()),
                request: a.request.clone(),
                at: turn.at,
                status: status.into(),
            });
            answer_index += 1;
        }
        replacement.extend(remaining);
        if self.turns[first..end] == replacement {
            return self.acknowledge(&a.dir) || changed;
        }
        self.replace_turn_span(first, end, replacement);
        self.acknowledge(&a.dir);
        true
    }
    fn acknowledge(&mut self, dir: &std::path::Path) -> bool {
        let through = self.turns.len();
        if let Some(index) = self.bindings.iter().position(|b| b.dir == dir) {
            let mut binding = self.bindings[index].clone();
            let changed = binding.through != through;
            binding.through = through;
            if changed {
                self.set_binding(index, binding);
            }
            changed
        } else {
            false
        }
    }
    pub(super) fn stop(&mut self, request: &str) -> bool {
        if request.is_empty() {
            return false;
        }
        let mut changed = false;
        for index in self.request_indices(request).to_vec() {
            if self.turns[index].status != "complete" {
                self.set_turn_status(index, "stopped");
                changed = true;
            }
        }
        changed
    }
}

impl SlateApp {
    pub(super) fn presence_draft_attachments(&self, who: &str) -> Vec<chrome::Attachment<'_>> {
        self.presence_conversation(who)
            .into_iter()
            .flat_map(|conversation| &conversation.draft_images)
            .map(|path| chrome::Attachment {
                name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Image"),
            })
            .collect()
    }

    pub(super) fn saved_presence_participant<'a>(
        &'a self,
        profile: &'a StoredAgent,
        messages: &'a [chrome::ConversationMessage<'a>],
        message_total: usize,
        copy_messages: Option<&'a [chrome::ConversationMessage<'a>]>,
        attachments: &'a [chrome::Attachment<'a>],
        status: &'a str,
    ) -> chrome::Participant<'a> {
        chrome::Participant {
            id: &profile.id,
            name: &profile.name,
            hat: &profile.hat,
            status,
            provider: &profile.provider,
            human: false,
            inactive: true,
            paused: true,
            autonomous: false,
            full_access: false,
            file_inputs: false,
            own_chats: false,
            parent_name: None,
            chats: &[],
            local_profile: false,
            photo_texture: None,
            busy: false,
            messages,
            copy_messages,
            activity: &[],
            attachments,
            message_total,
            proposals: &[],
            draft: self
                .presence_conversation(&profile.id)
                .map_or("", |conversation| conversation.draft.as_str()),
            queued_provider: None,
            model: "",
            models: &[],
        }
    }

    pub(crate) fn presence_conversation(&self, who: &str) -> Option<&Conversation> {
        self.presence.conversations.get(who)
    }
    pub(crate) fn presence_history_status(&self) -> Option<String> {
        self.presence.conversations.status()
    }
    pub(crate) fn presence_save_draft(&mut self, who: &str, text: &str) -> Result<(), String> {
        if text.len() > 32_768 {
            return Err("Draft is too long".into());
        }
        self.presence.conversations.edit(who)?.set_draft(text);
        self.presence.conversations.dirty(who);
        Ok(())
    }
    pub(crate) fn presence_send_chat(&mut self, who: &str, text: &str) -> Result<(), String> {
        if who.starts_with("human:") {
            return self.presence_send_human(who, text);
        }
        if who != "self" {
            self.ensure_saved_presence_joined(who)?;
            let text = if text.trim().is_empty()
                && self
                    .presence_conversation(who)
                    .is_some_and(|c| !c.draft_images.is_empty())
            {
                "[Attached images]"
            } else {
                text
            };
            return self.send_presence_message(who, text);
        }
        if text.trim().is_empty() || text.len() > 32_768 {
            return Err("Message is empty or too long".into());
        }
        let name = self.settings.presence_identity.name.clone();
        let conversation = self.presence.conversations.edit("self")?;
        conversation.push("user", text.trim(), &name, None, "", "complete");
        conversation.set_draft("");
        self.presence.conversations.save("self");
        Ok(())
    }
    pub(crate) fn presence_update_identity(
        &mut self,
        name: &str,
        photo: Option<PathBuf>,
    ) -> Result<(), String> {
        let name = name.trim().chars().take(80).collect::<String>();
        if name.is_empty() || name.chars().any(char::is_control) {
            return Err("Choose a readable name".into());
        }
        self.settings.presence_identity = HumanProfile {
            id: self.settings.presence_identity.id.clone(),
            name,
            photo,
        };
        self.settings.save();
        Ok(())
    }
}
