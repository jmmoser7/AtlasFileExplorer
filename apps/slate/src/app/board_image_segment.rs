//! Local image subject highlights. Durable highlights are ordinary paint paths.

use super::{board::BoardXf, SlateApp};
use atlas_segment::PointPrompt;
use atlas_shell::{canvas_scale, selection_tools};
use eframe::egui::{self, Id, Pos2, Rect};
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
const LASSO_CLOSE_NORM: f32 = 0.015;
const LASSO_DOUBLE_CLICK: Duration = Duration::from_millis(350);
const LASSO_DOUBLE_CLICK_PX: f32 = 8.0;
const HIGHLIGHT: Rgba = Rgba([255, 150, 40, 100]);

#[cfg(test)]
#[path = "image_segment_tests.rs"]
mod tests;

#[derive(Clone)]
pub(crate) struct SegmentResult {
    pub host: NodeId,
    pub gen: u64,
    pub source_key: String,
    pub local: Node,
    mask: std::sync::Arc<Vec<Vec<[f32; 2]>>>,
    candidates: std::sync::Arc<Vec<atlas_segment::Candidate>>,
    selected: usize,
}

mod paint;
mod runtime;
mod selection;
pub(crate) use runtime::ImageSegmentRuntime;
use runtime::{Hover, LassoDraft, PinnedSelection, PromptMode};

impl SlateApp {
    /// The silhouette replaces the ordinary hover outline while it shows.
    pub(crate) fn paint_hover_or_segment(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        select: egui::Color32,
    ) {
        if self.board_crop.is_none()
            && self.image_segments.result.is_none()
            && self.image_segments.pinned.is_none()
            && self.image_segments.lasso.is_none()
        {
            self.paint_hover_preview(painter, xf, select);
        }
        self.paint_image_segment_hover(ui, painter, xf);
    }

    /// Esc ends the dwell either way, but only a visible offer is a cancel
    /// layer; a bare dwell lets Esc fall through to the selection.
    pub(crate) fn dismiss_image_segment(&mut self) -> bool {
        let active = self.image_segments.hover.is_some()
            || self.image_segments.pinned.is_some()
            || self.image_segments.lasso.is_some();
        if !active {
            return false;
        }
        let offered = self.image_segments.offered();
        let consumes_escape = offered
            || self.image_segments.pinned.is_some()
            || self.image_segments.lasso.is_some();
        self.image_segments.dismissed = self.image_segments.hover.as_ref().map(|hover| hover.screen);
        self.image_segments.pinned = None;
        self.image_segments.clear();
        consumes_escape
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
        let tab = self.tab().id;
        let pinned_stale = self
            .image_segments
            .pinned
            .as_ref()
            .is_some_and(|selection| {
                selection.tab != tab
                    || self.segment_source_key(selection.host).as_deref()
                        != Some(selection.source_key.as_str())
            });
        if pinned_stale {
            self.image_segments.pinned = None;
            self.image_segments.clear();
        }
        if self.image_segments.pinned.is_some() {
            // Refinement responses use this immutable request context. Keep it
            // across frames until the worker has delivered the pinned mask.
            if self.image_segments.pending.is_none() {
                self.image_segments.hover = None;
            }
            self.image_segments.result = None;
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
        // Preserve the offer across the gap to its capsule and over the capsule
        // itself, where the canvas is no longer the hovered widget. Its press is
        // chrome, never a drag or a click-through on the underlying image.
        if self.image_segments.offered()
            && self.board_tool == super::board::BoardTool::Select
            && self.board_drag.is_none()
            && self
                .image_segments
                .corridor
                .is_some_and(|r| r.contains(screen))
        {
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
                source_key: String::new(),
                point_norm,
                prompts: vec![PointPrompt {
                    point: point_norm,
                    positive: true,
                }],
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
        let Some(request) = self.segment_request(host, &hover.prompts) else {
            return;
        };
        if let Some(hover) = self.image_segments.hover.as_mut() {
            hover.source_key = request.source_key.clone();
        }
        if self.image_segments.worker.is_none() {
            self.image_segments.start_worker(ui.ctx().clone());
        }
        let serial = self.image_segments.serial.wrapping_add(1);
        self.image_segments.serial = serial;
        self.image_segments.submit(serial, request);
    }

    fn segment_pixel_key(&self, host: NodeId) -> Option<String> {
        let node = self.doc().scene.node(host)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        if img.item.is_none() {
            super::preview::linked_image_key(&self.agent_shown_path(host)?, "shown")
        } else {
            self.resolved_item_key(self.doc().item(img.item)?)
        }
    }

    /// Selection identity includes crop geometry. Pixel residency deliberately
    /// uses the unsuffixed preview key above, while inference identity includes
    /// mirroring because the worker mirrors pixels before encoding.
    fn segment_source_key(&self, host: NodeId) -> Option<String> {
        let node = self.doc().scene.node(host)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        let pixels = self.segment_pixel_key(host)?;
        let mirror = img.mirror();
        let crop = img.crop;
        Some(format!(
            "{pixels}:{}:{}:{:08x}:{:08x}:{:08x}:{:08x}",
            mirror.x,
            mirror.y,
            crop.x.to_bits(),
            crop.y.to_bits(),
            crop.w.to_bits(),
            crop.h.to_bits(),
        ))
    }

    fn segment_request(&mut self, host: NodeId, prompts: &[PointPrompt]) -> Option<PendingImage> {
        let source_key = self.segment_source_key(host)?;
        let pixels_key = self.segment_pixel_key(host)?;
        let host_node = self.doc().scene.node(host)?;
        let NodeKind::Image(img) = &host_node.kind else {
            return None;
        };
        let mirror = img.mirror();
        let model_key = format!("{pixels_key}:{}:{}", mirror.x, mirror.y);
        let window = slate_doc::image_paint::visible_window_norm(host_node, img);
        let pixels = self
            .preview_cache
            .get(&pixels_key)
            .map(|e| &e.pixels)
            .or_else(|| self.thumb_pixels.get(&pixels_key))?;
        Some(PendingImage {
            key: model_key,
            source_key,
            pixels: pixels.clone(),
            mirror: [mirror.x, mirror.y],
            prompts: prompts.to_vec(),
            window,
        })
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

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
