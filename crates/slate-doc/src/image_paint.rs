//! Paint layers ("trace paper") owned by [`crate::scene::ImageNode`].
//!
//! Child nodes store geometry in normalized coordinates relative to the host
//! **content** rect (see [`crate::geom::image_content_rect`]), so move,
//! scale, rotate, and re-crop of the host carries ink.

use crate::geom::{child_norm_rect_to_world, child_world_rect_to_norm};
use crate::scene::{ImageNode, Node, NodeId, NodeKind, WorldRect};
use serde::{Deserialize, Serialize};

mod results;
pub use results::*;

/// Stable id for a paint layer on one image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerNodeRef {
    pub image: NodeId,
    /// `None` identifies the pre-album legacy stack.
    pub result: Option<ImageResultId>,
    pub layer_index: usize,
    pub node_index: usize,
}

/// Drop layer children whose kinds are not allowed (load / paste hygiene).
pub fn sanitize_paint_layers(scene: &mut crate::scene::Scene) {
    for host in &mut scene.nodes {
        let NodeKind::Image(img) = &mut host.kind else {
            continue;
        };
        for layer in img.paint_layers.iter_mut().chain(
            img.result_layers
                .iter_mut()
                .flat_map(|stack| &mut stack.layers),
        ) {
            layer.nodes.retain(|n| layer_node_kind_allowed(&n.kind));
        }
        for stack in &mut img.result_layers {
            stack.normalize_metadata();
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
                        result: None,
                        layer_index,
                        node_index,
                    });
                }
            }
        }
        for stack in &img.result_layers {
            for (layer_index, layer) in stack.layers.iter().enumerate() {
                for (node_index, child) in layer.nodes.iter().enumerate() {
                    if child.id == id {
                        return Some(LayerNodeRef {
                            image: host.id,
                            result: Some(stack.result.clone()),
                            layer_index,
                            node_index,
                        });
                    }
                }
            }
        }
    }
    None
}

/// Only visible, still-present paint regions contribute instructions.
pub fn region_prompts(img: &ImageNode) -> Vec<String> {
    region_prompts_from_layers(&img.paint_layers)
}

pub fn region_prompts_from_layers(layers: &[PaintLayer]) -> Vec<String> {
    layers
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

/// Instructions attached to the displayed result only, with legacy ordinary
/// image layers resolved through `layers_for_result`.
pub fn region_prompts_for_result(img: &ImageNode, result: &ImageResultId) -> Vec<String> {
    layers_for_result(img, result)
        .iter()
        .filter(|layer| layer.visible)
        .flat_map(|layer| {
            layer
                .prompts
                .iter()
                .filter(move |prompt| layer.nodes.iter().any(|node| node.id == prompt.node))
        })
        .map(|prompt| prompt.prompt.trim().to_string())
        .filter(|prompt| !prompt.is_empty())
        .collect()
}

/// Convert a host-image-normalized selection into a child layer node's
/// normalized `Node.clip` coordinates. The child can have its own rotation;
/// the mask is inverse-rotated before being mapped through its rect.
pub fn clip_path_for_layer_node(
    child: &Node,
    mask_in_host_norm: &[Vec<[f32; 2]>],
) -> Option<crate::scene::PathData> {
    let rect = child.rect;
    if rect.w.abs() <= 1e-6 || rect.h.abs() <= 1e-6 {
        return None;
    }
    let (cx, cy) = rect.center();
    let (sin, cos) = child.rotation_deg.to_radians().sin_cos();
    let contours = mask_in_host_norm
        .iter()
        .map(|contour| {
            contour
                .iter()
                .map(|point| {
                    let dx = point[0] - cx;
                    let dy = point[1] - cy;
                    let local_x = cx + dx * cos + dy * sin;
                    let local_y = cy - dx * sin + dy * cos;
                    [(local_x - rect.x) / rect.w, (local_y - rect.y) / rect.h]
                })
                .collect()
        })
        .collect();
    region_path(contours)
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
mod tests;
