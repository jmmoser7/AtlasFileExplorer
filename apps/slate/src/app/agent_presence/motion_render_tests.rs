//! Actual linked-image and wire motion through command admission and app paint.
use super::*;
use slate_doc::scene::{CmdAuthor, ConnectorEnd, NodeKind, Side};

#[path = "motion_render_support.rs"]
mod support;
use support::Fixture;

#[path = "crop_render_tests.rs"]
mod crop;

#[path = "approved_render_tests.rs"]
mod approved;

fn move_image(f: &mut Fixture, dx: f32, dy: f32) {
    let source = f.h.app.doc().scene.node(f.images[0]).unwrap().clone();
    let scene = f.h.app.doc().scene.clone();
    let depth = f.h.app.tab().journal.undo_depth();
    f.invoke(
        0,
        "board.objects.move",
        &[source.id],
        json!({"dx":dx,"dy":dy}),
    );
    let mut positions = Vec::new();
    let mut visible_positions = 0;
    let (mut travel, mut grab, mut early, mut late) = (false, false, false, false);
    for _ in 0..80 {
        let out = f.step();
        if !f.pending() {
            f.save("move-committed", out);
            break;
        }
        assert_eq!(f.h.app.doc().scene, scene);
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
        match f.sample().phase {
            MotionPhase::Travel if !travel => {
                travel = true;
                f.save("approach", out);
            }
            MotionPhase::Grab if !grab => {
                grab = true;
                let (x, y) = source.rect.center();
                assert!((f.sample().position.x - f64::from(x)).abs() < 0.01);
                assert!((f.sample().position.y - f64::from(y)).abs() < 0.01);
                f.save("contact", out);
            }
            MotionPhase::Work => {
                assert!(travel && grab, "approach and contact must precede movement");
                let preview = f.h.app.presence_preview(source.id).unwrap().clone();
                assert_eq!(preview.id, source.id);
                assert_eq!(preview.kind, source.kind, "the same linked item moves");
                if preview.rect.x == source.rect.x {
                    continue;
                }
                assert!(preview.rect.x > source.rect.x && preview.rect.x < source.rect.x + dx);
                if let Some(previous) = positions.last() {
                    assert!(preview.rect.x > *previous);
                }
                positions.push(preview.rect.x);
                let screen = f.h.app.board_xf().rect_w2s(preview.rect);
                if f.h.app.canvas_rect.contains_rect(screen) {
                    f.draw(&out);
                    assert_eq!(f.blue(&preview), Some(true));
                    visible_positions += 1;
                    let progress = (preview.rect.x - source.rect.x) / dx;
                    if !early && progress < 0.65 {
                        early = true;
                        f.save("drag-early", out);
                        assert_eq!(f.blue(&preview), Some(true));
                    } else if !late && progress > 0.7 {
                        late = true;
                        f.save("drag-late", out);
                        assert_eq!(f.blue(&preview), Some(true));
                        if f.blue(&source).is_some() {
                            assert_eq!(f.blue(&source), Some(false));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    assert!(!f.pending(), "a bounded witnessed move completes");
    assert!(travel && grab && early && late && positions.len() >= 3 && visible_positions >= 3);
    f.record("move-samples", json!({"sameNodeId":source.id,"monotoneWorkX":positions,
        "visibleImagePixelSamples":visible_positions,"approachBeforeGrabBeforeWork":travel && grab}));
    let after = f.h.app.doc().scene.node(source.id).unwrap();
    assert_eq!(after.rect.x, source.rect.x + dx);
    assert_eq!(after.rect.y, source.rect.y + dy);
    assert_eq!(after.kind, source.kind);
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        f.h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    assert_eq!(f.blue(after), Some(true));
}

#[test]
#[ignore = "retains real command-motion software frames; native/GPU acceptance is separate"]
fn witnessed_linked_image_wire_and_cancellation_retain_actual_motion_frames() {
    let mut f = Fixture::new(-320.0);
    let out = f.step();
    f.save("before", out);
    let original = f.h.app.doc().scene.clone();
    let items = f.h.app.doc().items.clone();
    move_image(&mut f, 240.0, 100.0);
    let moved = f.h.app.doc().scene.clone();
    let depth = f.h.app.tab().journal.undo_depth();
    let targets = f.images;
    f.invoke(1, "board.objects.wire", &targets, json!({}));
    let wire = f.h.app.presence.agents[0]
        .work
        .as_ref()
        .unwrap()
        .prepared
        .created_ids()[0];
    let mut free_points = Vec::<[f32; 2]>::new();
    let (mut wire_early, mut wire_late) = (false, false);
    for _ in 0..100 {
        let out = f.step();
        if !f.pending() {
            f.save("wire-attached", out);
            break;
        }
        assert_eq!(f.h.app.doc().scene, moved);
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
        assert!(f.h.app.doc().scene.node(wire).is_none());
        if let Some(preview) = f.h.app.presence_preview(wire) {
            let NodeKind::Connector(connector) = &preview.kind else {
                panic!("real connector preview");
            };
            if let ConnectorEnd::Free { point } = connector.b {
                let sample = f.sample();
                assert!((f64::from(point[0]) - sample.position.x).abs() < 0.01);
                assert!((f64::from(point[1]) - sample.position.y).abs() < 0.01);
                assert!(
                    matches!(connector.a, ConnectorEnd::Anchored { node, .. } if node == targets[0])
                );
                if sample.progress > 0.05 {
                    if let Some(previous) = free_points.last() {
                        assert!(
                            (egui::Pos2::from(point) - egui::Pos2::from(*previous)).length() > 0.5
                        );
                    }
                    free_points.push(point);
                    if !wire_early && sample.progress < 0.6 {
                        wire_early = true;
                        f.save("wire-free-early", out);
                    } else if !wire_late && sample.progress > 0.7 {
                        wire_late = true;
                        f.save("wire-free-late", out);
                    }
                }
            }
        }
    }
    assert!(wire_early && wire_late && free_points.len() >= 3 && !f.pending());
    f.record(
        "wire-samples",
        json!({"sameWireId":wire,"freePoints":free_points,
        "authoredConnectorAbsentDuringFreeSamples":true}),
    );
    let NodeKind::Connector(connector) = &f.h.app.doc().scene.node(wire).unwrap().kind else {
        panic!("authored connector");
    };
    assert_eq!(
        connector.a,
        ConnectorEnd::Anchored {
            node: targets[0],
            side: Side::Right,
            t: 0.5
        }
    );
    assert_eq!(
        connector.b,
        ConnectorEnd::Anchored {
            node: targets[1],
            side: Side::Left,
            t: 0.5
        }
    );
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        f.h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    let completed = f.h.app.doc().scene.clone();
    f.invoke(
        2,
        "board.objects.move",
        &[targets[0]],
        json!({"dx":-180,"dy":0}),
    );
    let mut cancelled = false;
    for _ in 0..80 {
        let out = f.step();
        assert_eq!(f.h.app.doc().scene, completed);
        let restored = completed.node(targets[0]).unwrap();
        let preview = f.h.app.presence_preview(targets[0]).cloned();
        if let Some(preview) = preview.filter(|p| restored.rect.x - p.rect.x > restored.rect.w) {
            f.save("cancel-preview", out);
            assert_eq!(f.blue(&preview), Some(true));
            let position = f.sample().position;
            f.h.app.stop_presence(&f.who, false).unwrap();
            let out = f.step();
            assert_eq!(f.sample().position, position);
            assert!(f.h.app.presence_preview(targets[0]).is_none());
            assert!(!f.h.app.presence.holds_node(targets[0]));
            assert_eq!(f.h.app.doc().scene, completed);
            assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
            f.save("cancel-restored", out);
            assert_eq!(f.blue(restored), Some(true));
            assert_eq!(
                f.blue(&preview),
                Some(false),
                "canceled image ghost must disappear"
            );
            cancelled = true;
            break;
        }
    }
    assert!(cancelled);
    assert_eq!(f.h.app.doc().items, items);
    let tab = f.h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(tab.doc.scene.nodes, moved.nodes);
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(tab.doc.scene.nodes, original.nodes);
    f.finish();
}

#[test]
#[ignore = "retains an actual offscreen-source preview entering the viewport"]
fn witnessed_offscreen_linked_image_enters_the_view_before_its_single_commit() {
    let mut f = Fixture::new(-900.0);
    let out = f.step();
    f.save("before", out);
    let source = f.h.app.doc().scene.node(f.images[0]).unwrap();
    assert!(!f
        .h
        .app
        .board_xf()
        .rect_w2s(source.rect)
        .intersects(f.h.app.canvas_rect));
    move_image(&mut f, 800.0, 0.0);
    f.finish();
}
