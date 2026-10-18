//! SVG serialization of the same triangle gradients as native paint.
use super::*;

pub(super) fn push_field(
    html: &mut String,
    id: u64,
    shape: &slate_doc::scene::ShapeNode,
    path: Option<&PathData>,
    w: f32,
    h: f32,
) {
    let Some(path) = path.filter(|p| !p.field_colors.is_empty() && shape.fill.is_some()) else {
        return;
    };
    let mesh = slate_doc::field::mesh(
        path,
        WorldRect::new(0.0, 0.0, w, h),
        0.0,
        shape.corner,
        0.25,
    );
    for (triangle, indices) in mesh.indices.chunks_exact(3).enumerate() {
        let points: [[f32; 2]; 3] = std::array::from_fn(|i| mesh.vertices[indices[i] as usize].pos);
        let d = format!(
            "M{} {}L{} {}L{} {}Z",
            fmt_px(points[0][0]),
            fmt_px(points[0][1]),
            fmt_px(points[1][0]),
            fmt_px(points[1][1]),
            fmt_px(points[2][0]),
            fmt_px(points[2][1])
        );
        html.push_str("<g style=\"isolation:isolate\">");
        for i in 0..3 {
            let (p, a, b) = (points[i], points[(i + 1) % 3], points[(i + 2) % 3]);
            let edge = [b[0] - a[0], b[1] - a[1]];
            let length = edge[0] * edge[0] + edge[1] * edge[1];
            if length <= f32::EPSILON {
                continue;
            }
            let t = ((p[0] - a[0]) * edge[0] + (p[1] - a[1]) * edge[1]) / length;
            let opposite = [a[0] + t * edge[0], a[1] + t * edge[1]];
            let color = mesh.colors[indices[i] as usize];
            let rgb = Rgba([
                (color[0] * 255.0).round() as u8,
                (color[1] * 255.0).round() as u8,
                (color[2] * 255.0).round() as u8,
                255,
            ])
            .css();
            let gid = format!("field{id}_{triangle}_{i}");
            html.push_str(&format!("<defs><linearGradient id=\"{gid}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" color-interpolation=\"sRGB\"><stop offset=\"0\" stop-color=\"{rgb}\" stop-opacity=\"{}\"/><stop offset=\"1\" stop-color=\"{rgb}\" stop-opacity=\"0\"/></linearGradient></defs><path d=\"{d}\" fill=\"url(#{gid})\" style=\"mix-blend-mode:plus-lighter\"/>", fmt_px(p[0]),fmt_px(p[1]),fmt_px(opposite[0]),fmt_px(opposite[1]),fmt_px(color[3])));
        }
        html.push_str("</g>");
    }
}
