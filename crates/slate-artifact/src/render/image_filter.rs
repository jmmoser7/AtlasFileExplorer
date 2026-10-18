//! Alpha-preserving image tint. SVG filter math lives with ImageAdjust.
use slate_doc::{scene::ImageAdjust, NodeId};
use std::fmt::Write;

pub(super) fn html_filter(html: &mut String, adjust: &ImageAdjust, node: NodeId) -> String {
    let mut filter = adjust.css_filter();
    if let Some(matrix) = slate_doc::image_adjust::overlay_color_matrix(adjust) {
        // SVG feColorMatrix operates on straight color; sRGB matches CPU pixels.
        // https://www.w3.org/TR/SVG11/filters.html#feColorMatrixElement
        let values = matrix
            .iter()
            .map(|v| format!("{v:.6}"))
            .collect::<Vec<_>>()
            .join(" ");
        let _ = write!(html,
            "<svg width=\"0\" height=\"0\" aria-hidden=\"true\" style=\"position:absolute\"><defs><filter id=\"image-tint-{}\" color-interpolation-filters=\"sRGB\"><feColorMatrix type=\"matrix\" values=\"{}\"/></filter></defs></svg>",
            node.0, values);
        if !filter.is_empty() {
            filter.push(' ');
        }
        let _ = write!(filter, "url(#image-tint-{})", node.0);
    }
    filter
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{ImageNode, NodeKind, PortalNode, Rgba, WorldRect};

    #[test]
    fn blank_image_generator_keeps_its_neutral_placeholder_without_tint() {
        let mut doc = slate_doc::SlateDoc::new("blank generator");
        let mut image = ImageNode::new(slate_doc::ItemId::NONE);
        image.agent = PortalNode::unbound_agent("", "comfy").agent;
        image.adjust.overlay = Some(Rgba::opaque(255, 0, 0));
        let node = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 300.0, 200.0),
            NodeKind::Image(image),
        );
        doc.scene.nodes.push(node);
        let html = crate::render_html(&doc, &crate::AssetMap::default());
        assert!(!html.contains("class=\"ovl\""));
    }

    #[test]
    fn svg_tint_preserves_cutout_alpha_and_matches_cpu_color() {
        let adjust = ImageAdjust {
            overlay: Some(Rgba([0, 0, 0, 128])),
            ..Default::default()
        };
        let mut definition = String::new();
        let filter = html_filter(&mut definition, &adjust, NodeId(42));
        let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4\" height=\"4\">{definition}<rect width=\"2\" height=\"4\" fill=\"rgb(200,200,200)\" fill-opacity=\"0.5\" style=\"filter:{filter}\"/></svg>");
        let pixels = crate::rasterize_paint_layers_svg(&svg, 4, 4).unwrap();
        let expected =
            slate_doc::image_adjust::PixelFilter::new(&adjust).apply([200, 200, 200, 128]);
        let sample = &pixels[(2 * 4) * 4..][..4];
        for (got, expected) in sample.iter().zip(expected) {
            assert!(
                got.abs_diff(expected) <= 2,
                "SVG tint must match CPU without filling transparency"
            );
        }
        assert_eq!(pixels[(2 * 4 + 3) * 4 + 3], 0);
    }
}
