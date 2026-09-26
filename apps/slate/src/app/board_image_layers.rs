//! Image paint layers (trace paper). Contract: `image-paint-layers.md`.

use super::{board::BoardTool, board::BoardXf, SlateApp};
use eframe::egui::{self, Color32, Pos2};
use slate_doc::{
    image_paint::{
        find_layer_node, layer_node_from_world, layer_node_kind_allowed, layer_node_to_world,
        LayerNodeRef, PaintLayerId,
    },
    scene::{ImageNode, Node, NodeKind, Rgba, SceneCmd, WorldRect},
    NodeId, PaintLayer, ViewState,
};

/// Which chip owns the shared intensity slider in the Filters capsule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageStripFocus {
    Filter,
    Layer(usize),
}

#[derive(Clone, Debug)]
pub struct ImagePaintSession {
    pub image: NodeId,
    pub layer_index: usize,
    pub focus: ImageStripFocus,
}

#[derive(Clone)]
pub(crate) struct PaintLayerTextureCache {
    key: u128,
    texture: egui::TextureHandle,
}

fn paint_layer_cache_key(host: &Node, img: &ImageNode, gen: u64, zoom_bucket: u32) -> u128 {
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
    key.wrapping_mul(31).wrapping_add(c.h.to_bits() as u128)
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
}

impl SlateApp {
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
            .is_some_and(|s| s.image == image && tool_hosts_on_image(self.board_tool))
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
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return;
        };
        let NodeKind::Image(ref img) = node.kind else {
            return;
        };
        let layer_index = self
            .image_paint
            .as_ref()
            .filter(|s| s.image == id)
            .map(|s| s.layer_index)
            .unwrap_or_else(|| img.paint_layers.len().saturating_sub(1));
        let focus = if img.paint_layers.is_empty() {
            ImageStripFocus::Layer(0)
        } else {
            ImageStripFocus::Layer(layer_index.min(img.paint_layers.len() - 1))
        };
        if self.image_paint.as_ref().map(|s| s.image) != Some(id) {
            self.image_paint = Some(ImagePaintSession {
                image: id,
                layer_index: match focus {
                    ImageStripFocus::Layer(i) => i,
                    ImageStripFocus::Filter => 0,
                },
                focus,
            });
        }
    }

    pub(crate) fn validate_image_paint_session(&mut self) {
        let Some(session) = self.image_paint.as_ref() else {
            return;
        };
        // A layer chip picked on the Filters strip in Select keeps its focus
        // so the strip's one slider drives that layer's opacity.
        let strip_layer = self.board_tool == BoardTool::Select
            && self.shape_properties.panel == Some(super::board_properties::Panel::Filter)
            && matches!(session.focus, ImageStripFocus::Layer(_));
        if !(tool_hosts_on_image(self.board_tool) || strip_layer)
            || self.board_sel.len() != 1
            || !self.board_sel.contains(&session.image)
        {
            self.image_paint = None;
        }
    }

    fn next_paint_layer_id(img: &ImageNode) -> PaintLayerId {
        let next = img
            .paint_layers
            .iter()
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
        let layer_index = if img.paint_layers.is_empty() {
            self.add_paint_layer_on_image(image);
            0
        } else {
            self.image_paint
                .as_ref()
                .filter(|s| s.image == image)
                .map(|s| s.layer_index)
                .unwrap_or(img.paint_layers.len().saturating_sub(1))
        };
        self.image_paint = Some(ImagePaintSession {
            image,
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
        let id = Self::next_paint_layer_id(img);
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return None;
        };
        img.paint_layers.push(PaintLayer::new(id));
        let index = img.paint_layers.len() - 1;
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
        let Some(layer) = img.paint_layers.get(index) else {
            return;
        };
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return;
        };
        img.paint_layers[index].opacity = opacity.clamp(0.0, 1.0);
        if (img.paint_layers[index].opacity - layer.opacity).abs() < 1e-4 {
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
        if index >= img.paint_layers.len() {
            return;
        }
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return;
        };
        img.paint_layers.remove(index);
        let remaining = img.paint_layers.len();
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
        let locals: Vec<Node> = accepted
            .iter()
            .map(|n| layer_node_from_world(&host, host_img, n))
            .collect();
        let ids: Vec<NodeId> = locals.iter().map(|n| n.id).collect();
        let mut cmds = Vec::new();
        let (layer_id, layer_index) = if host_img.paint_layers.is_empty() {
            let before = host.clone();
            let layer_id = Self::next_paint_layer_id(host_img);
            let mut after = before.clone();
            let NodeKind::Image(ref mut img) = after.kind else {
                return Some(Vec::new());
            };
            img.paint_layers.push(PaintLayer::new(layer_id));
            cmds.push(SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
            (layer_id, 0)
        } else {
            let idx = session.layer_index.min(host_img.paint_layers.len() - 1);
            (host_img.paint_layers[idx].id, idx)
        };
        let base = self
            .doc()
            .scene
            .node(session.image)
            .and_then(|n| match &n.kind {
                NodeKind::Image(img) => img
                    .paint_layers
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint_image_paint_layers(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        host: &Node,
        img: &ImageNode,
        outline: &[Pos2],
        _srect: egui::Rect,
        host_alpha: f32,
        _z: f32,
    ) {
        if img.paint_layers.is_empty() {
            return;
        }
        let ppp = ui.ctx().pixels_per_point();
        let longest_px = (host.rect.w.max(host.rect.h) * xf.z * ppp)
            .ceil()
            .clamp(1.0, 4096.0);
        let zoom_bucket = (longest_px as u32).max(1).next_power_of_two();
        let key = paint_layer_cache_key(host, img, self.scene_gen, zoom_bucket);
        let stale = self
            .paint_layer_texture_cache
            .get(&host.id)
            .is_none_or(|entry| entry.key != key);
        if stale {
            let aspect = host.rect.w.max(1e-6) / host.rect.h.max(1e-6);
            let (w, h) = if aspect >= 1.0 {
                (zoom_bucket, (zoom_bucket as f32 / aspect).ceil() as u32)
            } else {
                ((zoom_bucket as f32 * aspect).ceil() as u32, zoom_bucket)
            };
            let mut stamps = std::mem::take(&mut self.paint_layer_stamps);
            let raster = slate_artifact::rasterize_paint_layers(
                host,
                img,
                w,
                h,
                Some(self.doc()),
                &mut stamps,
            );
            self.paint_layer_stamps = stamps;
            if let Some(rgba) = raster {
                let image =
                    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                let texture = ui.ctx().load_texture(
                    format!("paint-layer-{}", host.id.0),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                self.paint_layer_texture_cache
                    .insert(host.id, PaintLayerTextureCache { key, texture });
            }
        }
        let Some(texture) = self
            .paint_layer_texture_cache
            .get(&host.id)
            .map(|entry| entry.texture.clone())
        else {
            return;
        };
        let tint = Color32::WHITE.gamma_multiply(host_alpha.clamp(0.0, 1.0));
        if let Some(clip) = &host.clip {
            super::board::paint_clipped_texture(
                painter,
                xf,
                &texture,
                host,
                clip,
                slate_doc::scene::Crop::full(),
                slate_doc::scene::Mirror::default(),
                tint,
            );
        } else if outline.len() >= 3 {
            let outline_world = outline
                .iter()
                .map(|point| {
                    let world = xf.s2w(*point);
                    (world.x, world.y)
                })
                .collect::<Vec<_>>();
            super::board::textured_polygon_world(
                painter,
                &texture,
                outline,
                &outline_world,
                host.rect,
                slate_doc::scene::Crop::full(),
                tint,
                host.rotation_deg,
            );
        }
    }

    pub fn on_image_paint_add_clicked(&mut self, image: NodeId) {
        if let Some(idx) = self.add_paint_layer_on_image(image) {
            self.image_paint = Some(ImagePaintSession {
                image,
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
        let Some(host) = self.doc().scene.node(session.image) else {
            return Vec::new();
        };
        let NodeKind::Image(img) = &host.kind else {
            return Vec::new();
        };
        let Some(layer) = img.paint_layers.get(session.layer_index) else {
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
        let Some(host) = self.doc().scene.node(session.image) else {
            return;
        };
        let NodeKind::Image(img) = &host.kind else {
            return;
        };
        let Some(layer) = img.paint_layers.get(session.layer_index) else {
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

    /// Eraser release for paint-layer strokes (active layer only).
    pub(crate) fn finish_erase_layer_spot(
        &mut self,
        spot: &[NodeId],
        points: &[Pos2],
        span: slate_doc::scene::StrokeSpan,
        live: &std::collections::HashMap<NodeId, super::board_path::EraseLive>,
    ) -> (Vec<SceneCmd>, usize) {
        let mut cmds = Vec::new();
        let mut removes: Vec<(LayerNodeRef, Node, PaintLayerId)> = Vec::new();
        let mut touched = 0usize;
        for id in spot {
            if live.get(id).is_some_and(|l| !l.changed) {
                continue;
            }
            let Some(loc) = find_layer_node(&self.doc().scene, *id) else {
                continue;
            };
            let Some(session) = self.image_paint.as_ref() else {
                continue;
            };
            if session.image != loc.image || session.layer_index != loc.layer_index {
                continue;
            }
            let Some(host) = self.doc().scene.node(loc.image).cloned() else {
                continue;
            };
            let NodeKind::Image(ref snap_img) = host.kind else {
                continue;
            };
            let Some(layer_id) = snap_img.paint_layers.get(loc.layer_index).map(|l| l.id) else {
                continue;
            };
            let Some(before_local) = snap_img.paint_layers[loc.layer_index]
                .nodes
                .get(loc.node_index)
                .cloned()
            else {
                continue;
            };
            let world = layer_node_to_world(&host, snap_img, &before_local);
            let Some((world, gone)) = super::board_color::stamp_erase_mark(&world, points, span)
            else {
                continue;
            };
            touched += 1;
            if gone {
                removes.push((loc, before_local, layer_id));
            } else {
                let after_local = layer_node_from_world(&host, snap_img, &world);
                cmds.push(SceneCmd::LayerNodePatch {
                    host: loc.image,
                    layer: layer_id,
                    index: loc.node_index,
                    before: Box::new(before_local),
                    after: Box::new(after_local),
                });
            }
        }
        removes.sort_by(|a, b| {
            b.0.layer_index
                .cmp(&a.0.layer_index)
                .then(b.0.node_index.cmp(&a.0.node_index))
        });
        for (loc, node, layer_id) in removes {
            cmds.push(SceneCmd::LayerNodeRemove {
                host: loc.image,
                layer: layer_id,
                index: loc.node_index,
                node,
            });
        }
        (cmds, touched)
    }

    pub(crate) fn paint_image_paint_recent_colors(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        host: &Node,
        srect: egui::Rect,
    ) {
        let Some(session) = self.image_paint.as_ref() else {
            return;
        };
        if session.image != host.id {
            return;
        }
        let z = xf.z;
        let recents = self.doc().view.recent_colors.clone().unwrap_or_default();
        let n = recents.len().min(ViewState::RECENT_COLOR_LIMIT);
        if n == 0 {
            return;
        }
        let gap = atlas_shell::canvas_scale::px(6.0, z);
        let radius = atlas_shell::canvas_scale::px(8.0, z);
        let ring = self.palette().border_strong;
        let row_w = n as f32 * (radius * 2.0 + gap) - gap;
        let center = egui::pos2(srect.center().x, srect.bottom() + gap + radius);
        let x = center.x - row_w * 0.5 + radius;
        let pointer = ui.ctx().pointer_latest_pos();
        for (i, rgb) in recents.iter().take(n).enumerate() {
            let at = egui::pos2(x + i as f32 * (radius * 2.0 + gap), center.y);
            let color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            painter.circle_filled(at, radius, color);
            painter.circle_stroke(
                at,
                radius,
                egui::Stroke::new(atlas_shell::canvas_scale::px(1.0, z), ring),
            );
            if pointer.is_some_and(|p| (p - at).length() <= radius * 1.2)
                && ui.input(|i| i.pointer.primary_clicked())
            {
                self.adopt_board_color_rgb(*rgb);
            }
        }
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
        let mut child = self.doc_mut().scene.build_node(
            WorldRect::new(host.rect.x, host.rect.y, host.rect.w, host.rect.h),
            NodeKind::Image(ImageNode::new(item)),
        );
        child.rotation_deg = host.rotation_deg;
        let local = layer_node_from_world(&host, host_img, &child);
        let mut cmds = Vec::new();
        let (layer_id, layer_index) = if host_img.paint_layers.is_empty() {
            let layer_id = Self::next_paint_layer_id(host_img);
            let mut after = host.clone();
            let NodeKind::Image(ref mut img) = after.kind else {
                return false;
            };
            img.paint_layers.push(PaintLayer::new(layer_id));
            cmds.push(SceneCmd::Patch {
                before: Box::new(host.clone()),
                after: Box::new(after),
            });
            (layer_id, 0)
        } else {
            let index = self
                .image_paint
                .as_ref()
                .filter(|session| session.image == target)
                .map(|session| session.layer_index)
                .unwrap_or(host_img.paint_layers.len() - 1)
                .min(host_img.paint_layers.len() - 1);
            (host_img.paint_layers[index].id, index)
        };
        cmds.push(SceneCmd::LayerNodeAdd {
            host: target,
            layer: layer_id,
            index: host_img
                .paint_layers
                .get(layer_index)
                .map_or(0, |layer| layer.nodes.len()),
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
        let target = self.image_under_point(world, source_id);
        let Some(target) = target else {
            return;
        };
        self.image_drop = Some(ImageDropOffer {
            target,
            source: ImageDropSource::Node(source_id),
            highlight: None,
        });
    }

    pub(crate) fn image_drop_target_at(&self, world: Pos2) -> Option<NodeId> {
        self.image_under_point(world, NodeId(u64::MAX))
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
        });
        self.image_drop_screen = Some(screen);
    }

    fn image_under_point(&self, world: Pos2, skip: NodeId) -> Option<NodeId> {
        let pick = super::board_path::board_pick_node_routed(
            &self.doc().scene,
            world.x,
            world.y,
            self.tab().cam.z,
            false,
            self.board_wire_routing,
        )?;
        if pick == skip {
            return None;
        }
        let node = self.doc().scene.node(pick)?;
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
        if self.image_drop_screen.is_some() {
            let over_capsule = rects.iter().any(|(rect, _, _)| rect.contains(pointer));
            let over_target = target.is_some_and(|target| {
                let local = slate_doc::geom::world_to_local(
                    self.board_xf().s2w(pointer).x,
                    self.board_xf().s2w(pointer).y,
                    target.rect,
                    target.rotation_deg,
                );
                (0.0..=1.0).contains(&local.0) && (0.0..=1.0).contains(&local.1)
            });
            if !over_capsule && !over_target {
                self.image_drop = None;
                self.image_drop_screen = None;
                return;
            }
        }
        if let Some(d) = &mut self.image_drop {
            d.highlight = highlight;
        }
        for (rect, label, choice) in rects {
            let hot = highlight == Some(choice);
            atlas_shell::selection_tools::paint_capsule_row(
                painter,
                rect,
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
    }

    pub(crate) fn try_commit_image_drop(&mut self, ids: &[NodeId], before: &[Node]) -> bool {
        let Some(offer) = self.image_drop.take() else {
            return false;
        };
        let Some(choice) = offer.highlight else {
            return false;
        };
        if !ids.is_empty() {
            // Rewind the live drag preview before building the one journal
            // group that owns both target and source mutations (P0.2).
            let scene = &mut self.doc_mut().scene;
            for (id, baseline) in ids.iter().zip(before.iter()) {
                if let Some(node) = scene.node_mut(*id) {
                    *node = baseline.clone();
                }
            }
        }
        let item = match offer.source {
            ImageDropSource::Node(id) => {
                let Some(node) = self.doc().scene.node(id) else {
                    return false;
                };
                let NodeKind::Image(img) = &node.kind else {
                    return false;
                };
                img.item
            }
            ImageDropSource::Item(item) => item,
        };
        if item.is_none() {
            return false;
        }
        let source_node = match offer.source {
            ImageDropSource::Node(id) => Some(id),
            ImageDropSource::Item(_) => None,
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

    pub(crate) fn try_commit_image_drop_click(&mut self) -> bool {
        let ready = self
            .image_drop
            .as_ref()
            .is_some_and(|o| matches!(o.source, ImageDropSource::Item(_)) && o.highlight.is_some());
        if ready {
            return self.try_commit_image_drop(&[], &[]);
        }
        false
    }
}
