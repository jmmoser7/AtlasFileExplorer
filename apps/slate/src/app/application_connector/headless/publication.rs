//! Verified headless output acceptance through the owning workbook journal.
use super::*;
impl SlateApp {
    /// Called only after the existing caller validation and Lease::publish win.
    pub(crate) fn native_publish_headless_model(
        &mut self,
        node: NodeId,
        completion: &Completion,
    ) -> Result<(), String> {
        self.native_publish_headless_model_in(self.tab().id, node, completion)
    }

    pub(in crate::app::application_connector) fn native_publish_headless_model_in(
        &mut self,
        doc: u64,
        node: NodeId,
        completion: &Completion,
    ) -> Result<(), String> {
        let NativeOutcome::Completed {
            result: NativeResult::Model {
                path, size, mtime, ..
            },
        } = &completion.outcome
        else {
            return Ok(());
        };
        let tab = self
            .tabs
            .iter()
            .find(|tab| tab.id == doc)
            .ok_or("Model task workbook was closed")?;
        if tab.read_only || tab.doc.scene.node(node).is_none_or(|n| n.locked) {
            return Err("Model is read-only, locked or removed; prior output retained".into());
        }
        let binding = self.native_binding_in(doc, node)?;
        if binding.key() != completion.key
            || completion.binding_after.as_ref() != Some(&binding.binding)
            || !path.starts_with(atlas_core::index::data_dir().join("headless-rhino/models"))
        {
            return Err("Headless model source/generation changed before acceptance".into());
        }
        if self.connectors.sessions[&(doc, node)]
            .descriptor
            .as_ref()
            .is_none_or(|s| s.mode != SurfaceMode::Headless)
        {
            return Err("This result is not from the headless adapter".into());
        }
        let item = self
            .item_for_ready_path_in(doc, path, *size, *mtime)
            .ok_or("Model workbook was closed")?;
        let output = slate_doc::scene::ModelOutputRef {
            id: completion.ticket.clone(),
            source: slate_doc::SourceUri {
                locator: slate_doc::scene::source_locator(
                    self.tabs
                        .iter()
                        .find(|tab| tab.id == doc)
                        .unwrap()
                        .path
                        .as_deref(),
                    path,
                ),
            },
            size: *size,
            mtime: *mtime,
        };
        if self
            .tabs
            .iter()
            .find(|tab| tab.id == doc)
            .unwrap()
            .doc
            .scene
            .node(node)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.generator.model_outputs.len() >= 128)
        {
            return Err("Model revision catalog is full; create a new builder branch before generating more outputs".into());
        }
        let author = completion
            .actor
            .as_ref()
            .map_or(slate_doc::scene::CmdAuthor::Human, |a| {
                slate_doc::scene::CmdAuthor::Agent(a.agent.clone())
            });
        self.connectors.sessions[&(doc, node)]
            .tx
            .try_send(worker::Action::AcceptHeadless {
                key: completion.key.clone(),
                path: path.clone(),
                revision: binding.binding.source.revision.clone(),
            })
            .map_err(|_| {
                "Accepted model could not be queued for headless source synchronization"
            })?;
        let before = self
            .tabs
            .iter()
            .find(|tab| tab.id == doc)
            .unwrap()
            .doc
            .scene
            .node(node)
            .unwrap()
            .clone();
        let mut after = before.clone();
        if let slate_doc::scene::NodeKind::Image(image) = &mut after.kind {
            image.item = item;
        }
        if let Some(agent) = slate_doc::agent_chat::agent_mut(&mut after) {
            agent.generator.model_outputs.push(output);
            agent.generator.shown_model = Some(completion.ticket.clone());
        }
        if !self.commit_scene_in(
            doc,
            vec![slate_doc::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            }],
            author,
        ) {
            return Err("Headless result source changed before journal acceptance".into());
        }
        self.note_scene_change();
        if self.tab().id == doc {
            self.model3d.live.remove(&node);
        }
        if let Some(session) = self.connectors.sessions.get_mut(&(doc, node)) {
            session.path = path.clone();
            session.status = "Verified headless model accepted".into();
        }
        if self.tab().id == doc && !self.at_home {
            self.unlock_model(node);
        }
        Ok(())
    }
}
