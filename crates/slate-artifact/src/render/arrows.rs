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

#[allow(clippy::too_many_arguments)] // Interpreter inputs are kept explicit.
pub(super) fn render_connector(
    html: &mut String,
    doc: &SlateDoc,
    node: &Node,
    conn: &ConnectorNode,
    origin_x: f32,
    origin_y: f32,
    routing: WireRouting,
    theme_wire: Option<slate_doc::scene::Rgba>,
) {
    let scene = &doc.scene;
    let host_of = |id| {
        scene.node(id).filter(|node| !node.hidden).map(|node| {
            slate_doc::WireHost::from_node_with_paths(node, &|item| {
                doc.item(item).map(|file| file.path.as_path())
            })
        })
    };
    // Geometry is derived from the *current* rects of anchored nodes.
    // Hidden anchor nodes resolve to nothing: the wire is skipped until the
    // node is shown again (simplest per the scene-flags spec).
    let Some(path) = connector_route_in_scene_with_hosts(
        scene,
        Some(node.id),
        &conn.a,
        &conn.b,
        conn.effective_routing(routing),
        host_of,
    ) else {
        return;
    };
    let color = conn.paint_color(theme_wire.unwrap_or(WIRE_FALLBACK));
    let color_css = color.css();
    let drawn = connector_drawn_stroke(conn.stroke);
    let path = retreat_off_hosts(path, &conn.a, &conn.b, host_of, drawn.width * 0.5);

    // Position the wrapper at the curve AABB, padded so stroke width and
    // arrowheads survive even a degenerate (straight axis-aligned) box.
    let pad = drawn.width.max(1.0) * 0.5 + arrow_len(&drawn) + 2.0;
    let aabb = path.aabb();
    let boxed = WorldRect::new(
        aabb.x - pad,
        aabb.y - pad,
        aabb.w + pad * 2.0,
        aabb.h + pad * 2.0,
    );
    let rel = boxed.translated(-origin_x, -origin_y);
    let local = |p: [f32; 2]| (p[0] - boxed.x, p[1] - boxed.y);

    let mut wrap = geometry_style(rel, node.rotation_deg);
    let opacity = match conn.display {
        WireDisplay::Faint => node.opacity * FAINT_OPACITY,
        WireDisplay::Default => node.opacity,
    };
    append_opacity(&mut wrap, opacity);
    wrap.push_str("overflow:visible;background:transparent;");

    html.push_str("<div class=\"node\" style=\"");
    html.push_str(&wrap);
    html.push_str("\"><svg width=\"");
    html.push_str(&fmt_px(rel.w));
    html.push_str("\" height=\"");
    html.push_str(&fmt_px(rel.h));
    html.push_str("\" viewBox=\"0 0 ");
    html.push_str(&fmt_px(rel.w));
    html.push(' ');
    html.push_str(&fmt_px(rel.h));
    html.push_str("\" style=\"display:block;overflow:visible\">");

    // The wire itself.
    let d = svg_d_for_path(&path, &local);
    push_path_open(html, &d, "none", PathFillRule::NonZero);
    html.push_str(" stroke=\"");
    html.push_str(&color_css);
    html.push_str("\" stroke-width=\"");
    html.push_str(&fmt_px(drawn.width));
    html.push_str("\" stroke-linecap=\"");
    html.push_str(stroke_cap_css(conn.stroke.cap));
    html.push('"');
    if let Some(dash) = line_dash_attrs(&conn.stroke) {
        html.push_str(" stroke-dasharray=\"");
        html.push_str(dash);
        html.push('"');
    }
    html.push_str("></path>");

    // Arrowheads: small filled triangles oriented to the end tangents,
    // computed inline (the writer builds elements directly).
    if conn.arrow_a {
        push_arrow_head(
            html,
            conn,
            local(path.start()),
            path.start_dir(),
            &color_css,
        );
    }
    if conn.arrow_b {
        push_arrow_head(html, conn, local(path.end()), path.end_dir(), &color_css);
    }

    // The board paints a crosstalk's live chip; the artifact states its count.
    let crosstalk = slate_doc::crosstalk::export_label(scene, node);
    if let Some(label) = conn
        .label
        .as_deref()
        .filter(|l| !l.is_empty())
        .or(crosstalk.as_deref())
    {
        let (mx, my) = local(path.midpoint());
        html.push_str("<text x=\"");
        html.push_str(&fmt_px(mx));
        html.push_str("\" y=\"");
        html.push_str(&fmt_px(my));
        html.push_str("\" text-anchor=\"middle\" dominant-baseline=\"middle\" fill=\"");
        html.push_str(&color_css);
        html.push_str("\" style=\"font:");
        html.push_str(&fmt_px(CONNECTOR_LABEL_SIZE));
        html.push_str("px ");
        html.push_str(slate_doc::scene::FontChoice::Sans.css_stack());
        html.push_str("\">");
        html.push_str(&escape_html(label));
        html.push_str("</text>");
    }

    html.push_str("</svg></div>\n");
}

#[cfg(test)]
mod tests;
