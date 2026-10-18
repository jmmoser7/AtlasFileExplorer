//! Real board drawing through collaborator admission, witness frames, and commit.
use super::*;
use slate_doc::scene::{CmdAuthor, Node, NodeKind, PathSeg};

#[path = "motion_render_support.rs"]
mod support;
use support::Fixture;

fn drawing_arguments() -> serde_json::Value {
    json!({"strokes":[
        {"tool":"polyline","points":[[-250,-180],[250,-180],[250,200]],"width":7,"color":[30,70,110,255]},
        {"tool":"pen","points":[[-250,-50],[250,-50],[250,200]],"width":5,"opacity":0.8,"color":[150,30,40,255]},
        {"tool":"brush","points":[[-250,100],[250,100],[250,250]],"width":12,"softness":0.4,"color":[20,120,60,220]},
        {"tool":"polyline","points":[[-250,300],[250,300],[250,350]],"width":4,"color":[80,40,140,255]}
    ]})
}

fn prepare_drawing(f: &mut Fixture, sequence: u64) -> (Vec<Node>, slate_doc::Scene, usize) {
    f.h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .helper_limit = 4;
    let scene = f.h.app.doc().scene.clone();
    let depth = f.h.app.tab().journal.undo_depth();
    f.invoke(sequence, "board.paths.draw", &[], drawing_arguments());
    let nodes = f.h.app.presence.agents[0]
        .work
        .as_ref()
        .unwrap()
        .steps
        .iter()
        .filter_map(|(_, after)| after.clone())
        .collect();
    (nodes, scene, depth)
}

fn world_path_points(node: &Node) -> Vec<[f32; 2]> {
    let NodeKind::Shape(shape) = &node.kind else {
        panic!("drawing preview must remain a shape")
    };
    let path = shape.path.as_deref().expect("drawing preview path");
    let mut points = Vec::with_capacity(path.segs.len() + 1);
    let world = |point| slate_doc::geom::world_point(point, node.rect, node.rotation_deg);
    let first = world(path.start);
    points.push([first.x as f32, first.y as f32]);
    for segment in &path.segs {
        let PathSeg::Line { to } = segment else {
            panic!("partial preview must expose its traced line geometry")
        };
        let world = world(*to);
        points.push([world.x as f32, world.y as f32]);
    }
    points
}

fn near(point: [f32; 2], x: f32, y: f32) -> bool {
    (point[0] - x).abs() < 0.05 && (point[1] - y).abs() < 0.05
}

#[test]
fn drawing_shows_real_tool_geometry_helpers_then_commits_once_with_undo_redo() {
    let mut f = Fixture::new(-320.0);
    let (source_nodes, original, depth) = prepare_drawing(&mut f, 0);
    assert_eq!(source_nodes.len(), 4);
    let ids: Vec<_> = source_nodes.iter().map(|node| node.id).collect();
    assert_eq!(f.h.app.doc().scene, original);

    let mut seen = [false; 4];
    let mut partial_seen = [false; 4];
    let mut corner_seen = false;
    let mut helper_count = 0usize;
    for _ in 0..190 {
        let _out = f.step();
        if !f.pending() {
            break;
        }
        assert_eq!(
            f.h.app.doc().scene,
            original,
            "drawing stays ephemeral before commit"
        );
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
        assert_eq!(
            f.h.app.presence.agents.len(),
            1,
            "visual helpers are not provider agents"
        );
        assert_eq!(f.h.app.presence.agents[0].profile.name, "Dot");

        let sample = f.sample();
        if sample.phase != MotionPhase::Work {
            continue;
        }
        let work = f.h.app.presence.agents[0].work.as_ref().unwrap();
        let helpers: Vec<_> = work.drawing_helpers().collect();
        helper_count = helper_count.max(helpers.len());
        assert!(helpers.len() <= 4, "helper budget is enforced");

        for (index, id) in ids.iter().enumerate() {
            let Some(preview) = work.previews.get(id) else {
                continue;
            };
            let Some((_, source)) = work
                .steps
                .iter()
                .find(|(_, after)| after.as_ref().is_some_and(|node| node.id == *id))
            else {
                panic!("preview maps to its actual prepared node")
            };
            let source = source.as_ref().unwrap();
            let is_partial = preview != source;
            partial_seen[index] |= is_partial;
            let NodeKind::Shape(actual) = &preview.kind else {
                panic!("drawing preview preserves the authored shape kind")
            };
            let NodeKind::Shape(expected) = &source.kind else {
                unreachable!()
            };
            assert_eq!(actual.stroke, expected.stroke);
            assert_eq!(actual.fill, None);
            assert_eq!(preview.opacity, source.opacity);
            if index == 1 {
                assert_eq!(actual.stroke.width, 5.0);
                assert_eq!(preview.opacity, 0.8);
                seen[index] = true;
            } else if index == 2 {
                assert_eq!(actual.stroke.width, 12.0);
                assert_eq!(actual.stroke.softness, 0.4);
                assert_eq!(actual.stroke.color.0, [20, 120, 60, 220]);
                seen[index] = true;
            } else {
                seen[index] = true;
            }
            let NodeKind::Shape(shape) = &preview.kind else {
                unreachable!()
            };
            let path = shape.path.as_deref().unwrap();
            if is_partial {
                assert!(!path.closed, "partial drawing stays open until completion");
            }

            // The parent owns stroke zero. At its authored corner, the avatar
            // and visible prefix both reach the bend; the endpoint is not
            // replaced by a straight start-to-finish chord.
            if index == 0
                && near(
                    [sample.position.x as f32, sample.position.y as f32],
                    250.0,
                    -180.0,
                )
            {
                let points = world_path_points(preview);
                assert!(points.iter().any(|p| near(*p, 250.0, -180.0)));
                assert!(!points.iter().any(|p| near(*p, 250.0, 200.0)));
                assert!(points
                    .windows(2)
                    .any(|pair| { near(pair[0], -250.0, -180.0) && near(pair[1], 250.0, -180.0) }));
                corner_seen = true;
            }
        }
    }

    assert!(
        helper_count > 0,
        "complex independent strokes recruit bounded visual helpers"
    );
    assert!(
        corner_seen,
        "the parent avatar must visit the actual polyline corner"
    );
    assert!(
        seen.iter().all(|value| *value),
        "pen, brush, and polyline previews appear"
    );
    assert!(
        partial_seen.iter().all(|value| *value),
        "each stroke shows partial geometry before it completes"
    );
    assert!(
        !f.pending(),
        "the witnessed drawing completes within its bounded run"
    );
    let committed = f.h.app.doc().scene.clone();
    for node in &source_nodes {
        assert_eq!(committed.node(node.id), Some(node));
    }
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        f.h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    {
        let tab = f.h.app.tab_mut();
        assert!(tab.journal.undo(&mut tab.doc.scene));
        assert_eq!(tab.doc.scene.nodes, original.nodes);
        assert!(tab.journal.redo(&mut tab.doc.scene));
        assert_eq!(tab.doc.scene.nodes, committed.nodes);
    }
}

#[test]
fn cancelling_partial_drawing_leaves_scene_and_journal_untouched() {
    let mut f = Fixture::new(-320.0);
    let (_, original, depth) = prepare_drawing(&mut f, 0);
    let mut canceled = false;
    for _ in 0..40 {
        let _ = f.step();
        if f.h.app.presence.agents[0]
            .work
            .as_ref()
            .is_some_and(|work| !work.previews.is_empty())
        {
            let position = f.sample().position;
            f.h.app.stop_presence(&f.who, false).unwrap();
            let _ = f.step();
            assert_eq!(f.sample().position, position);
            assert!(!f.pending());
            assert_eq!(f.h.app.doc().scene, original);
            assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
            canceled = true;
            break;
        }
    }
    assert!(
        canceled,
        "cancellation should occur after a visible partial result exists"
    );
}

#[test]
#[ignore = "retains actual production board-render frames; native/GPU acceptance is separate"]
fn drawing_partial_geometry_renders_early_mid_late_at_multiple_zooms() {
    for zoom in [0.5, 1.0, 2.0] {
        let mut f = Fixture::new(-320.0);
        f.set_zoom(zoom);
        let _ = f.step();
        let _ = prepare_drawing(&mut f, 0);
        let (mut early, mut mid, mut late) = (false, false, false);
        for _ in 0..190 {
            let out = f.step();
            if !f.pending() {
                break;
            }
            let sample = f.sample();
            if sample.phase != MotionPhase::Work {
                continue;
            }
            if !early && sample.progress < 0.35 {
                early = true;
                f.save("early", out);
            } else if !mid && (0.4..0.7).contains(&sample.progress) {
                mid = true;
                f.save("mid", out);
            } else if !late && sample.progress > 0.75 {
                late = true;
                f.save("late", out);
            }
        }
        assert!(
            early && mid && late,
            "zoom {zoom} has three retained partial frames"
        );
        f.finish();
    }
}
