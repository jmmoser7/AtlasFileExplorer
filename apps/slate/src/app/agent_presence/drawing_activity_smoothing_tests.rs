use super::*;
use slate_doc::scene::{PathData, PathSeg, Rgba, ShapeKind, ShapeNode, Stroke, WorldRect};
use std::sync::Arc;

fn add_smoothable_path(f: &mut Fixture) -> Node {
    let mut path = PathData::default();
    path.start = [0.0, 0.5];
    path.segs = vec![
        PathSeg::Line { to: [0.25, 0.32] },
        PathSeg::Line { to: [0.5, 0.68] },
        PathSeg::Line { to: [0.75, 0.32] },
        PathSeg::Line { to: [1.0, 0.5] },
    ];
    let shape = ShapeNode {
        shape: ShapeKind::Path,
        fill: None,
        stroke: Stroke {
            width: 8.0,
            color: Rgba::opaque(30, 70, 110),
            ..Stroke::default()
        },
        corner: Default::default(),
        sides: 5,
        phase_deg: 0.0,
        flip: false,
        path: Some(Arc::new(path)),
        text: None,
    };
    let node = f.h.app.doc_mut().scene.build_node(
        WorldRect::new(-250.0, -100.0, 500.0, 200.0),
        NodeKind::Shape(shape),
    );
    f.h.app.add_nodes(vec![node.clone()]);
    node
}

fn smooth_args() -> serde_json::Value {
    json!({"points":[[-180.0,0.0],[0.0,0.0],[180.0,0.0]],"width":100.0,"strength":0.8})
}

#[test]
fn smooth_witnesses_only_reached_sweep_then_commits_exactly_once() {
    let mut f = Fixture::new(-600.0);
    let source = add_smoothable_path(&mut f);
    let original = f.h.app.doc().scene.clone();
    let depth = f.h.app.tab().journal.undo_depth();
    let expected = crate::app::board_smooth::prepare_smooth(
        std::slice::from_ref(&source),
        &[
            egui::pos2(-180.0, 0.0),
            egui::pos2(0.0, 0.0),
            egui::pos2(180.0, 0.0),
        ],
        100.0,
        0.8,
        f.h.app.tab().cam.z,
    )
    .unwrap();
    assert_eq!(
        expected.len(),
        1,
        "the full route reaches the authored path"
    );

    f.invoke(0, "board.smooth.stroke", &[source.id], smooth_args());
    let mut partial = false;
    for _ in 0..190 {
        let _ = f.step();
        if !f.pending() {
            break;
        }
        assert_eq!(
            f.h.app.doc().scene,
            original,
            "smooth previews are ephemeral"
        );
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
        let work = f.h.app.presence.agents[0].work.as_ref().unwrap();
        if let Some(preview) = work.previews.get(&source.id) {
            assert_ne!(preview, &source, "a witnessed prefix has reached the path");
            partial = true;
        }
    }
    assert!(
        partial,
        "the actual smooth sweep must be visible before commit"
    );
    assert!(!f.pending(), "the finite smooth route completes");
    let committed = f.h.app.doc().scene.node(source.id).unwrap().clone();
    assert_eq!(committed, expected[0]);
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        f.h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    {
        let tab = f.h.app.tab_mut();
        assert!(tab.journal.undo(&mut tab.doc.scene));
        assert_eq!(tab.doc.scene.node(source.id), Some(&source));
        assert!(tab.journal.redo(&mut tab.doc.scene));
        assert_eq!(tab.doc.scene.node(source.id), Some(&committed));
    }
}

#[test]
fn cancelling_partial_smooth_keeps_the_source_and_journal_unchanged() {
    let mut f = Fixture::new(-600.0);
    let source = add_smoothable_path(&mut f);
    let original = f.h.app.doc().scene.clone();
    let depth = f.h.app.tab().journal.undo_depth();
    f.invoke(0, "board.smooth.stroke", &[source.id], smooth_args());
    let mut canceled = false;
    for _ in 0..80 {
        let _ = f.step();
        let previewed = f.h.app.presence.agents[0]
            .work
            .as_ref()
            .is_some_and(|work| work.previews.contains_key(&source.id));
        if previewed {
            f.h.app.stop_presence(&f.who, false).unwrap();
            let _ = f.step();
            assert!(!f.pending());
            assert_eq!(f.h.app.doc().scene, original);
            assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
            canceled = true;
            break;
        }
    }
    assert!(canceled, "cancel is tested after a visible prefix exists");
}

#[test]
fn witnessed_arc_and_parametric_shape_prefixes_keep_exact_recipe_results() {
    let mut f = Fixture::new(-600.0);
    let original = f.h.app.doc().scene.clone();
    let depth = f.h.app.tab().journal.undo_depth();
    f.invoke(
        0,
        "board.paths.draw",
        &[],
        json!({"strokes":[
            {"tool":"arc","points":[[-250,-250],[250,-250],[0,-80]],"width":4},
            {"tool":"rect","points":[[-250,-40],[-100,90]],"width":4},
            {"tool":"ellipse","points":[[-50,-40],[100,90]],"width":4},
            {"tool":"polygon","points":[[150,-40],[300,90]],"sides":6,"width":4}
        ]}),
    );
    let work = f.h.app.presence.agents[0].work.as_ref().unwrap();
    let expected: Vec<_> = work
        .steps
        .iter()
        .filter_map(|(_, after)| after.clone())
        .collect();
    assert_eq!(expected.len(), 4);
    let ids: Vec<_> = expected.iter().map(|node| node.id).collect();
    let kinds: Vec<_> = expected
        .iter()
        .map(|node| match &node.kind {
            NodeKind::Shape(shape) => shape.shape,
            _ => panic!("recipe result is a shape"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ShapeKind::Path,
            ShapeKind::Rect,
            ShapeKind::Ellipse,
            ShapeKind::RegularPolygon
        ]
    );
    assert_eq!(f.h.app.doc().scene, original);
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth);

    let mut partial_by_node = [false; 4];
    for _ in 0..190 {
        let _ = f.step();
        if !f.pending() {
            break;
        }
        assert_eq!(f.h.app.doc().scene, original);
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
        let work = f.h.app.presence.agents[0].work.as_ref().unwrap();
        for (index, id) in ids.iter().enumerate() {
            let Some(preview) = work.previews.get(id) else {
                continue;
            };
            partial_by_node[index] |= preview != &expected[index];
            if preview == &expected[index] {
                continue;
            }
            let NodeKind::Shape(shape) = &preview.kind else {
                panic!("swept outline remains a shape")
            };
            assert_eq!(shape.shape, ShapeKind::Path);
            assert!(shape.fill.is_none());
            assert!(!shape.path.as_ref().unwrap().closed);
        }
    }
    assert!(!f.pending(), "bounded shape drawing completes");
    assert!(partial_by_node.iter().all(|partial| *partial));
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
    for expected_node in &expected {
        assert_eq!(
            f.h.app.doc().scene.node(expected_node.id),
            Some(expected_node)
        );
    }
}
