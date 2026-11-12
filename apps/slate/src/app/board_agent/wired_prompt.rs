//! Project live wired instructions into composers without authoring them.
use super::*;
use atlas_agent::PortalView;

impl SlateApp {
    pub(crate) fn has_dynamic_prompt_inputs(&self, inputs: &atlas_agent::InputSnapshot) -> bool {
        inputs
            .on(atlas_agent::InputSlot::Prompt)
            .any(|item| slate_doc::agent_inputs::is_web_page(&self.doc().scene, NodeId(item.node)))
    }

    pub(crate) fn has_dynamic_prompt_source(&self, id: NodeId) -> bool {
        self.agent_input_snapshot(id)
            .is_ok_and(|inputs| self.has_dynamic_prompt_inputs(&inputs))
    }

    pub(crate) fn prompt_with_inputs(
        inputs: &atlas_agent::InputSnapshot,
        authored: &str,
    ) -> String {
        let wired = inputs
            .on(atlas_agent::InputSlot::Prompt)
            .map(|item| item.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        join_prompt(&wired, authored, "\n\n")
    }

    fn prompt_projection(&self, id: NodeId) -> (String, &'static str) {
        if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|agent| agent.view == PortalView::Images)
        {
            return (self.generator_view(id).prompt.clone(), ", ");
        }
        (self.generator_view(id).wired_prompt.clone(), "\n\n")
    }

    pub(crate) fn agent_prompt_text(&self, id: NodeId) -> String {
        self.project_agent_prompt(id, self.authored_agent_prompt_text(id))
    }

    pub(crate) fn authored_agent_prompt_text(&self, id: NodeId) -> &str {
        let authored = self
            .agents
            .prompt(id)
            .filter(|_| {
                self.pending_generator_host(id) == Some(crate::app::board_flow::PendingHost::Chat)
            })
            .or_else(|| self.pending_generator_prompt(id))
            .or_else(|| self.agents.prompt(id))
            .or_else(|| {
                self.doc()
                    .scene
                    .node(id)
                    .and_then(slate_doc::agent_chat::agent)
                    .map(|agent| agent.instruction.as_str())
            })
            .unwrap_or("");
        authored
    }

    /// Capture static image annotations separately from live Prompt sources.
    pub(crate) fn image_non_prompt_instructions(
        &self,
        inputs: &atlas_agent::InputSnapshot,
    ) -> String {
        let mut extra = Vec::new();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for item in &inputs.wired {
            let slot = item.port();
            if !slot.takes_text() && !item.images.is_empty() {
                self.append_image_region_prompts(NodeId(item.node), slot, &mut hash, &mut extra);
            } else if slot != atlas_agent::InputSlot::Prompt && !item.text.trim().is_empty() {
                extra.push(item.text.trim().to_owned());
            }
        }
        extra.join(", ")
    }

    pub(crate) fn project_agent_prompt(&self, id: NodeId, authored: &str) -> String {
        let (wired, separator) = self.prompt_projection(id);
        join_prompt(&wired, authored, separator)
    }

    pub(crate) fn authored_agent_prompt_edit(
        &self,
        id: NodeId,
        text: &str,
        previous: &str,
    ) -> String {
        let (wired, separator) = self.prompt_projection(id);
        if wired.is_empty() {
            return text.to_owned();
        }
        let prefix = format!("{wired}{separator}");
        if let Some(authored) = text.strip_prefix(&prefix) {
            return authored.to_owned();
        }
        let before = join_prompt(&wired, previous, separator);
        let common = before
            .chars()
            .zip(text.chars())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        let suffix = before[common..]
            .chars()
            .rev()
            .zip(text[common..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        if common == before.len() && suffix == 0 {
            return text[common..].to_owned();
        }
        if suffix == 0 && common < prefix.len().min(before.len()) {
            return text.to_owned();
        }
        // Edits confined to linked words never author a stale copy. An edit crossing
        // into the suffix keeps its replacement and the surviving authored tail.
        if before.len() - suffix <= prefix.len().min(before.len()) {
            previous.to_owned()
        } else {
            text[common..].to_owned()
        }
    }

    pub(crate) fn sync_projected_prompt_cursor(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        edited: &str,
        authored: &str,
    ) {
        let projected = self.project_agent_prompt(id, authored);
        if projected == edited {
            return;
        }
        if let Some(field) = ctx.memory(|memory| memory.focused()) {
            if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, field) {
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(
                        egui::text::CCursor::new(projected.chars().count()),
                    )));
                state.store(ctx, field);
            }
        }
    }

    /// Only the edited suffix is authored; the wire remains a live projection.
    pub(crate) fn edit_agent_prompt_text(&mut self, id: NodeId, text: String) {
        let previous = self.agents.prompt(id).unwrap_or("");
        let authored = self.authored_agent_prompt_edit(id, &text, previous);
        if authored != previous && self.tab().journal.can_redo() {
            self.agents.prompt_fit_edit =
                Some((self.tab().id, self.tab().journal.undo_depth(), id));
        }
        *self.agents.prompt_mut(id) = authored;
        self.agents.prompt_epoch = self.agents.prompt_epoch.wrapping_add(1);
    }
}

fn join_prompt(wired: &str, authored: &str, separator: &str) -> String {
    if wired.is_empty() || wired == authored {
        authored.to_owned()
    } else if authored.is_empty() {
        wired.to_owned()
    } else {
        format!("{wired}{separator}{authored}")
    }
}
