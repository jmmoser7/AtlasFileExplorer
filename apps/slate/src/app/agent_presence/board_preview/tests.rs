use super::*;
use crate::app::agent_presence::tests::{add, host};
use serde_json::json;
use serde_json::Value;
use slate_doc::scene::{
    Corner, FrameNode, ImageNode, NodeKind, PortalNode, Rgba, ShapeKind, ShapeNode, Stroke,
    TextAlign, TextNode, TextStyle, Typeface,
};
use std::time::{Duration, Instant};

fn frame(title: &str) -> FrameNode {
    FrameNode {
        title: title.into(),
        order: 1,
        fill: Rgba([28, 36, 48, 255]),
        fill_authored: true,
        assignments: Default::default(),
        stroke: Stroke::default(),
        corner: Corner::Square,
    }
}

fn fixture() -> (crate::app::tests::Harness, String, slate_doc::NodeId) {
    let mut h = host();
    h.app.ensure_work_tab();
    let who = add(&mut h, "Preview reader");
    let frame = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        NodeKind::Frame(frame("Preview slide")),
    );
    let frame_id = frame.id;
    let shape = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(20.0, 20.0, 50.0, 50.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: Some(Rgba([235, 24, 18, 255])),
            stroke: Stroke::default(),
            corner: Corner::Square,
            sides: 4,
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let text = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(100.0, 20.0, 80.0, 60.0),
        NodeKind::Text(TextNode {
            text: "VISIBLE".into(),
            style: TextStyle::default(),
            family: Typeface::Sans,
            size: 28.0,
            color: Rgba([18, 225, 52, 255]),
            align: TextAlign::Left,
            fill: None,
            stroke: Stroke::default(),
            agent: None,
        }),
    );
    let ids = [frame.id, shape.id, text.id];
    h.app.add_nodes(vec![frame, shape, text]);
    let grant = h.app.presence.agents[0].grant.as_mut().unwrap();
    grant.readable_board = false;
    grant.readable_nodes = ids.iter().map(|id| id.0.to_string()).collect();
    (h, who, frame_id)
}

fn preview_call(h: &crate::app::tests::Harness, targets: Vec<String>) -> BoardCall {
    let a = &h.app.presence.agents[0];
    BoardCall {
        id: atlas_agent::request_id(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "presence.preview".into(),
            targets,
            arguments: json!({}),
        },
    }
}

fn render(h: &mut crate::app::tests::Harness, call: &BoardCall) -> Value {
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap().clone();
    let mut job = PreviewJob::new(&h.app, &grant, call).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while job.result.is_none() && Instant::now() < deadline {
        let mut poll = None;
        let _ = h.ctx.run(Default::default(), |ctx| {
            poll = Some(job.poll(&mut h.app, ctx, &grant));
        });
        poll.expect("egui poll ran").unwrap();
        std::thread::yield_now();
    }
    job.result
        .expect("preview worker completed before deadline")
}

#[test]
fn scoped_preview_renders_production_frame_shape_and_text_without_mutating_board_state() {
    let (mut h, _, frame_id) = fixture();
    h.app.board_sel.insert(frame_id);
    h.app.tab_mut().cam.offset = egui::vec2(713.0, -219.0);
    h.app.tab_mut().cam.z = 2.75;
    let scene = h.app.doc().scene.clone();
    let selection = h.app.board_sel.clone();
    let camera = (h.app.tab().cam.offset, h.app.tab().cam.z);
    let journal = h.app.tab().journal.undo_depth();

    let call = preview_call(&h, vec![frame_id.0.to_string()]);
    let value = render(&mut h, &call);
    assert_eq!(value["renderer"], "slate-production-board-painter");
    assert_eq!(value["complete"], true);
    assert_eq!(value["width"], 1200);
    assert_eq!(value["height"], 600);
    let path = value["image_source"].as_str().unwrap();
    let pixels = image::open(path).unwrap().to_rgba8();
    let pixel = |x: u32, y: u32| pixels.get_pixel(x, y).0;
    let frame_fill = pixel(60, 60);
    assert!(
        frame_fill[2] > frame_fill[0],
        "frame fill should be visible: {frame_fill:?}"
    );
    let shape_fill = pixel(240, 240);
    assert!(
        shape_fill[0] > 180 && shape_fill[1] < 80,
        "shape fill should be visible: {shape_fill:?}"
    );
    let text_has_green_ink = (120..480).any(|y| {
        (600..1080).any(|x| {
            let p = pixel(x, y);
            p[1] > 140 && p[1] > p[0].saturating_add(50)
        })
    });
    assert!(
        text_has_green_ink,
        "authored text should contribute visible pixels"
    );
    assert_eq!(h.app.doc().scene, scene);
    assert_eq!(h.app.board_sel, selection);
    assert_eq!((h.app.tab().cam.offset, h.app.tab().cam.z), camera);
    assert_eq!(h.app.tab().journal.undo_depth(), journal);

    let whole_board = preview_call(&h, vec![]);
    let whole_board = render(&mut h, &whole_board);
    assert_eq!(whole_board["complete"], true);
    assert_eq!(whole_board["width"], value["width"]);
    assert_eq!(whole_board["height"], value["height"]);
}

#[test]
fn preview_poll_stays_ready_after_consuming_worker_result() {
    let (mut h, _, frame_id) = fixture();
    let call = preview_call(&h, vec![frame_id.0.to_string()]);
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap().clone();
    let mut job = PreviewJob::new(&h.app, &grant, &call).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut completed = false;

    while Instant::now() < deadline {
        let mut poll = None;
        let _ = h.ctx.run(Default::default(), |ctx| {
            poll = Some(job.poll(&mut h.app, ctx, &grant));
        });
        if poll.expect("egui poll ran").unwrap() {
            completed = true;
            break;
        }
        std::thread::yield_now();
    }

    assert!(completed, "preview worker completed before deadline");
    assert!(job.result.is_some());

    let mut repeated_poll = None;
    let _ = h.ctx.run(Default::default(), |ctx| {
        repeated_poll = Some(job.poll(&mut h.app, ctx, &grant));
    });
    assert!(repeated_poll.expect("egui poll ran").unwrap());
    assert!(job.result.is_some());
}

#[test]
fn preview_constructor_refuses_unscoped_members_missing_image_source_and_invalid_bounds() {
    let (mut h, _, frame_id) = fixture();
    let member = h.app.doc().scene.nodes[1].id;
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .readable_nodes = vec![frame_id.0.to_string()];
    let call = preview_call(&h, vec![frame_id.0.to_string()]);
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap();
    let error = PreviewJob::new(&h.app, grant, &call).err().unwrap();
    assert!(error.contains("outside this read grant"));
    assert_ne!(member, frame_id);

    let item = h
        .app
        .doc_mut()
        .add_item(h.base.join("source.png"), "source.png", 10, 0, "image");
    let image = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(25.0, 25.0, 20.0, 20.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    h.app.add_nodes(vec![image]);
    let all_node_ids: Vec<_> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .map(|node| node.id.0.to_string())
        .collect();
    let grant = h.app.presence.agents[0].grant.as_mut().unwrap();
    grant.readable_nodes = all_node_ids;
    let call = preview_call(&h, vec![frame_id.0.to_string()]);
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap();
    let error = PreviewJob::new(&h.app, grant, &call).err().unwrap();
    assert!(error.contains("Allow board images"));

    let mut empty = host();
    empty.app.ensure_work_tab();
    add(&mut empty, "Empty board reader");
    let call = preview_call(&empty, vec![]);
    let grant = empty.app.presence.agents[0].grant.as_ref().unwrap();
    assert!(PreviewJob::new(&empty.app, grant, &call)
        .err()
        .unwrap()
        .contains("1–512"));
}

#[test]
fn preview_rejects_machine_private_images_before_texture_cache_side_effects() {
    let (mut h, _, frame_id) = fixture();
    let private_path = h
        .base
        .join("NativeFileAtlas")
        .join("secrets")
        .join("private.png");
    assert!(atlas_core::secrets::is_machine_private(&private_path));
    let item = h
        .app
        .doc_mut()
        .add_item(private_path, "private.png", 10, 0, "image");
    let image = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(25.0, 25.0, 20.0, 20.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let image_id = image.id;
    h.app.add_nodes(vec![image]);
    let grant = h.app.presence.agents[0].grant.as_mut().unwrap();
    grant.readable_nodes.push(image_id.0.to_string());
    grant.readable_sources.push("board-file-inputs".into());

    let before_preview = h.app.preview_cache.len();
    let before_thumbnails = h.app.thumb_pixels.len();
    let call = preview_call(&h, vec![frame_id.0.to_string()]);
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap();
    let error = PreviewJob::new(&h.app, grant, &call)
        .err()
        .expect("private source should reject construction");
    assert!(error.contains("Machine-private"));
    assert_eq!(h.app.preview_cache.len(), before_preview);
    assert_eq!(h.app.thumb_pixels.len(), before_thumbnails);
}

#[test]
fn preview_omits_allowed_live_web_portal_without_changing_its_host_state() {
    let (mut h, _, frame_id) = fixture();
    let host = crate::app::tests::with_fake_host(&mut h);
    let portal = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(40.0, 20.0, 80.0, 50.0),
        NodeKind::Portal(PortalNode::bound_web(
            "Live page",
            "https://example.com/live",
        )),
    );
    let portal_id = portal.id;
    h.app.add_nodes(vec![portal]);
    h.app.web.grant_consent("https://example.com");
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .readable_nodes
        .push(portal_id.0.to_string());
    crate::app::tests::web_settle(&mut h, &[(portal_id, 600.0)], 3);
    assert!(
        h.app.web.is_live(portal_id),
        "fixture starts with a live page"
    );

    let state = h.app.web.state(portal_id);
    let live_count = h.app.web.live_count();
    let admissions = host.admit_targets();
    let url = host.current_url(portal_id);
    let call = preview_call(&h, vec![frame_id.0.to_string()]);
    let value = render(&mut h, &call);

    assert_eq!(value["complete"], false);
    assert_eq!(value["unavailable_objects"], 1);
    assert_eq!(h.app.web.state(portal_id), state);
    assert_eq!(h.app.web.live_count(), live_count);
    assert!(h.app.web.is_live(portal_id));
    assert_eq!(host.admit_targets(), admissions);
    assert_eq!(host.current_url(portal_id), url);
}

#[test]
fn preview_bounds_and_target_count_are_rejected_before_rendering() {
    let (h, _, frame_id) = fixture();
    let call = preview_call(&h, vec![frame_id.0.to_string(), frame_id.0.to_string()]);
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap();
    assert!(PreviewJob::new(&h.app, grant, &call)
        .err()
        .unwrap()
        .contains("one frame"));

    let mut oversized = host();
    oversized.app.ensure_work_tab();
    add(&mut oversized, "Large board reader");
    for index in 0..513 {
        let node = oversized.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(index as f32 * 2.0, 0.0, 1.0, 1.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: Some(Rgba([100, 100, 100, 255])),
                stroke: Stroke::default(),
                corner: Corner::Square,
                sides: 4,
                phase_deg: 0.0,
                flip: false,
                path: None,
                text: None,
            }),
        );
        oversized.app.add_nodes(vec![node]);
    }
    let call = preview_call(&oversized, vec![]);
    let grant = oversized.app.presence.agents[0].grant.as_ref().unwrap();
    assert!(PreviewJob::new(&oversized.app, grant, &call)
        .err()
        .unwrap()
        .contains("1–512"));

    let mut nonfinite = host();
    nonfinite.app.ensure_work_tab();
    add(&mut nonfinite, "Nonfinite reader");
    let node = nonfinite.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(f32::NAN, 0.0, 20.0, 20.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: Some(Rgba([100, 100, 100, 255])),
            stroke: Stroke::default(),
            corner: Corner::Square,
            sides: 4,
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    nonfinite.app.add_nodes(vec![node]);
    let call = preview_call(&nonfinite, vec![]);
    let grant = nonfinite.app.presence.agents[0].grant.as_ref().unwrap();
    assert!(PreviewJob::new(&nonfinite.app, grant, &call)
        .err()
        .unwrap()
        .contains("finite and nonempty"));
}

#[test]
fn existing_read_owner_rejects_stale_preview_and_cancels_on_stop() {
    let (mut h, who, frame_id) = fixture();
    let mut a = h.app.presence.agents.remove(0);
    a.request = "preview-generation-request".into();
    h.app
        .presence
        .roster
        .session_mut(&a.motion)
        .unwrap()
        .begin_request(id(&a.request).unwrap())
        .unwrap();
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "presence.preview".into(),
            targets: vec![frame_id.0.to_string()],
            arguments: json!({}),
        },
    };
    h.app.begin_presence_read(&mut a, &call).unwrap();
    h.app.presence.agents.insert(0, a);
    h.app
        .patch_nodes(&[frame_id], |node| node.rotation_deg += 1.0);
    let mut a = h.app.presence.agents.remove(0);
    let mut budget = 4;
    let reply = h
        .app
        .advance_presence_read_with_context(Some(&h.ctx), &mut a, &mut budget, Duration::ZERO)
        .unwrap();
    assert!(!reply.success);
    assert!(reply.result.to_string().contains("changed during the read"));
    h.app.presence.agents.insert(0, a);

    let mut a = h.app.presence.agents.remove(0);
    a.request = "preview-cancel-request".into();
    h.app
        .presence
        .roster
        .session_mut(&a.motion)
        .unwrap()
        .begin_request(id(&a.request).unwrap())
        .unwrap();
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "presence.preview".into(),
            targets: vec![frame_id.0.to_string()],
            arguments: json!({}),
        },
    };
    h.app.begin_presence_read(&mut a, &call).unwrap();
    h.app.presence.agents.insert(0, a);
    h.app.stop_presence(&who, false).unwrap();
    assert!(h.app.presence.agents[0].read.is_none());
}
mod stamped;
