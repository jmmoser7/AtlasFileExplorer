//! Picture items and a filleted polyline.

use super::*;

/// The open U polyline the grip-overlap tests select: turning vertices 1
/// and 2, a shared 30-unit fillet, and vertex 2 overridden to sharp.
pub(crate) fn filleted_u_polyline(h: &mut Harness) -> (NodeId, [Pos2; 4]) {
    h.app.tab_mut().cam.offset = egui::vec2(0.0, 0.0);
    let pts = [
        Pos2::new(-100.0, -100.0),
        Pos2::new(100.0, -100.0),
        Pos2::new(100.0, 100.0),
        Pos2::new(-100.0, 100.0),
    ];
    let id = commit_polyline(h, &pts, false);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.corner = slate_doc::scene::Corner::Rounded { radius: 30.0 };
        }
        slate_doc::scene::set_vertex_corner_amount(n, 2, 0.0);
    });
    h.frame();
    (id, pts)
}

pub(crate) fn picture_from(h: &mut Harness, name: &str, x: f32) -> NodeId {
    let path = h.base.join(name);
    image::RgbaImage::from_pixel(8, 8, image::Rgba([90, 140, 200, 255]))
        .save(&path)
        .unwrap();
    let item = h.app.add_paths(&[path])[0];
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(x, 0.0, 160.0, 120.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    h.app.add_nodes(vec![node])[0]
}

pub(crate) fn picture_item(h: &Harness, id: NodeId) -> slate_doc::ItemId {
    match &h.app.doc().scene.node(id).unwrap().kind {
        slate_doc::scene::NodeKind::Image(img) => img.item,
        _ => panic!("image"),
    }
}

// ---------- per-vertex stroke width (P1.curve.vertex-style) ----------
