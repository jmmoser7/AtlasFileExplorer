//! Image paint layers (trace paper). Contract: `image-paint-layers.md`.

use super::{board::BoardTool, board::BoardXf, SlateApp};
use eframe::egui::{self, Pos2};
use slate_doc::image_paint::layer_node_to_world;
use slate_doc::{
    image_paint::{
        layer_node_from_world, layer_node_kind_allowed, world_to_host_norm, PaintLayerId,
    },
    scene::{ImageNode, Node, NodeKind, SceneCmd},
    NodeId, PaintLayer,
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
            | BoardTool::Sticky
    )
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
        if img.agent.is_some() {
            return false;
        }
        let Some(item) = app.doc().item(img.item) else {
            return false;
        };
        slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Image
    }

    pub fn sync_image_paint_for_tool(&mut self) {
        if !tool_hosts_on_image(self.board_tool) {
            return;
        }
        if self.board_sel.len() != 1 {
            return;
        }
        let id = *self.board_sel.iter().next().unwrap();
        if !Self::supports_image_paint(id, self) {
            return;
        }
        if self.image_paint.as_ref().map(|s| s.image) != Some(id) {
            self.ensure_paint_layer(id);
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
        let NodeKind::Image(img) = &node.kind else {
            return false;
        };
        let (u, v) = world_to_host_norm(node, world.x, world.y);
        let c = img.crop.clamped();
        (c.x..=c.x + c.w).contains(&u) && (c.y..=c.y + c.h).contains(&v)
    }

    /// When hosting, route scene Adds into the active paint layer instead.
    pub fn try_commit_layer_nodes(&mut self, nodes: Vec<Node>) -> bool {
        let Some(session) = self.image_paint.clone() else {
            return false;
        };
        if nodes.is_empty() {
            return false;
        }
        if !nodes.iter().all(|n| layer_node_kind_allowed(&n.kind)) {
            return false;
        }
        let Some(host) = self.doc().scene.node(session.image).cloned() else {
            return false;
        };
        if !nodes
            .iter()
            .all(|n| self.stroke_intersects_image_window(&host, n))
        {
            return false;
        }
        let locals: Vec<Node> = nodes
            .iter()
            .map(|n| layer_node_from_world(&host, n))
            .collect();
        let Some(before) = self.doc().scene.node(session.image).cloned() else {
            return false;
        };
        let mut after = before.clone();
        let NodeKind::Image(ref mut img) = after.kind else {
            return false;
        };
        let Some(layer) = img.paint_layers.get_mut(session.layer_index) else {
            return false;
        };
        layer.nodes.extend(locals);
        if !self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            return false;
        }
        true
    }

    fn stroke_intersects_image_window(&self, host: &Node, node: &Node) -> bool {
        let NodeKind::Image(img) = &host.kind else {
            return false;
        };
        let c = img.crop.clamped();
        let corners = [
            (node.rect.x, node.rect.y),
            (node.rect.x + node.rect.w, node.rect.y),
            (node.rect.x + node.rect.w, node.rect.y + node.rect.h),
            (node.rect.x, node.rect.y + node.rect.h),
        ];
        corners.iter().any(|(x, y)| {
            let (u, v) = world_to_host_norm(host, *x, *y);
            (c.x..=c.x + c.w).contains(&u) && (c.y..=c.y + c.h).contains(&v)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint_image_paint_layers(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        host: &Node,
        img: &ImageNode,
        _outline: &[Pos2],
        srect: egui::Rect,
        host_alpha: f32,
        _z: f32,
    ) {
        if img.paint_layers.is_empty() {
            return;
        }
        let clip = srect.intersect(painter.clip_rect());
        let sub = painter.with_clip_rect(clip);
        for layer in &img.paint_layers {
            if !layer.visible {
                continue;
            }
            for local in &layer.nodes {
                let mut world = layer_node_to_world(host, local);
                world.opacity = (world.opacity * layer.opacity * host_alpha).clamp(0.0, 1.0);
                self.paint_board_node(ui, &sub, xf, &world, false);
            }
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
}
