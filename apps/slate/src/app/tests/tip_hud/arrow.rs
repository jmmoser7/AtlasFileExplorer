//! An exported open curve draws its arrowhead.

use super::*;

#[test]
fn an_exported_open_curve_draws_its_arrowhead() {
    let mut doc = slate_doc::SlateDoc::new("Arrow");
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 10.0);
    let mut stroke = board_path::default_curve_stroke(slate_doc::scene::Rgba::BLACK);
    stroke.arrow_end = true;
    let node = doc.scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Path,
            fill: None,
            stroke,
            corner: slate_doc::scene::Corner::Square,
            phase_deg: 0.0,
            flip: false,
            path: Some(
                slate_doc::scene::PathData {
                    start: [0.0, 0.5],
                    segs: vec![slate_doc::scene::PathSeg::Line { to: [1.0, 0.5] }],
                    ..Default::default()
                }
                .into(),
            ),
            text: None,
            sides: slate_doc::scene::default_regular_sides(),
        }),
    );
    let index = doc.scene.nodes.len();
    doc.scene
        .apply(&slate_doc::scene::SceneCmd::Add { index, node });
    let html = slate_artifact::render_html(&doc, &slate_artifact::AssetMap::default());
    assert!(
        html.contains("M 100.0 5.0 L"),
        "arrow tip at the end:\n{html}"
    );
}
