//! Saved-view drop target and journaled camera restore.

use super::*;

impl SlateApp {
    pub(crate) fn model_node_at_world(&self, world: Pos2) -> Option<NodeId> {
        self.doc().scene.nodes.iter().rev().find_map(|n| {
            if n.hidden || n.locked || !self.model_has_viewport(n.id) {
                return None;
            }
            n.rect
                .contains_rotated(world.x, world.y, n.rotation_deg)
                .then_some(n.id)
        })
    }

    pub(crate) fn queue_view_drop_from_path(&mut self, model: NodeId, image_path: PathBuf) {
        self.queue_released_view_drop(model, image_path, None);
    }

    fn queue_released_view_drop(
        &mut self,
        model: NodeId,
        image_path: PathBuf,
        picture: Option<Node>,
    ) {
        if self.pending_view_drop.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let tab_id = self.tab().id;
        let thumbnail = picture.is_none();
        let worker_path = image_path.clone();
        let ctx = self.documents.ui_ctx();
        let pointer = ctx.pointer_latest_pos().unwrap_or_else(|| {
            self.model_node_info(model).map_or(Pos2::ZERO, |info| {
                self.board_xf().rect_w2s(info.rect).center()
            })
        });
        std::thread::spawn(move || {
            let msg = if atlas_core::cloud::is_dehydrated(&worker_path) {
                ViewDropMsg::NoMeta
            } else {
                match view_meta::read_view_meta(&worker_path) {
                    Ok(Some(parsed)) => ViewDropMsg::Restored(
                        parsed,
                        thumbnail.then(|| file_thumbnail(&worker_path)).flatten(),
                    ),
                    Ok(None) => ViewDropMsg::NoMeta,
                    Err(e) => ViewDropMsg::Err(e.to_string()),
                }
            };
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
        self.pending_view_drop = Some(PendingViewDrop {
            tab_id,
            model,
            image_path,
            picture,
            pointer,
            rx,
        });
    }

    #[cfg(test)]
    pub(crate) fn queue_view_drop_from_item(&mut self, model: NodeId, item: slate_doc::ItemId) {
        let Some(path) = self.item_path(item) else {
            return;
        };
        self.queue_view_drop_from_path(model, path);
    }

    pub(crate) fn maintain_view_drop(&mut self) {
        let Some(pending) = &self.pending_view_drop else {
            return;
        };
        let Ok(msg) = pending.rx.try_recv() else {
            return;
        };
        let pending = self.pending_view_drop.take().unwrap();
        let tab_id = pending.tab_id;
        if tab_id != self.tab().id {
            return;
        }
        match msg {
            ViewDropMsg::NoMeta => self.toast("No saved Slate view in that image"),
            ViewDropMsg::Err(e) => self.toast(&e),
            ViewDropMsg::Restored(parsed, thumbnail) => {
                self.begin_released_view_drop(pending, parsed, thumbnail)
            }
        }
    }

    pub(crate) fn apply_view_drop(&mut self, model: NodeId, parsed: view_meta::ViewMetaParsed) {
        if self.doc().scene.node(model).is_none_or(|n| n.locked) || self.refuse_read_only_edit() {
            return;
        }
        let info = match self.model_node_info(model) {
            Some(i) => i,
            None => return,
        };
        let mismatch = !parsed.model_hash.is_empty()
            && self
                .model3d
                .model_hashes
                .get(&info.cache_key)
                .is_some_and(|h| h != &parsed.model_hash);
        self.last_board_edit = None;
        self.patch_nodes(&[model], |n| {
            if let NodeKind::Image(img) = &mut n.kind {
                img.model = parsed.camera;
            }
        });
        self.last_board_edit = None;
        if let Some(vp) = self.model3d.live.get_mut(&model) {
            // The document now holds this pose; locking must not patch again.
            vp.cam = parsed.camera;
            vp.before = parsed.camera;
            vp.view_tween = None;
        }
        if mismatch {
            self.toast("Model changed since capture — view applied anyway");
        } else {
            self.toast("Restored viewport camera from screenshot");
        }
    }

    pub(crate) fn maybe_intercept_image_drop_on_model(
        &mut self,
        paths: &[PathBuf],
        at: Pos2,
    ) -> bool {
        if paths.len() != 1 {
            return false;
        }
        let path = &paths[0];
        if !path.is_file() {
            return false;
        }
        if slate_doc::media_kind(path) != slate_doc::MediaKind::Image {
            return false;
        }
        let Some(model) = self.model_node_at_world(at) else {
            return false;
        };
        let source = ViewDropSource::File(path.clone());
        match self.take_view_drop_preview(model, &source) {
            Some((parsed, entered)) => self.finish_view_drop(model, parsed, entered),
            None => self.queue_view_drop_from_path(model, path.clone()),
        }
        true
    }

    /// A moved picture released at `world` over a model: the model, the
    /// picture, and the item its saved view is read from.
    pub(crate) fn node_view_drop_target(
        &self,
        moved: &[NodeId],
        world: Pos2,
    ) -> Option<NodeViewDrop> {
        let model = self.model_node_at_world(world)?;
        let (image, item) = self.dragged_view_image(moved)?;
        let picture = self.doc().scene.node(image)?.clone();
        Some(NodeViewDrop {
            model,
            image,
            item,
            picture,
        })
    }

    /// Commit a picture's saved view onto the model as one camera patch.
    /// The move itself has already been rewound.
    pub(crate) fn commit_node_view_drop(&mut self, drop: NodeViewDrop) {
        let NodeViewDrop {
            model,
            image,
            item,
            picture,
        } = drop;
        match self.take_view_drop_preview(model, &ViewDropSource::Node(image)) {
            Some((parsed, entered)) => self.finish_view_drop(model, parsed, entered),
            None => {
                if let Some(path) = self.item_path(item) {
                    self.queue_released_view_drop(model, path, Some(picture));
                }
            }
        }
    }

    /// The dragged picture a saved view can come from: a placed raster image
    /// that is not itself a model.
    pub(super) fn dragged_view_image(
        &self,
        moved: &[NodeId],
    ) -> Option<(NodeId, slate_doc::ItemId)> {
        moved.iter().find_map(|id| {
            let NodeKind::Image(img) = &self.doc().scene.node(*id)?.kind else {
                return None;
            };
            let raster = self
                .doc()
                .item(img.item)
                .is_some_and(|it| slate_doc::media_kind(&it.path) == slate_doc::MediaKind::Image);
            (raster && !self.model_has_viewport(*id)).then_some((*id, img.item))
        })
    }

    /// Release over the model: the previewed view becomes one camera patch,
    /// and a viewport the preview opened freezes again.
    fn finish_view_drop(
        &mut self,
        model: NodeId,
        parsed: view_meta::ViewMetaParsed,
        entered: bool,
    ) {
        self.apply_view_drop(model, parsed);
        if entered {
            self.lock_model(model);
        }
    }
}
