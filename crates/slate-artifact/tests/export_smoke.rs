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
                ..Default::default()
            },
            adjust: ImageAdjust {
                brightness: 1.1,
                grayscale: 0.5,
                overlay: Some(Rgba([255, 0, 0, 60])),
                ..Default::default()
            },
            video: Default::default(),
            model: Default::default(),
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
                ..Default::default()
            },
            corner: Corner::Chamfer { cut: 20.0 },
            flip: false,
            path: None,
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

/// 3D model nodes export their frozen-camera poster (per node — the same
/// model placed twice can show two perspectives); nodes without a rendered
/// poster fall back to a labeled card. Both link to the copied original.
#[test]
fn model_nodes_export_per_node_posters() {
    let mut doc = SlateDoc::new("Model Deck");
    let dir = std::env::temp_dir().join("slate-smoke-3dm");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let model_path = dir.join("tower.3dm");
    std::fs::write(&model_path, b"3D Geometry File Format fake").unwrap();
    let poster_path = dir.join("poster-a.png");
    std::fs::write(&poster_path, b"\x89PNG fake poster").unwrap();
    let item = doc.add_item(model_path, "tower.3dm", 28, 1, "modelkey");

    // Two placements of the same model = two saved perspectives.
    let mut node_ids = Vec::new();
    for (x, yaw) in [(0.0f32, 0.2f32), (300.0, 1.4)] {
        let mut img = ImageNode::new(item);
        img.model.yaw = yaw;
        let node = doc
            .scene
            .build_node(WorldRect::new(x, 0.0, 240.0, 180.0), NodeKind::Image(img));
        node_ids.push(node.id);
        let i = doc.scene.nodes.len();
        doc.scene.apply(&SceneCmd::Add { index: i, node });
    }

    // Only the first node has a rendered poster.
    let mut opts = slate_artifact::ExportOptions::default();
    opts.model_posters.insert(node_ids[0], poster_path);

    let out = dir.join("out");
    slate_artifact::export_html(&doc, &out, &opts).unwrap();
    let html = std::fs::read_to_string(out.join("index.html")).unwrap();

    // Node A: poster-backed card with the 3DM badge.
    assert!(
        html.contains("poster-a"),
        "poster asset referenced:\n{html}"
    );
    assert!(html.contains("3DM"), "extension badge present");
    // Node B: no poster → labeled file card, still linking to the model.
    assert!(
        html.contains("class=\"filecard\""),
        "fallback card:\n{html}"
    );
    assert!(html.contains("tower-"), "original copied and linked");
    // Poster copied into the assets dir.
    let assets: Vec<_> = std::fs::read_dir(out.join("assets"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        assets.iter().any(|n| n.starts_with("poster-a")),
        "{assets:?}"
    );
    assert!(assets.iter().any(|n| n.starts_with("tower-")), "{assets:?}");
}
