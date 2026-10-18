//! Cached native interpreter of boundary-authored fill gradients.
use super::*;

pub(super) fn paint_field(
    app: &mut SlateApp,
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
    fade: &impl Fn(Color32) -> Color32,
) {
    let key = path_content_hash(
        path,
        &shape.stroke,
        node.rect,
        node.rotation_deg,
        shape.corner,
        zoom_bucket(xf.z),
    ) ^ 0xf1e1_d001;
    let cached = app.path_mesh_cache.get_or_tessellate(node.id, key, || {
        slate_doc::field::mesh(
            path,
            node.rect,
            node.rotation_deg,
            shape.corner,
            curve_tolerance(xf.z),
        )
    });
    let mesh = ink_mesh_to_epaint(&cached, xf, Color32::WHITE, fade);
    painter.add(Shape::mesh(mesh));
}
