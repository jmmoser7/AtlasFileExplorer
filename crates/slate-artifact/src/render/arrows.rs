//! SVG arrowheads retain the curve's endpoint-to-base color blend.
use super::*;

pub(super) fn push_curve_heads(
    html: &mut String,
    id: u64,
    shape: &slate_doc::scene::ShapeNode,
    path: Option<&PathData>,
    tipped: Option<&slate_doc::geom::TippedStroke>,
    w: f32,
    h: f32,
) {
    let Some(path) = path.filter(|p| !p.closed) else {
        return;
    };
    if shape.stroke.is_none() {
        return;
    }
    let bez = tipped.map_or_else(|| path_data_to_bez(path, w, h), |t| t.bez.clone());
    for end in (0..2).filter(|i| shape.stroke.arrows()[*i]) {
        let width = tipped
            .and_then(|t| {
                if end == 0 {
                    t.widths.first()
                } else {
                    t.widths.last()
                }
            })
            .copied()
            .unwrap_or(shape.stroke.width);
        let size = shape.stroke.arrow_size(end, width);
        let Some((tip, into)) = slate_doc::geom::path_arrow_sized(&bez, end, size) else {
            continue;
        };
        let [a, b, c] = slate_doc::geom::arrow_head_sized(tip, into, size);
        let colors = tipped.and_then(|t| slate_doc::geom::arrow_colors(t, end, size[0]));
        let fill = if let Some(colors) = colors {
            let base = [(b[0] + c[0]) * 0.5, (b[1] + c[1]) * 0.5];
            let gid = format!("arrow{id}_{end}");
            html.push_str(&format!("<defs><linearGradient id=\"{gid}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" color-interpolation=\"sRGB\">", fmt_px(a[0]), fmt_px(a[1]), fmt_px(base[0]), fmt_px(base[1])));
            for (offset, color) in colors.into_iter().enumerate() {
                let rgba = Rgba(color.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
                html.push_str(&format!(
                    "<stop offset=\"{offset}\" stop-color=\"{}\"/>",
                    rgba.css()
                ));
            }
            html.push_str("</linearGradient></defs>");
            format!("url(#{gid})")
        } else {
            shape.stroke.color.css()
        };
        html.push_str(&format!(
            "<path d=\"M {} {} L {} {} L {} {} Z\" fill=\"{fill}\" stroke=\"none\"></path>",
            fmt_px(a[0]),
            fmt_px(a[1]),
            fmt_px(b[0]),
            fmt_px(b[1]),
            fmt_px(c[0]),
            fmt_px(c[1])
        ));
    }
}
