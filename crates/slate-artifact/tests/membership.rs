//! Membership determinism at the export boundary: overlapping frames
//! partition their content, so a node is serialized onto exactly one slide.

use slate_doc::scene::*;
use slate_doc::SlateDoc;
use std::collections::BTreeMap;

fn add_frame(doc: &mut SlateDoc, rect: WorldRect, title: &str, order: u32) -> NodeId {
    let node = doc.scene.build_node(
        rect,
        NodeKind::Frame(FrameNode {
            title: title.into(),
            order,
            fill: Rgba::WHITE,
            assignments: BTreeMap::new(),
        }),
    );
    let id = node.id;
    let index = doc.scene.nodes.len();
    doc.scene.apply(&SceneCmd::Add { index, node });
    id
}

#[test]
fn overlapping_frames_do_not_duplicate_a_slide_member() {
    const MARKER: &str = "OverlapMarker";

    let mut doc = SlateDoc::new("Overlap Deck");
    let under = add_frame(&mut doc, WorldRect::new(0.0, 0.0, 960.0, 540.0), "Under", 0);
    let over = add_frame(
        &mut doc,
        WorldRect::new(480.0, 0.0, 960.0, 540.0),
        "Over",
        1,
    );

    // Centre (700, 270) lies inside both frames.
    let text = doc.scene.build_node(
        WorldRect::new(600.0, 210.0, 200.0, 120.0),
        NodeKind::Text(TextNode {
            text: MARKER.into(),
            family: FontChoice::Sans,
            size: 32.0,
            color: Rgba::opaque(10, 10, 10),
            align: TextAlign::Center,
            fill: None,
        }),
    );
    let text_id = text.id;
    let index = doc.scene.nodes.len();
    doc.scene.apply(&SceneCmd::Add { index, node: text });

    assert_eq!(
        doc.scene.frame_of(text_id),
        Some(over),
        "topmost frame owns"
    );
    assert!(doc.scene.members_of(under).is_empty());

    let dir = std::env::temp_dir().join("slate-overlap-membership");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out");
    let rep =
        slate_artifact::export_html(&doc, &out, &slate_artifact::ExportOptions::default()).unwrap();
    assert_eq!(rep.slides, 2, "both frames are slides");

    let html = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert_eq!(
        html.matches(MARKER).count(),
        1,
        "member appears on one slide only:\n{html}"
    );
}
