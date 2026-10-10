//! Per-vertex width and color samples, on the board and in export.

use super::*;

/// Pick `points` on the selected curve (point selection), then set the Stroke
/// stringer's width through the property strip.
pub(crate) fn vertex_stringer_width(h: &mut Harness, id: NodeId, points: &[usize], width: f32) {
    h.app.board_sel = [id].into_iter().collect();
    h.app.direct.grip_points = Default::default();
    for (k, i) in points.iter().enumerate() {
        h.app.direct.grip_points.pick(id, *i, k > 0);
    }
    h.app.sync_shape_properties();
    h.app
        .preview_shape_property(board_properties::Property::StrokeWidth(width));
    h.app.apply_shape_preview(&h.ctx, true);
    h.frame();
}

pub(crate) fn curve_shape(
    h: &Harness,
    id: NodeId,
) -> (slate_doc::Node, slate_doc::scene::ShapeNode) {
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let NodeKind::Shape(s) = &n.kind else {
        panic!("a shape")
    };
    let s = s.clone();
    (n, s)
}

pub(crate) fn tip_widths(h: &Harness, id: NodeId) -> Vec<f32> {
    let (_, s) = curve_shape(h, id);
    s.path
        .as_ref()
        .unwrap()
        .tips
        .iter()
        .map(|t| t.width)
        .collect()
}

/// Painted half-width of `id`'s board stroke at `p` along unit `normal`
/// (camera at 1:1), less the anti-aliasing fringe.
pub(crate) fn ink_half_width(h: &Harness, id: NodeId, p: Pos2, normal: EVec2) -> f32 {
    ink_half_width_at(h, id, p, normal, 1.0)
}

/// [`ink_half_width`] on the mesh the board builds at `zoom`.
pub(crate) fn ink_half_width_at(h: &Harness, id: NodeId, p: Pos2, normal: EVec2, zoom: f32) -> f32 {
    let (n, s) = curve_shape(h, id);
    let mesh = board_path::vector_stroke_ink(&n, &s, s.path.as_ref().unwrap(), zoom);
    let verts: Vec<[f32; 2]> = mesh.vertices.iter().map(|v| v.pos).collect();
    let inside = |d: f32| {
        let q = p + normal * d;
        vector_ink::point_in_mesh(&verts, &mesh.indices, [q.x, q.y])
    };
    assert!(inside(0.0), "{p:?} is on the stroke");
    let (mut lo, mut hi) = (0.0_f32, 64.0_f32);
    for _ in 0..40 {
        let mid = (lo + hi) * 0.5;
        if inside(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo - board_path::FEATHER_PX * 0.5 / zoom
}

pub(crate) fn assert_close(got: f32, want: f32, tol: f32, what: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: {got} != {want} (±{tol})"
    );
}

pub(crate) fn smoothstep(s: f32) -> f32 {
    s * s * (3.0 - 2.0 * s)
}

// ---------- per-vertex stroke color (P1.curve.vertex-style) ----------

/// Pick `points` on the selected curve, then choose `rgb` in the Stroke
/// color editor through the property strip.
pub(crate) fn vertex_stringer_color(h: &mut Harness, id: NodeId, points: &[usize], rgb: [u8; 3]) {
    h.app.board_sel = [id].into_iter().collect();
    h.app.direct.grip_points = Default::default();
    for (k, i) in points.iter().enumerate() {
        h.app.direct.grip_points.pick(id, *i, k > 0);
    }
    h.app.sync_shape_properties();
    h.app
        .preview_shape_property(board_properties::Property::StrokeRgb(rgb));
    h.app.apply_shape_preview(&h.ctx, true);
    h.frame();
}

pub(crate) fn tip_colors(h: &Harness, id: NodeId) -> Vec<[u8; 4]> {
    let (_, s) = curve_shape(h, id);
    s.path
        .as_ref()
        .unwrap()
        .tips
        .iter()
        .map(|t| t.color.0)
        .collect()
}

/// Board stroke color (0..255 per channel) at world `p`, interpolated inside
/// the painted mesh triangle that holds it.
pub(crate) fn ink_color_at(h: &Harness, id: NodeId, p: Pos2) -> [f32; 4] {
    let (n, s) = curve_shape(h, id);
    let mesh = board_path::vector_stroke_ink(&n, &s, s.path.as_ref().unwrap(), 1.0);
    mesh_color_at(&mesh, p)
}

pub(crate) fn mesh_color_at(mesh: &vector_ink::InkMesh, p: Pos2) -> [f32; 4] {
    assert_eq!(mesh.colors.len(), mesh.vertices.len(), "a tinted mesh");
    for tri in mesh.indices.chunks(3) {
        let [a, b, c] = [0, 1, 2].map(|k| tri[k] as usize);
        let [pa, pb, pc] = [a, b, c].map(|k| mesh.vertices[k].pos);
        let det = (pb[1] - pc[1]) * (pa[0] - pc[0]) + (pc[0] - pb[0]) * (pa[1] - pc[1]);
        if det.abs() < 1e-9 {
            continue;
        }
        let l1 = ((pb[1] - pc[1]) * (p.x - pc[0]) + (pc[0] - pb[0]) * (p.y - pc[1])) / det;
        let l2 = ((pc[1] - pa[1]) * (p.x - pc[0]) + (pa[0] - pc[0]) * (p.y - pc[1])) / det;
        let l3 = 1.0 - l1 - l2;
        if l1 < -1e-4 || l2 < -1e-4 || l3 < -1e-4 {
            continue;
        }
        let [ca, cb, cc] = [a, b, c].map(|k| mesh.colors[k]);
        return std::array::from_fn(|i| (ca[i] * l1 + cb[i] * l2 + cc[i] * l3) * 255.0);
    }
    panic!("{p:?} is not on the painted stroke");
}

pub(crate) fn assert_color_close(got: [f32; 4], want: [f32; 4], tol: f32, what: &str) {
    for i in 0..4 {
        assert!(
            (got[i] - want[i]).abs() <= tol,
            "{what}: {got:?} != {want:?} (±{tol})"
        );
    }
}

pub(crate) fn mix_rgba(a: [u8; 4], b: [u8; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|i| a[i] as f32 + (b[i] as f32 - a[i] as f32) * t)
}

/// Exported color at local point `p` of the stroke's SVG: the fill of the
/// topmost piece holding it, a solid color or a two-stop linear gradient.
pub(crate) fn export_color_at(svg: &str, p: [f32; 2]) -> Option<[f32; 4]> {
    svg.split("<path")
        .skip(1)
        .filter_map(|tag| export_piece_color(svg, tag, p))
        .last()
}

pub(crate) fn export_piece_color(svg: &str, tag: &str, p: [f32; 2]) -> Option<[f32; 4]> {
    let attr = |tag: &str, name: &str| -> Option<String> {
        let key = format!(" {name}=\"");
        let rest = &tag[tag.find(&key)? + key.len()..];
        Some(rest[..rest.find('"')?].to_string())
    };
    let parse_rgb = |css: &str, opacity: f32| -> [f32; 4] {
        let inner = css.trim_start_matches("rgba(").trim_start_matches("rgb(");
        let v: Vec<f32> = inner
            .trim_end_matches(')')
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect();
        let a = v.get(3).copied().unwrap_or(1.0) * opacity;
        [v[0], v[1], v[2], a * 255.0]
    };
    let tag = &tag[..tag.find('>')?];
    let bez = vector_ink::kurbo::BezPath::from_svg(&attr(tag, "d")?).ok()?;
    if !vector_ink::point_in_polygon(&vector_ink::flatten_contours(&bez, 0.05), p) {
        return None;
    }
    let fill = attr(tag, "fill")?;
    let Some(id) = fill.strip_prefix("url(#").and_then(|s| s.strip_suffix(')')) else {
        return Some(parse_rgb(&fill, 1.0));
    };
    let start = svg.find(&format!("<linearGradient id=\"{id}\""))?;
    let grad = &svg[start..start + svg[start..].find("</linearGradient>")?];
    let num = |name: &str| attr(grad, name).unwrap().parse::<f32>().unwrap();
    let (x1, y1, x2, y2) = (num("x1"), num("y1"), num("x2"), num("y2"));
    let stops: Vec<[f32; 4]> = grad
        .split("<stop")
        .skip(1)
        .map(|s| {
            let op = attr(s, "stop-opacity").map_or(1.0, |o| o.parse().unwrap());
            parse_rgb(&attr(s, "stop-color").unwrap(), op)
        })
        .collect();
    let (dx, dy) = (x2 - x1, y2 - y1);
    let t = (((p[0] - x1) * dx + (p[1] - y1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    Some(std::array::from_fn(|i| {
        stops[0][i] + (stops[1][i] - stops[0][i]) * t
    }))
}

// ---------- per-vertex style survives trim, split and join ----------

/// Give curve `id` one tip per vertex (`widths`, red channel `reds`) and
/// per-vertex corner overrides, as a vertex edit would store them.
pub(crate) fn style_vertices(
    h: &mut Harness,
    id: NodeId,
    widths: &[f32],
    reds: &[u8],
    corners: &[Option<f32>],
) {
    let n = h.app.doc_mut().scene.node_mut(id).unwrap();
    let NodeKind::Shape(s) = &mut n.kind else {
        panic!("a shape")
    };
    let mut path = s.path.as_deref().cloned().unwrap();
    assert_eq!(widths.len(), path.segs.len() + 1, "one tip per vertex");
    let base = s.stroke.color.0;
    path.tips = widths
        .iter()
        .zip(reds)
        .map(|(&width, &r)| slate_doc::scene::StrokeSpan {
            width,
            softness: 0.0,
            color: Rgba([r, base[1], base[2], 255]),
            texture: Default::default(),
        })
        .collect();
    path.corner_amounts = corners.to_vec();
    s.stroke.width = widths.iter().copied().fold(0.0, f32::max);
    s.path = Some(path.into());
    h.frame();
}

/// Painted width and color at each vertex of curve `id`.
pub(crate) fn painted_vertex_tips(h: &Harness, id: NodeId) -> Vec<(f32, [u8; 4])> {
    let (_, s) = curve_shape(h, id);
    let p = s.path.as_ref().unwrap();
    let n = p.segs.len() + 1;
    let widths = p
        .vector_widths(&s.stroke)
        .unwrap_or_else(|| vec![s.stroke.width; n]);
    let colors = p.vector_colors().unwrap_or_else(|| vec![s.stroke.color; n]);
    widths.into_iter().zip(colors.iter().map(|c| c.0)).collect()
}

pub(crate) fn corner_overrides(h: &Harness, id: NodeId) -> Vec<Option<f32>> {
    let (_, s) = curve_shape(h, id);
    let p = s.path.as_ref().unwrap();
    if p.corner_amounts.is_empty() {
        vec![None; p.segs.len() + 1]
    } else {
        p.corner_amounts.clone()
    }
}

/// World position of each vertex of curve `id` (start, then segment ends).
pub(crate) fn world_vertices(h: &Harness, id: NodeId) -> Vec<Pos2> {
    let (n, s) = curve_shape(h, id);
    let p = s.path.as_ref().unwrap();
    let ends = p.segs.iter().map(|seg| match *seg {
        slate_doc::scene::PathSeg::Line { to }
        | slate_doc::scene::PathSeg::Quad { to, .. }
        | slate_doc::scene::PathSeg::Cubic { to, .. } => to,
    });
    std::iter::once(p.start)
        .chain(ends)
        .map(|q| {
            let w = n.rect.rotate_point(
                [n.rect.x + q[0] * n.rect.w, n.rect.y + q[1] * n.rect.h],
                n.rotation_deg,
            );
            Pos2::new(w[0], w[1])
        })
        .collect()
}

pub(crate) fn assert_vertex(got: (f32, [u8; 4]), width: f32, red: f32, what: &str) {
    assert_close(got.0, width, 0.02, &format!("{what} width"));
    assert_close(got.1[0] as f32, red, 1.0, &format!("{what} red"));
}

/// The four-vertex polyline the trim and split tests cut at x = 25.
pub(crate) fn styled_cut_polyline(h: &mut Harness) -> NodeId {
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(100.0, 100.0),
        Pos2::new(200.0, 100.0),
    ];
    let id = commit_polyline(h, &pts, false);
    style_vertices(
        h,
        id,
        &[2.0, 20.0, 6.0, 12.0],
        &[0, 200, 100, 50],
        &[None, Some(8.0), Some(4.0), None],
    );
    id
}

/// Board stroke of `id` at `p` on a horizontal run: painted width and color.
pub(crate) fn ink_sample(h: &Harness, id: NodeId, p: Pos2) -> (f32, [f32; 4]) {
    let width = 2.0 * ink_half_width(h, id, p, EVec2::new(0.0, 1.0));
    (width, ink_color_at(h, id, p))
}

pub(crate) fn assert_matches_ink(got: (f32, [u8; 4]), ink: (f32, [f32; 4]), what: &str) {
    assert_close(got.0, ink.0, 0.3, &format!("{what}: width as painted"));
    let color = got.1.map(|c| c as f32);
    assert_color_close(color, ink.1, 1.5, &format!("{what}: color as painted"));
}
