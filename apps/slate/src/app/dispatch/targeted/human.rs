//! Human command front end; preparation and commit remain the same owners.
use super::*;
impl SlateApp {
    pub(crate) fn dispatch_targeted_human(&mut self, command: &str, detail: Option<&str>) -> bool {
        self.dispatch_targeted_human_context(None, command, detail)
    }
    pub(crate) fn dispatch_targeted_human_with_context(
        &mut self,
        ctx: &eframe::egui::Context,
        command: &str,
        detail: Option<&str>,
    ) -> bool {
        self.dispatch_targeted_human_context(Some(ctx), command, detail)
    }
    fn dispatch_targeted_human_context(
        &mut self,
        ctx: Option<&eframe::egui::Context>,
        command: &str,
        detail: Option<&str>,
    ) -> bool {
        let result = (|| {
            let value: Value = serde_json::from_str(
                detail.ok_or("This command requires explicit targets and arguments")?,
            )
            .map_err(|e| e.to_string())?;
            let targets: Vec<String> = serde_json::from_value(
                value
                    .get("targets")
                    .cloned()
                    .unwrap_or_else(|| Value::Array(vec![])),
            )
            .map_err(|_| "Targets must be stable object ID strings")?;
            let mut prepared = self.prepare_targeted_command_context(
                ctx,
                command,
                &targets,
                value.get("arguments").unwrap_or(&Value::Null),
            )?;
            self.check_targeted_command(&prepared)?;
            for tag in &mut prepared.tag_commands {
                tag.set_author(slate_doc::scene::CmdAuthor::Human);
            }
            if let Some(invocation) = prepared
                .invocation
                .take()
                .filter(|invocation| invocation.pending())
            {
                return self
                    .finish_deferred_invocation(
                        invocation,
                        prepared.cmds,
                        slate_doc::scene::CmdAuthor::Human,
                    )
                    .map(|_| ());
            }
            let id = super::super::commands::SPECS
                .iter()
                .find(|spec| spec.id.0 == command)
                .map(|spec| spec.id)
                .ok_or("Unknown board command")?;
            let removed = prepare::removed_ids(&prepared.cmds);
            let added = prepare::added_wire_ids(&prepared.cmds);
            let model_output = prepared.model_output_target(self);
            let created = prepared.created_ids();
            let history_cut = prepared.history_cut;
            if let Some(target) = history_cut {
                self.cancel_generator_history_work(target);
            }
            if !prepared.cmds.is_empty() && !self.commit_scene(prepared.cmds) {
                return Err("The journal refused the command".into());
            }
            if !prepared.tag_commands.is_empty() && !self.commit_tag_commands(prepared.tag_commands)
            {
                return Err("The tag journal refused the command".into());
            }
            self.finish_targeted_bookkeeping(&removed, &added);
            self.finish_image_history_action(prepared.history_action.as_ref());
            if let Some((source, action)) = &prepared.history_action {
                if action == "downstream" {
                    self.board_sel.remove(source);
                    for id in &created {
                        self.board_sel.remove(id);
                    }
                    if self.shape_properties.ids.contains(source) {
                        self.shape_properties.panel = None;
                    }
                }
            }
            if let Some(target) = history_cut {
                self.finish_generator_history_cut(target);
            }
            if let Some(node) = model_output {
                self.finish_model_output_selection(node);
            }
            self.push_history(id, Some(format!("{} objects", prepared.targets.len())));
            Ok::<(), String>(())
        })();
        match result {
            Ok(()) => true,
            Err(error) => {
                self.toast(error);
                false
            }
        }
    }
}
