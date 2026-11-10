//! First-bind fixture access stays beside the actual pending editor owner.
use super::*;

impl SlateApp {
    pub(crate) fn submit_pending_model_task_for_test(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        prompt: &str,
    ) {
        assert_eq!(self.pending_generator_host(id), Some(PendingHost::Model));
        self.prepare_pending_chat_choice(id);
        let mut draft = self.agents.flow.drafts.get(&id).unwrap().clone();
        draft.prompt = prompt.into();
        self.submit_pending_generator(ctx, id, &mut draft);
        self.agents.flow.drafts.insert(id, draft);
    }
}
