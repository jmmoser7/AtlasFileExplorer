//! Paint layers ("trace paper") owned by [`crate::scene::ImageNode`].
//!
//! Child nodes store geometry in normalized coordinates relative to the host
//! **content** rect (see [`crate::geom::image_content_rect`]), so move,
//! scale, rotate, and re-crop of the host carries ink.

use crate::geom::{child_norm_rect_to_world, child_world_rect_to_norm};
use crate::scene::{ImageNode, Node, NodeId, NodeKind, WorldRect};
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
    /// Agent instructions pinned to specific layer children.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<PaintLayerPrompt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintLayerPrompt {
    pub node: NodeId,
    pub prompt: String,
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
            prompts: Vec::new(),
            nodes: Vec::new(),
        }
    }
}

/// The world rect the full uncropped image occupies (crop window + UV crop).
pub fn image_content_rect(host: &Node, img: &ImageNode) -> WorldRect {
    crate::geom::image_content_rect(host.rect, img.crop)
}

fn layer_basis(host: &Node, img: &ImageNode) -> (WorldRect, f32, (f32, f32)) {
    let basis = image_content_rect(host, img);
    let pivot = host.rect.center();
    (basis, host.rotation_deg, pivot)
}

/// World point → normalized coords in the host content rect.
pub fn world_to_host_norm(host: &Node, img: &ImageNode, wx: f32, wy: f32) -> (f32, f32) {
    let basis = image_content_rect(host, img);
    let pivot = host.rect.center();
    let (lx, ly) = crate::geom::world_to_local_about(wx, wy, pivot.0, pivot.1, host.rotation_deg);
    (
        (lx - basis.x) / basis.w.max(1e-6),
        (ly - basis.y) / basis.h.max(1e-6),
    )
}

/// Map a layer-local node into world space for painting and hit-testing.
pub fn layer_node_to_world(host: &Node, img: &ImageNode, local: &Node) -> Node {
    let (basis, rot, pivot) = layer_basis(host, img);
    let mut out = local.clone();
    let (rect, rotation_deg) =
        child_norm_rect_to_world(basis, rot, pivot, local.rect, local.rotation_deg);
    out.rect = rect;
    out.rotation_deg = rotation_deg;
    out
}

/// Map a layer-local node into an already-rotated host's local coordinates.
///
/// Renderers that place children inside the host must not orbit their centers
/// again: the host transform supplies that rotation. The returned origin is
/// relative to `host.rect`, and the child keeps only its own rotation.
pub fn layer_node_to_host_local(host: &Node, img: &ImageNode, local: &Node) -> Node {
    let basis = image_content_rect(host, img);
    let mut out = local.clone();
    out.rect = WorldRect::new(
        basis.x - host.rect.x + local.rect.x * basis.w,
        basis.y - host.rect.y + local.rect.y * basis.h,
        local.rect.w * basis.w,
        local.rect.h * basis.h,
    );
    out.rotation_deg = local.rotation_deg;
    out
}

/// Map a world-space authored node into host-normalized storage.
pub fn layer_node_from_world(host: &Node, img: &ImageNode, world: &Node) -> Node {
    let (basis, rot, pivot) = layer_basis(host, img);
    let mut local = world.clone();
    let (rect, rotation_deg) =
        child_world_rect_to_norm(basis, rot, pivot, world.rect, world.rotation_deg);
    local.rect = rect;
    local.rotation_deg = rotation_deg;
    local
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

/// Fresh [`NodeId`]s for every child on duplicated or pasted images.
pub fn fresh_layer_node_ids(img: &mut ImageNode, mut next_id: impl FnMut() -> NodeId) {
    for layer in &mut img.paint_layers {
        for child in &mut layer.nodes {
            child.id = next_id();
        }
    }
}

/// Drop layer children whose kinds are not allowed (load / paste hygiene).
pub fn sanitize_paint_layers(scene: &mut crate::scene::Scene) {
    for host in &mut scene.nodes {
        let NodeKind::Image(img) = &mut host.kind else {
            continue;
        };
        for layer in &mut img.paint_layers {
            layer.nodes.retain(|n| layer_node_kind_allowed(&n.kind));
        }
    }
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

/// Only visible, still-present paint regions contribute instructions.
pub fn region_prompts(img: &ImageNode) -> Vec<String> {
    img.paint_layers
        .iter()
        .filter(|layer| layer.visible)
        .flat_map(|layer| {
            layer
                .prompts
                .iter()
                .filter(move |p| layer.nodes.iter().any(|n| n.id == p.node))
        })
        .map(|p| p.prompt.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// A segmentation mask becomes ordinary SVG-compatible, even-odd paint geometry.
/// Coordinates cover the image's normalized content, including holes.
pub fn region_path(contours: Vec<Vec<[f32; 2]>>) -> Option<crate::scene::PathData> {
    use crate::scene::{PathContour, PathData, PathFillRule, PathSeg};
    let mut contours = contours.into_iter().filter(|points| points.len() >= 3);
    let first = contours.next()?;
    let contour = |points: Vec<[f32; 2]>| PathContour {
        start: points[0],
        segs: points[1..].iter().map(|&to| PathSeg::Line { to }).collect(),
        closed: true,
    };
    let first = contour(first);
    Some(PathData {
        start: first.start,
        segs: first.segs,
        closed: true,
        extra: contours.map(contour).collect(),
        fill_rule: PathFillRule::EvenOdd,
        ..Default::default()
    })
}

/// The visible crop/fillet/trim window in content coordinates, for image masks.
pub fn visible_window_norm(host: &Node, img: &ImageNode) -> Vec<Vec<[f32; 2]>> {
    let contours = if host.clip.is_some() {
        crate::geom::node_closed_poly(host, 0.25).unwrap_or_default()
    } else {
        vec![img
            .corner
            .outline(host.rect, 0.25)
            .into_iter()
            .map(|p| host.rect.rotate_point(p, host.rotation_deg))
            .collect()]
    };
    contours
        .into_iter()
        .map(|contour| {
            contour
                .into_iter()
                .map(|p| {
                    let (x, y) = world_to_host_norm(host, img, p[0], p[1]);
                    [x, y]
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Corner, Crop, NodeKind, ShapeKind, ShapeNode, Stroke};

    fn path_shape() -> ShapeNode {
        ShapeNode {
            shape: ShapeKind::Path,
            sides: 6,
            phase_deg: 0.0,
            fill: None,
            stroke: Stroke::default(),
            corner: Corner::Square,
            flip: false,
            path: None,
            text: None,
        }
    }

    fn host_with_crop(crop: Crop) -> Node {
        let mut scene = crate::scene::Scene::default();
        scene.build_node(
            WorldRect::new(10.0, 20.0, 100.0, 50.0),
            NodeKind::Image(ImageNode {
                item: crate::ItemId(1),
                crop,
                ..ImageNode::new(crate::ItemId(1))
            }),
        )
    }

    fn stroke_local(u: f32, v: f32) -> Node {
        let mut scene = crate::scene::Scene::default();
        scene.build_node(
            WorldRect::new(u, v, 0.2, 0.1),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                sides: 6,
                phase_deg: 0.0,
                fill: None,
                stroke: Stroke::default(),
                corner: Corner::Square,
                flip: false,
                path: None,
                text: None,
            }),
        )
    }

    #[test]
    fn layer_norm_is_content_not_crop_window() {
        let crop = Crop {
            x: 0.25,
            y: 0.25,
            w: 0.5,
            h: 0.5,
        };
        let host = host_with_crop(crop);
        let img = match &host.kind {
            NodeKind::Image(i) => i,
            _ => unreachable!(),
        };
        let (cx, cy) = host.rect.center();
        let (u, v) = world_to_host_norm(&host, img, cx, cy);
        assert!((u - 0.5).abs() < 1e-3 && (v - 0.5).abs() < 1e-3);
        let mut local = stroke_local(0.0, 0.0);
        local.rect = WorldRect::new(0.4, 0.4, 0.2, 0.2);
        let world = layer_node_to_world(&host, img, &local);
        let content = image_content_rect(&host, img);
        let (ccx, ccy) = content.center();
        let (wx, wy) = world.rect.center();
        assert!((wx - ccx).abs() < 1e-2 && (wy - ccy).abs() < 1e-2);
    }

    #[test]
    fn layer_coords_round_trip_unrotated() {
        let host = host_with_crop(Crop::full());
        let img = match &host.kind {
            NodeKind::Image(i) => i,
            _ => unreachable!(),
        };
        let local = stroke_local(0.2, 0.2);
        let back = layer_node_to_world(&host, img, &local);
        let again = layer_node_from_world(&host, img, &back);
        assert!((again.rect.x - local.rect.x).abs() < 1e-4);
        assert!((again.rect.y - local.rect.y).abs() < 1e-4);
    }

    #[test]
    fn layer_coords_survive_host_rotation() {
        for deg in [37.0, 90.0, 180.0] {
            let mut host = host_with_crop(Crop::full());
            host.rotation_deg = deg;
            let img = match &host.kind {
                NodeKind::Image(i) => i.clone(),
                _ => unreachable!(),
            };
            let local = stroke_local(0.3, 0.4);
            let world = layer_node_to_world(&host, &img, &local);
            let back = layer_node_from_world(&host, &img, &world);
            assert!((back.rect.x - local.rect.x).abs() < 1e-3, "deg={deg} x");
            assert!((back.rect.y - local.rect.y).abs() < 1e-3, "deg={deg} y");
            assert!(
                (back.rotation_deg - local.rotation_deg).abs() < 1e-3,
                "deg={deg} rot"
            );
        }
    }

    #[test]
    fn host_local_mapping_does_not_orbit_with_rotated_host() {
        let mut host = host_with_crop(Crop {
            x: 0.25,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        });
        host.rotation_deg = 90.0;
        let img = match &host.kind {
            NodeKind::Image(i) => i,
            _ => unreachable!(),
        };
        let local = stroke_local(0.0, 0.0);
        let mapped = layer_node_to_host_local(&host, img, &local);
        assert!((mapped.rect.x + 50.0).abs() < 1e-3);
        assert!((mapped.rect.y - 0.0).abs() < 1e-3);
        assert!((mapped.rect.w - 40.0).abs() < 1e-3);
        assert_eq!(mapped.rotation_deg, local.rotation_deg);
    }

    #[test]
    fn rotate_after_draw_round_trip() {
        let mut host = host_with_crop(Crop::full());
        let img = match &host.kind {
            NodeKind::Image(i) => i.clone(),
            _ => unreachable!(),
        };
        let local = stroke_local(0.25, 0.35);
        let world_before = layer_node_to_world(&host, &img, &local);
        host.rotation_deg = 45.0;
        let world_after = layer_node_to_world(&host, &img, &local);
        assert!(world_before.rect.x != world_after.rect.x);
        let back = layer_node_from_world(&host, &img, &world_after);
        assert!((back.rect.x - local.rect.x).abs() < 1e-3);
    }

    #[test]
    fn old_image_without_layers_field_defaults_empty() {
        let json = r#"{"item":1,"crop":{"x":0,"y":0,"w":1,"h":1}}"#;
        let img: ImageNode = serde_json::from_str(json).unwrap();
        assert!(img.paint_layers.is_empty());
    }

    #[test]
    fn layer_node_kind_allows_drawing_tools_and_layer_images() {
        use crate::scene::{FrameNode, Rgba, TextNode};
        assert!(layer_node_kind_allowed(&NodeKind::Text(TextNode {
            text: String::new(),
            family: Default::default(),
            size: 14.0,
            color: Rgba([0, 0, 0, 255]),
            align: Default::default(),
            fill: None,
            stroke: Default::default(),
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
            NodeKind::Image(ImageNode::new(crate::ItemId(1))),
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
