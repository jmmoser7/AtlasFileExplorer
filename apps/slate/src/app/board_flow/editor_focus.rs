//! The shared instruction draft and prompt keyboard-ownership lifecycle.
use super::*;

impl SlateApp {
    /// Keep the draft and the journaled instruction in step: the document
    /// wins when it changed (undo, reload); otherwise the draft is committed
    /// once its field loses the caret.
    pub(crate) fn sync_instruction(&mut self, id: NodeId, journaled: &str) {
        let doc_changed = self.agents.flow.synced.get(&id).map(String::as_str) != Some(journaled);
        if doc_changed || !self.agents.has_draft(id) {
            *self.agents.prompt_mut(id) = journaled.to_string();
            self.agents.flow.synced.insert(id, journaled.to_string());
            return;
        }
        let draft = self.agents.prompt_mut(id).clone();
        if self.agents.composing(id) || draft == journaled || self.tab().read_only {
            return;
        }
        self.patch_nodes(&[id], |n| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                a.instruction = draft.clone();
            }
        });
        self.agents.flow.synced.insert(id, draft);
    }

    pub(crate) fn take_flow_prompt_focus(&mut self, id: NodeId) -> bool {
        let take_focus = self.agents.flow.focus_prompt == Some(id);
        if take_focus {
            self.agents.flow.focus_prompt = None;
        }
        take_focus
    }

    pub(crate) fn track_flow_prompt_focus(&mut self, id: NodeId, focused: bool) {
        if focused {
            self.agents.flow.typing = Some(id);
            self.web_release_keyboard();
        } else if self.agents.flow.typing == Some(id) {
            self.agents.flow.typing = None;
        }
    }
}
