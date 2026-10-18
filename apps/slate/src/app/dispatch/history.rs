//! Attributed command intent, shared by every caller of the dispatcher.
use super::*;

impl SlateApp {
    pub(crate) fn push_history(&mut self, id: CommandId, detail: Option<String>) {
        self.push_history_as(id, detail, CmdAuthor::Human);
    }

    pub(crate) fn push_history_as(
        &mut self,
        id: CommandId,
        detail: Option<String>,
        author: CmdAuthor,
    ) {
        self.feedback.on_history(id.0);
        let Some(spec) = self.registry.by_id(id) else {
            return;
        };
        self.cmd_history.push(HistoryEntry {
            id,
            name: spec.name,
            author,
            detail,
            at: std::time::SystemTime::now(),
        });
        self.brush_setting_undo.clear();
    }
}
