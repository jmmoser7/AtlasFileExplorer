//! Local image subject highlights. Durable highlights are ordinary paint paths.

use super::{board::BoardXf, SlateApp};
use eframe::egui::{self, Pos2};
use runtime::PendingImage;
use slate_doc::{
    image_paint::{layer_node_to_world, PaintLayer, PaintLayerPrompt},
    scene::{Node, NodeKind, Rgba, SceneCmd, ShapeKind, ShapeNode, Stroke, WorldRect},
    NodeId,
};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

const LINGER: Duration = Duration::from_millis(400);
const MOVE_RESET_PX: f32 = 8.0;
const HIGHLIGHT: Rgba = Rgba([255, 150, 40, 100]);

#[cfg(test)]
#[path = "image_segment_drag_tests.rs"]
mod drag_tests;
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

mod grow;
mod runtime;
mod sticker;
mod tag;
use runtime::Hover;
pub(crate) use runtime::ImageSegmentRuntime;

impl SlateApp {
    /// The silhouette replaces the ordinary hover outline while it shows.
    pub(crate) fn paint_hover_or_segment(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        select: egui::Color32,
    ) {
        if self.board_crop.is_none() && self.image_segments.result.is_none() {
            self.paint_hover_preview(painter, xf, select);
        }
        self.paint_image_segment_hover(ui, painter, xf);
    }

    /// Esc ends the dwell either way, but only a visible offer is a cancel
    /// layer; a bare dwell lets Esc fall through to the selection.
    pub(crate) fn dismiss_image_segment(&mut self) -> bool {
        let chrome =
            self.image_segments.layers_menu.is_some() || self.image_segments.tag_at.is_some();
        if chrome {
            self.image_segments.layers_menu = None;
            self.image_segments.layers_fresh = false;
            self.image_segments.layer_rects.clear();
            self.image_segments.tag_at = None;
            self.image_segments.tag_fresh = false;
            self.image_segments.tag_rects.clear();
        }
        let Some(hover) = self.image_segments.hover.as_ref() else {
            return chrome;
        };
        let offered = self.image_segments.offered();
        self.image_segments.dismissed = Some(hover.screen);
        self.image_segments.clear();
        offered
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
        if self.segment_drag_frame(ui, pointer, world) {
            return;
        }
        if self.image_segments.tag_at.is_some() {
            return;
        }
        if self
            .image_segments
            .hover
            .as_ref()
            .is_some_and(|h| h.gen != self.scene_gen || h.tab != self.tab().id)
        {
            self.image_segments.clear();
        }
        let Some(screen) = pointer else {
            self.image_segments.clear();
            return;
        };
        if !self.pack_ok("sam") && !self.image_segments.offered() {
            return;
        }
        let (true, Some(world)) = (hover_live, world) else {
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
            self.segment_grow_step(screen, xf.z);
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
        if self.image_segments.offered() {
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

    fn segment_request(&mut self, host: NodeId, point: [f32; 2]) -> Option<PendingImage> {
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
        Some(PendingImage {
            key: format!("{key}:{}:{}", mirror.x, mirror.y),
            pixels: pixels.clone(),
            mirror: [mirror.x, mirror.y],
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
        self.paint_layer_squircle(ui, xf);
        let Some(hover) = self.image_segments.hover.as_ref() else {
            return;
        };
        if hover.gen != self.scene_gen || hover.tab != self.tab().id {
            return;
        }
        let host_id = hover.host;
        let Some(host) = self.doc().scene.node(host_id) else {
            return;
        };
        let NodeKind::Image(img) = &host.kind else {
            return;
        };
        if let Some(result) = self.image_segments.result.clone() {
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
        self.paint_sticker_ghost(ui, painter, xf);
        self.paint_segment_tag(ui, xf);
    }

    pub(crate) fn commit_hover_segment(&mut self, prompt: Option<&str>) -> Option<NodeId> {
        let result = self.image_segments.result.clone()?;
        if result.gen != self.scene_gen || self.image_segments.hover.as_ref()?.tab != self.tab().id
        {
            self.image_segments.clear();
            return None;
        }
        let host = result.host;
        let id = self.commit_image_segment(host, result.local, prompt)?;
        self.image_segments.clear();
        self.image_segments.layers_menu = Some(host);
        self.image_segments.layers_fresh = true;
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
            .push(PaintLayer::new(Self::next_paint_layer_id(img)));
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
