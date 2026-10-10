//! Input snapshots and image export.

use super::super::image_composite::item_file;
use super::*;

impl SlateApp {
    pub(crate) fn agent_input_snapshot(
        &self,
        id: NodeId,
    ) -> Result<atlas_ai::agent::InputSnapshot, String> {
        let mut inputs = self.agent_input_refs(id)?;
        self.clip_agent_images(&mut inputs);
        Ok(inputs)
    }
    /// The context publish's snapshot, with each wired picture's clip made
    /// off the frame loop. `None` while one is still being made: the last
    /// published context stays rather than naming a picture's hidden part.
    fn published_agent_inputs(
        &self,
        id: NodeId,
    ) -> Option<Result<atlas_ai::agent::InputSnapshot, String>> {
        let mut inputs = match self.agent_input_refs(id) {
            Ok(inputs) => inputs,
            Err(error) => return Some(Err(error)),
        };
        let revision = (self.scene_gen, self.doc().scene.scene_gen());
        let mut clips = self.agents.publish_clips.borrow_mut();
        clips.receive();
        let mut waiting = false;
        for item in inputs.context.iter_mut().chain(inputs.wired.iter_mut()) {
            let Some(node) = self.doc().scene.node(NodeId(item.node)) else {
                continue;
            };
            let NodeKind::Image(img) = &node.kind else {
                continue;
            };
            let painted = img
                .paint_layers
                .iter()
                .any(|layer| layer.visible && !layer.nodes.is_empty());
            if !painted && img.crop.is_full() {
                continue;
            }
            let Some(source) = item_file(self.doc(), self.tab().path.as_deref(), img.item) else {
                continue;
            };
            let key = clips.key(node, img, revision);
            match clips.ready.get(&key) {
                Some(Some(clip)) => {
                    let clip = clip.to_string_lossy().into_owned();
                    for slot in item.images.iter_mut().chain(item.depth.as_mut()) {
                        if std::path::Path::new(&*slot) == source {
                            *slot = clip.clone();
                        }
                    }
                }
                Some(None) => {}
                None => {
                    waiting = true;
                    if !clips.pending.insert(key) {
                        continue;
                    }
                    let tx = clips.sender();
                    if painted {
                        let doc = self.doc().clone();
                        let node = node.clone();
                        let book = self.tab().path.clone();
                        std::thread::spawn(move || {
                            let clip = match &node.kind {
                                NodeKind::Image(img) => {
                                    super::super::image_composite::agent_wired_image_file(
                                        &doc,
                                        book.as_deref(),
                                        &node,
                                        img,
                                    )
                                }
                                _ => None,
                            };
                            let _ = tx.send((key, clip));
                        });
                    } else {
                        let crop = img.crop;
                        std::thread::spawn(move || {
                            let _ = tx.send((
                                key,
                                super::super::imagefx::visible_crop_file(&source, crop),
                            ));
                        });
                    }
                }
            }
        }
        (!waiting).then_some(Ok(inputs))
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
    fn agent_input_refs(&self, id: NodeId) -> Result<atlas_ai::agent::InputSnapshot, String> {
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
                    let text = self
                        .snippets
                        .get(&image.item)
                        .cloned()
                        .flatten()
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
        // Engines open files: a pasted picture's `assets/…` locator is a path
        // relative to the workbook, not to the engine's working directory.
        let book = self.tab().path.as_deref();
        for item in inputs.context.iter_mut().chain(inputs.wired.iter_mut()) {
            for slot in item.images.iter_mut().chain(item.depth.as_mut()) {
                *slot = resolve_source(book, slot).to_string_lossy().into_owned();
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
    /// Replace wired image inputs with their visible crop and paint composite.
    fn clip_agent_images(&self, inputs: &mut atlas_ai::agent::InputSnapshot) {
        for item in inputs.context.iter_mut().chain(inputs.wired.iter_mut()) {
            let Some(node) = self.doc().scene.node(NodeId(item.node)) else {
                continue;
            };
            let NodeKind::Image(img) = &node.kind else {
                continue;
            };
            if img.mirror().any()
                || img
                    .paint_layers
                    .iter()
                    .any(|layer| layer.visible && !layer.nodes.is_empty())
            {
                super::super::image_composite::replace_wired_image_slots(
                    self,
                    item,
                    NodeId(item.node),
                );
                continue;
            }
            if img.crop.is_full() {
                continue;
            }
            let Some(source) = item_file(self.doc(), self.tab().path.as_deref(), img.item) else {
                continue;
            };
            let Some(clipped) = super::super::imagefx::visible_crop_file(&source, img.crop) else {
                continue;
            };
            let clipped = clipped.to_string_lossy().into_owned();
            for slot in item.images.iter_mut().chain(item.depth.as_mut()) {
                if std::path::Path::new(&*slot) == source {
                    *slot = clipped.clone();
                }
            }
        }
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
