//! Existing saved-revision selection and atomic output visibility removal.
use super::*;

impl SlateApp {
    pub(crate) fn accept_artifact_output(&mut self, id: NodeId) -> bool {
        self.accept_artifact_output_from(id, false, None)
    }
    pub(super) fn accept_artifact_output_from(
        &mut self,
        id: NodeId,
        fresh_generation: bool,
        remove: Option<slate_doc::scene::HiddenOutputRef>,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(pending) = self.agents.artifact.pending.get(&id) else {
            self.toast("There is no artifact preview to apply. Play to build one first.");
            return false;
        };
        if !self.artifact_target_current(pending.tab, id, &pending.previous) {
            self.toast(
                "This preview belongs to an earlier artifact state. Discard it and Play again.",
            );
            return false;
        }
        let pending = self.agents.artifact.pending.remove(&id).unwrap();
        let command =
            match self.prepare_artifact_output_patch(id, &pending, fresh_generation, remove) {
                Ok(command) => command,
                Err(error) => {
                    self.toast(error);
                    return false;
                }
            };
        let author = if fresh_generation {
            slate_doc::scene::CmdAuthor::Agent(pending.revision.provenance.provider)
        } else {
            slate_doc::scene::CmdAuthor::Human
        };
        let applied = self.commit_scene_as(vec![command], author);
        if applied {
            self.toast(
                "Artifact preview applied. This revision is now available to downstream artifacts.",
            );
        }
        applied
    }
    pub(super) fn prepare_artifact_output_patch(
        &self,
        id: NodeId,
        pending: &Pending,
        fresh_generation: bool,
        remove: Option<slate_doc::scene::HiddenOutputRef>,
    ) -> Result<SceneCmd, String> {
        let before = self
            .doc()
            .scene
            .node(id)
            .ok_or("Artifact target disappeared")?
            .clone();
        let mut after = before.clone();
        let base = self.tab().path.as_deref();
        let source = SourceUri {
            locator: super::super::super::board_portal::source_locator(
                base,
                &pending.reference.materialized_entry,
            ),
        };
        let root = SourceUri {
            locator: super::super::super::board_portal::source_locator(
                base,
                &pending.reference.source_root,
            ),
        };
        if let NodeKind::Portal(p) = &mut after.kind {
            p.source = Some(source);
        }
        let a = artifact_mut(&mut after).unwrap();
        a.family = pending.revision.family_id.clone();
        a.history_root = Some(root);
        a.revision = Some(pending.reference.revision_id.clone());
        if !a.revisions.contains(&pending.reference.revision_id) {
            a.revisions.push(pending.reference.revision_id.clone());
        }
        a.checkpoint = pending.revision.provenance.checkpoint.through;
        a.packaged = None;
        if fresh_generation {
            a.active_memory = Some(slate_doc::agent_inputs::ArtifactMemoryPin {
                revision: a.revision.clone().unwrap(),
                through: a.checkpoint,
            });
        }
        if let Some(agent) =
            slate_doc::agent_chat::agent_mut(&mut after).filter(|_| fresh_generation)
        {
            agent.generator.history_cut = false;
        }
        if let Some(reference) = remove {
            let Some(agent) = slate_doc::agent_chat::agent_mut(&mut after) else {
                return Err("Artifact target has no generator".into());
            };
            if !agent.generator.hidden_outputs.contains(&reference) {
                agent.generator.hidden_outputs.push(reference);
            }
        }
        Ok(SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        })
    }
    pub(crate) fn saved_artifact_pick_plan(
        &self,
        id: NodeId,
        reference: &slate_doc::scene::HiddenOutputRef,
        remove: bool,
    ) -> Result<Option<(String, Option<slate_doc::scene::HiddenOutputRef>)>, String> {
        let slate_doc::scene::HiddenOutputRef::Artifact { family, revision } = reference else {
            return Err("Not an artifact output".into());
        };
        let binding = self
            .doc()
            .scene
            .node(id)
            .and_then(artifact)
            .ok_or("Artifact target is missing")?;
        if &binding.family != family {
            return Err("Artifact family changed".into());
        }
        let visible = self.result_outputs(id);
        if !visible.iter().any(|output| &output.reference == reference) {
            return Err("Artifact output is no longer visible".into());
        }
        if !remove {
            return Ok(Some((revision.clone(), None)));
        }
        if binding.revision.as_deref() == Some(revision) {
            if let Some(fallback) = visible
                .iter()
                .rev()
                .find(|output| &output.reference != reference)
            {
                return Ok(Some((fallback.id.clone(), Some(reference.clone()))));
            }
        }
        Ok(None)
    }
    pub(crate) fn reject_artifact_output(&mut self, id: NodeId) -> bool {
        if self.agents.artifact.pending.remove(&id).is_some() {
            self.toast("Artifact preview discarded. The accepted revision is unchanged.");
            true
        } else {
            self.toast("There is no artifact preview to discard.");
            false
        }
    }
    pub(crate) fn artifact_pick_revision(&mut self, id: NodeId, revision: &str) -> bool {
        self.artifact_pick_revision_removing(id, revision, None, None)
    }

    pub(crate) fn artifact_pick_revision_removing(
        &mut self,
        id: NodeId,
        revision: &str,
        remove: Option<slate_doc::scene::HiddenOutputRef>,
        lease: Option<Box<crate::app::agent_presence::ArtifactResultLease>>,
    ) -> bool {
        if self.tab().read_only
            || self.doc().scene.node(id).is_none_or(|node| node.locked)
            || self.agents.artifact.active_workers >= 2
            || self.agents.artifact.preparing.contains_key(&id)
            || self.agents.artifact.running.contains_key(&id)
        {
            self.toast("Wait for artifact preparation or generation to finish before selecting a revision.");
            return false;
        }
        let reference = match self.artifact_reference(id, revision) {
            Ok(r) => r,
            Err(e) => {
                self.toast(e);
                return false;
            }
        };
        let tab = self.tab().id;
        let Some(expected) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let tx = self.artifact_work_tx();
        let token = atlas_ai::agent::request_id();
        self.agents.artifact.picks.insert(id, token.clone());
        self.agents.artifact.active_workers += 1;
        std::thread::spawn(move || {
            let result = (|| {
                atlas_ai::versions::materialize_artifact_revision(&reference)
                    .map_err(|e| e.to_string())?;
                let manifest = atlas_ai::versions::load_artifact_revision(&reference)
                    .map_err(|e| e.to_string())?;
                Ok((reference, manifest))
            })();
            let _ = tx.send(WorkDone::Picked(
                tab,
                id,
                token,
                Box::new(expected),
                remove,
                lease,
                result,
            ));
        });
        true
    }

    pub(crate) fn remove_artifact_result(
        &mut self,
        id: NodeId,
        family: &str,
        revision: &str,
    ) -> bool {
        use slate_doc::scene::HiddenOutputRef;
        let Some(binding) = self.doc().scene.node(id).and_then(artifact) else {
            return false;
        };
        if binding.family != family {
            return false;
        }
        let reference = HiddenOutputRef::Artifact {
            family: family.to_owned(),
            revision: revision.to_owned(),
        };
        match self.saved_artifact_pick_plan(id, &reference, true) {
            Ok(Some((revision, remove))) => {
                return self.artifact_pick_revision_removing(id, &revision, remove, None)
            }
            Ok(None) => {}
            Err(error) => {
                self.toast(error);
                return false;
            }
        }
        match self.prepare_result_output_mutation(id, &reference, true) {
            Ok(command) => self.commit_scene(vec![command]),
            Err(error) => {
                self.toast(error);
                false
            }
        }
    }
}
