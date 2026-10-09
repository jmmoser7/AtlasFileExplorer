//! Image-paint boards and settled brush rasters.

use super::*;

/// An image at `rect` turned by `rotation_deg`, selected, with the Brush
/// painting on it at 50 % opacity and one mark drawn through `mark`.
pub(crate) fn image_paint_board(
    name: &str,
    rect: slate_doc::scene::WorldRect,
    rotation_deg: f32,
    mark: &[Pos2],
) -> (Harness, NodeId) {
    let mut h = Harness::new(name);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let mut node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    node.rotation_deg = rotation_deg;
    let image_id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_sel = std::iter::once(image_id).collect();
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.app.brush_opacity = 0.5;
    h.app.brush_softness = 0.0;
    h.frame();
    h.frame();
    assert!(h.app.image_paint_session().is_some());
    press_drag_release_frames(&mut h, mark, egui::Modifiers::NONE, |_| {});
    (h, image_id)
}

/// The marks on each paint layer of `image`, layer by layer.
pub(crate) fn layer_marks(h: &Harness, image: NodeId) -> Vec<Vec<slate_doc::Node>> {
    match &h.app.doc().scene.node(image).unwrap().kind {
        NodeKind::Image(img) => img.paint_layers.iter().map(|l| l.nodes.clone()).collect(),
        _ => panic!("the image"),
    }
}

pub(crate) fn painted_textures(out: &egui::FullOutput) -> Vec<egui::TextureId> {
    fn walk(shape: &egui::Shape, into: &mut Vec<egui::TextureId>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, into)),
            egui::Shape::Mesh(mesh) => into.push(mesh.texture_id),
            _ => {}
        }
    }
    let mut into = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut into);
    }
    into
}

/// Frames until the brush rasters are settled and `ready` holds.
pub(crate) fn settle_brush(h: &mut Harness, what: &str, ready: impl Fn(&SlateApp) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        h.frame();
        if h.app.brush_tiles.last.settled && ready(&h.app) {
            return;
        }
        assert!(std::time::Instant::now() < deadline, "{what} never settled");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
