//! Hosted shape text adapts to the existing SVG text interpreter.
use super::*;
use slate_doc::scene::{ShapeNode, TextNode};
use std::fmt::Write;

pub(super) fn render_hosted_shape_text_svg(
    svg: &mut String,
    node: &Node,
    shape: &ShapeNode,
    rel: WorldRect,
    scale: f32,
) {
    let Some(block) = shape.text.as_ref().filter(|text| !text.body.is_empty()) else {
        return;
    };
    if !slate_doc::scene::shape_hosts_text(shape) {
        return;
    }
    let inset = 8.0 * scale;
    let inner = WorldRect::new(
        rel.x + inset,
        rel.y + inset,
        (rel.w - 2.0 * inset).max(0.0),
        (rel.h - 2.0 * inset).max(0.0),
    );
    let text = TextNode {
        text: block.body.clone(),
        family: block.family,
        style: block.style,
        size: block.size,
        color: block.color,
        align: block.align,
        fill: None,
        stroke: Default::default(),
        agent: None,
    };
    // The shape's outer group already owns rotation and opacity. The text
    // interpreter receives identity values so neither is applied twice.
    let mut identity = node.clone();
    identity.rotation_deg = 0.0;
    identity.opacity = 1.0;
    let clip = format!("hosted-shape-text-{}", node.id.0);
    let _ = write!(svg,
        "<defs><clipPath id=\"{clip}\"><rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\"/></clipPath></defs><g clip-path=\"url(#{clip})\">",
        inner.x, inner.y, inner.w, inner.h);
    render_text_svg(svg, &identity, &text, &block.body, inner, scale);
    svg.push_str("</g>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{ShapeText, TextStyle};

    #[test]
    fn hosted_shape_text_keeps_inset_alignment_style_and_one_host_transform() {
        for (align, x, anchor) in [
            (TextAlign::Left, 46.0, "start"),
            (TextAlign::Center, 180.0, "middle"),
            (TextAlign::Right, 314.0, "end"),
        ] {
            let mut scene = Scene::default();
            let block = ShapeText {
                body: "Ink <&>".into(),
                family: Default::default(),
                size: 24.0,
                color: Rgba::BLACK,
                align,
                style: TextStyle {
                    bold: true,
                    italic: true,
                    underline: true,
                },
            };
            let shape = ShapeNode {
                shape: ShapeKind::Rect,
                fill: None,
                stroke: Default::default(),
                corner: Default::default(),
                sides: 6,
                phase_deg: 0.0,
                flip: false,
                path: None,
                text: Some(block),
            };
            let rel = WorldRect::new(30.0, 40.0, 300.0, 180.0);
            let mut node = scene.build_node(rel, NodeKind::Shape(shape.clone()));
            node.rotation_deg = 37.0;
            node.opacity = 0.25;
            let mut svg = String::new();
            render_shape_svg(&mut svg, &node, &shape, rel, 2.0);
            assert!(
                svg.contains(&format!("<text x=\"{x:.3}\" y=\"130.000\"")),
                "{svg}"
            );
            assert!(svg.contains(&format!("text-anchor=\"{anchor}\"")));
            assert!(svg.contains("font-size=\"48.000\""));
            assert!(svg.contains("font-weight:bold;font-style:italic;text-decoration:underline;"));
            assert!(svg.contains("Ink &lt;&amp;&gt;"));
            assert_eq!(svg.matches("rotate(37.000 180.000 130.000)").count(), 1);
            assert_eq!(svg.matches("opacity=\"0.250\"").count(), 1);
            assert!(svg.contains("x=\"46.000\" y=\"56.000\" width=\"268.000\" height=\"148.000\""));
        }
    }
}
