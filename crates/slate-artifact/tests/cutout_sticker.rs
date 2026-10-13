//! A cut-out sticker is an image node with a crop and an even-odd clip path.
//! The export must clip with that exact path, holes included, over the
//! same crop window the board painter samples.

use slate_artifact::{render_html, AssetMap};
use slate_doc::scene::*;
use slate_doc::SlateDoc;
use std::collections::BTreeMap;

fn lines(points: &[[f32; 2]]) -> Vec<PathSeg> {
    points.iter().map(|&to| PathSeg::Line { to }).collect()
}

#[test]
fn sticker_exports_its_crop_and_even_odd_clip() {
    let mut doc = SlateDoc::new("Sticker");
    let path = std::env::temp_dir().join("slate-sticker-photo.png");
    let item = doc.add_item(path.clone(), "photo.png", 9, 1, "key");
    let frame = doc.scene.build_node(
        WorldRect::new(0.0, 0.0, 960.0, 540.0),
        NodeKind::Frame(FrameNode {
            title: "Cut".into(),
            order: 0,
            fill: Rgba::WHITE,
            fill_authored: false,
            assignments: BTreeMap::new(),
            stroke: Stroke::none(),
            corner: Corner::Square,
        }),
    );
    doc.scene.apply(&SceneCmd::Add {
        index: 0,
        node: frame,
    });
    let mut img = ImageNode::new(item);
    img.crop = Crop {
        x: 0.2,
        y: 0.25,
        w: 0.5,
        h: 0.4,
    };
    let mut node = doc.scene.build_node(
        WorldRect::new(100.0, 100.0, 200.0, 100.0),
        NodeKind::Image(img),
    );
    node.clip = Some(PathData {
        start: [0.0, 0.0],
        segs: lines(&[[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
        closed: true,
        extra: vec![PathContour {
            start: [0.25, 0.25],
            segs: lines(&[[0.5, 0.25], [0.5, 0.5]]),
            closed: true,
        }],
        fill_rule: PathFillRule::EvenOdd,
        ..Default::default()
    });
    doc.scene.apply(&SceneCmd::Add { index: 1, node });
    let mut assets = AssetMap::default();
    assets.insert(path, "assets/photo.png".into());
    let html = render_html(&doc, &assets);

    assert!(
        html.contains(
            "clip-path:path(evenodd, 'M 0.0 0.0 L 200.0 0.0 L 200.0 100.0 L 0.0 100.0 Z \
             M 50.0 25.0 L 100.0 25.0 L 100.0 50.0 Z');clip-rule:evenodd;"
        ),
        "the node-local clip scales to the node, hole as its own subpath: {html}"
    );
    // Crop window 0.5 × 0.4 at (0.2, 0.25): the full image is 200% × 250%.
    assert!(
        html.contains("width:200.0000%;height:250.0000%;left:-40.0000%;top:-62.5000%;"),
        "{html}"
    );
}
