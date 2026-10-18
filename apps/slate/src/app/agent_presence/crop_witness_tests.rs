//! Canonical closed outlines supply one bounded body-contact policy.
use super::*;
use crate::app::agent_presence::tests::{add, host};
use slate_doc::scene::{ImageNode, PathContour, PathData, PathFillRule, PathSeg, WorldRect};

fn masked(points: &[[f32; 2]]) -> Node {
    let mut scene = slate_doc::Scene::default();
    let mut node = scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
        NodeKind::Image(ImageNode::new(slate_doc::ItemId(1))),
    );
    node.clip = Some(PathData {
        start: points[0],
        segs: points[1..]
            .iter()
            .map(|to| PathSeg::Line { to: *to })
            .collect(),
        closed: true,
        ..Default::default()
    });
    node
}

fn holed() -> Node {
    let mut node = masked(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    let clip = node.clip.as_mut().unwrap();
    clip.fill_rule = PathFillRule::EvenOdd;
    clip.extra.push(PathContour {
        start: [0.25, 0.25],
        segs: vec![
            PathSeg::Line { to: [0.75, 0.25] },
            PathSeg::Line { to: [0.75, 0.75] },
            PathSeg::Line { to: [0.25, 0.75] },
        ],
        closed: true,
    });
    node
}

#[test]
fn body_contact_avoids_centered_holes_and_concave_masks_deterministically() {
    for node in [
        holed(),
        masked(&[
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 0.2],
            [0.2, 0.2],
            [0.2, 1.0],
            [0.0, 1.0],
        ]),
    ] {
        assert!(!slate_doc::geom::point_in_node_outline(
            &node, 50.0, 50.0, 0.05
        ));
        let contact = body_contact(&node).unwrap();
        assert_ne!(contact, point(50.0, 50.0));
        assert!(slate_doc::geom::point_in_node_outline(
            &node,
            contact.x as f32,
            contact.y as f32,
            0.05
        ));
        assert_eq!(body_contact(&node).unwrap(), contact);
    }
}

#[test]
fn known_outline_exhaustion_refuses_contact_without_falling_back_into_a_hole() {
    // A real but narrower-than-candidate strip; every grid center and the
    // frame center lies outside. Exhaustion must not fabricate a body contact.
    let node = masked(&[[0.0, 0.0], [0.01, 0.0], [0.01, 1.0], [0.0, 1.0]]);
    assert!(body_contact(&node).is_err());
    assert!(slate_doc::geom::point_in_node_outline(
        &node, 0.5, 50.0, 0.05
    ));
    let mut empty_mask = node.clone();
    empty_mask.clip = Some(PathData::default());
    assert!(slate_doc::geom::node_closed_poly(&empty_mask, 0.05).is_none());
    assert!(body_contact(&empty_mask).is_err());
}

#[test]
fn ordinary_move_reuses_hole_safe_contact_and_refuses_exhaustion_before_motion_or_mutation() {
    for (node, has_body) in [
        (holed(), true),
        (
            masked(&[[0.0, 0.0], [0.01, 0.0], [0.01, 1.0], [0.0, 1.0]]),
            false,
        ),
    ] {
        let mut h = host();
        h.app.ensure_work_tab();
        h.app.add_nodes(vec![node.clone()]);
        add(&mut h, "Dot");
        let prepared = h
            .app
            .prepare_targeted_command(
                "board.objects.move",
                &[node.id.0.to_string()],
                &serde_json::json!({"dx":40,"dy":20}),
            )
            .unwrap();
        let agent = &mut h.app.presence.agents[0];
        agent.request = atlas_agent::request_id();
        let call = BoardCall {
            id: atlas_agent::request_id(),
            session: agent.binding.clone(),
            request_id: agent.request.clone(),
            sequence: 0,
            operation: BoardOperation::Invoke {
                command: "board.objects.move".into(),
                targets: vec![node.id.0.to_string()],
                arguments: serde_json::json!({"dx":40,"dy":20}),
            },
        };
        let motion_id = agent.motion.clone();
        let request = agent.request.clone();
        let motion = h.app.presence.roster.session_mut(&motion_id).unwrap();
        motion.begin_request(id(&request).unwrap()).unwrap();
        let mut work =
            WitnessedCommand::new(call, prepared, &h.app.doc().scene, h.app.board_wire_routing)
                .unwrap();
        let motion = h.app.presence.roster.session_mut(&motion_id).unwrap();
        if !has_body {
            assert!(work.begin_step(motion).is_err());
            assert_eq!(motion.sample().phase, MotionPhase::Idle);
            assert!(work.previews.is_empty());
            assert_eq!(h.app.doc().scene.node(node.id), Some(&node));
            continue;
        }
        work.begin_step(motion).unwrap();
        let mut sample = motion.advance(std::time::Duration::from_millis(50));
        for _ in 0..20 {
            if sample.phase == MotionPhase::Grab {
                break;
            }
            sample = motion.advance(std::time::Duration::from_millis(10));
        }
        assert_eq!(sample.phase, MotionPhase::Grab);
        assert_eq!(sample.position, body_contact(&node).unwrap());
        assert_eq!(h.app.doc().scene.node(node.id), Some(&node));
    }
}

#[test]
fn cover_clamped_pan_witness_stops_with_the_source_and_keeps_the_exact_prepared_endpoint() {
    let mut h = host();
    h.app.ensure_work_tab();
    let image = crate::app::tests::picture_from(&mut h, "clamped-pan.png", 0.0);
    h.app.patch_nodes(&[image], |node| {
        node.rotation_deg = 19.0;
        if let NodeKind::Image(image) = &mut node.kind {
            image.crop = slate_doc::scene::Crop {
                x: 0.25,
                y: 0.25,
                w: 0.5,
                h: 0.5,
            };
        }
    });
    add(&mut h, "Dot");
    let before = h.app.doc().scene.node(image).unwrap().clone();
    let args = serde_json::json!({"mode":"pan","dx":10000,"dy":0});
    let prepared = h
        .app
        .prepare_targeted_command("board.objects.crop", &[image.0.to_string()], &args)
        .unwrap();
    let after = match &prepared.cmds[0] {
        SceneCmd::Patch { after, .. } => after.as_ref().clone(),
        _ => unreachable!(),
    };
    let agent = &mut h.app.presence.agents[0];
    agent.request = atlas_agent::request_id();
    let motion_id = agent.motion.clone();
    let request = agent.request.clone();
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: agent.binding.clone(),
        request_id: request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "board.objects.crop".into(),
            targets: vec![image.0.to_string()],
            arguments: args,
        },
    };
    let mut work =
        WitnessedCommand::new(call, prepared, &h.app.doc().scene, h.app.board_wire_routing)
            .unwrap();
    let motion = h.app.presence.roster.session_mut(&motion_id).unwrap();
    motion.begin_request(id(&request).unwrap()).unwrap();
    work.begin_step(motion).unwrap();
    let contact = work.crop_contact.unwrap();
    let mut previous = None;
    let mut reached = false;
    for _ in 0..40 {
        let sample = motion.advance(std::time::Duration::from_millis(50));
        work.preview(sample).unwrap();
        if matches!(sample.phase, MotionPhase::Work | MotionPhase::ReadyToCommit) {
            let distance = ((sample.position.x - contact.x).powi(2)
                + (sample.position.y - contact.y).powi(2))
            .sqrt();
            assert!(
                distance < 160.0,
                "source cover limits must clamp the requested 10000-unit pull"
            );
            let shown = match &work.previews[&image].kind {
                NodeKind::Image(image) => image.crop,
                _ => unreachable!(),
            };
            if let Some((last_position, last_crop)) = previous {
                if sample.phase == MotionPhase::Work && sample.position != last_position {
                    assert_ne!(
                        shown, last_crop,
                        "a moving avatar cannot pull a stationary source"
                    );
                }
            }
            previous = Some((sample.position, shown));
        }
        if sample.phase == MotionPhase::ReadyToCommit {
            assert_eq!(work.previews.get(&image), Some(&after));
            reached = true;
            break;
        }
    }
    assert!(reached);
    assert_eq!(h.app.doc().scene.node(image), Some(&before));
}
