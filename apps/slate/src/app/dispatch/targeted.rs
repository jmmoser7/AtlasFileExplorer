//! One typed, selection-independent command entry for people and collaborators.

mod creation;
mod invocation;
mod prepare;
mod schema;
mod text_style;

use super::super::SlateApp;
use atlas_commands::CmdAuthor;
use serde_json::Value;
use slate_doc::scene::{Node, NodeId, Scene, SceneCmd};
use slate_doc::stage::{Proposal, ProposalStatus, ProposalTarget};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct PreparedBoardCommand {
    pub command: String,
    pub cmds: Vec<SceneCmd>,
    pub targets: Vec<NodeId>,
    workbook: Option<PathBuf>,
    tab_id: u64,
    expected: Vec<Node>,
    order: Option<Vec<NodeId>>,
    invocation: Option<invocation::DeferredInvocation>,
    crop: Option<super::super::board_crop::CropGesture>,
    linked_items: Vec<slate_doc::SlateItem>,
}

impl PreparedBoardCommand {
    pub(crate) fn crop_gesture(&self) -> Option<super::super::board_crop::CropGesture> {
        self.crop
    }

    pub(crate) fn created_ids(&self) -> Vec<NodeId> {
        self.cmds
            .iter()
            .filter_map(|cmd| match cmd {
                SceneCmd::Add { node, .. }
                    if !self.expected.iter().any(|old| old.id == node.id) =>
                {
                    Some(node.id)
                }
                _ => None,
            })
            .collect()
    }

    pub(crate) fn requires_linked_inputs(&self) -> bool {
        !self.linked_items.is_empty()
            || self
                .invocation
                .as_ref()
                .is_some_and(invocation::DeferredInvocation::requires_linked_inputs)
    }
    /// Explicit existing targets for commands whose effect is a provider job.
    pub(crate) fn witness_nodes(&self, scene: &Scene) -> Vec<Node> {
        if let Some(invocation) = &self.invocation {
            return scene
                .node(invocation.target())
                .cloned()
                .into_iter()
                .collect();
        }
        self.targets
            .iter()
            .filter_map(|id| scene.node(*id).cloned())
            .collect()
    }
    pub(crate) fn tab_id(&self) -> u64 {
        self.tab_id
    }
    pub(crate) fn workbook(&self) -> Option<&Path> {
        self.workbook.as_deref()
    }

    /// A preview never licenses applying stale before-states after human edits.
    pub(crate) fn applicable(&self, scene: &Scene) -> Result<(), String> {
        self.expected_matches(scene)?;
        let mut scratch = scene.clone();
        for cmd in &self.cmds {
            match cmd {
                SceneCmd::Patch { before, .. }
                    if scratch.node(before.id) != Some(before.as_ref()) =>
                {
                    return Err(format!("Object {} changed", before.id.0));
                }
                SceneCmd::Remove { node, .. } if scratch.node(node.id) != Some(node) => {
                    return Err(format!("Object {} changed", node.id.0));
                }
                _ => {}
            }
            if !scratch.apply(cmd) {
                return Err("The prepared command no longer applies to this board".into());
            }
        }
        Ok(())
    }

    /// Cheap conflict check for live previews; full preflight runs at commit.
    pub(crate) fn expected_matches(&self, scene: &Scene) -> Result<(), String> {
        for expected in &self.expected {
            if scene.node(expected.id) != Some(expected) {
                return Err(format!(
                    "Object {} changed while the action was prepared",
                    expected.id.0
                ));
            }
            if expected.locked
                && self
                    .invocation
                    .as_ref()
                    .is_none_or(|invocation| invocation.target() == expected.id)
            {
                return Err(format!("Object {} is locked", expected.id.0));
            }
        }
        if self.order.as_ref().is_some_and(|order| {
            !order
                .iter()
                .copied()
                .eq(scene.nodes.iter().map(|node| node.id))
        }) {
            return Err("The board order changed while the action was prepared".into());
        }
        Ok(())
    }
}

impl SlateApp {
    pub(crate) fn targeted_command_schema() -> Value {
        schema::value()
    }

    pub(crate) fn prepare_targeted_command(
        &self,
        command: &str,
        targets: &[String],
        arguments: &Value,
    ) -> Result<PreparedBoardCommand, String> {
        self.prepare_targeted_command_context(None, command, targets, arguments)
    }
    pub(crate) fn prepare_targeted_command_with_context(
        &self,
        ctx: &eframe::egui::Context,
        command: &str,
        targets: &[String],
        arguments: &Value,
    ) -> Result<PreparedBoardCommand, String> {
        self.prepare_targeted_command_context(Some(ctx), command, targets, arguments)
    }
    pub(crate) fn prepare_targeted_command_context(
        &self,
        ctx: Option<&eframe::egui::Context>,
        command: &str,
        targets: &[String],
        arguments: &Value,
    ) -> Result<PreparedBoardCommand, String> {
        if self.tab().read_only {
            return Err("This workbook is read-only".into());
        }
        if super::super::commands::SPECS
            .iter()
            .all(|spec| spec.id.0 != command)
        {
            return Err("Unknown board command".into());
        }
        if command == "board.objects.crop" && targets.len() != 1 {
            return Err("Crop requires exactly one stable image ID".into());
        }
        let ids = prepare::target_ids(&self.doc().scene, targets)?;
        let (cmds, targets, invocation, crop) = if command == "board.objects.crop" {
            let (cmds, targets, gesture) = prepare::crop(self, &ids, arguments)?;
            (cmds, targets, None, Some(gesture))
        } else if matches!(
            command,
            "board.objects.set_text" | "board.shape.edit" | "board.wire.edit"
        ) {
            let (cmds, targets) = text_style::prepare(self, ctx, command, &ids, arguments)?;
            (cmds, targets, None, None)
        } else if command == "portal.agent.run" {
            let (cmds, targets, invocation) = invocation::prepare(self, &ids, arguments)?;
            (cmds, targets, Some(invocation), None)
        } else {
            let (cmds, targets) = prepare::commands(self, command, &ids, arguments)?;
            (cmds, targets, None, None)
        };
        if cmds.is_empty() && invocation.is_none() {
            return Err("The command would make no changes".into());
        }
        let mut expected: Vec<Node> = targets
            .iter()
            .filter_map(|id| self.doc().scene.node(*id).cloned())
            .collect();
        for cmd in &cmds {
            let node = match cmd {
                SceneCmd::Patch { before, .. } => Some(before.as_ref()),
                SceneCmd::Remove { node, .. } => Some(node),
                _ => None,
            };
            if let Some(node) = node.filter(|node| !expected.iter().any(|n| n.id == node.id)) {
                expected.push(node.clone());
            }
        }
        let linked_items = if command == "board.objects.place_images" {
            creation::linked_items(self.doc(), &cmds)
        } else {
            Vec::new()
        };
        let prepared = PreparedBoardCommand {
            command: command.to_owned(),
            cmds,
            targets,
            workbook: self.tab().path.clone(),
            tab_id: self.tab().id,
            expected,
            order: matches!(command, "board.objects.order" | "board.objects.delete")
                .then(|| self.doc().scene.nodes.iter().map(|node| node.id).collect()),
            invocation,
            crop,
            linked_items,
        };
        prepared.applicable(&self.doc().scene)?;
        Ok(prepared)
    }

    /// Called only after witnessed placement; authority is checked by the host.
    pub(crate) fn finish_targeted_command(
        &mut self,
        mut prepared: PreparedBoardCommand,
        author: &str,
        autonomy: bool,
        session: &str,
    ) -> Result<String, String> {
        self.check_targeted_command(&prepared)?;
        if author.trim().is_empty() || author.chars().count() > 128 || session.len() > 128 {
            return Err("A bounded participant identity and session are required".into());
        }
        if let Some(invocation) = prepared.invocation.take() {
            if !autonomy {
                return Err("Grant autonomous board edits before invoking provider jobs; otherwise use its human Run control".into());
            }
            return self.finish_generator_invocation(
                invocation,
                prepared.cmds,
                slate_doc::scene::CmdAuthor::Agent(author.to_owned()),
            );
        }
        let id = format!("presence-{}", slate_doc::scene::new_agent_session_id());
        if !autonomy {
            if prepared.workbook.is_none() || self.ai.config.valid_workspace().is_none() {
                return Err(
                    "Save the workbook and establish an AI workspace before proposing board edits"
                        .into(),
                );
            }
            self.queue_stage_proposal(Proposal {
                id: id.clone(),
                author: author.to_owned(),
                title: prepared.command,
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                target: ProposalTarget {
                    workbook: prepared.workbook,
                    format_version: slate_doc::SlateDoc::CURRENT,
                },
                cmds: prepared.cmds,
                session: Some(session.to_owned()),
                status: ProposalStatus::Pending,
            })?;
            return Ok(format!("staged:{id}"));
        }
        let command = super::super::commands::SPECS
            .iter()
            .find(|spec| spec.id.0 == prepared.command)
            .map(|spec| spec.id)
            .ok_or("Unknown board command")?;
        let removed = prepare::removed_ids(&prepared.cmds);
        let added = prepare::added_wire_ids(&prepared.cmds);
        if !self.commit_scene_as(
            prepared.cmds,
            slate_doc::scene::CmdAuthor::Agent(author.to_owned()),
        ) {
            return Err("The journal refused the prepared command".into());
        }
        self.finish_targeted_bookkeeping(&removed, &added);
        self.push_history_as(
            command,
            Some(format!("{} objects", prepared.targets.len())),
            CmdAuthor::Agent(author.to_owned()),
        );
        Ok(format!("applied:{id}"))
    }

    pub(crate) fn check_targeted_command(
        &mut self,
        prepared: &PreparedBoardCommand,
    ) -> Result<(), String> {
        if self.refuse_read_only_edit() {
            return Err("This workbook is read-only".into());
        }
        if self.tab().id != prepared.tab_id || self.tab().path != prepared.workbook {
            return Err("The active workbook changed while the action was prepared".into());
        }
        if prepared
            .linked_items
            .iter()
            .any(|item| self.doc().item(item.id) != Some(item))
        {
            return Err("A linked image changed while the action was prepared".into());
        }
        if prepared.command == "board.objects.place_images" {
            creation::check_placement_tags(self, &prepared.cmds)?;
        }
        prepared.applicable(&self.doc().scene)
    }

    pub(crate) fn finish_targeted_bookkeeping(&mut self, removed: &[NodeId], wires: &[NodeId]) {
        if !removed.is_empty() {
            self.stop_pruned_agent_runs(removed);
            self.agents.text_output.retire(removed);
            self.forget_deleted_agent_cards(removed);
            self.board_sel.retain(|id| !removed.contains(id));
        }
        self.note_view_wires_added(wires);
    }

    pub(crate) fn finish_targeted_proposal_bookkeeping(&mut self, cmds: &[SceneCmd]) {
        self.finish_targeted_bookkeeping(
            &prepare::removed_ids(cmds),
            &prepare::added_wire_ids(cmds),
        );
    }

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
            if let Some(invocation) = prepared.invocation.take() {
                return self
                    .finish_generator_invocation(
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
            if !self.commit_scene(prepared.cmds) {
                return Err("The journal refused the command".into());
            }
            self.finish_targeted_bookkeeping(&removed, &added);
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
