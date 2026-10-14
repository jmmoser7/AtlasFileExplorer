//! Shape-hosted text survives both paint-layer serialization and rasterization.
use slate_doc::image_paint::{PaintLayer, PaintLayerId};
use slate_doc::scene::*;
use slate_doc::ItemId;

#[test]
fn hosted_shape_emphasis_exports_escaped_words_and_matching_raster_pixels() {
    let mut scene = Scene::default();
    let mut host = scene.build_node(
        WorldRect::new(0.0, 0.0, 160.0, 160.0),
        NodeKind::Image(ImageNode::new(ItemId(1))),
    );
    let mut child = scene.build_node(
        WorldRect::new(0.1, 0.1, 0.8, 0.8),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: None,
            stroke: Default::default(),
            corner: Default::default(),
            sides: 6,
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: Some(ShapeText {
                body: "Ink <&>".into(),
                family: Typeface::Sans,
                size: 24.0,
                color: Rgba::BLACK,
                align: TextAlign::Center,
                style: TextStyle {
                    bold: true,
                    italic: true,
                    underline: true,
                },
            }),
        }),
    );
    child.rotation_deg = 25.0;
    child.opacity = 0.6;
    let NodeKind::Image(image) = &mut host.kind else {
        unreachable!()
    };
    let mut layer = PaintLayer::new(PaintLayerId(1));
    layer.opacity = 0.75;
    layer.nodes.push(child);
    image.paint_layers.push(layer);
    let image = image.clone();
    let svg = slate_artifact::paint_layers_svg(&host, &image, 160, 160);
    assert!(svg.contains("Ink &lt;&amp;&gt;"));
    assert!(svg.contains("font-weight:bold;font-style:italic;text-decoration:underline;"));
    let serialized = slate_artifact::rasterize_paint_layers_svg(&svg, 160, 160).unwrap();
    assert!(
        serialized
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] != 0),
        "transparent geometry leaves only the hosted words to produce pixels"
    );
    let direct = slate_artifact::rasterize_paint_layers(
        &host,
        &image,
        160,
        160,
        None,
        &mut slate_artifact::LayerStamps::default(),
    )
    .unwrap();
    assert!(
        serialized
            .iter()
            .zip(&direct)
            .all(|(a, b)| a.abs_diff(*b) <= 1),
        "same glyph pixels; separate layer-opacity compositing permits one alpha rounding unit"
    );
}
