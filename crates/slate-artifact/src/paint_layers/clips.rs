//! Node-local selection clips, shared by layer SVG and board rasterization.
use slate_doc::scene::{Node, PathFillRule, WorldRect};
use std::fmt::Write;

pub(super) fn begin(svg: &mut String, node: &Node, rect: WorldRect) -> bool {
    if !definition(svg, node, rect) {
        return false;
    }
    let _ = write!(svg, "<g clip-path=\"url(#layer-clip-{})\">", node.id.0);
    true
}

fn definition(svg: &mut String, node: &Node, rect: WorldRect) -> bool {
    let Some(clip) = &node.clip else {
        return false;
    };
    let rule = match clip.fill_rule {
        PathFillRule::EvenOdd => "evenodd",
        PathFillRule::NonZero => "nonzero",
    };
    let path = crate::render::path_data_d(clip, rect.w, rect.h);
    let _ = write!(svg,
        "<defs><clipPath id=\"layer-clip-{}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{}\" clip-rule=\"{}\" transform=\"translate({:.3} {:.3}) rotate({:.3} {:.3} {:.3})\"/></clipPath></defs>",
        node.id.0, crate::render::escape_attr(&path), rule, rect.x, rect.y,
        node.rotation_deg, rect.w * 0.5, rect.h * 0.5);
    true
}

/// Clip a full-host HTML layer wrapper with the same SVG definition.
pub(crate) fn html_style(html: &mut String, node: &Node, rect: WorldRect) -> String {
    if node.clip.is_none() {
        return String::new();
    }
    html.push_str(
        "<svg width=\"0\" height=\"0\" aria-hidden=\"true\" style=\"position:absolute\">",
    );
    definition(html, node, rect);
    html.push_str("</svg>");
    format!("clip-path:url(#layer-clip-{});", node.id.0)
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use slate_doc::image_paint::{region_path, PaintLayer, PaintLayerId};
    use slate_doc::scene::{ImageNode, NodeKind, WorldRect};
    #[test]
    fn linked_image_selection_keeps_alpha_holes_in_both_interpreters() {
        let dir = std::env::temp_dir().join(format!("slate-selection-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("generated-red.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([240, 10, 20, 255]))
            .save(&path)
            .unwrap();
        let mut doc = SlateDoc::new("selection");
        let item = doc.add_item(path, "generated-red.png", 0, 0, "");
        let mut host = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            NodeKind::Image(ImageNode::new(item)),
        );
        let mut child = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Image(ImageNode::new(item)),
        );
        child.clip = region_path(vec![
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            vec![[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]],
        ]);
        let NodeKind::Image(img) = &mut host.kind else {
            unreachable!()
        };
        img.paint_layers.push(PaintLayer {
            nodes: vec![child],
            ..PaintLayer::new(PaintLayerId(1))
        });
        let img = img.clone();
        let svg = paint_layers_svg_with_doc(&host, &img, 32, 32, &doc);
        let exported = rasterize_paint_layers_svg(&svg, 32, 32).unwrap();
        let board =
            rasterize_paint_layers(&host, &img, 32, 32, Some(&doc), &mut LayerStamps::default())
                .unwrap();
        for pixels in [&exported, &board] {
            assert!(
                pixels[(4 * 32 + 4) * 4 + 3] > 240,
                "selected exterior must retain image"
            );
            assert_eq!(
                pixels[(16 * 32 + 16) * 4 + 3],
                0,
                "selection hole must stay transparent"
            );
        }
        assert_eq!(exported, board);
    }
}
