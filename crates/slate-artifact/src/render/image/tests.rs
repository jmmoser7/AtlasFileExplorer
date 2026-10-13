use super::*;
use slate_doc::scene::{Crop, ImageNode, ShapeNode, Stroke};
#[test]
fn compound_mask_clips_pixels_and_layers_but_stroke_follows_every_contour() {
    let mut doc = SlateDoc::new("Mask");
    let path = std::env::temp_dir().join("mask-panorama.png");
    let item = doc.add_item(path.clone(), "mask-panorama.png", 1, 1, "mask");
    let mut shape = ShapeNode {
        shape: ShapeKind::Ellipse,
        fill: None,
        stroke: Stroke {
            width: 3.0,
            ..Stroke::default()
        },
        corner: Corner::Square,
        sides: 6,
        phase_deg: 0.0,
        flip: false,
        path: None,
        text: None,
    };
    let mut node = doc.scene.build_node(
        WorldRect::new(10.0, 20.0, 100.0, 100.0),
        NodeKind::Shape(shape.clone()),
    );
    let mut clip = slate_doc::geom::shape_image_clip(&node).unwrap();
    clip.extra.push(slate_doc::scene::PathContour {
        start: [0.4, 0.4],
        segs: vec![
            PathSeg::Line { to: [0.6, 0.4] },
            PathSeg::Line { to: [0.6, 0.6] },
            PathSeg::Line { to: [0.4, 0.6] },
        ],
        closed: true,
    });
    shape.shape = ShapeKind::Path;
    let mut image = ImageNode::new(item);
    image.crop = slate_doc::geom::cover_crop(node.rect, 100.0, Crop::full());
    image.stroke = shape.stroke;
    node.kind = NodeKind::Image(image.clone());
    node.clip = Some(clip);
    node.rotation_deg = 25.0;
    node.opacity = 0.7;
    let mut assets = AssetMap::default();
    assets.insert(path, "assets/panorama.png".into());
    let mut html = String::new();
    render_image(&mut html, &doc, &assets, &node, &image, node.rect);
    assert!(html.contains("class=\"image-mask\""));
    assert!(html.contains("clip-path:path(evenodd,"));
    assert!(html.contains(" C ") && html.contains("M 40.0 40.0"));
    assert!(html.contains("width:10000.0000%;height:100.0000%;"));
    let pixels_end = html
        .find("</div><div class=\"node\"")
        .expect("mask closes before stroke wrapper");
    assert!(html[..pixels_end].contains("<img"));
    assert!(html[pixels_end..].contains("<svg"));
    assert!(html[pixels_end..].contains("fill-rule=\"evenodd\""));
    assert!(html[pixels_end..].contains("stroke-width=\""));
    assert_eq!(html.matches("rotate(25.000deg)").count(), 1);
    assert!(!html[..pixels_end].contains("border:"));
}

#[test]
fn text_card_export_uses_authored_override_while_preserving_original_link() {
    let mut doc = SlateDoc::new("Edited text");
    let path = std::env::temp_dir().join("authored-text-card.txt");
    let item = doc.add_item(path.clone(), "authored-text-card.txt", 0, 0, "text");
    let mut image = ImageNode::new(item);
    image.text_override = Some("Canvas <edit> & words".into());
    let node = doc.scene.build_node(
        WorldRect::new(0.0, 0.0, 300.0, 200.0),
        NodeKind::Image(image.clone()),
    );
    let mut assets = AssetMap::default();
    assets.insert(path.clone(), "assets/original.txt".into());
    assets.insert_snippet(path, "original excerpt".into());
    let mut html = String::new();
    render_image(&mut html, &doc, &assets, &node, &image, node.rect);
    assert!(html.contains("Canvas &lt;edit&gt; &amp; words"));
    assert!(!html.contains("original excerpt"));
    assert!(html.contains("href=\"assets/original.txt\""));
    image.text_override = Some(String::new());
    html.clear();
    render_image(&mut html, &doc, &assets, &node, &image, node.rect);
    assert!(
        html.contains("<pre></pre>"),
        "empty override is authored, not a source fallback"
    );
}
