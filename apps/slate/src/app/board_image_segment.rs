//! Local image subject highlights. Durable highlights are ordinary paint paths.

use super::{board::BoardXf, SlateApp};
use atlas_segment::Request;
use atlas_shell::selection_tools;
use eframe::egui::{self, Id, Pos2, Rect};
use slate_doc::{
    image_paint::{layer_node_to_world, PaintLayer, PaintLayerPrompt},
    scene::{Node, NodeKind, Rgba, SceneCmd, ShapeKind, ShapeNode, Stroke, WorldRect},
    NodeId,
};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

const LINGER: Duration = Duration::from_millis(400);
const MOVE_RESET_PX: f32 = 8.0;
const SAMPLE_EDGE: usize = 768;
const HIGHLIGHT: Rgba = Rgba([255, 150, 40, 100]);

#[cfg(test)]
#[path = "image_segment_tests.rs"]
mod tests;

#[derive(Clone)]
pub(crate) struct SegmentResult {
    pub host: NodeId,
    pub gen: u64,
    pub local: Node,
    mask: std::sync::Arc<Vec<Vec<[f32; 2]>>>,
}

mod runtime;
use runtime::Hover;
pub(crate) use runtime::ImageSegmentRuntime;

impl SlateApp {
    pub(crate) fn image_segment_preview_live(&self) -> bool {
        self.image_segments.result.is_some()
    }

    pub(crate) fn dismiss_image_segment(&mut self) -> bool {
        let Some(hover) = self.image_segments.hover.as_ref() else {
            return false;
        };
        self.image_segments.dismissed = Some(hover.screen);
        self.image_segments.clear();
        true
    }

    pub(crate) fn tick_image_segment_hover(
        &mut self,
        ui: &egui::Ui,
        xf: &BoardXf,
        hover_live: bool,
        pointer: Option<Pos2>,
        world: Option<Pos2>,
    ) {
        self.image_segments.receive();
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.dismiss_image_segment();
            return;
        }
        if !hover_live {
            self.image_segments.clear();
            return;
        }
        let (Some(screen), Some(world)) = (pointer, world) else {
            self.image_segments.clear();
            return;
        };
        if self
            .image_segments
            .dismissed
            .is_some_and(|p| p.distance(screen) <= MOVE_RESET_PX)
        {
            return;
        }
        self.image_segments.dismissed = None;
        if self
            .image_segments
            .hover
            .as_ref()
            .is_some_and(|h| h.gen != self.scene_gen || h.tab != self.tab().id)
        {
            self.image_segments.clear();
        }
        // Preserve the offer across the gap to its capsule. Its press is chrome,
        // never a drag or a click-through on the underlying image.
        if (self.image_segments.result.is_some()
            || self.image_segments.pending.is_some()
            || self.image_segments.error.is_some())
            && self
                .image_segments
                .corridor
                .is_some_and(|r| r.contains(screen))
        {
            return;
        }
        let Some(host) = self.board_pick_node(world.x, world.y).filter(|&host| {
            Self::supports_image_paint(host, self) && self.point_in_image_paint_window(host, world)
        }) else {
            self.image_segments.clear();
            return;
        };
        let node = self.doc().scene.node(host).unwrap();
        let NodeKind::Image(img) = &node.kind else {
            return;
        };
        let point = slate_doc::image_paint::world_to_host_norm(node, img, world.x, world.y);
        let point_norm = [point.0.clamp(0.0, 1.0), point.1.clamp(0.0, 1.0)];
        // Mask hit-testing uses cached normalized polygons, without flattening.
        if self
            .image_segments
            .result
            .as_ref()
            .is_some_and(|r| r.host == host && vector_ink::point_in_polygon(&r.mask, point_norm))
        {
            return;
        }
        let basis = slate_doc::image_paint::image_content_rect(node, img);
        let reset = self.image_segments.hover.as_ref().is_none_or(|h| {
            h.host != host
                || h.screen.distance(screen) > MOVE_RESET_PX
                || ((h.point_norm[0] - point_norm[0]) * basis.w * xf.z)
                    .hypot((h.point_norm[1] - point_norm[1]) * basis.h * xf.z)
                    > MOVE_RESET_PX
        });
        if reset {
            self.image_segments.clear();
            self.image_segments.error = None;
            self.image_segments.hover = Some(Hover {
                tab: self.tab().id,
                host,
                gen: self.scene_gen,
                point_norm,
                screen,
                since: Instant::now(),
            });
        }
        let hover = self.image_segments.hover.as_ref().unwrap();
        if hover.since.elapsed() < LINGER {
            ui.ctx()
                .request_repaint_after(LINGER.saturating_sub(hover.since.elapsed()));
            return;
        }
        if self.image_segments.pending.is_some()
            || self.image_segments.result.is_some()
            || self.image_segments.error.is_some()
        {
            return;
        }
        if self
            .image_segments
            .worker
            .as_ref()
            .is_some_and(|tx| tx.is_full())
        {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
            return;
        }
        let Some(request) = self.segment_request(host, hover.point_norm) else {
            return;
        };
        if self.image_segments.worker.is_none() {
            self.image_segments.start_worker(ui.ctx().clone());
        }
        let serial = self.image_segments.serial;
        if self
            .image_segments
            .worker
            .as_ref()
            .unwrap()
            .try_send((serial, request))
            .is_ok()
        {
            self.image_segments.pending = Some(serial);
        } else {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
    }

    fn segment_request(&mut self, host: NodeId, point: [f32; 2]) -> Option<Request> {
        let item = self.image_item(host)?;
        let key = if item.is_none() {
            super::preview::linked_image_key(&self.agent_shown_path(host)?, "shown")
        } else {
            self.resolved_item_preview(item)?.0
        };
        let host_node = self.doc().scene.node(host)?;
        let NodeKind::Image(img) = &host_node.kind else {
            return None;
        };
        let mirror = img.mirror();
        let window = slate_doc::image_paint::visible_window_norm(host_node, img);
        let pixels = self
            .preview_cache
            .get(&key)
            .map(|e| &e.pixels)
            .or_else(|| self.thumb_pixels.get(&key))?;
        let [w, h] = pixels.size;
        if w < 2 || h < 2 {
            return None;
        }
        let scale = (SAMPLE_EDGE as f32 / w.max(h) as f32).min(1.0);
        let width = (w as f32 * scale).round().max(2.0) as usize;
        let height = (h as f32 * scale).round().max(2.0) as usize;
        let mut rgb = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                let sx = x * w / width;
                let sy = y * h / height;
                let sx = if mirror.x { w - 1 - sx } else { sx };
                let sy = if mirror.y { h - 1 - sy } else { sy };
                let color = pixels.pixels[sy * w + sx].to_srgba_unmultiplied();
                rgb.extend_from_slice(&color[..3]);
            }
        }
        Some(Request {
            key: format!("{key}:{}:{}", mirror.x, mirror.y),
            width,
            height,
            rgb,
            point,
            window,
        })
    }

    pub(crate) fn paint_image_segment_hover(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
    ) {
        let Some(hover) = self.image_segments.hover.as_ref() else {
            return;
        };
        if hover.gen != self.scene_gen || hover.tab != self.tab().id {
            return;
        }
        let host_id = hover.host;
        let point = hover.point_norm;
        let Some(host) = self.doc().scene.node(host_id) else {
            return;
        };
        let NodeKind::Image(img) = &host.kind else {
            return;
        };
        let basis = slate_doc::image_paint::image_content_rect(host, img);
        let world = slate_doc::geom::orbit_point(
            host.rect.center(),
            (basis.x + point[0] * basis.w, basis.y + point[1] * basis.h),
            host.rotation_deg,
        );
        let screen = xf.w2s(Pos2::new(world.0, world.1));
        let host_bounds = xf.rect_w2s(slate_doc::node_aabb(host));
        let result = self.image_segments.result.clone();
        if let Some(result) = &result {
            let region = layer_node_to_world(host, img, &result.local);
            if let NodeKind::Shape(shape) = &region.kind {
                if let Some(path) = &shape.path {
                    super::board_path::paint_path_shape(
                        self,
                        painter,
                        xf,
                        &region,
                        shape,
                        path,
                        &|c| c,
                    );
                }
            }
        }
        if result.is_none()
            && self.image_segments.pending.is_none()
            && self.image_segments.error.is_none()
        {
            return;
        }
        let anchor = Rect::from_center_size(screen, egui::Vec2::splat(12.0 * xf.z));
        let size = egui::vec2(selection_tools::STACK_WIDTH, selection_tools::CORNER_HEIGHT) * xf.z;
        let rect =
            selection_tools::place_popup(size, anchor, host_bounds, 10.0 * xf.z, ui.clip_rect());
        self.image_segments.action_rect = Some(rect);
        self.image_segments.corridor = Some(anchor.union(rect).expand(6.0 * xf.z));
        self.shape_properties.chrome_hits.push(rect);
        let label = if result.is_some() {
            "Highlight"
        } else if self.image_segments.error.is_some() {
            "Highlight unavailable"
        } else {
            "Finding object..."
        };
        let response = selection_tools::capsule(
            ui,
            Id::new(("image_highlight", host_id.0)),
            rect,
            &selection_tools::Capsule {
                label,
                disabled: result.is_none(),
                ..Default::default()
            },
            xf.z,
            self.palette(),
        );
        if let Some(error) = &self.image_segments.error {
            response.response.clone().on_hover_text(error);
        }
        if response.response.clicked() {
            self.commit_hover_segment(None);
        }
    }

    pub(crate) fn commit_hover_segment(&mut self, prompt: Option<&str>) -> Option<NodeId> {
        let result = self.image_segments.result.clone()?;
        if result.gen != self.scene_gen || self.image_segments.hover.as_ref()?.tab != self.tab().id
        {
            self.image_segments.clear();
            return None;
        }
        let id = self.commit_image_segment(result.host, result.local, prompt)?;
        self.image_segments.clear();
        Some(id)
    }

    pub(crate) fn commit_image_segment(
        &mut self,
        image: NodeId,
        mut local: Node,
        prompt: Option<&str>,
    ) -> Option<NodeId> {
        let before = self.doc().scene.node(image)?.clone();
        let NodeKind::Image(img) = &before.kind else {
            return None;
        };
        local.id = self
            .doc_mut()
            .scene
            .build_node(local.rect, local.kind.clone())
            .id;
        let id = local.id;
        let mut after = before.clone();
        let NodeKind::Image(after_img) = &mut after.kind else {
            return None;
        };
        after_img
            .paint_layers
            .push(PaintLayer::new(next_layer_id(img)));
        let layer = after_img.paint_layers.last_mut()?;
        layer.nodes.push(local);
        if let Some(prompt) = prompt.map(str::trim).filter(|p| !p.is_empty()) {
            layer.prompts.push(PaintLayerPrompt {
                node: id,
                prompt: prompt.to_string(),
            });
        }
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return None;
        }
        self.push_history(
            atlas_commands::CommandId("board.image.segment.commit"),
            Some(if prompt.is_some() { "agent" } else { "layer" }.into()),
        );
        Some(id)
    }

    pub(crate) fn append_image_region_prompts(
        &self,
        node: NodeId,
        slot: atlas_agent::InputSlot,
        hasher: &mut impl Hasher,
        prompts: &mut Vec<String>,
    ) {
        if matches!(
            slot,
            atlas_agent::InputSlot::Media | atlas_agent::InputSlot::View
        ) {
            for prompt in self.image_region_prompts(node) {
                prompt.hash(hasher);
                prompts.push(format!("Highlighted region on image: {prompt}"));
            }
        }
    }

    pub(crate) fn image_region_prompts(&self, image: NodeId) -> Vec<String> {
        match self.doc().scene.node(image).map(|n| &n.kind) {
            Some(NodeKind::Image(img)) => slate_doc::image_paint::region_prompts(img),
            _ => Vec::new(),
        }
    }
}

fn next_layer_id(img: &slate_doc::scene::ImageNode) -> slate_doc::image_paint::PaintLayerId {
    slate_doc::image_paint::PaintLayerId(
        img.paint_layers
            .iter()
            .map(|l| l.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1),
    )
}

fn region_node(path: slate_doc::scene::PathData) -> Node {
    Node {
        id: NodeId(u64::MAX),
        rect: WorldRect::new(0.0, 0.0, 1.0, 1.0),
        rotation_deg: 0.0,
        opacity: 1.0,
        locked: false,
        hidden: false,
        group: None,
        clip: None,
        bumper: None,
        kind: NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: Some(HIGHLIGHT),
            stroke: Stroke {
                width: 0.0,
                ..Default::default()
            },
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(std::sync::Arc::new(path)),
            text: None,
        }),
    }
}
