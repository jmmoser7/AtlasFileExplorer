use super::*;
#[path = "transfer/copy.rs"]
mod copy;
#[path = "transfer/readiness.rs"]
mod readiness;
#[path = "transfer/result_copy.rs"]
mod result_copy;
pub(crate) use readiness::LayerBakeReadiness;

#[derive(Debug)]
pub(crate) struct LayerBakeCompletion {
    pub tab_id: u64,
    pub host: NodeId,
    pub result: ImageResultId,
    pub layer: PaintLayerId,
    pub placeholder: NodeId,
    pub item: slate_doc::ItemId,
    pub path: std::path::PathBuf,
    pub at: Pos2,
    pub saved: Result<(u64, i64), String>,
    pub readiness: LayerBakeReadiness,
}

struct LayerBakeRequest {
    completion: LayerBakeCompletion,
    host_node: Node,
    image: ImageNode,
    doc: super::raster::RasterDoc,
    composite: bool,
    workbook: Option<std::path::PathBuf>,
    w: u32,
    h: u32,
    dir: std::path::PathBuf,
    ctx: egui::Context,
}

pub(crate) struct LayerBakeJobs {
    request_tx: crossbeam_channel::Sender<LayerBakeRequest>,
    pub rx: crossbeam_channel::Receiver<LayerBakeCompletion>,
    pub deferred: Vec<LayerBakeCompletion>,
    readiness: std::collections::HashMap<(u64, slate_doc::ItemId), LayerBakeReadiness>,
    failed_tabs: std::collections::HashSet<u64>,
}

impl LayerBakeJobs {
    pub fn new() -> Self {
        let (request_tx, requests) = crossbeam_channel::bounded::<LayerBakeRequest>(8);
        let (tx, rx) = crossbeam_channel::unbounded();
        for worker in 0..2 {
            let requests = requests.clone();
            let tx = tx.clone();
            let _ = std::thread::Builder::new()
                .name(format!("slate-layer-bake-{worker}"))
                .spawn(move || {
                    while let Ok(mut request) = requests.recv() {
                        request.completion.saved =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                bake_layer(&request)
                            }))
                            .unwrap_or_else(|_| Err("Paint layer rendering stopped".into()));
                        let _ = tx.send(request.completion);
                        request.ctx.request_repaint();
                    }
                });
        }
        Self {
            request_tx,
            rx,
            deferred: Vec::new(),
            readiness: Default::default(),
            failed_tabs: Default::default(),
        }
    }

    pub(crate) fn pending_for(&self, tab: u64) -> bool {
        self.readiness
            .iter()
            .any(|((owner, _), ready)| *owner == tab && ready.result().is_none())
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.readiness
            .values()
            .any(|ready| ready.result().is_none())
    }

    pub(crate) fn readiness_for(
        &self,
        tab: u64,
        item: slate_doc::ItemId,
    ) -> Option<LayerBakeReadiness> {
        self.readiness.get(&(tab, item)).cloned()
    }

    pub(crate) fn register_readiness(
        &mut self,
        tab: u64,
        item: slate_doc::ItemId,
        ready: LayerBakeReadiness,
    ) {
        self.readiness.insert((tab, item), ready);
        if self.readiness.len() > 256 {
            let remove: Vec<_> = self
                .readiness
                .iter()
                .filter(|(_, ready)| ready.result().is_some())
                .take(self.readiness.len() - 256)
                .map(|(key, _)| *key)
                .collect();
            for key in remove {
                self.readiness.remove(&key);
            }
        }
    }

    pub(crate) fn take_failed_tabs(&mut self) -> std::collections::HashSet<u64> {
        std::mem::take(&mut self.failed_tabs)
    }

    fn receive(&mut self) {
        for completion in self.rx.try_iter() {
            completion.readiness.resolve(&completion.saved);
            if completion.saved.is_err() {
                self.failed_tabs.insert(completion.tab_id);
            } else {
                self.readiness.remove(&(completion.tab_id, completion.item));
            }
            self.deferred.push(completion);
        }
    }
}

impl Drop for LayerBakeJobs {
    fn drop(&mut self) {
        for ready in self.readiness.values() {
            if ready.result().is_none() {
                ready.resolve(&Err("Paint layer bake was abandoned".into()));
            }
        }
    }
}

fn bake_layer(request: &LayerBakeRequest) -> Result<(u64, i64), String> {
    let mut doc = request
        .doc
        .lock()
        .map_err(|_| "Layer document snapshot is unavailable")?
        .clone();
    super::resolve_image_doc_paths(&mut doc, request.workbook.as_deref());
    let rgba = if request.composite {
        let source = super::super::image_composite::item_file(&doc, None, request.image.item)
            .ok_or_else(|| "Cutout source is unavailable".to_string())?;
        let pixels = super::super::image_composite::composite_rgba(
            &doc,
            &request.host_node,
            &request.image,
            &source,
        )
        .ok_or_else(|| "Couldn't render this image cutout".to_string())?;
        image::imageops::resize(
            &pixels,
            request.w,
            request.h,
            image::imageops::FilterType::Triangle,
        )
        .into_raw()
    } else {
        slate_artifact::rasterize_paint_layers(
            &request.host_node,
            &request.image,
            request.w,
            request.h,
            Some(&doc),
            &mut slate_artifact::LayerStamps::default(),
        )
        .ok_or_else(|| "Couldn't render this paint layer".to_string())?
    };
    let png = atlas_core::clipboard_image::encode_png(request.w, request.h, &rgba)
        .ok_or_else(|| "Couldn't encode the paint layer".to_string())?;
    if !atlas_core::clipboard_image::png_within_limits(&png) {
        return Err("Paint layer exceeds PNG size limits".into());
    }
    std::fs::create_dir_all(&request.dir).map_err(|error| error.to_string())?;
    std::fs::write(&request.completion.path, png).map_err(|error| error.to_string())?;
    let meta = std::fs::metadata(&request.completion.path).map_err(|error| error.to_string())?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|time| time.as_secs() as i64)
        .unwrap_or(0);
    Ok((meta.len(), mtime))
}

impl Default for LayerBakeJobs {
    fn default() -> Self {
        Self::new()
    }
}

fn image_for_result(img: &ImageNode, result: &ImageResultId) -> ImageNode {
    let mut resolved = img.clone();
    resolved.paint_layers = SlateApp::renderer_layers(img, result);
    resolved.result_layers.clear();
    resolved
}

impl SlateApp {
    /// Add a selected image as a normalized, clipped child of the current
    /// result. This keeps the host selected and journals stack + child once.
    pub(crate) fn add_selection_cutout_layer(
        &mut self,
        image: NodeId,
        item: slate_doc::ItemId,
        mask: &[Vec<[f32; 2]>],
    ) -> Option<PaintLayerId> {
        if self.refuse_read_only_edit() {
            return None;
        }
        let before = self.doc().scene.node(image)?.clone();
        let NodeKind::Image(host_img) = &before.kind else {
            return None;
        };
        let result = self.image_result_id(image)?;
        let layer_id = Self::next_paint_layer_id(host_img);
        let mut after = before.clone();
        let NodeKind::Image(after_img) = &mut after.kind else {
            return None;
        };
        Self::prepare_result_stack(after_img, result.clone());
        let stack = Self::selected_stack_mut(after_img, &result);
        let index = stack.add_layer(PaintLayer::new(layer_id));
        if index > 0 {
            stack.move_layer(index, 0);
        }
        let child = self.selection_cutout_child(host_img, item, mask)?;
        if !self.commit_scene(vec![
            SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            },
            SceneCmd::LayerNodeAdd {
                host: image,
                layer: layer_id,
                index: 0,
                node: child,
            },
        ]) {
            return None;
        }
        self.image_paint = Some(super::ImagePaintSession {
            image,
            result,
            layer_index: 0,
            focus: super::ImageStripFocus::Layer(0),
            layer_drag: None,
        });
        self.push_history(
            atlas_commands::CommandId("board.image.selection.layer"),
            Some("layer".into()),
        );
        Some(layer_id)
    }

    /// Stable image-result identity selected by an ordinary or generated
    /// picture. Generated images use the active bundle id, which does not
    /// change when the album is reordered.
    pub(crate) fn image_result_id(&self, image: NodeId) -> Option<ImageResultId> {
        let node = self.doc().scene.node(image)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        if img.agent.is_some() {
            self.agent_active_output(image)
                .map(ImageResultId::Output)
                .or_else(|| (!img.item.is_none()).then_some(ImageResultId::Item(img.item)))
        } else {
            Some(ImageResultId::Item(img.item))
        }
    }

    /// Clone the currently displayed image result with its own authored layer
    /// stack in the legacy field expected by raster/composite interpreters.
    pub(crate) fn image_for_selected_result(&self, image: NodeId) -> Option<ImageNode> {
        self.image_for_result_id(image, &self.image_result_id(image)?)
    }

    /// Resolve an explicitly addressed result without changing the album pick.
    /// Card-wide legacy layers only belong to the currently displayed result.
    pub(crate) fn image_for_result_id(
        &self,
        image: NodeId,
        result: &ImageResultId,
    ) -> Option<ImageNode> {
        let node = self.doc().scene.node(image)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        let mut resolved = image_for_result(img, result);
        if self.image_result_id(image).as_ref() != Some(result)
            && result_layers(img, result).is_none()
        {
            resolved.paint_layers.clear();
        }
        Some(resolved)
    }

    pub(crate) fn poll_layer_bake_completions(&mut self) {
        self.layer_bakes.receive();
        for completion in std::mem::take(&mut self.layer_bakes.deferred) {
            let Some(index) = self.tabs.iter().position(|tab| tab.id == completion.tab_id) else {
                continue;
            };
            let tab = &mut self.tabs[index];
            let placeholder_is_current = matches!(
                tab.doc.scene.node(completion.placeholder).map(|node| &node.kind),
                Some(NodeKind::Image(image)) if image.item == completion.item
            );
            let item_matches_path = tab
                .doc
                .items
                .iter()
                .find(|item| item.id == completion.item)
                .is_some_and(|item| {
                    slate_doc::scene::resolve_source(
                        tab.path.as_deref(),
                        &item.path.to_string_lossy(),
                    ) == completion.path
                });
            if !item_matches_path {
                continue;
            }
            let _source_identity = (
                completion.host,
                &completion.result,
                completion.layer,
                completion.at,
            );
            match completion.saved {
                Ok((size, mtime)) => {
                    if let Some(item) = tab
                        .doc
                        .items
                        .iter_mut()
                        .find(|item| item.id == completion.item)
                    {
                        item.size = size;
                        item.mtime = mtime;
                        item.cache_key = atlas_core::thumbs::cache_key(
                            &completion.path.to_string_lossy(),
                            item.size,
                            item.mtime,
                        );
                    }
                    tab.dirty = true;
                    if placeholder_is_current && index == self.active_tab {
                        self.scene_gen = self.scene_gen.wrapping_add(1);
                    }
                }
                Err(error) => {
                    if placeholder_is_current {
                        self.toast(format!(
                            "Couldn't copy paint layer: {error}. Undo removes the incomplete copy."
                        ));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "transfer/tests.rs"]
mod tests;
