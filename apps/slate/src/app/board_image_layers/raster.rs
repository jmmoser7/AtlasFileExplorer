use super::*;
use crate::app::preview::upload::{checked_texture_bytes, UploadBudget};
use std::collections::HashSet;
use std::sync::Arc;

const READY_CAPACITY: usize = 2;

// The renderer needs asset records only; authored geometry/style travels in
// the request. The mutex permits sharing SlateDoc's non-Sync derived caches.
pub(super) type RasterDoc = Arc<std::sync::Mutex<slate_doc::SlateDoc>>;

pub(super) fn snapshot_for_image(doc: &slate_doc::SlateDoc, image: &ImageNode) -> RasterDoc {
    let mut needed = HashSet::new();
    needed.insert(image.item);
    // Match the hosted-image interpreter: direct layer Image children resolve
    // their item, crop and filters; shapes/text need no workbook asset lookup.
    for layer in &image.paint_layers {
        for child in &layer.nodes {
            if let NodeKind::Image(image) = &child.kind {
                needed.insert(image.item);
            }
        }
    }
    needed.remove(&slate_doc::ItemId::NONE);
    let mut sources = slate_doc::SlateDoc::new("Image raster sources");
    for item in &doc.items {
        if needed.is_empty() {
            break;
        }
        if needed.remove(&item.id) {
            sources.items.push(item.clone());
        }
    }
    Arc::new(std::sync::Mutex::new(sources))
}

pub(crate) type ThumbKey = (NodeId, ImageResultId, PaintLayerId);

#[derive(Clone)]
pub(crate) enum RasterTarget {
    Host { node: NodeId, key: u128 },
    Thumb { key: ThumbKey, gen: u64 },
}

pub(crate) struct RasterCompletion {
    pub target: RasterTarget,
    pub w: u32,
    pub h: u32,
    pub rgba: Option<Vec<u8>>,
}

pub(super) struct RasterInput {
    host: Node,
    image: ImageNode,
    doc: RasterDoc,
    workbook: Option<std::path::PathBuf>,
    size: (u32, u32),
}

impl RasterInput {
    fn valid_for(&self, ctx: &egui::Context) -> bool {
        checked_texture_bytes(
            [self.size.0 as usize, self.size.1 as usize],
            ctx.input(|input| input.max_texture_side).min(4096),
        )
        .is_some()
    }

    pub(super) fn new(
        host: Node,
        image: ImageNode,
        doc: RasterDoc,
        workbook: Option<std::path::PathBuf>,
        size: (u32, u32),
    ) -> Self {
        Self {
            host,
            image,
            doc,
            workbook,
            size,
        }
    }
}

struct RasterRequest {
    target: RasterTarget,
    input: RasterInput,
    thumb_crop: Option<(u32, u32, u32)>,
    ctx: egui::Context,
}

pub(crate) struct ImageRasterJobs {
    tx: crossbeam_channel::Sender<RasterRequest>,
    rx: crossbeam_channel::Receiver<RasterCompletion>,
    pending_hosts: HashSet<(NodeId, u128)>,
    pending_thumbs: HashSet<(ThumbKey, u64)>,
}

impl ImageRasterJobs {
    pub fn new() -> Self {
        let (tx, requests) = crossbeam_channel::bounded::<RasterRequest>(16);
        // A 4096-square composite owns 64 MiB of decoded bytes. Backpressure
        // belongs on these workers, never on the frame loop.
        let (results, rx) = crossbeam_channel::bounded(READY_CAPACITY);
        for worker in 0..2 {
            let requests = requests.clone();
            let results = results.clone();
            let _ = std::thread::Builder::new()
                .name(format!("slate-image-raster-{worker}"))
                .spawn(move || {
                    while let Ok(request) = requests.recv() {
                        let mut rgba = render_request(&request);
                        if let Some((x, y, side)) = request.thumb_crop {
                            rgba = rgba.and_then(|pixels| {
                                crop_square(&pixels, request.input.size.0, x, y, side)
                            });
                        }
                        request.ctx.request_repaint();
                        if results
                            .send(RasterCompletion {
                                target: request.target,
                                w: request
                                    .thumb_crop
                                    .map_or(request.input.size.0, |(_, _, side)| side),
                                h: request
                                    .thumb_crop
                                    .map_or(request.input.size.1, |(_, _, side)| side),
                                rgba,
                            })
                            .is_err()
                        {
                            break;
                        }
                        request.ctx.request_repaint();
                    }
                });
        }
        Self {
            tx,
            rx,
            pending_hosts: HashSet::new(),
            pending_thumbs: HashSet::new(),
        }
    }

    pub(super) fn request_host(
        &mut self,
        node: NodeId,
        key: u128,
        input: RasterInput,
        ctx: egui::Context,
    ) -> bool {
        let pending = (node, key);
        if !self.needs_host(node, key) || !input.valid_for(&ctx) {
            return false;
        }
        let request = RasterRequest {
            target: RasterTarget::Host { node, key },
            input,
            thumb_crop: None,
            ctx,
        };
        if self.tx.try_send(request).is_ok() {
            self.pending_hosts.insert(pending);
            return true;
        }
        false
    }

    pub(super) fn request_thumb(
        &mut self,
        key: ThumbKey,
        gen: u64,
        input: RasterInput,
        side: u32,
        ctx: egui::Context,
    ) {
        let pending = (key.clone(), gen);
        if !self.needs_thumb(&key, gen)
            || !input.valid_for(&ctx)
            || side == 0
            || side > input.size.0
            || side > input.size.1
        {
            return;
        }
        let thumb_crop = Some(((input.size.0 - side) / 2, (input.size.1 - side) / 2, side));
        let request = RasterRequest {
            target: RasterTarget::Thumb {
                key: key.clone(),
                gen,
            },
            input,
            thumb_crop,
            ctx,
        };
        if self.tx.try_send(request).is_ok() {
            self.pending_thumbs.insert(pending);
        }
    }

    pub fn needs_host(&self, node: NodeId, key: u128) -> bool {
        !self.pending_hosts.contains(&(node, key)) && !self.tx.is_full()
    }

    pub fn needs_thumb(&self, key: &ThumbKey, gen: u64) -> bool {
        !self.pending_thumbs.contains(&(key.clone(), gen)) && !self.tx.is_full()
    }

    pub fn drain(&mut self, ctx: &egui::Context) -> Vec<RasterCompletion> {
        let mut completions = Vec::with_capacity(READY_CAPACITY);
        let mut budget = UploadBudget::default();
        while budget.may_receive() {
            let completion = match self.rx.try_recv() {
                Ok(completion) => completion,
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    // A worker failure/shutdown cannot leave requests forever
                    // marked in flight. Subsequent requests remain nonblocking.
                    self.pending_hosts.clear();
                    self.pending_thumbs.clear();
                    break;
                }
            };
            budget.received(completion.rgba.is_some());
            match &completion.target {
                RasterTarget::Host { node, key } => {
                    self.pending_hosts.remove(&(*node, *key));
                }
                RasterTarget::Thumb { key, gen } => {
                    self.pending_thumbs.remove(&(key.clone(), *gen));
                }
            }
            completions.push(completion);
        }
        if !self.rx.is_empty() {
            ctx.request_repaint();
        }
        completions
    }
}

fn render_request(request: &RasterRequest) -> Option<Vec<u8>> {
    let mut doc = request.input.doc.lock().ok()?.clone();
    super::resolve_image_doc_paths(&mut doc, request.input.workbook.as_deref());
    if matches!(&request.target, RasterTarget::Host { .. }) {
        let mut stamps = slate_artifact::LayerStamps::default();
        slate_artifact::rasterize_paint_layers(
            &request.input.host,
            &request.input.image,
            request.input.size.0,
            request.input.size.1,
            Some(&doc),
            &mut stamps,
        )
    } else {
        let svg = slate_artifact::paint_layers_svg_with_doc(
            &request.input.host,
            &request.input.image,
            request.input.size.0,
            request.input.size.1,
            &doc,
        );
        slate_artifact::rasterize_paint_layers_svg(&svg, request.input.size.0, request.input.size.1)
    }
}

fn crop_square(rgba: &[u8], width: u32, x: u32, y: u32, side: u32) -> Option<Vec<u8>> {
    let height = rgba.len().checked_div(width as usize)?.checked_div(4)?;
    if checked_texture_bytes([width as usize, height], usize::MAX) != Some(rgba.len())
        || x.checked_add(side)? > width
        || y.checked_add(side)? as usize > height
    {
        return None;
    }
    let bytes = checked_texture_bytes([side as usize, side as usize], usize::MAX)?;
    let mut output = Vec::with_capacity(bytes);
    for row in 0..side {
        let start = ((y as usize + row as usize) * width as usize + x as usize) * 4;
        output.extend_from_slice(&rgba[start..start + side as usize * 4]);
    }
    Some(output)
}

#[cfg(test)]
#[path = "raster_queue_tests.rs"]
mod queue_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_sources_omit_unrelated_pool_and_scene_but_retain_exact_asset_records() {
        let mut doc = slate_doc::SlateDoc::new("Large workbook");
        doc.add_group("Unrelated tags");
        let base = doc.add_item("assets/base.png".into(), "base.png", 10, 20, "base-key");
        let child = doc.add_item("assets/child.png".into(), "child.png", 30, 40, "child-key");
        for index in 0..64 {
            doc.add_item(
                format!("pool/{index}.png").into(),
                "unused.png",
                1,
                2,
                "unused",
            );
        }
        let mut image = ImageNode::new(base);
        let child_node = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Image(ImageNode::new(child)),
        );
        let mut layer = PaintLayer::new(PaintLayerId(1));
        layer.nodes.push(child_node.clone());
        image.paint_layers.push(layer);
        doc.scene.nodes.push(child_node);
        let shared = snapshot_for_image(&doc, &image);
        let sources = shared.lock().unwrap();
        assert_eq!(sources.items.len(), 2);
        assert_eq!(sources.item(base), doc.item(base));
        assert_eq!(sources.item(child), doc.item(child));
        assert!(sources.scene.nodes.is_empty());
        assert!(sources.groups.is_empty());
        drop(sources);
        let host = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            NodeKind::Image(image.clone()),
        );
        let groups: [RasterInput; 3] = std::array::from_fn(|_| {
            RasterInput::new(host.clone(), image.clone(), shared.clone(), None, (8, 8))
        });
        assert!(Arc::ptr_eq(&groups[0].doc, &groups[1].doc));
        assert!(Arc::ptr_eq(&groups[1].doc, &groups[2].doc));
    }
}
