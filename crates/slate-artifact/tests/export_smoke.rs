//! End-to-end export smoke test: one slide exercising every style feature
//! (crop, rounded + chamfered corners, dashed stroke, filters, overlay,
//! serif text with escaping) through the public `export_html` API.

use slate_doc::scene::*;
use slate_doc::SlateDoc;
use std::collections::BTreeMap;

#[test]
fn smoke_full_feature_export() {
    let mut doc = SlateDoc::new("Smoke Deck");
    let dir = std::env::temp_dir().join("slate-smoke");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let img_path = dir.join("photo.png");
    std::fs::write(&img_path, b"\x89PNG fake").unwrap();
    let item = doc.add_item(img_path, "photo.png", 9, 1, "key");

    let frame = doc.scene.build_node(
        WorldRect::new(0.0, 0.0, 960.0, 540.0),
        NodeKind::Frame(FrameNode {
            title: "Intro".into(),
            order: 0,
            fill: Rgba::WHITE,
            assignments: BTreeMap::new(),
        }),
    );
    let fidx = doc.scene.nodes.len();
    doc.scene.apply(&SceneCmd::Add {
        index: fidx,
        node: frame,
    });

    let image = doc.scene.build_node(
        WorldRect::new(40.0, 60.0, 400.0, 300.0),
        NodeKind::Image(ImageNode {
            item,
            crop: Crop {
                x: 0.1,
                y: 0.1,
                w: 0.8,
                h: 0.8,
            },
            corner: Corner::Rounded { radius: 16.0 },
            stroke: Stroke {
                width: 3.0,
                color: Rgba::opaque(20, 20, 20),
                dash: Dash::Solid,
            },
            adjust: ImageAdjust {
                brightness: 1.1,
                grayscale: 0.5,
                overlay: Some(Rgba([255, 0, 0, 60])),
                ..Default::default()
            },
        }),
    );
    let text = doc.scene.build_node(
        WorldRect::new(480.0, 80.0, 400.0, 120.0),
        NodeKind::Text(TextNode {
            text: "Hello <world> & friends".into(),
            family: FontChoice::Serif,
            size: 36.0,
            color: Rgba::opaque(10, 10, 40),
            align: TextAlign::Center,
        }),
    );
    let shape = doc.scene.build_node(
        WorldRect::new(480.0, 260.0, 300.0, 140.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: Some(Rgba([100, 180, 255, 120])),
            stroke: Stroke {
                width: 2.0,
                color: Rgba::opaque(0, 90, 200),
                dash: Dash::Dashed,
            },
            corner: Corner::Chamfer { cut: 20.0 },
            flip: false,
        }),
    );
    for n in [image, text, shape] {
        let i = doc.scene.nodes.len();
        doc.scene.apply(&SceneCmd::Add { index: i, node: n });
    }

    let out = dir.join("out");
    let rep =
        slate_artifact::export_html(&doc, &out, &slate_artifact::ExportOptions::default()).unwrap();
    assert_eq!(rep.slides, 1);
    let html = std::fs::read_to_string(out.join("index.html")).unwrap();
    println!("=== HTML ===\n{html}");
}
