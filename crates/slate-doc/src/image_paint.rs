//! Paint layers ("trace paper") owned by [`crate::scene::ImageNode`].
//!
//! Child nodes store geometry in normalized coordinates relative to the host
//! image rect (0..1 on each axis), so move/scale/rotate of the host carries ink.

use crate::scene::{Node, NodeId, NodeKind, WorldRect};
use serde::{Deserialize, Serialize};

/// Stable id for a paint layer on one image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaintLayerId(pub u64);

impl PaintLayerId {
    pub const NONE: PaintLayerId = PaintLayerId(0);
}

/// One transparent sheet above the filtered base image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintLayer {
    pub id: PaintLayerId,
    #[serde(default = "default_layer_opacity")]
    pub opacity: f32,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<Node>,
}

fn default_layer_opacity() -> f32 {
    1.0
}

fn default_true() -> bool {
    true
}

impl PaintLayer {
    pub fn new(id: PaintLayerId) -> Self {
        Self {
            id,
            opacity: 1.0,
            visible: true,
            nodes: Vec::new(),
        }
    }
}

/// World point → normalized coords in the host's unrotated rect.
pub fn world_to_host_norm(host: &Node, wx: f32, wy: f32) -> (f32, f32) {
    let r = host.rect;
    let (cx, cy) = r.center();
    let (lx, ly) = to_local(wx, wy, cx, cy, host.rotation_deg);
    let u = ((lx - r.x) / r.w.max(1e-6)).clamp(-0.5, 1.5);
    let v = ((ly - r.y) / r.h.max(1e-6)).clamp(-0.5, 1.5);
    (u, v)
}

/// Normalized host rect → axis-aligned world rect (pre-rotation AABB).
pub fn host_norm_rect_to_world(host: &Node, norm: WorldRect) -> WorldRect {
    let r = host.rect;
    WorldRect::new(
        r.x + norm.x * r.w,
        r.y + norm.y * r.h,
        norm.w * r.w,
        norm.h * r.h,
    )
}

/// Map a layer-local node into world space for painting and hit-testing.
pub fn layer_node_to_world(host: &Node, local: &Node) -> Node {
    let mut out = local.clone();
    out.rect = host_norm_rect_to_world(host, local.rect);
    out.rotation_deg += host.rotation_deg;
    out
}

/// Map a world-space authored node into host-normalized storage.
pub fn layer_node_from_world(host: &Node, world: &Node) -> Node {
    let mut local = world.clone();
    let r = host.rect;
    let nw = world.rect;
    local.rect = WorldRect::new(
        (nw.x - r.x) / r.w.max(1e-6),
        (nw.y - r.y) / r.h.max(1e-6),
        nw.w / r.w.max(1e-6),
        nw.h / r.h.max(1e-6),
    );
    local.rotation_deg = world.rotation_deg - host.rotation_deg;
    local
}

/// True when `norm` center lies inside the unit host square (visible window test
/// is applied separately via crop in the app).
pub fn norm_point_inside_host(u: f32, v: f32) -> bool {
    (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)
}

/// Child kinds allowed on a paint layer (drawing tools + dropped images).
pub fn layer_node_kind_allowed(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Shape(_) | NodeKind::Text(_) | NodeKind::Image(_)
    )
}

/// Where a layer-owned node lives in the scene graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerNodeRef {
    pub image: NodeId,
    pub layer_index: usize,
    pub node_index: usize,
}

/// Find a paint-layer child by its stable [`NodeId`].
pub fn find_layer_node(scene: &crate::scene::Scene, id: NodeId) -> Option<LayerNodeRef> {
    for host in &scene.nodes {
        let NodeKind::Image(img) = &host.kind else {
            continue;
        };
        for (layer_index, layer) in img.paint_layers.iter().enumerate() {
            for (node_index, child) in layer.nodes.iter().enumerate() {
                if child.id == id {
                    return Some(LayerNodeRef {
                        image: host.id,
                        layer_index,
                        node_index,
                    });
                }
            }
        }
    }
    None
}

fn to_local(px: f32, py: f32, cx: f32, cy: f32, rotation_deg: f32) -> (f32, f32) {
    if rotation_deg.abs() < f32::EPSILON {
        return (px, py);
    }
    let rad = (-rotation_deg).to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = px - cx;
    let dy = py - cy;
    (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Corner, NodeId, NodeKind, ShapeKind, ShapeNode, Stroke};

    fn path_shape() -> ShapeNode {
        ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: Stroke::default(),
            corner: Corner::Square,
            flip: false,
            path: None,
            text: None,
        }
    }

    fn host_node() -> Node {
        let mut scene = crate::scene::Scene::default();
        scene.build_node(
            WorldRect::new(10.0, 20.0, 100.0, 50.0),
            NodeKind::Image(crate::scene::ImageNode::new(crate::ItemId(1))),
        )
    }

    #[test]
    fn layer_coords_round_trip_unrotated() {
        let host = host_node();
        let mut world = host.clone();
        world.id = NodeId(99);
        world.rect = WorldRect::new(30.0, 30.0, 40.0, 10.0);
        world.kind = NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: None,
            stroke: Stroke::default(),
            corner: Corner::Square,
            flip: false,
            path: None,
            text: None,
        });
        let local = layer_node_from_world(&host, &world);
        assert!((local.rect.x - 0.2).abs() < 1e-4);
        let back = layer_node_to_world(&host, &local);
        assert!((back.rect.x - 30.0).abs() < 1e-3);
    }

    #[test]
    fn old_image_without_layers_field_defaults_empty() {
        let json = r#"{"item":1,"crop":{"x":0,"y":0,"w":1,"h":1}}"#;
        let img: crate::scene::ImageNode = serde_json::from_str(json).unwrap();
        assert!(img.paint_layers.is_empty());
    }

    #[test]
    fn layer_node_kind_allows_drawing_tools_and_layer_images() {
        use crate::scene::{FrameNode, ImageNode, Rgba, TextNode};
        assert!(layer_node_kind_allowed(&NodeKind::Text(TextNode {
            text: String::new(),
            family: Default::default(),
            size: 14.0,
            color: Rgba([0, 0, 0, 255]),
            align: Default::default(),
            fill: None,
            agent: None,
        })));
        for shape in [
            ShapeKind::Path,
            ShapeKind::Line,
            ShapeKind::Rect,
            ShapeKind::Ellipse,
        ] {
            let mut s = path_shape();
            s.shape = shape;
            assert!(layer_node_kind_allowed(&NodeKind::Shape(s)));
        }
        assert!(layer_node_kind_allowed(&NodeKind::Image(ImageNode::new(
            crate::ItemId(1)
        ))));
        assert!(!layer_node_kind_allowed(&NodeKind::Frame(FrameNode {
            title: String::new(),
            order: 0,
            fill: Rgba([255, 255, 255, 255]),
            fill_authored: false,
            assignments: Default::default(),
            stroke: Stroke::none(),
            corner: Corner::Square,
        })));
    }

    #[test]
    fn find_layer_node_locates_nested_stroke() {
        let mut scene = crate::scene::Scene::default();
        let host = scene.build_node(
            WorldRect::new(10.0, 20.0, 100.0, 50.0),
            NodeKind::Image(crate::scene::ImageNode::new(crate::ItemId(1))),
        );
        let host_id = host.id;
        scene.apply(&crate::scene::SceneCmd::Add {
            index: 0,
            node: host,
        });
        let local = scene.build_node(
            WorldRect::new(0.1, 0.1, 0.2, 0.2),
            NodeKind::Shape(path_shape()),
        );
        let stroke_id = local.id;
        let host = scene.node_mut(host_id).unwrap();
        let NodeKind::Image(ref mut img) = host.kind else {
            unreachable!()
        };
        img.paint_layers.push(PaintLayer::new(PaintLayerId(1)));
        img.paint_layers[0].nodes.push(local);
        let loc = find_layer_node(&scene, stroke_id).unwrap();
        assert_eq!(loc.image, host_id);
        assert_eq!(loc.layer_index, 0);
        assert_eq!(loc.node_index, 0);
    }
}
