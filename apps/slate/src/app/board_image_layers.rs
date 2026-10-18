//! Image paint layers (trace paper). Contract: `image-paint-layers.md`.

use super::{board::BoardTool, board::BoardXf, SlateApp};
use eframe::egui::{self, Color32, Pos2};
use slate_doc::{
    image_paint::{
        clip_path_for_layer_node, ensure_result_layers_mut, find_layer_node, layer_node_from_world,
        layer_node_to_world, layers_for_result, result_layers, seed_result_from_legacy,
        ImageResultId, ImageResultLayers, LayerNodeRef, PaintLayerId,
    },
    scene::{ImageNode, Node, NodeKind, SceneCmd, WorldRect},
    NodeId, PaintLayer,
};

#[path = "board_image_layers/raster.rs"]
mod raster;
#[path = "board_image_layers/result_palette.rs"]
mod result_palette;
pub(crate) use result_palette::opacity::LayerOpacityDrag;
#[path = "board_image_layers/drop.rs"]
mod drop;
mod image_add;
mod insertion;
#[path = "board_image_layers/shape_fill.rs"]
mod shape_fill;
#[path = "board_image_layers/transfer.rs"]
mod transfer;
pub use drop::ImageDropOffer;
#[cfg(test)]
pub use drop::{ImageDropChoice, ImageDropSource};
pub(crate) use raster::{ImageRasterJobs, ThumbKey as PaintLayerThumbKey};
pub(crate) use shape_fill::ShapeFillPreview;
pub(crate) use transfer::{LayerBakeJobs, LayerBakeReadiness};

pub(crate) fn resolve_image_doc_paths(
    doc: &mut slate_doc::SlateDoc,
    workbook: Option<&std::path::Path>,
) {
    for item in &mut doc.items {
        item.path = slate_doc::scene::resolve_source(workbook, &item.path.to_string_lossy());
    }
}
/// The layer the palette slider and Delete address; `Filter` means the image
/// has no layer yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageStripFocus {
    Filter,
    Layer(usize),
}

#[derive(Clone, Debug)]
pub struct ImagePaintSession {
    pub image: NodeId,
    pub result: ImageResultId,
    pub layer_index: usize,
    pub focus: ImageStripFocus,
    pub layer_drag: Option<(PaintLayerId, bool)>,
}

/// What an edit of a layer mark in world space asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerMarkEdit {
    Keep,
    Patch,
    Remove,
}

#[derive(Clone)]
pub(crate) struct PaintLayerTextureCache {
    key: u128,
    texture: egui::TextureHandle,
}

/// One layer-palette preview circle and the scene generation it shows.
pub(crate) struct PaintLayerThumb {
    gen: u64,
    texture: egui::TextureHandle,
}

fn paint_layer_cache_key(
    host: &Node,
    img: &ImageNode,
    result: &ImageResultId,
    gen: u64,
    zoom_bucket: u32,
) -> u128 {
    let c = img.crop.clamped();
    let mut key = host.id.0 as u128;
    key = key.wrapping_mul(31).wrapping_add(gen as u128);
    key = key
        .wrapping_mul(31)
        .wrapping_add(host.rect.x.to_bits() as u128);
    key = key
        .wrapping_mul(31)
        .wrapping_add(host.rect.y.to_bits() as u128);
    key = key
        .wrapping_mul(31)
        .wrapping_add(host.rect.w.to_bits() as u128);
    key = key
        .wrapping_mul(31)
        .wrapping_add(host.rect.h.to_bits() as u128);
    key = key
        .wrapping_mul(31)
        .wrapping_add(host.rotation_deg.to_bits() as u128);
    key = key.wrapping_mul(31).wrapping_add(zoom_bucket as u128);
    key = key.wrapping_mul(31).wrapping_add(c.x.to_bits() as u128);
    key = key.wrapping_mul(31).wrapping_add(c.y.to_bits() as u128);
    key = key.wrapping_mul(31).wrapping_add(c.w.to_bits() as u128);
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    result.hash(&mut hasher);
    key.wrapping_mul(31)
        .wrapping_add(c.h.to_bits() as u128)
        .wrapping_mul(31)
        .wrapping_add(hasher.finish() as u128)
}

pub fn tool_hosts_on_image(tool: BoardTool) -> bool {
    matches!(
        tool,
        BoardTool::RectShape
            | BoardTool::Ellipse
            | BoardTool::Line
            | BoardTool::Arc
            | BoardTool::Polyline
            | BoardTool::BezierSpan
            | BoardTool::Pen
            | BoardTool::Text
            | BoardTool::Brush
            | BoardTool::Eraser
    )
}

impl SlateApp {
    fn selected_layers<'a>(img: &'a ImageNode, result: &ImageResultId) -> &'a [PaintLayer] {
        Self::displayed_layers(img, result)
    }

    fn session_layer_raw_index(
        img: &ImageNode,
        result: &ImageResultId,
        index: usize,
    ) -> Option<usize> {
        let layers = Self::displayed_layers(img, result);
        if matches!(result, ImageResultId::Output(_))
            && result_layers(img, result).is_none()
            && !img.paint_layers.is_empty()
        {
            layers.len().checked_sub(index + 1)
        } else {
            (index < layers.len()).then_some(index)
        }
    }

    /// Compatibility view for the one result the host currently displays.
    /// Legacy generated cards owned one stack at card scope; callers pass only
    /// `image_result_id(image)`, so this never makes a fallback available to
    /// inactive or newly generated album outputs.
    pub(super) fn displayed_layers<'a>(
        img: &'a ImageNode,
        result: &ImageResultId,
    ) -> &'a [PaintLayer] {
        if matches!(result, ImageResultId::Output(_))
            && result_layers(img, result).is_none()
            && !img.paint_layers.is_empty()
        {
            &img.paint_layers
        } else {
            layers_for_result(img, result)
        }
    }

    /// Adapt the canonical top-first result stack to the legacy raster/SVG
    /// interpreter contract, which paints vectors bottom-to-top.
    pub(super) fn renderer_layers(img: &ImageNode, result: &ImageResultId) -> Vec<PaintLayer> {
        match result_layers(img, result) {
            Some(stack) => stack.layers.iter().rev().cloned().collect(),
            None => Self::displayed_layers(img, result).to_vec(),
        }
    }

    pub(crate) fn prepare_result_stack(img: &mut ImageNode, result: ImageResultId) {
        if result_layers(img, &result).is_some() {
            return;
        }
        if img.paint_layers.is_empty() {
            ensure_result_layers_mut(img, result);
        } else {
            seed_result_from_legacy(img, result);
        }
    }

    fn selected_layers_mut<'a>(
        img: &'a mut ImageNode,
        result: &ImageResultId,
    ) -> &'a mut Vec<PaintLayer> {
        &mut Self::selected_stack_mut(img, result).layers
    }

    pub(crate) fn selected_stack_mut<'a>(
        img: &'a mut ImageNode,
        result: &ImageResultId,
    ) -> &'a mut ImageResultLayers {
        img.result_layers
            .iter_mut()
            .find(|stack| &stack.result == result)
            .expect("result stack is prepared")
    }

    pub fn image_paint_session(&self) -> Option<&ImagePaintSession> {
        self.image_paint.as_ref()
    }

    pub fn clear_image_paint_session(&mut self) {
        self.image_paint = None;
    }

    pub(crate) fn cancel_image_layer_drag(&mut self) -> bool {
        self.image_paint
            .as_mut()
            .and_then(|session| session.layer_drag.take())
            .is_some()
    }

    pub fn paint_layer_host(&self) -> Option<NodeId> {
        self.image_paint.as_ref().map(|s| s.image)
    }

    pub(crate) fn image_layer_index_exists(&self, image: NodeId, index: usize) -> bool {
        let Some(result) = self.image_result_id(image) else {
            return false;
        };
        self.doc()
            .scene
            .node(image)
            .and_then(|node| match &node.kind {
                NodeKind::Image(img) => Self::session_layer_raw_index(img, &result, index),
                _ => None,
            })
            .is_some()
    }

    pub fn supports_image_paint(id: NodeId, app: &SlateApp) -> bool {
        let Some(node) = app.doc().scene.node(id) else {
            return false;
        };
        let NodeKind::Image(img) = &node.kind else {
            return false;
        };
        // A generated picture shows its newest result until one is picked.
        if let Some(agent) = img.agent.as_ref().filter(|_| img.item.is_none()) {
            return agent.view != atlas_ai::agent::PortalView::Text
                && app.agent_shown_path(id).is_some();
        }
        let Some(item) = app.doc().item(img.item) else {
            return false;
        };
        if slate_doc::media_kind(&item.path) != slate_doc::MediaKind::Image {
            return false;
        }
        true
    }

    pub fn agent_picture_draw_mode(&self, image: NodeId) -> bool {
        self.image_paint.as_ref().is_some_and(|s| {
            s.image == image
                && self.image_result_id(image).as_ref() == Some(&s.result)
                && tool_hosts_on_image(self.board_tool)
        })
    }

    pub fn sync_image_paint_for_tool(&mut self) {
        let layer_controls_open =
            self.shape_properties.panel == Some(super::board_properties::Panel::Layers);
        if !tool_hosts_on_image(self.board_tool) && !layer_controls_open {
            self.image_paint = None;
            return;
        }
        if self.board_sel.len() != 1 {
            self.image_paint = None;
            return;
        }
        let id = *self.board_sel.iter().next().unwrap();
        if !Self::supports_image_paint(id, self) {
            self.image_paint = None;
            return;
        }
        let Some(result) = self.image_result_id(id) else {
            self.image_paint = None;
            return;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return;
        };
        let NodeKind::Image(ref img) = node.kind else {
            return;
        };
        let layers = Self::selected_layers(img, &result);
        let layer_index = self
            .image_paint
            .as_ref()
            .filter(|s| s.image == id && s.result == result)
            .map(|s| s.layer_index)
            .unwrap_or(0);
        let focus = if layers.is_empty() {
            ImageStripFocus::Filter
        } else {
            ImageStripFocus::Layer(layer_index.min(layers.len() - 1))
        };
        if self
            .image_paint
            .as_ref()
            .is_none_or(|s| s.image != id || s.result != result)
        {
            self.image_paint = Some(ImagePaintSession {
                image: id,
                result,
                layer_index: match focus {
                    ImageStripFocus::Layer(i) => i,
                    ImageStripFocus::Filter => 0,
                },
                focus,
                layer_drag: None,
            });
        }
    }

    /// Where layer mark `id` sits, while the image paint session paints its
    /// layer. A mark on another image or layer is out of reach.
    pub(crate) fn session_layer_mark(&self, id: NodeId) -> Option<LayerNodeRef> {
        let loc = find_layer_node(&self.doc().scene, id)?;
        let session = self.image_paint.as_ref()?;
        let result_matches = loc.result.as_ref().map_or_else(
            || {
                self.doc()
                    .scene
                    .node(loc.image)
                    .and_then(|host| match &host.kind {
                        NodeKind::Image(img) => Some(match &session.result {
                            ImageResultId::Item(item) => img.item == *item,
                            ImageResultId::Output(_) => {
                                result_layers(img, &session.result).is_none()
                                    && !img.paint_layers.is_empty()
                            }
                        }),
                        _ => None,
                    })
                    .unwrap_or(false)
            },
            |result| result == &session.result,
        );
        let selected = self.image_result_id(session.image).as_ref() == Some(&session.result);
        let session_raw_index =
            self.doc()
                .scene
                .node(session.image)
                .and_then(|host| match &host.kind {
                    NodeKind::Image(img) => {
                        Self::session_layer_raw_index(img, &session.result, session.layer_index)
                    }
                    _ => None,
                });
        (selected
            && session.image == loc.image
            && result_matches
            && session_raw_index == Some(loc.layer_index))
        .then_some(loc)
    }

    /// Edit layer mark `id` as a world-space node and return the layer
    /// command that stores the result. `None` when the mark is out of the
    /// session's reach or `edit` keeps it.
    pub(crate) fn patch_layer_node_in_world(
        &self,
        id: NodeId,
        edit: impl FnOnce(&mut Node) -> LayerMarkEdit,
    ) -> Option<SceneCmd> {
        let loc = self.session_layer_mark(id)?;
        let host = self.doc().scene.node(loc.image)?;
        let NodeKind::Image(img) = &host.kind else {
            return None;
        };
        let layers = match &loc.result {
            Some(result) => result_layers(img, result)?.layers.as_slice(),
            None => img.paint_layers.as_slice(),
        };
        let layer = layers.get(loc.layer_index)?;
        let before = layer.nodes.get(loc.node_index)?.clone();
        let mut world = layer_node_to_world(host, img, &before);
        match edit(&mut world) {
            LayerMarkEdit::Keep => None,
            LayerMarkEdit::Patch => Some(SceneCmd::LayerNodePatch {
                host: loc.image,
                layer: layer.id,
                index: loc.node_index,
                after: Box::new(layer_node_from_world(host, img, &world)),
                before: Box::new(before),
            }),
            LayerMarkEdit::Remove => Some(SceneCmd::LayerNodeRemove {
                host: loc.image,
                layer: layer.id,
                index: loc.node_index,
                node: before,
            }),
        }
    }

    pub(crate) fn validate_image_paint_session(&mut self) {
        if self.image_paint.is_none()
            && self.shape_properties.panel == Some(super::board_properties::Panel::Layers)
        {
            self.sync_image_paint_for_tool();
        }
        let Some(session) = self.image_paint.as_ref() else {
            return;
        };
        let layer_controls_open =
            self.shape_properties.panel == Some(super::board_properties::Panel::Layers);
        if (!tool_hosts_on_image(self.board_tool) && !layer_controls_open)
            || self.board_sel.len() != 1
            || !self.board_sel.contains(&session.image)
            || self.image_result_id(session.image).as_ref() != Some(&session.result)
        {
            self.image_paint = None;
        }
    }

    pub(crate) fn next_paint_layer_id(img: &ImageNode) -> PaintLayerId {
        let next = img
            .paint_layers
            .iter()
            .chain(img.result_layers.iter().flat_map(|stack| &stack.layers))
            .map(|l| l.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        PaintLayerId(next)
    }

    pub fn ensure_paint_layer(&mut self, image: NodeId) {
        let node = match self.doc().scene.node(image) {
            Some(n) => n.clone(),
            None => return,
        };
        let NodeKind::Image(img) = &node.kind else {
            return;
        };
        let Some(result) = self.image_result_id(image) else {
            return;
        };
        let layers = Self::selected_layers(img, &result);
        let layer_index = if layers.is_empty() {
            self.add_paint_layer_on_image(image);
            0
        } else {
            self.image_paint
                .as_ref()
                .filter(|s| s.image == image && s.result == result)
                .map(|s| s.layer_index)
                .unwrap_or(0)
        };
        self.image_paint = Some(ImagePaintSession {
            image,
            result,
            layer_index,
            focus: ImageStripFocus::Layer(layer_index),
            layer_drag: None,
        });
    }

    pub fn add_paint_layer_on_image(&mut self, image: NodeId) -> Option<usize> {
        if self.refuse_read_only_edit() {
            return None;
        }
        let before = self.doc().scene.node(image)?.clone();
        let NodeKind::Image(ref img) = before.kind else {
            return None;
        };
        let result = self.image_result_id(image)?;
        let id = Self::next_paint_layer_id(img);
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return None;
        };
        Self::prepare_result_stack(img, result.clone());
        let index = Self::selected_stack_mut(img, &result).add_layer(PaintLayer::new(id));
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return None;
        }
        self.push_history(
            atlas_commands::CommandId("board.image.layer.add"),
            Some(format!("layer:{}", id.0)),
        );
        Some(index)
    }

    pub fn point_in_image_paint_window(&self, image: NodeId, world: Pos2) -> bool {
        let Some(node) = self.doc().scene.node(image) else {
            return false;
        };
        slate_doc::geom::point_in_node_outline(
            node,
            world.x,
            world.y,
            super::board_trim::trim_tokens::GEOMETRY_TOLERANCE,
        )
    }

    fn stroke_intersects_image_window(&self, host: &Node, _img: &ImageNode, node: &Node) -> bool {
        slate_doc::geom::stroke_intersects_node_outline(
            node,
            host,
            super::board_trim::trim_tokens::GEOMETRY_TOLERANCE,
        )
    }

    pub fn on_image_paint_add_clicked(&mut self, image: NodeId) {
        if let Some(idx) = self.add_paint_layer_on_image(image) {
            let Some(result) = self.image_result_id(image) else {
                return;
            };
            self.image_paint = Some(ImagePaintSession {
                image,
                result,
                layer_index: idx,
                focus: ImageStripFocus::Layer(idx),
                layer_drag: None,
            });
        }
    }

    pub(crate) fn eraser_hits_active_layer_along(&self, from: Pos2, to: Pos2) -> Vec<NodeId> {
        let Some(session) = self.image_paint.as_ref() else {
            return Vec::new();
        };
        if self.image_result_id(session.image).as_ref() != Some(&session.result) {
            return Vec::new();
        }
        let Some(host) = self.doc().scene.node(session.image) else {
            return Vec::new();
        };
        let NodeKind::Image(img) = &host.kind else {
            return Vec::new();
        };
        let Some(index) = Self::session_layer_raw_index(img, &session.result, session.layer_index)
        else {
            return Vec::new();
        };
        let Some(layer) = Self::selected_layers(img, &session.result).get(index) else {
            return Vec::new();
        };
        let zoom = self.tab().cam.z;
        let slop = (self.eraser_width * 0.5).max(1.0);
        let mut hits = Vec::new();
        for local in &layer.nodes {
            let n = layer_node_to_world(host, img, local);
            if super::board_color::eraser_sweep_hits_node(&n, from, to, slop, zoom) {
                hits.push(local.id);
            }
        }
        hits
    }

    pub(crate) fn collect_layer_erase_spot(
        &self,
        spot: &mut Vec<NodeId>,
        points: &[Pos2],
        straight: bool,
    ) {
        let Some(session) = self.image_paint.as_ref() else {
            return;
        };
        if self.image_result_id(session.image).as_ref() != Some(&session.result) {
            return;
        }
        let Some(host) = self.doc().scene.node(session.image) else {
            return;
        };
        let NodeKind::Image(img) = &host.kind else {
            return;
        };
        let Some(index) = Self::session_layer_raw_index(img, &session.result, session.layer_index)
        else {
            return;
        };
        let Some(layer) = Self::selected_layers(img, &session.result).get(index) else {
            return;
        };
        let tail: &[Pos2] = if straight || points.len() < 2 {
            points
        } else {
            &points[points.len() - 2..]
        };
        let r = self.eraser_width * 0.5;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in tail {
            x0 = x0.min(p.x - r);
            y0 = y0.min(p.y - r);
            x1 = x1.max(p.x + r);
            y1 = y1.max(p.y + r);
        }
        if !x0.is_finite() {
            return;
        }
        let bounds = WorldRect::new(x0, y0, x1 - x0, y1 - y0);
        for local in &layer.nodes {
            if spot.contains(&local.id) {
                continue;
            }
            let n = layer_node_to_world(host, img, local);
            if super::board_color::spot_reaches(&n, bounds) {
                spot.push(local.id);
            }
        }
    }

    /// Eraser release for paint-layer strokes (active layer only), with
    /// `removes`, the layer's vector marks the pass crossed, in the same
    /// ordered batch. Layer marks have no live preview: a coarse check runs
    /// within the frame's raster budget, and past it the mark takes the
    /// pass unseen (it stays, even when fully erased).
    pub(crate) fn finish_erase_layer_spot(
        &mut self,
        spot: &[NodeId],
        points: &[Pos2],
        span: slate_doc::scene::StrokeSpan,
        mut removes: Vec<SceneCmd>,
    ) -> (Vec<SceneCmd>, usize) {
        let mut cmds = Vec::new();
        let mut touched = 0usize;
        for id in spot {
            let mut world = None;
            self.patch_layer_node_in_world(*id, |w| {
                world = Some(w.clone());
                LayerMarkEdit::Keep
            });
            let Some(world) = world else {
                continue;
            };
            let fits = &mut |area| super::board_path::take_sync_budget(self, area);
            let result = super::board_color::stamp_erase_mark_within(&world, points, span, fits).0;
            let cmd = self.patch_layer_node_in_world(*id, |w| match result {
                None => LayerMarkEdit::Keep,
                Some((_, true)) => LayerMarkEdit::Remove,
                Some((erased, false)) => {
                    *w = erased;
                    LayerMarkEdit::Patch
                }
            });
            match cmd {
                Some(remove @ SceneCmd::LayerNodeRemove { .. }) => removes.push(remove),
                Some(patch) => cmds.push(patch),
                None => continue,
            }
            touched += 1;
        }
        // Every mark here is on the session's one layer and is addressed by
        // index: patch first, then remove from the back, so each index still
        // names its mark when it applies.
        removes.sort_by_key(|cmd| match cmd {
            SceneCmd::LayerNodeRemove { index, .. } => std::cmp::Reverse(*index),
            _ => std::cmp::Reverse(0),
        });
        cmds.extend(removes);
        (cmds, touched)
    }

    /// Legacy separate-area entry point. The shared image blister now owns
    /// these controls through `image_layer_palette_body`.
    pub(crate) fn image_paint_palette_ui(&mut self, ui: &mut egui::Ui, xf: &BoardXf) -> bool {
        let _ = (ui, xf);
        false
    }

    pub fn adopt_board_color_rgb(&mut self, rgb: [u8; 3]) {
        self.adopt_sampled_brush_color(rgb);
    }

    pub fn replace_image_item_preserve_layers(
        &mut self,
        target: NodeId,
        item: slate_doc::ItemId,
        remove_source: Option<NodeId>,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(before) = self.doc().scene.node(target).cloned() else {
            return false;
        };
        let NodeKind::Image(ref img) = before.kind else {
            return false;
        };
        if img.item == item && img.agent.is_none() {
            return false;
        }
        let current = self.image_result_id(target);
        let result = ImageResultId::Item(item);
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return false;
        };
        if let Some(current) = current.filter(|current| *current != result) {
            Self::prepare_result_stack(img, current.clone());
            // The displayed stack follows its replaced base; its authored
            // layer identities survive, as do other inactive result stacks.
            if result_layers(img, &result).is_none() {
                Self::selected_stack_mut(img, &current).result = result.clone();
            }
        }
        img.item = item;
        img.agent = None;
        let mut cmds = vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }];
        if let Some(id) = remove_source.filter(|id| *id != target) {
            if let Some(index) = self.doc().scene.nodes.iter().position(|node| node.id == id) {
                if let Some(node) = self.doc().scene.node(id).cloned() {
                    cmds.push(SceneCmd::Remove { index, node });
                }
            }
        }
        if !self.commit_scene(cmds) {
            return false;
        }
        if let Some(session) = self.image_paint.as_mut().filter(|s| s.image == target) {
            session.result = result;
        }
        self.push_history(
            atlas_commands::CommandId("board.image.replace"),
            Some("replace base".into()),
        );
        true
    }
}
