//! Input snapshots and image export.

use super::*;

impl SlateApp {
    /// A direct run on a picture edits that picture when no upstream Media was
    /// authored. Style references keep their separate role. Chat context and
    /// live runs stay wire-only so outputs never feed themselves automatically.
    pub(crate) fn generator_input_refs(
        &self,
        id: NodeId,
        current_image: bool,
    ) -> Result<atlas_ai::agent::InputSnapshot, String> {
        let mut inputs = self.agent_input_refs(id)?;
        if !current_image || inputs.on(atlas_agent::InputSlot::Media).next().is_some() {
            return Ok(inputs);
        }
        let Some(node) = self.doc().scene.node(id) else {
            return Ok(inputs);
        };
        if !matches!(&node.kind, NodeKind::Image(_))
            || slate_doc::agent_chat::agent(node).is_some_and(|agent| {
                agent.view != atlas_ai::agent::PortalView::Images
                    || !atlas_ai::agent::local_image_engine(&agent.provider)
            })
        {
            return Ok(inputs);
        }
        if let Some(path) = self.image_item_path(id) {
            inputs.wired.push(atlas_ai::agent::ContextItem {
                node: id.0,
                text: String::new(),
                images: vec![
                    resolve_source(self.tab().path.as_deref(), &path.to_string_lossy())
                        .to_string_lossy()
                        .into_owned(),
                ],
                outputs: Default::default(),
                active: None,
                depth: None,
                slot: Some(atlas_agent::InputSlot::Media),
            });
        }
        Ok(inputs)
    }

    /// Resolve authored inputs to references only; request paths prepare clips
    /// asynchronously before dispatching any image-consuming operation.
    pub(crate) fn agent_input_snapshot(
        &self,
        id: NodeId,
    ) -> Result<atlas_ai::agent::InputSnapshot, String> {
        self.agent_input_refs(id)
    }
    /// The context publish's snapshot, with each wired picture's clip made
    /// off the frame loop. `None` while one is still being made: the last
    /// published context stays rather than naming a picture's hidden part.
    fn published_agent_inputs(
        &self,
        id: NodeId,
    ) -> Option<Result<atlas_ai::agent::InputSnapshot, String>> {
        let inputs = match self.agent_input_refs(id) {
            Ok(inputs) => inputs,
            Err(error) => return Some(Err(error)),
        };
        let mut prep = match self.begin_agent_input_preparation(inputs) {
            Ok(prep) => prep,
            Err(error) => return Some(Err(error)),
        };
        match self.poll_agent_input_preparation(&mut prep) {
            Ok(true) => Some(Ok(prep.inputs)),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        }
    }

    /// Schedules any image work against the selected result captured now.
    pub(crate) fn begin_agent_input_preparation(
        &self,
        inputs: atlas_ai::agent::InputSnapshot,
    ) -> Result<input_prep::InputPreparation, String> {
        let revision = (self.scene_gen, self.doc().scene.scene_gen());
        let workbook = self.tab().path.clone();
        let mut replacements = Vec::new();
        let mut bake_dependencies = Vec::new();
        let tab_id = self.tab().id;
        let mut clips = self.agents.image_prep.borrow_mut();
        clips.receive();
        for item in inputs.context.iter().chain(inputs.wired.iter()) {
            let Some(node) = self.doc().scene.node(NodeId(item.node)) else {
                continue;
            };
            let Some(img) = self.image_for_selected_result(node.id) else {
                continue;
            };
            let painted = img
                .paint_layers
                .iter()
                .any(|layer| layer.visible && !layer.nodes.is_empty());
            let composed = painted || img.mirror().any() || img.adjust != Default::default();
            let bake = self.layer_bakes.readiness_for(tab_id, img.item);
            let bake_state = bake.as_ref().and_then(|bake| bake.result());
            if let Some(Err(error)) = &bake_state {
                return Err(format!(
                    "The copied paint layer could not be baked: {error}."
                ));
            }
            let bake_pending = bake.is_some() && bake_state.is_none();
            if bake_pending {
                bake_dependencies.extend(bake.iter().cloned());
            }
            if !bake_pending && !composed && img.crop.is_full() {
                continue;
            }
            let Some(source) = self.image_item_path(node.id) else {
                return Err("A wired image result has no source file.".into());
            };
            let source =
                slate_doc::scene::resolve_source(workbook.as_deref(), &source.to_string_lossy());
            let source_is_captured = item
                .images
                .iter()
                .chain(item.depth.as_ref())
                .any(|slot| std::path::Path::new(slot) == source.as_path());
            if !source_is_captured {
                if item.images.is_empty() {
                    continue;
                }
                return Err("The selected image result changed before input capture.".into());
            }
            let key = clips.key(node, &img, &source, revision);
            replacements.push((node.id, key, source.clone()));
            if composed {
                let mut doc = self.doc().clone();
                let node = node.clone();
                let img = img.clone();
                let source = source.clone();
                let workbook = workbook.clone();
                if clips
                    .schedule(
                        key,
                        bake.filter(|_| bake_pending),
                        Box::new(move || {
                            super::super::board_image_layers::resolve_image_doc_paths(
                                &mut doc,
                                workbook.as_deref(),
                            );
                            super::super::image_composite::agent_wired_image_file_from_source(
                                &doc, &node, &img, &source,
                            )
                        }),
                    )
                    .is_err()
                {
                    return Err("Image preparation is busy; try the send again shortly.".into());
                }
            } else {
                let crop = img.crop;
                let source = source.clone();
                if clips
                    .schedule(
                        key,
                        bake.filter(|_| bake_pending),
                        Box::new(move || {
                            if crop.is_full() {
                                Some(source)
                            } else {
                                super::super::imagefx::visible_crop_file(&source, crop)
                            }
                        }),
                    )
                    .is_err()
                {
                    return Err("Image preparation is busy; try the send again shortly.".into());
                }
            }
        }
        Ok(input_prep::InputPreparation {
            inputs,
            replacements,
            bake_dependencies,
        })
    }

    /// Polls captured clip keys and only substitutes a successful result.
    pub(super) fn poll_agent_input_preparation(
        &self,
        prep: &mut input_prep::InputPreparation,
    ) -> Result<bool, String> {
        let mut clips = self.agents.image_prep.borrow_mut();
        clips.receive();
        for bake in &prep.bake_dependencies {
            match bake.result() {
                None => return Ok(false),
                Some(Ok(())) => {}
                Some(Err(error)) => {
                    return Err(format!(
                        "The copied paint layer could not be baked: {error}."
                    ));
                }
            }
        }
        let mut made = Vec::with_capacity(prep.replacements.len());
        for (node, key, source) in &prep.replacements {
            match clips.result(*key) {
                None if clips.pending(*key) => return Ok(false),
                None => {
                    return Err("An image preparation result expired; send again to retry.".into());
                }
                Some(None) => return Err("Could not prepare a wired image input.".into()),
                Some(Some(path)) => made.push((*node, source, path.to_string_lossy().into_owned())),
            }
        }
        for item in prep
            .inputs
            .context
            .iter_mut()
            .chain(prep.inputs.wired.iter_mut())
        {
            for (node, source, clip) in &made {
                if item.node != node.0 {
                    continue;
                }
                for slot in item.images.iter_mut().chain(item.depth.as_mut()) {
                    if std::path::Path::new(&*slot) == source.as_path() {
                        *slot = clip.clone();
                    }
                }
            }
        }
        prep.replacements.clear();
        Ok(true)
    }
    /// One link folder's context: every card of a chat train shares the
    /// folder, so each card's inputs are published, in card order. `None`
    /// while any card's picture is still being clipped.
    pub(super) fn published_train_inputs(
        &self,
        cards: &[NodeId],
    ) -> Option<Result<atlas_ai::agent::InputSnapshot, String>> {
        let parts: Vec<_> = cards
            .iter()
            .map(|&id| self.published_agent_inputs(id))
            .collect();
        let mut merged: Option<atlas_ai::agent::InputSnapshot> = None;
        let mut error = None;
        for part in parts {
            match part? {
                Ok(inputs) => match &mut merged {
                    None => merged = Some(inputs),
                    Some(all) => {
                        for (into, items) in [
                            (&mut all.context, inputs.context),
                            (&mut all.wired, inputs.wired),
                        ] {
                            for item in items {
                                if !into.contains(&item) {
                                    into.push(item);
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    error.get_or_insert(e);
                }
            }
        }
        merged.map(Ok).or(error.map(Err))
    }
    /// Wired and context inputs as board references, before any picture is
    /// clipped.
    pub(super) fn agent_input_refs(
        &self,
        id: NodeId,
    ) -> Result<atlas_ai::agent::InputSnapshot, String> {
        let mut outputs = std::collections::BTreeMap::new();
        for node in &self.doc().scene.nodes {
            if slate_doc::agent_chat::is_agent_node(node) {
                let images = self
                    .agent_images(node.id)
                    .iter()
                    .map(|i| {
                        resolve_source(self.tab().path.as_deref(), &i.source)
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect();
                let text = self
                    .agents
                    .sessions
                    .get(&node.id)
                    .filter(|s| s.status == AgentStatus::Idle)
                    .and_then(|s| s.turns.iter().rev().find(|t| t.role == "assistant"))
                    .map(|t| t.text.clone())
                    .unwrap_or_default();
                let image_outputs = self
                    .agent_images(node.id)
                    .iter()
                    .map(|i| {
                        (
                            i.id.clone(),
                            resolve_source(self.tab().path.as_deref(), &i.source)
                                .to_string_lossy()
                                .into_owned(),
                        )
                    })
                    .collect();
                let active = self.agent_active_output(node.id);
                outputs.insert(
                    node.id,
                    atlas_ai::agent::ContextItem {
                        node: node.id.0,
                        text,
                        images,
                        outputs: image_outputs,
                        active,
                        depth: None,
                        slot: None,
                    },
                );
            }
            // A placed text document supplies the words its card shows. Read
            // once by the card or a queued run (`snippet_for`), never here.
            if let NodeKind::Image(image) = &node.kind {
                let text_doc = self.doc().item(image.item).is_some_and(|item| {
                    slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Text
                });
                if text_doc {
                    let text = image
                        .text_override
                        .clone()
                        .or_else(|| self.snippets.get(&image.item).cloned().flatten())
                        .unwrap_or_default();
                    outputs.insert(
                        node.id,
                        atlas_ai::agent::ContextItem {
                            node: node.id.0,
                            text,
                            images: vec![],
                            outputs: Default::default(),
                            active: None,
                            depth: None,
                            slot: None,
                        },
                    );
                }
            }
        }
        let mut inputs = slate_doc::agent_inputs::snapshot(
            self.doc(),
            id,
            AgentContextScope::Selection,
            &[],
            &outputs,
        )?;
        for item in inputs.context.iter_mut().chain(inputs.wired.iter_mut()) {
            for source in item
                .images
                .iter_mut()
                .chain(item.depth.as_mut())
                .chain(item.outputs.values_mut())
            {
                *source = resolve_source(self.tab().path.as_deref(), source)
                    .to_string_lossy()
                    .into_owned();
            }
        }
        // A prompt being typed steers before the edit commits.
        if let Some((editing, text)) = &self.text_edit {
            for item in &mut inputs.wired {
                if item.node == editing.0 && item.images.is_empty() {
                    item.text = text.clone();
                }
            }
        }
        Ok(inputs)
    }
    pub(crate) fn export_agent_images(&self) -> std::collections::BTreeMap<NodeId, Vec<PathBuf>> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter_map(|node| {
                let mut images = self.agent_images(node.id).as_ref().clone();
                let active = self.agent_shown_index(node.id)?;
                images.rotate_left(active);
                Some((
                    node.id,
                    images
                        .iter()
                        .map(|i| resolve_source(self.tab().path.as_deref(), &i.source))
                        .collect(),
                ))
            })
            .collect()
    }
    pub(crate) fn agent_is_running(&self, id: NodeId) -> bool {
        self.agents.chat_preparing.contains_key(&id)
            || self
                .agents
                .comfy_queue
                .get(&id)
                .is_some_and(|queue| !queue.is_empty())
            || matches!(
                self.agents.awaiting.get(&id),
                Some(
                    AgentAwait::Sent { .. }
                        | AgentAwait::Thinking { .. }
                        | AgentAwait::Responding { .. }
                )
            )
            || self
                .agents
                .sessions
                .get(&id)
                .is_some_and(|s| s.status == AgentStatus::Thinking)
    }

    pub(super) fn agent_generation_dispatched(&self, id: NodeId) -> bool {
        matches!(
            self.agents.awaiting.get(&id),
            Some(
                AgentAwait::Sent { .. }
                    | AgentAwait::Thinking { .. }
                    | AgentAwait::Responding { .. }
            )
        ) || self
            .agents
            .sessions
            .get(&id)
            .is_some_and(|s| s.status == AgentStatus::Thinking)
    }
}
