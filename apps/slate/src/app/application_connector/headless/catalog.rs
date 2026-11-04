//! Durable model revision references over the existing source and scene journal.
use super::*;

pub(crate) struct ModelGeneratorOutput {
    pub id: String,
    pub source: PathBuf,
    pub current: bool,
}

impl SlateApp {
    pub(crate) fn valid_model_output_identity(identity: &str) -> bool {
        !identity.is_empty() && identity.chars().take(257).count() <= 256
    }
    pub(crate) fn model_generator_preview_info(
        &self,
        host: NodeId,
        identity: &str,
    ) -> Option<super::super::super::model3d::ModelNodeInfo> {
        let output = self
            .doc()
            .scene
            .node(host)
            .and_then(slate_doc::agent_chat::agent)?
            .generator
            .model_outputs
            .iter()
            .filter(|output| Self::valid_model_output_identity(&output.id))
            .find(|o| o.id == identity)?;
        let mut info = self.model_node_info(host)?;
        info.path = super::super::super::board_portal::resolve_source(
            self.tab().path.as_deref(),
            &output.source.locator,
        );
        info.cache_key =
            atlas_core::thumbs::cache_key(&info.path.to_string_lossy(), output.size, output.mtime);
        Some(info)
    }
    pub(crate) fn model_generator_outputs(&self, node: NodeId) -> Vec<ModelGeneratorOutput> {
        let Some(agent) = self
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return vec![];
        };
        agent
            .generator
            .model_outputs
            .iter()
            .filter(|output| !agent.generator.hidden_outputs.iter().any(|reference| {
                matches!(reference, slate_doc::scene::HiddenOutputRef::Model { ticket } if ticket == &output.id)
            }))
            .map(|output| ModelGeneratorOutput {
                id: output.id.clone(),
                source: super::super::super::board_portal::resolve_source(
                    self.tab().path.as_deref(),
                    &output.source.locator,
                ),
                current: agent.generator.shown_model.as_ref() == Some(&output.id),
            })
            .collect()
    }

    pub(crate) fn select_model_generator_output(
        &mut self,
        node: NodeId,
        identity: &str,
    ) -> Result<(), String> {
        if self.tab().read_only || self.doc().scene.node(node).is_none_or(|n| n.locked) {
            return Err("Model is read-only or locked".into());
        }
        let output = self
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|a| a.generator.model_outputs.iter().find(|o| o.id == identity))
            .cloned()
            .ok_or("Model revision is unavailable")?;
        let path = super::super::super::board_portal::resolve_source(
            self.tab().path.as_deref(),
            &output.source.locator,
        );
        let item = self.item_for_ready_path(&path, output.size, output.mtime);
        let already_selected = self.doc().scene.node(node).is_some_and(|node| {
            slate_doc::agent_chat::agent(node)
                .is_some_and(|agent| agent.generator.shown_model.as_deref() == Some(identity))
                && match &node.kind {
                    slate_doc::scene::NodeKind::Image(image) => image.item == item,
                    _ => true,
                }
        });
        if already_selected {
            self.finish_model_output_selection(node);
            self.unlock_model(node);
            return Ok(());
        }
        let command = self.prepare_model_output(node, identity, item)?;
        if !self.commit_scene(vec![command]) {
            return Err("The journal refused model output selection".into());
        }
        self.finish_model_output_selection(node);
        self.unlock_model(node);
        Ok(())
    }

    /// Targeted selection uses only a previously registered immutable output.
    /// Publication registers these links; human selection retains link recovery.
    pub(crate) fn prepare_registered_model_output(
        &self,
        node: NodeId,
        identity: &str,
    ) -> Result<slate_doc::scene::SceneCmd, String> {
        let output = self
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|agent| {
                agent
                    .generator
                    .model_outputs
                    .iter()
                    .find(|output| output.id == identity)
            })
            .ok_or("Model revision is unavailable")?;
        let path = super::super::super::board_portal::resolve_source(
            self.tab().path.as_deref(),
            &output.source.locator,
        );
        let item = self.doc().items.iter().find(|item| {
            item.pdf_page == 0 && super::super::super::board_portal::resolve_source(self.tab().path.as_deref(), &item.path.to_string_lossy()) == path
        }).ok_or("Model output link is missing; select this saved output with the human preview control to restore its registration, then retry")?;
        self.prepare_model_output(node, identity, item.id)
    }

    fn prepare_model_output(
        &self,
        node: NodeId,
        identity: &str,
        item: slate_doc::ItemId,
    ) -> Result<slate_doc::scene::SceneCmd, String> {
        let before = self.doc().scene.node(node).ok_or("Model is missing")?;
        if !matches!(before.kind, slate_doc::scene::NodeKind::Image(_)) {
            return Err("Select an existing model image host with saved model outputs".into());
        }
        if self.tab().read_only || before.locked {
            return Err("Model is read-only or locked".into());
        }
        if self.doc().item(item).is_none() {
            return Err("Model output link is missing".into());
        }
        let mut after = before.clone();
        let agent = slate_doc::agent_chat::agent_mut(&mut after)
            .ok_or("Target has no model output history")?;
        if !agent
            .generator
            .model_outputs
            .iter()
            .any(|output| output.id == identity)
        {
            return Err("Model revision is unavailable".into());
        }
        agent.generator.shown_model = Some(identity.to_owned());
        if let slate_doc::scene::NodeKind::Image(image) = &mut after.kind {
            image.item = item;
        }
        if before == &after {
            return Err("This model output is already selected".into());
        }
        Ok(slate_doc::scene::SceneCmd::Patch {
            before: Box::new(before.clone()),
            after: Box::new(after),
        })
    }

    pub(crate) fn finish_model_output_selection(&mut self, node: NodeId) {
        // Accepted board selection refreshes its preview, never activates a
        // native connector or changes human contents focus. Human UI may unlock
        // its viewport separately through the existing owner above.
        self.model3d.live.remove(&node);
    }

    /// A history preview may display another immutable revision without changing
    /// the live kernel binding. A subsequent explicit activation can branch it.
    pub(in crate::app::application_connector) fn headless_history_preview(
        &self,
        key: (u64, NodeId),
        path: &std::path::Path,
    ) -> bool {
        if self
            .connectors
            .sessions
            .get(&key)
            .and_then(|s| s.descriptor.as_ref())
            .is_none_or(|s| s.mode != SurfaceMode::Headless)
        {
            return false;
        }
        self.tabs.iter().find(|t| t.id == key.0).is_some_and(|tab| {
            tab.doc
                .scene
                .node(key.1)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|agent| {
                    agent.generator.model_outputs.iter().any(|output| {
                        super::super::super::board_portal::resolve_source(
                            tab.path.as_deref(),
                            &output.source.locator,
                        ) == path
                    })
                })
        })
    }
}
