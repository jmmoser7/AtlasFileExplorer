//! Real board drawing through collaborator admission, witness frames, and commit.
use super::*;
use slate_doc::scene::{CmdAuthor, Node, NodeKind, PathSeg};

use super::motion_support::Fixture;

fn drawing_arguments() -> serde_json::Value {
    json!({"strokes":[
        {"tool":"polyline","points":[[-250,-180],[250,-180],[250,200]],"width":7,"color":[30,70,110,255]},
        {"tool":"pen","points":[[-250,-50],[250,-50],[250,200]],"width":5,"opacity":0.8,"color":[150,30,40,255]},
        {"tool":"brush","points":[[-250,100],[250,100],[250,250]],"width":12,"softness":0.4,"color":[20,120,60,220]},
        {"tool":"polyline","points":[[-250,300],[250,300],[250,350]],"width":4,"color":[80,40,140,255]}
    ]})
}

fn prepare_drawing(f: &mut Fixture, sequence: u64) -> (Vec<Node>, slate_doc::Scene, usize) {
    f.h.app.presence.agents[0].profile.hat = "cap".into();
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

fn assert_visible_brush_prefix(f: &Fixture) {
    let work = f.h.app.presence.agents[0].work.as_ref().unwrap();
    let brush = work
        .previews
        .values()
        .find(|node| matches!(&node.kind, NodeKind::Shape(shape) if shape.stroke.stamp))
        .expect("the brush helper has begun drawing");
    assert_eq!(
        f.h.app.agent_brush_coverage(brush.id, None),
        Some(BrushCoverage::Active),
        "a visible helper stroke must use its incremental live canvas"
    );
    let points = world_path_points(brush);
    let [start, tip, ..] = points.as_slice() else {
        panic!("the brush must have a visible traced segment")
    };
    assert!(tip[0] - start[0] > 40.0);
    // Halfway along the traced segment is clear of the marble at its tip.
    // A stale initial raster dab cannot satisfy this pixel assertion.
    let pixel = f.pixel(egui::pos2((start[0] + tip[0]) * 0.5, start[1]));
    assert!(
        pixel[1] > pixel[0] + 0.10 && pixel[1] > pixel[2] + 0.05,
        "the actual green brush ink must follow its helper: {pixel:?}; {}",
        f.h.app.drawing_brush_debug(brush.id)
    );
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
            assert!(
                f.h.app.presence.agents[0]
                    .work
                    .as_ref()
                    .unwrap()
                    .previews
                    .is_empty(),
                "ink cannot appear before the agent reaches its contact"
            );
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
fn translucent_brush_keeps_its_ink_and_opacity_through_the_exact_raster_handoff() {
    brush_handoff_opacity(false);
}

#[test]
fn live_brush_keeps_search_dimming_through_the_exact_raster_handoff() {
    brush_handoff_opacity(true);
}

fn brush_handoff_opacity(dimmed: bool) {
    // Keep the source image clear of the sampled stroke: this measures ink
    // opacity against the board, rather than a blend over the blue fixture.
    let mut f = Fixture::new(-600.0);
    f.h.app.search.open = dimmed;
    f.h.app.search.query = "target".into();
    f.invoke(
        0,
        "board.paths.draw",
        &[],
        json!({"strokes":[{
            "tool":"brush","points":[[-250,100],[250,100],[250,250]],
            "width":12,"softness":0.4,"opacity":0.35,"color":[20,120,60,220]
        }]}),
    );
    let source = f.h.app.presence.agents[0].work.as_ref().unwrap().steps[0]
        .1
        .as_ref()
        .unwrap()
        .clone();
    let mut reference: Option<[f32; 4]> = None;
    let mut handed_off = false;
    for _ in 0..250 {
        let out = f.step();
        let pending = f.pending();
        let sample = f.sample();
        if pending && (sample.phase != MotionPhase::Work || sample.progress < 0.45) {
            continue;
        }
        if pending && reference.is_some() && sample.progress < 0.95 {
            continue;
        }
        // Deliberately rasterize this output without the usual helper's worker
        // drain: a one-frame blank or double-painted handoff must remain visible.
        f.draw_exact(out);
        let pixel = f.pixel(egui::pos2(-220.0, 100.0));
        let (red_floor, green_delta) = if dimmed { (0.85, 0.015) } else { (0.60, 0.06) };
        assert!(
            pixel[0] > red_floor && pixel[0] < 0.98,
            "opacity: {pixel:?}"
        );
        assert!(
            pixel[1] > pixel[0] + green_delta && pixel[1] > pixel[2] + green_delta * 0.5,
            "brush ink cannot disappear between live and canonical paint: {pixel:?}"
        );
        if let Some(first) = reference {
            assert!(
                pixel[..3]
                    .iter()
                    .zip(first)
                    .all(|(a, b)| (a - b).abs() < 0.07),
                "handoff changes opacity: {first:?} -> {pixel:?}"
            );
        } else {
            reference = Some(pixel);
        }
        if !pending && f.h.app.agent_brush_coverage(source.id, None).is_none() {
            assert_eq!(f.h.app.doc().scene.node(source.id), Some(&source));
            handed_off = true;
            break;
        }
        std::thread::yield_now();
    }
    assert!(
        reference.is_some() && handed_off,
        "the exact final raster must take over"
    );
}

#[test]
fn a_live_brush_stays_below_a_later_pen_in_the_same_drawing() {
    let mut f = Fixture::new(-600.0);
    f.h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .helper_limit = 2;
    f.invoke(
        0,
        "board.paths.draw",
        &[],
        json!({"strokes":[
            {"tool":"brush","points":[[-250,100],[250,100]],
                "width":12,"softness":0.4,"color":[20,120,60,255]},
            {"tool":"pen","points":[[-250,100],[250,100]],
                "width":20,"color":[180,30,40,255]}
        ]}),
    );
    let mut checked = false;
    for _ in 0..190 {
        let out = f.step();
        if !f.pending() {
            break;
        }
        let sample = f.sample();
        if sample.phase != MotionPhase::Work || sample.progress < 0.65 {
            continue;
        }
        let work = f.h.app.presence.agents[0].work.as_ref().unwrap();
        let pen = work
            .previews
            .values()
            .find(|node| matches!(&node.kind, NodeKind::Shape(shape) if !shape.stroke.stamp));
        if !pen.is_some_and(|node| world_path_points(node).last().unwrap()[0] > -100.0) {
            continue;
        }
        let expected: Vec<_> = work
            .steps
            .iter()
            .filter_map(|(_, after)| after.as_ref())
            .filter(|node| work.previews.contains_key(&node.id))
            .map(|node| node.id)
            .collect();
        let actual: Vec<_> =
            f.h.app
                .presence_preview_additions(f.h.app.canvas_rect)
                .iter()
                .map(|node| node.id)
                .collect();
        assert_eq!(
            actual, expected,
            "preview order must match the eventual commit"
        );
        f.draw_exact(out);
        let pixel = f.pixel(egui::pos2(-200.0, 100.0));
        assert!(
            pixel[0] > pixel[1] + 0.25 && pixel[0] > pixel[2] + 0.25,
            "the later red pen must cover the earlier live green brush: {pixel:?}"
        );
        checked = true;
        break;
    }
    assert!(checked, "overlapping strokes must be checked before commit");
}

#[test]
fn a_human_creation_conflict_removes_only_the_preview_and_keeps_the_chat_running() {
    let mut f = Fixture::new(-320.0);
    let (nodes, original, depth) = prepare_drawing(&mut f, 0);
    let mut saw_preview = false;
    for _ in 0..60 {
        let _ = f.step();
        if f.h.app.presence.agents[0]
            .work
            .as_ref()
            .is_some_and(|w| !w.previews.is_empty())
        {
            saw_preview = true;
            break;
        }
    }
    assert!(saw_preview && f.pending());
    f.h.app.presence.agents[0].inflight = true;
    let mut human = nodes[0].clone();
    human.rect.x += 80.0;
    f.h.app.add_nodes(vec![human.clone()]);
    let _ = f.step();
    assert!(
        !f.pending(),
        "a conflicting preview must be canceled before it paints over the human object"
    );
    assert!(
        f.h.app.presence.agents[0].inflight,
        "a tool conflict must not terminate the provider conversation"
    );
    assert_eq!(f.h.app.doc().scene.node(human.id), Some(&human));
    assert_eq!(f.h.app.doc().scene.nodes.len(), original.nodes.len() + 1);
    for node in &original.nodes {
        assert_eq!(f.h.app.doc().scene.node(node.id), Some(node));
    }
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
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
                assert_visible_brush_prefix(&f);
            } else if !late && sample.progress > 0.75 {
                late = true;
                f.save("late", out);
                assert_visible_brush_prefix(&f);
            }
        }
        assert!(
            early && mid && late,
            "zoom {zoom} has three retained partial frames"
        );
        f.finish();
    }
}
