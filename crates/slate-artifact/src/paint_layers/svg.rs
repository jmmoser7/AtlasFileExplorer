//! SVG interpreter for individual hosted paint-layer children.
use super::*;

pub(super) fn node_svg(
    html: &mut String,
    mapped: &Node,
    child_rel: WorldRect,
    scale: f32,
    doc: Option<&SlateDoc>,
) {
    let clipped = clips::begin(html, mapped, child_rel);
    match &mapped.kind {
        NodeKind::Shape(shape) => render_shape_svg(html, mapped, shape, child_rel, scale),
        NodeKind::Text(text) => render_text_svg(html, mapped, text, &text.text, child_rel, scale),
        NodeKind::Image(layer_img) => {
            if let Some(href) = doc.and_then(|doc| image_data_uri(doc, layer_img)) {
                let cx = child_rel.x + child_rel.w * 0.5;
                let cy = child_rel.y + child_rel.h * 0.5;
                let mirror = layer_img
                    .mirror()
                    .scale()
                    .map(|(sx, sy)| {
                        format!(
                            " translate({cx:.3} {cy:.3}) scale({sx} {sy}) translate({:.3} {:.3})",
                            -cx, -cy
                        )
                    })
                    .unwrap_or_default();
                let _ = write!(
                    html,
                    "<image x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" preserveAspectRatio=\"none\" opacity=\"{:.3}\" transform=\"rotate({:.3} {:.3} {:.3}){}\" href=\"{}\"/>",
                    child_rel.x,
                    child_rel.y,
                    child_rel.w,
                    child_rel.h,
                    mapped.opacity.clamp(0.0, 1.0),
                    mapped.rotation_deg,
                    cx,
                    cy,
                    mirror,
                    escape_attr(&href)
                );
            }
        }
        _ => {}
    }
    if clipped {
        html.push_str("</g>");
    }
}
