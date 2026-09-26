//! Mirroring authored content about its node's center.
//!
//! One owner for the resize crossing (P1.node.transform) and the Mirror
//! commands. A mirror rewrites what the node stores, so every interpreter
//! draws the result without knowing a mirror happened. The one exception is
//! a picture's pixels, which carry [`ImageNode::flip_x`] and
//! [`ImageNode::flip_y`]; its crop window and paint layers are rewritten in
//! displayed coordinates like everything else.

use crate::media::{media_kind, web_safe_video, MediaKind};
use crate::scene::{ImageNode, Node, NodeKind, PathData, PathSeg, ShapeKind, ShapeNode};
use crate::SlateDoc;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorAxis {
    /// Left to right.
    Horizontal,
    /// Top to bottom.
    Vertical,
}

/// Whether both interpreters can mirror this picture's pixels: raster
/// images, web-safe video, and agent pictures. Text, sheet, document, and
/// 3D cards are laid out or rendered, not sampled, so they cannot.
pub fn picture_mirrors(doc: &SlateDoc, img: &ImageNode) -> bool {
    if img.item.is_none() {
        return img.agent.is_some();
    }
    doc.item(img.item)
        .is_some_and(|item| match media_kind(&item.path) {
            MediaKind::Image => true,
            MediaKind::Video => web_safe_video(&item.path),
            _ => false,
        })
}

/// Whether [`mirror_on_board`] changes this node honestly: mirrorable
/// pictures, paths, and lines. Rectangles, ellipses, and text would be
/// unchanged or unreadable, and a regular polygon's vertex order is fixed.
pub fn node_mirrors(doc: &SlateDoc, node: &Node) -> bool {
    match &node.kind {
        NodeKind::Image(img) => picture_mirrors(doc, img),
        NodeKind::Shape(shape) => shape_mirrors(shape),
        _ => false,
    }
}

fn shape_mirrors(shape: &ShapeNode) -> bool {
    match shape.shape {
        ShapeKind::Line => true,
        ShapeKind::Path => shape.path.as_ref().is_some_and(|p| !p.is_empty()),
        _ => false,
    }
}

/// Mirror the node's content across its own local axis through its center.
/// The rect and rotation stay; this is what dragging an edge past its
/// opposite does. Returns false, changing nothing, for kinds that do not
/// mirror (pictures are not checked against their media here; callers gate
/// with [`node_mirrors`]).
pub fn mirror_local(node: &mut Node, axis: MirrorAxis) -> bool {
    let mirrored = match &mut node.kind {
        NodeKind::Image(img) => {
            mirror_picture(img, axis);
            true
        }
        NodeKind::Shape(shape) if shape_mirrors(shape) => {
            mirror_shape(shape, axis);
            true
        }
        _ => false,
    };
    if mirrored {
        if let Some(clip) = &mut node.clip {
            mirror_path(clip, axis);
        }
    }
    mirrored
}

/// Mirror across the board's horizontal or vertical axis through the node's
/// center: a local mirror plus the rotation turned the other way.
pub fn mirror_on_board(node: &mut Node, axis: MirrorAxis) -> bool {
    if !mirror_local(node, axis) {
        return false;
    }
    if node.rotation_deg != 0.0 {
        node.rotation_deg = -node.rotation_deg;
    }
    true
}

fn mirror_picture(img: &mut ImageNode, axis: MirrorAxis) {
    match axis {
        MirrorAxis::Horizontal => {
            img.flip_x = !img.flip_x;
            img.crop.x = 1.0 - img.crop.x - img.crop.w;
        }
        MirrorAxis::Vertical => {
            img.flip_y = !img.flip_y;
            img.crop.y = 1.0 - img.crop.y - img.crop.h;
        }
    }
    for layer in &mut img.paint_layers {
        for child in &mut layer.nodes {
            mirror_layer_child(child, axis);
        }
    }
}

/// A paint-layer child is stored normalized in the host content rect, so
/// its placement mirrors inside the unit square. Text keeps reading forward.
fn mirror_layer_child(child: &mut Node, axis: MirrorAxis) {
    match axis {
        MirrorAxis::Horizontal => child.rect.x = 1.0 - child.rect.x - child.rect.w,
        MirrorAxis::Vertical => child.rect.y = 1.0 - child.rect.y - child.rect.h,
    }
    if child.rotation_deg != 0.0 {
        child.rotation_deg = -child.rotation_deg;
    }
    match &mut child.kind {
        NodeKind::Image(img) => mirror_picture(img, axis),
        NodeKind::Shape(shape) if shape_mirrors(shape) => mirror_shape(shape, axis),
        _ => {}
    }
    if let Some(clip) = &mut child.clip {
        mirror_path(clip, axis);
    }
}

fn mirror_shape(shape: &mut ShapeNode, axis: MirrorAxis) {
    match shape.shape {
        ShapeKind::Line => shape.flip = !shape.flip,
        ShapeKind::Path => {
            if let Some(path) = &mut shape.path {
                mirror_path(Arc::make_mut(path), axis);
            }
        }
        _ => {}
    }
}

fn mirror_path(path: &mut PathData, axis: MirrorAxis) {
    mirror_point(&mut path.start, axis);
    mirror_segs(&mut path.segs, axis);
    for contour in &mut path.extra {
        mirror_point(&mut contour.start, axis);
        mirror_segs(&mut contour.segs, axis);
    }
    for mark in &mut path.erase {
        for point in &mut mark.points {
            mirror_point(point, axis);
        }
    }
}

fn mirror_segs(segs: &mut [PathSeg], axis: MirrorAxis) {
    for seg in segs {
        match seg {
            PathSeg::Line { to } => mirror_point(to, axis),
            PathSeg::Quad { ctrl, to } => {
                mirror_point(ctrl, axis);
                mirror_point(to, axis);
            }
            PathSeg::Cubic { c1, c2, to } => {
                mirror_point(c1, axis);
                mirror_point(c2, axis);
                mirror_point(to, axis);
            }
        }
    }
}

fn mirror_point(point: &mut [f32; 2], axis: MirrorAxis) {
    match axis {
        MirrorAxis::Horizontal => point[0] = 1.0 - point[0],
        MirrorAxis::Vertical => point[1] = 1.0 - point[1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_paint::{layer_node_to_world, PaintLayer, PaintLayerId};
    use crate::scene::{
        Corner, Crop, ImageNode, Mirror, Node, NodeKind, PathData, PathSeg, Scene, ShapeKind,
        ShapeNode, Stroke, WorldRect,
    };
    use std::sync::Arc;

    fn path_shape(path: PathData) -> ShapeNode {
        ShapeNode {
            shape: ShapeKind::Path,
            sides: 6,
            phase_deg: 0.0,
            fill: None,
            stroke: Stroke::default(),
            corner: Corner::Square,
            flip: false,
            path: Some(Arc::new(path)),
            text: None,
        }
    }

    fn zigzag() -> PathData {
        PathData {
            start: [0.1, 0.2],
            segs: vec![
                PathSeg::Line { to: [0.7, 0.9] },
                PathSeg::Cubic {
                    c1: [0.8, 0.1],
                    c2: [0.3, 0.4],
                    to: [0.95, 0.05],
                },
            ],
            ..PathData::default()
        }
    }

    fn picture_with_ink() -> Node {
        let mut scene = Scene::default();
        let mut img = ImageNode::new(crate::ItemId(1));
        img.crop = Crop {
            x: 0.1,
            y: 0.2,
            w: 0.5,
            h: 0.6,
        };
        let mut ink = scene.build_node(
            WorldRect::new(0.1, 0.2, 0.3, 0.1),
            NodeKind::Shape(path_shape(zigzag())),
        );
        ink.rotation_deg = 20.0;
        let mut layer = PaintLayer::new(PaintLayerId(1));
        layer.nodes.push(ink);
        img.paint_layers.push(layer);
        let mut host = scene.build_node(
            WorldRect::new(40.0, 30.0, 200.0, 100.0),
            NodeKind::Image(img),
        );
        host.rotation_deg = 30.0;
        host.clip = Some(zigzag());
        host
    }

    fn image(node: &Node) -> &ImageNode {
        match &node.kind {
            NodeKind::Image(img) => img,
            _ => panic!("image"),
        }
    }

    fn ink(node: &Node) -> &Node {
        &image(node).paint_layers[0].nodes[0]
    }

    fn ink_path(node: &Node) -> &PathData {
        match &ink(node).kind {
            NodeKind::Shape(shape) => shape.path.as_deref().unwrap(),
            _ => panic!("path"),
        }
    }

    #[test]
    fn a_picture_mirrors_its_pixels_crop_and_ink_left_to_right() {
        let before = picture_with_ink();
        let mut after = before.clone();
        assert!(mirror_local(&mut after, MirrorAxis::Horizontal));

        assert_eq!(after.rect, before.rect);
        assert_eq!(after.rotation_deg, before.rotation_deg);
        let img = image(&after);
        assert!(img.flip_x && !img.flip_y);
        assert!((img.crop.x - 0.4).abs() < 1e-6, "crop {:?}", img.crop);
        assert_eq!((img.crop.y, img.crop.w, img.crop.h), (0.2, 0.5, 0.6));

        let child = ink(&after);
        assert!((child.rect.x - 0.6).abs() < 1e-6, "{:?}", child.rect);
        assert_eq!(child.rotation_deg, -20.0);
        let path = ink_path(&after);
        assert!((path.start[0] - 0.9).abs() < 1e-6 && path.start[1] == 0.2);
        assert!(matches!(path.segs[1], PathSeg::Cubic { c1, .. } if (c1[0] - 0.2).abs() < 1e-6));
        let clip = after.clip.as_ref().unwrap();
        assert!((clip.start[0] - 0.9).abs() < 1e-6);

        // The ink lands where a mirror across the picture's local vertical
        // axis puts it in world space.
        let world_before = layer_node_to_world(&before, image(&before), ink(&before));
        let world_after = layer_node_to_world(&after, image(&after), ink(&after));
        let (cx, cy) = before.rect.center();
        let (bx, by) = world_before.rect.center();
        let (ax, ay) = world_after.rect.center();
        let axis = before.rotation_deg.to_radians();
        let (ux, uy) = (axis.cos(), axis.sin());
        let before_u = (bx - cx) * ux + (by - cy) * uy;
        let after_u = (ax - cx) * ux + (ay - cy) * uy;
        let before_v = -(bx - cx) * uy + (by - cy) * ux;
        let after_v = -(ax - cx) * uy + (ay - cy) * ux;
        assert!((after_u + before_u).abs() < 1e-3, "{before_u} vs {after_u}");
        assert!((after_v - before_v).abs() < 1e-3, "{before_v} vs {after_v}");
    }

    #[test]
    fn a_picture_mirrors_top_to_bottom() {
        let mut node = picture_with_ink();
        assert!(mirror_local(&mut node, MirrorAxis::Vertical));
        let img = image(&node);
        assert!(!img.flip_x && img.flip_y);
        assert!((img.crop.y - 0.2).abs() < 1e-6, "crop {:?}", img.crop);
        assert!((ink(&node).rect.y - 0.7).abs() < 1e-6);
        assert!((ink_path(&node).start[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn mirroring_twice_restores_the_node() {
        let before = picture_with_ink();
        for axis in [MirrorAxis::Horizontal, MirrorAxis::Vertical] {
            let mut node = before.clone();
            mirror_local(&mut node, axis);
            mirror_local(&mut node, axis);
            let img = image(&node);
            let orig = image(&before);
            assert_eq!(img.flip_x, orig.flip_x);
            assert_eq!(img.flip_y, orig.flip_y);
            assert!((img.crop.x - orig.crop.x).abs() < 1e-6);
            assert!((img.crop.y - orig.crop.y).abs() < 1e-6);
            assert!((ink(&node).rect.x - ink(&before).rect.x).abs() < 1e-6);
            assert!((ink(&node).rect.y - ink(&before).rect.y).abs() < 1e-6);
            assert_eq!(ink(&node).rotation_deg, ink(&before).rotation_deg);
        }
    }

    #[test]
    fn board_mirror_also_negates_rotation() {
        let mut node = picture_with_ink();
        assert!(mirror_on_board(&mut node, MirrorAxis::Horizontal));
        assert_eq!(node.rotation_deg, -30.0);
        assert!(image(&node).flip_x);
    }

    #[test]
    fn paths_and_lines_mirror_but_symmetric_shapes_do_not() {
        let mut scene = Scene::default();
        let mut path = scene.build_node(
            WorldRect::new(0.0, 0.0, 50.0, 40.0),
            NodeKind::Shape(path_shape(zigzag())),
        );
        assert!(mirror_local(&mut path, MirrorAxis::Horizontal));
        let NodeKind::Shape(shape) = &path.kind else {
            unreachable!()
        };
        assert!((shape.path.as_ref().unwrap().start[0] - 0.9).abs() < 1e-6);

        let mut line_shape = path_shape(PathData::default());
        line_shape.shape = ShapeKind::Line;
        line_shape.path = None;
        let mut line = scene.build_node(
            WorldRect::new(0.0, 0.0, 50.0, 40.0),
            NodeKind::Shape(line_shape.clone()),
        );
        assert!(mirror_local(&mut line, MirrorAxis::Vertical));
        assert!(matches!(&line.kind, NodeKind::Shape(s) if s.flip));

        let mut rect_shape = line_shape;
        rect_shape.shape = ShapeKind::Rect;
        let mut rect = scene.build_node(
            WorldRect::new(0.0, 0.0, 50.0, 40.0),
            NodeKind::Shape(rect_shape),
        );
        let untouched = rect.clone();
        assert!(!mirror_local(&mut rect, MirrorAxis::Horizontal));
        assert_eq!(rect, untouched);
    }

    #[test]
    fn a_mirrored_crop_window_reads_the_texture_backwards() {
        let crop = Crop {
            x: 0.25,
            y: 0.1,
            w: 0.5,
            h: 0.6,
        };
        let plain = crop.texture_uv(0.0, 0.0, Mirror::default());
        assert!((plain[0] - 0.25).abs() < 1e-6 && (plain[1] - 0.1).abs() < 1e-6);
        let mirrored = crop.texture_uv(0.0, 0.0, Mirror { x: true, y: false });
        assert!((mirrored[0] - 0.75).abs() < 1e-6, "{mirrored:?}");
        assert!((mirrored[1] - 0.1).abs() < 1e-6);
        let both = crop.texture_uv(1.0, 1.0, Mirror { x: true, y: true });
        assert!((both[0] - 0.25).abs() < 1e-6 && (both[1] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn flips_default_off_and_stay_out_of_saved_files() {
        let img: ImageNode = serde_json::from_str(r#"{"item":1}"#).unwrap();
        assert!(!img.flip_x && !img.flip_y);
        let json = serde_json::to_string(&ImageNode::new(crate::ItemId(1))).unwrap();
        assert!(!json.contains("flip"), "{json}");
        let mut flipped = ImageNode::new(crate::ItemId(1));
        flipped.flip_x = true;
        let json = serde_json::to_string(&flipped).unwrap();
        assert!(json.contains("\"flip_x\":true"), "{json}");
    }
}
