//! Validated task-local harness handoff. It grants no additional authority.
use super::conversation::{ModelChoice, ProviderBinding};
use super::*;

impl PresenceRuntime {
    pub(super) fn cancel_pending_ancestors(&mut self, who: &str) {
        let mut current = Some(who.to_owned());
        let mut visited = std::collections::HashSet::new();
        while let Some(who) = current.take() {
            if !visited.insert(who.clone()) {
                break;
            }
            let Some(a) = self.agents.iter_mut().find(|a| a.profile.id == who) else {
                break;
            };
            a.pending_harness = None;
            current = a.parent.clone();
        }
    }
    pub(super) fn descendants(&self, who: &str) -> Vec<String> {
        let mut ids = vec![who.to_owned()];
        let mut at = 0;
        while at < ids.len() {
            let parent = ids[at].clone();
            for a in self
                .agents
                .iter()
                .filter(|a| a.parent.as_deref() == Some(&parent))
            {
                if !ids.contains(&a.profile.id) {
                    ids.push(a.profile.id.clone());
                }
            }
            at += 1;
        }
        ids
    }
    pub(super) fn task_busy(&self, who: &str) -> bool {
        let ids = self.descendants(who);
        self.agents
            .iter()
            .filter(|a| ids.contains(&a.profile.id) && a.departure.is_none())
            .any(|a| {
                a.inflight
                    || a.read.is_some()
                    || a.work.is_some()
                    || a.proposed.is_some()
                    || !a.launched.is_empty()
                    || a.snapshot.as_ref().is_some_and(|s| s.approval.is_some())
            })
    }
}
impl SlateApp {
    pub(crate) fn presence_provider_problem(&self, provider: &str) -> Option<String> {
        if provider == "local" {
            return None;
        }
        if !matches!(provider, "codex" | "cursor") {
            return Some("Unsupported collaborator harness".into());
        }
        if !self.ai.packs.ok(provider) {
            let detail = self.ai.packs.detail(provider);
            return Some(if detail.is_empty() {
                format!("Connect {provider} in Preferences → Connections first")
            } else {
                detail
            });
        }
        if let Some(error) = self.agents.catalog_error(provider) {
            return Some(error.to_owned());
        }
        if !self.agents.catalog_ready(provider) {
            return Some("Checking connection and models…".into());
        }
        None
    }
    /// choose opens busy choices; after-task binds to the current request;
    /// stop explicitly cancels descendants. cancel dismisses pending choices.
    pub(crate) fn presence_set_harness(
        &mut self,
        who: &str,
        provider: &str,
        mode: &str,
    ) -> Result<(), String> {
        let a = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who && a.departure.is_none())
            .ok_or("Collaborator is not present")?;
        if mode == "cancel" {
            let a = self
                .presence
                .agents
                .iter_mut()
                .find(|a| a.profile.id == who)
                .unwrap();
            a.pending_harness = None;
            a.switch_offer = None;
            return Ok(());
        }
        if a.profile.provider == provider {
            return Ok(());
        }
        if let Some(problem) = self.presence_provider_problem(provider) {
            return Err(problem);
        }
        self.presence.conversations.edit(who)?;
        if self.presence.task_busy(who) {
            match mode {
                "choose" => {
                    self.presence
                        .agents
                        .iter_mut()
                        .find(|a| a.profile.id == who)
                        .unwrap()
                        .switch_offer = Some(provider.into());
                    return Ok(());
                }
                "after-task" => {
                    let a = self
                        .presence
                        .agents
                        .iter_mut()
                        .find(|a| a.profile.id == who)
                        .unwrap();
                    a.pending_harness = Some((provider.into(), a.request.clone()));
                    a.switch_offer = None;
                    return Ok(());
                }
                "stop" => self.stop_presence(who, false)?,
                _ => return Err("Choose After this task, Stop task and switch, or Cancel".into()),
            }
        }
        self.apply_presence_harness(who, provider)
    }
    fn apply_presence_harness(&mut self, who: &str, provider: &str) -> Result<(), String> {
        // Recheck immediately before committing a queued choice.
        if let Some(problem) = self.presence_provider_problem(provider) {
            return Err(problem);
        }
        let ws = self
            .ai
            .config
            .valid_workspace()
            .ok_or("The AI workspace is unavailable")?
            .to_path_buf();
        let a = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == who)
            .ok_or("Unknown collaborator")?;
        self.presence.links.revoke(&a.dir);
        if let Some(runtime) = a.runtime.take() {
            runtime.stop();
        }
        if let Some(cursor) = a.cursor.take() {
            cursor.stop();
        }
        a.profile.provider = provider.into();
        // A directory and explicit grant lookup identity always agree. The old
        // vendor binding remains provenance, never a resumable live authority.
        a.binding.session_id = atlas_agent::request_id();
        a.dir = atlas_ai::agent::agent_dir(&ws, &a.binding.session_id);
        a.snapshot = None;
        a.request.clear();
        a.pending_harness = None;
        a.switch_offer = None;
        a.chat_only = false;
        a.reply_from = 0;
        a.status = "Ready; new harness on the next message".into();
        a.activity = "idle".into();
        let thread = self.presence.conversations.edit(who)?;
        let label = atlas_ai::agent::provider_by_id(provider).display_name;
        thread.push(
            "system",
            &format!("{label} will be used for the next message. Earlier messages remain visible."),
            "Slate",
            None,
            "",
            "complete",
        );
        self.presence.conversations.save(who);
        self.persist_presence_profiles();
        Ok(())
    }
    pub(super) fn finish_presence_handoffs(&mut self) {
        let choices: Vec<_> = self
            .presence
            .agents
            .iter()
            .filter_map(|a| {
                a.pending_harness.as_ref().map(|(provider, request)| {
                    (
                        a.profile.id.clone(),
                        provider.clone(),
                        request.clone(),
                        a.request.clone(),
                        a.had_response,
                    )
                })
            })
            .collect();
        for (who, provider, request, current, responded) in choices {
            if request != current || !responded {
                continue;
            }
            if !self.presence.task_busy(&who) {
                if let Err(error) = self.apply_presence_harness(&who, &provider) {
                    if let Some(a) = self
                        .presence
                        .agents
                        .iter_mut()
                        .find(|a| a.profile.id == who)
                    {
                        a.pending_harness = None;
                        a.status = error;
                        a.activity = "blocked".into();
                    }
                }
            }
        }
    }
    pub(crate) fn presence_model_choice(&self, who: &str, provider: &str) -> ModelChoice {
        self.presence_conversation(who)
            .and_then(|c| c.choices.get(provider))
            .cloned()
            .unwrap_or_default()
    }
    pub(crate) fn presence_set_model(
        &mut self,
        who: &str,
        model: Option<String>,
        parameters: std::collections::BTreeMap<String, String>,
    ) -> Result<(), String> {
        let provider = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .ok_or("Unknown collaborator")?
            .profile
            .provider
            .clone();
        self.validate_presence_model(&provider, model.as_deref(), &parameters)?;
        self.presence
            .conversations
            .edit(who)?
            .set_choice(provider, ModelChoice { model, parameters });
        self.presence.conversations.save(who);
        Ok(())
    }
    pub(super) fn validate_presence_model(
        &self,
        provider: &str,
        model: Option<&str>,
        parameters: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), String> {
        match model {
            None if parameters.is_empty() => Ok(()),
            Some(id) => self
                .agents
                .catalog(provider)
                .iter()
                .find(|m| m.id == id)
                .ok_or("The selected model is unavailable; choose a model again")?
                .validate_parameters(parameters),
            None => Err("Choose a model before changing its parameters".into()),
        }
    }
    pub(super) fn note_presence_binding(
        &mut self,
        who: &str,
        provider: &str,
        dir: &std::path::Path,
    ) -> Result<(), String> {
        let thread = self.presence.conversations.edit(who)?;
        if !thread.bindings.iter().any(|b| b.dir == dir) {
            thread.set_binding(
                thread.bindings.len(),
                ProviderBinding {
                    provider: provider.into(),
                    session: String::new(),
                    dir: dir.into(),
                    through: thread.turns.len(),
                },
            );
        }
        Ok(())
    }
}
