//! Image paint layers (trace paper). Contract: `image-paint-layers.md`.

use super::{board::BoardTool, board::BoardXf, SlateApp};
use eframe::egui::{self, Color32, Pos2};
use slate_doc::{
    image_paint::{
        clip_path_for_layer_node, ensure_result_layers_mut, find_layer_node, layer_node_from_world,
        layer_node_kind_allowed, layer_node_to_world, layers_for_result, result_layers,
        seed_result_from_legacy, ImageResultId, ImageResultLayers, LayerNodeRef, PaintLayerId,
    },
    scene::{ImageNode, Node, NodeKind, Rgba, SceneCmd, WorldRect},
    NodeId, PaintLayer, ViewState,
};

#[path = "board_image_layers/result_palette.rs"]
mod result_palette;
#[path = "board_image_layers/transfer.rs"]
mod transfer;
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

fn image_for_result(img: &ImageNode, result: &ImageResultId) -> ImageNode {
    let mut resolved = img.clone();
    resolved.paint_layers = layers_for_result(img, result).to_vec();
    resolved.result_layers.clear();
    resolved
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

/// Drop one image onto another: Replace base vs add as a layer child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageDropChoice {
    Replace,
    AddLayer,
}

#[derive(Clone, Debug)]
pub enum ImageDropSource {
    Node(NodeId),
    Item(slate_doc::ItemId),
}

#[derive(Clone, Debug)]
pub struct ImageDropOffer {
    pub target: NodeId,
    pub source: ImageDropSource,
    pub highlight: Option<ImageDropChoice>,
    /// Screen strip from the anchor through the last capsule, as last
    /// painted. A pointer crossing it on the way to a choice keeps the offer.
    pub row: Option<egui::Rect>,
}

impl SlateApp {
    fn selected_layers<'a>(img: &'a ImageNode, result: &ImageResultId) -> &'a [PaintLayer] {
        layers_for_result(img, result)
    }

    fn prepare_result_stack(img: &mut ImageNode, result: ImageResultId) {
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

    fn selected_stack_mut<'a>(
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

    pub fn paint_layer_host(&self) -> Option<NodeId> {
        self.image_paint.as_ref().map(|s| s.image)
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
        self.image_paint
            .as_ref()
            .is_some_and(|s| {
                s.image == image
                    && self.image_result_id(image).as_ref() == Some(&s.result)
                    && tool_hosts_on_image(self.board_tool)
            })
    }

    pub fn sync_image_paint_for_tool(&mut self) {
        if !tool_hosts_on_image(self.board_tool) {
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
            .unwrap_or_else(|| layers.len().saturating_sub(1));
        let focus = if layers.is_empty() {
            ImageStripFocus::Layer(0)
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
            });
        }
    }

    /// Where layer mark `id` sits, while the image paint session paints its
    /// layer. A mark on another image or layer is out of reach.
    pub(crate) fn session_layer_mark(&self, id: NodeId) -> Option<LayerNodeRef> {
        let loc = find_layer_node(&self.doc().scene, id)?;
        let session = self.image_paint.as_ref()?;
        let result_matches = loc.result.as_ref().map_or_else(
            || matches!(&session.result, ImageResultId::Item(_)),
            |result| result == &session.result,
        );
        let selected = self.image_result_id(session.image).as_ref() == Some(&session.result);
        (selected
            && session.image == loc.image
            && result_matches
            && session.layer_index == loc.layer_index)
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
        let Some(session) = self.image_paint.as_ref() else {
            return;
        };
        if !tool_hosts_on_image(self.board_tool)
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
                .unwrap_or(layers.len().saturating_sub(1))
        };
        self.image_paint = Some(ImagePaintSession {
            image,
            result,
            layer_index,
            focus: ImageStripFocus::Layer(layer_index),
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

    pub fn set_paint_layer_opacity(&mut self, image: NodeId, index: usize, opacity: f32) {
        if self.refuse_read_only_edit() {
            return;
        }
        let Some(before) = self.doc().scene.node(image).cloned() else {
            return;
        };
        let NodeKind::Image(ref img) = before.kind else {
            return;
        };
        let Some(result) = self.image_result_id(image) else {
            return;
        };
        if self
            .image_paint
            .as_ref()
            .is_some_and(|session| session.image == image && session.result != result)
        {
            return;
        }
        let Some(layer) = Self::selected_layers(img, &result).get(index) else {
            return;
        };
        let old_opacity = layer.opacity;
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return;
        };
        Self::prepare_result_stack(img, result.clone());
        let layers = Self::selected_layers_mut(img, &result);
        layers[index].opacity = opacity.clamp(0.0, 1.0);
        if (layers[index].opacity - old_opacity).abs() < 1e-4 {
            return;
        }
        let _ = self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]);
        self.push_history(
            atlas_commands::CommandId("board.image.layer.opacity"),
            Some(format!("layer:{index}")),
        );
    }

    pub fn delete_paint_layer(&mut self, image: NodeId, index: usize) {
        if self.refuse_read_only_edit() {
            return;
        }
        let Some(before) = self.doc().scene.node(image).cloned() else {
            return;
        };
        let NodeKind::Image(ref img) = before.kind else {
            return;
        };
        let Some(result) = self.image_result_id(image) else {
            return;
        };
        if self
            .image_paint
            .as_ref()
            .is_some_and(|session| session.image == image && session.result != result)
        {
            return;
        }
        if index >= Self::selected_layers(img, &result).len() {
            return;
        }
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return;
        };
        Self::prepare_result_stack(img, result.clone());
        let stack = Self::selected_stack_mut(img, &result);
        stack.remove_layer(index);
        let remaining = stack.layers.len();
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return;
        }
        self.push_history(
            atlas_commands::CommandId("board.image.layer.delete"),
            Some(format!("layer:{index}")),
        );
        if let Some(session) = &mut self.image_paint {
            if session.image == image {
                if remaining == 0 {
                    session.layer_index = 0;
                    session.focus = ImageStripFocus::Filter;
                } else {
                    session.layer_index = session.layer_index.min(remaining - 1);
                    session.focus = ImageStripFocus::Layer(session.layer_index);
                }
            }
        }
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

    /// When image-hosting, commit strokes to the active layer. `None` → use
    /// [`SlateApp::add_nodes`]. `Some` may be empty when nothing intersected (D03).
    pub fn commit_paint_layer_nodes(&mut self, nodes: Vec<Node>) -> Option<Vec<NodeId>> {
        let session = self.image_paint.clone()?;
        if nodes.is_empty() {
            return Some(Vec::new());
        }
        if self.image_result_id(session.image).as_ref() != Some(&session.result) {
            return Some(Vec::new());
        }
        if !nodes.iter().all(|n| layer_node_kind_allowed(&n.kind)) {
            return Some(Vec::new());
        }
        let Some(host) = self.doc().scene.node(session.image).cloned() else {
            return Some(Vec::new());
        };
        let NodeKind::Image(ref host_img) = host.kind else {
            return Some(Vec::new());
        };
        let accepted: Vec<Node> = nodes
            .into_iter()
            .filter(|n| self.stroke_intersects_image_window(&host, host_img, n))
            .collect();
        if accepted.is_empty() {
            return Some(Vec::new());
        }
        let mut locals: Vec<Node> = accepted
            .iter()
            .map(|n| layer_node_from_world(&host, host_img, n))
            .collect();
        if let Some(contours) = self.image_selection_clip(session.image) {
            for local in &mut locals {
                local.clip = clip_path_for_layer_node(local, &contours);
            }
        }
        let ids: Vec<NodeId> = locals.iter().map(|n| n.id).collect();
        let mut cmds = Vec::new();
        let mut after = host.clone();
        let NodeKind::Image(ref mut after_img) = after.kind else {
            return Some(Vec::new());
        };
        Self::prepare_result_stack(after_img, session.result.clone());
        let needs_layer = Self::selected_layers(after_img, &session.result).is_empty();
        let layer_id = if needs_layer {
            let layer_id = Self::next_paint_layer_id(after_img);
            Self::selected_stack_mut(after_img, &session.result)
                .add_layer(PaintLayer::new(layer_id));
            layer_id
        } else {
            let layers = Self::selected_layers(after_img, &session.result);
            layers[session.layer_index.min(layers.len() - 1)].id
        };
        let layer_index = Self::selected_layers(after_img, &session.result)
            .iter()
            .position(|layer| layer.id == layer_id)
            .unwrap_or(0);
        if after != host {
            cmds.push(SceneCmd::Patch {
                before: Box::new(host.clone()),
                after: Box::new(after),
            });
        }
        let base = self
            .doc()
            .scene
            .node(session.image)
            .and_then(|n| match &n.kind {
                NodeKind::Image(img) => layers_for_result(img, &session.result)
                    .iter()
                    .find(|l| l.id == layer_id)
                    .map(|l| l.nodes.len()),
                _ => None,
            })
            .unwrap_or(0);
        for (i, local) in locals.into_iter().enumerate() {
            cmds.push(SceneCmd::LayerNodeAdd {
                host: session.image,
                layer: layer_id,
                index: base + i,
                node: local,
            });
        }
        if !self.commit_scene(cmds) {
            return Some(Vec::new());
        }
        if let Some(session) = &mut self.image_paint {
            if session.image == host.id {
                session.layer_index = layer_index;
                session.focus = ImageStripFocus::Layer(layer_index);
            }
        }
        Some(ids)
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
            });
            if self.board_tool == BoardTool::Select {
                self.board_tool = BoardTool::Brush;
            }
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
        let Some(layer) = Self::selected_layers(img, &session.result).get(session.layer_index)
        else {
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
        let Some(layer) = Self::selected_layers(img, &session.result).get(session.layer_index)
        else {
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
        let a = self.board_colors.fg.0[3];
        self.board_colors.fg = Rgba([rgb[0], rgb[1], rgb[2], a]);
        self.settings.board_fg = Some(self.board_colors.fg.0);
        self.tab_mut().doc.view.remember_color(rgb);
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
        if img.item == item {
            return false;
        }
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return false;
        };
        img.item = item;
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
        self.push_history(
            atlas_commands::CommandId("board.image.replace"),
            Some("replace base".into()),
        );
        true
    }

    pub fn add_dropped_image_as_layer(
        &mut self,
        target: NodeId,
        item: slate_doc::ItemId,
        remove_source: Option<NodeId>,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(host) = self.doc().scene.node(target).cloned() else {
            return false;
        };
        let NodeKind::Image(ref host_img) = host.kind else {
            return false;
        };
        let Some(result) = self.image_result_id(target) else {
            return false;
        };
        let mut child = self.doc_mut().scene.build_node(
            WorldRect::new(host.rect.x, host.rect.y, host.rect.w, host.rect.h),
            NodeKind::Image(ImageNode::new(item)),
        );
        child.rotation_deg = host.rotation_deg;
        let local = layer_node_from_world(&host, host_img, &child);
        let mut cmds = Vec::new();
        let mut after = host.clone();
        let NodeKind::Image(ref mut after_img) = after.kind else {
            return false;
        };
        Self::prepare_result_stack(after_img, result.clone());
        let count = Self::selected_layers(after_img, &result).len();
        let layer_index = self
            .image_paint
            .as_ref()
            .filter(|session| session.image == target && session.result == result)
            .map(|session| session.layer_index)
            .unwrap_or_else(|| count.saturating_sub(1))
            .min(count.saturating_sub(1));
        let layer_id = if count == 0 {
            let id = Self::next_paint_layer_id(after_img);
            Self::selected_stack_mut(after_img, &result).add_layer(PaintLayer::new(id));
            id
        } else {
            Self::selected_layers(after_img, &result)[layer_index].id
        };
        if after != host {
            cmds.push(SceneCmd::Patch {
                before: Box::new(host.clone()),
                after: Box::new(after),
            });
        }
        let base = self
            .doc()
            .scene
            .node(target)
            .and_then(|node| match &node.kind {
                NodeKind::Image(img) => layers_for_result(img, &result)
                    .iter()
                    .find(|layer| layer.id == layer_id)
                    .map(|layer| layer.nodes.len()),
                _ => None,
            })
            .unwrap_or(0);
        cmds.push(SceneCmd::LayerNodeAdd {
            host: target,
            layer: layer_id,
            index: base,
            node: local,
        });
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
        self.image_paint = Some(ImagePaintSession {
            image: target,
            result,
            layer_index,
            focus: ImageStripFocus::Layer(layer_index),
        });
        self.push_history(
            atlas_commands::CommandId("board.image.layer.add_image"),
            Some("add as layer".into()),
        );
        true
    }

    pub(crate) fn update_image_drop_offer(&mut self, world: Pos2, ids: &[NodeId]) {
        let screen = self.board_xf().w2s(world);
        let keep = self.image_drop.as_ref().is_some_and(|offer| {
            matches!(offer.source, ImageDropSource::Node(id) if ids == [id])
                && offer.row.is_some_and(|row| row.contains(screen))
        });
        if keep {
            return;
        }
        self.image_drop = None;
        if ids.len() != 1 || self.board_tool != BoardTool::Select {
            return;
        };
        let source_id = ids[0];
        let Some(source) = self.doc().scene.node(source_id) else {
            return;
        };
        let NodeKind::Image(src_img) = &source.kind else {
            return;
        };
        if src_img.item.is_none() {
            return;
        };
        let target = self.image_under_point(world, ids);
        let Some(target) = target else {
            return;
        };
        self.image_drop = Some(ImageDropOffer {
            target,
            source: ImageDropSource::Node(source_id),
            highlight: None,
            row: None,
        });
    }

    pub(crate) fn image_drop_target_at(&self, world: Pos2) -> Option<NodeId> {
        self.image_under_point(world, &[])
    }

    pub(crate) fn offer_image_file_drop(
        &mut self,
        target: NodeId,
        item: slate_doc::ItemId,
        screen: Pos2,
    ) {
        self.image_drop = Some(ImageDropOffer {
            target,
            source: ImageDropSource::Item(item),
            highlight: None,
            row: None,
        });
        self.image_drop_screen = Some(screen);
    }

    /// The topmost node under `world` other than `skip`, when it is a raster
    /// image. A dragged image sits under the pointer itself, so it is skipped.
    fn image_under_point(&self, world: Pos2, skip: &[NodeId]) -> Option<NodeId> {
        let node = self.doc().scene.nodes.iter().rev().find(|n| {
            !n.hidden
                && !n.locked
                && !n.is_frame()
                && !skip.contains(&n.id)
                && !matches!(n.kind, NodeKind::Connector(_))
                && n.rect.contains_rotated(world.x, world.y, n.rotation_deg)
        })?;
        let pick = node.id;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        if img.item.is_none() {
            return None;
        };
        if !Self::supports_image_paint(pick, self) {
            return None;
        }
        Some(pick)
    }

    pub(crate) fn paint_image_drop_capsules(&mut self, ui: &egui::Ui, painter: &egui::Painter) {
        if self.image_drop.is_none() {
            return;
        }
        let pointer = ui.ctx().pointer_latest_pos().or(self.image_drop_screen);
        let Some(pointer) = pointer else {
            return;
        };
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.image_drop = None;
            self.image_drop_screen = None;
            return;
        }
        let z = self.tab().cam.z;
        let labels = [
            ("Replace", ImageDropChoice::Replace),
            ("Add as layer", ImageDropChoice::AddLayer),
        ];
        let pad = atlas_shell::canvas_scale::px(10.0, z);
        let h = atlas_shell::canvas_scale::px(22.0, z);
        let gap = atlas_shell::canvas_scale::px(6.0, z);
        let palette = self.palette();
        let target = self
            .image_drop
            .as_ref()
            .and_then(|offer| self.doc().scene.node(offer.target));
        let anchor = if let Some(screen) = self.image_drop_screen {
            screen
        } else if let Some(target) = target {
            let edge = target.rect.rotate_point(
                [target.rect.x + target.rect.w, target.rect.center().1],
                target.rotation_deg,
            );
            self.board_xf().w2s(Pos2::new(edge[0], edge[1]))
        } else {
            pointer
        };
        let mut rects = Vec::new();
        let mut x = anchor.x + pad;
        let y = anchor.y;
        let mut highlight = None;
        for (label, choice) in labels {
            let galley = atlas_shell::canvas_text::layout_no_wrap(
                painter,
                label.to_string(),
                atlas_shell::canvas_scale::font(12.0, z),
                palette.ink,
            );
            let w = galley.size().x + pad * 2.0;
            let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
            if rect.contains(pointer) {
                highlight = Some(choice);
            }
            rects.push((rect, label, choice));
            x += w + gap;
        }
        let row = rects.iter().fold(
            egui::Rect::from_min_size(egui::pos2(anchor.x, y), egui::vec2(0.0, h)),
            |row, (rect, _, _)| row.union(*rect),
        );
        let file_drop = self.image_drop_screen.is_some();
        // egui keeps the pointer's last position from before an OS drag, so
        // only real movement away from the target and row dismisses a drop.
        let moved = ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
        if file_drop && moved && !row.contains(pointer) {
            let over_target = target.is_some_and(|target| {
                let local = slate_doc::geom::world_to_local(
                    self.board_xf().s2w(pointer).x,
                    self.board_xf().s2w(pointer).y,
                    target.rect,
                    target.rotation_deg,
                );
                (0.0..=1.0).contains(&local.0) && (0.0..=1.0).contains(&local.1)
            });
            if !over_target {
                self.image_drop = None;
                self.image_drop_screen = None;
                return;
            }
        }
        // The capsules are their own foreground area: a press on one is that
        // choice and never starts a board gesture on the image beneath.
        let mut chosen = None;
        let canvas = self.canvas_rect;
        egui::Area::new(egui::Id::new("image_drop_capsules"))
            .order(egui::Order::Foreground)
            .fixed_pos(row.min)
            .constrain(false)
            .movable(false)
            .fade_in(false)
            .show(ui.ctx(), |ui| {
                ui.set_clip_rect(canvas);
                ui.set_min_size(row.size());
                let sense = if file_drop {
                    egui::Sense::click_and_drag()
                } else {
                    egui::Sense::hover()
                };
                for (rect, label, choice) in &rects {
                    let response = ui.interact(*rect, ui.id().with(label), sense);
                    let hot = highlight == Some(*choice);
                    if file_drop && (response.clicked() || (response.drag_stopped() && hot)) {
                        chosen = Some(*choice);
                    }
                    atlas_shell::selection_tools::paint_capsule_row(
                        ui.painter(),
                        *rect,
                        &atlas_shell::selection_tools::Capsule {
                            label,
                            chip: None,
                            badge: None,
                            selected: hot,
                            spawned: false,
                            disabled: false,
                            dim: false,
                        },
                        hot,
                        false,
                        z,
                        palette,
                    );
                }
            });
        if let Some(d) = &mut self.image_drop {
            d.highlight = chosen.or(highlight);
            d.row = Some(row);
        }
        if chosen.is_some() {
            let item = self.image_drop_item();
            self.commit_image_drop(item, false);
        }
    }

    /// A drop target is highlighted: releasing now commits a drop.
    pub(crate) fn image_drop_armed(&self) -> bool {
        self.image_drop
            .as_ref()
            .is_some_and(|offer| offer.highlight.is_some())
    }

    /// The picture the armed drop places. A dragged node's picture must be
    /// read before the drag rewinds, while a staged copy still exists.
    pub(crate) fn image_drop_item(&self) -> Option<slate_doc::ItemId> {
        let offer = self.image_drop.as_ref()?;
        let item = match offer.source {
            ImageDropSource::Node(id) => match &self.doc().scene.node(id)?.kind {
                NodeKind::Image(img) => img.item,
                _ => return None,
            },
            ImageDropSource::Item(item) => item,
        };
        (!item.is_none()).then_some(item)
    }

    /// Commit the armed drop of `item` as one journal group (P0.2). The
    /// caller has already rewound any live drag. A dragged source node is
    /// consumed unless `keep_source` (an Alt copy leaves its original).
    pub(crate) fn commit_image_drop(
        &mut self,
        item: Option<slate_doc::ItemId>,
        keep_source: bool,
    ) -> bool {
        let Some(offer) = self.image_drop.take() else {
            return false;
        };
        let (Some(choice), Some(item)) = (offer.highlight, item) else {
            return false;
        };
        let source_node = match offer.source {
            ImageDropSource::Node(id) if !keep_source => Some(id),
            _ => None,
        };
        let ok = match choice {
            ImageDropChoice::Replace => {
                self.replace_image_item_preserve_layers(offer.target, item, source_node)
            }
            ImageDropChoice::AddLayer => {
                self.add_dropped_image_as_layer(offer.target, item, source_node)
            }
        };
        if ok {
            self.image_drop_screen = None;
            return true;
        }
        false
    }
}
