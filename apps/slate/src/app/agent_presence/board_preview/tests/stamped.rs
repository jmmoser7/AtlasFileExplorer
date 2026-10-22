use super::*;

fn poll_preview(
    h: &mut crate::app::tests::Harness,
    job: &mut PreviewJob,
    grant: &CollaboratorGrant,
) -> Result<bool, String> {
    let mut result = None;
    let _ = h.ctx.run(Default::default(), |ctx| {
        result = Some(job.poll(&mut h.app, ctx, grant))
    });
    result.expect("egui poll ran")
}

#[test]
fn stamped_preview_waits_for_real_pixels_and_reports_timeout() {
    let (mut h, _, frame_id) = fixture();
    let scene = &mut h.app.doc_mut().scene;
    let make_stroke = |scene: &mut slate_doc::scene::Scene, x: f32| {
        let path = slate_doc::scene::PathData {
            start: [0.0, 0.5],
            segs: vec![slate_doc::scene::PathSeg::Line { to: [1.0, 0.5] }],
            ..Default::default()
        };
        let stroke = Stroke {
            width: 16.0,
            color: Rgba([235, 24, 18, 255]),
            stamp: true,
            ..Default::default()
        };
        scene.build_node(
            slate_doc::WorldRect::new(x, 65.0, 70.0, 20.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke,
                corner: Corner::Square,
                sides: 4,
                phase_deg: 0.0,
                flip: false,
                path: Some(path.into()),
                text: None,
            }),
        )
    };
    let first = make_stroke(scene, 15.0);
    let second = make_stroke(scene, 110.0);
    let (first_id, second_id) = (first.id, second.id);
    h.app.add_nodes(vec![first, second]);
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .readable_nodes
        .extend([first_id.0.to_string(), second_id.0.to_string()]);
    let call = preview_call(&h, vec![frame_id.0.to_string()]);
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap().clone();
    let mut job = PreviewJob::new(&h.app, &grant, &call).unwrap();
    h.app.brush_tiles.hold_rasters = true;
    h.app.stamp_sync_px = f32::MAX;
    assert!(!poll_preview(&mut h, &mut job, &grant).unwrap());
    assert!(job.result.is_none(), "missing stamp must not finish blank");

    let deadline = Instant::now() + Duration::from_secs(15);
    while h.app.brush_tiles.stroke_landed_len(false) == 0 && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(h.app.brush_tiles.stroke_landed_len(false) > 0);
    h.app.brush_tiles.hold_rasters = false;
    assert!(!poll_preview(&mut h, &mut job, &grant).unwrap());
    assert_eq!(
        job.next,
        job.nodes.len() - 1,
        "second brush is still pending"
    );
    h.app.brush_tiles.hold_rasters = true;
    job.stamp_wait_since = Some(Instant::now() - STAMP_WAIT_LIMIT);
    assert!(!poll_preview(&mut h, &mut job, &grant).unwrap());
    let deadline = Instant::now() + Duration::from_secs(15);
    while job.result.is_none() && Instant::now() < deadline {
        let _ = poll_preview(&mut h, &mut job, &grant).unwrap();
        std::thread::yield_now();
    }
    let value = job.result.unwrap();
    assert_eq!(value["complete"], false);
    assert_eq!(value["unavailable_objects"], 1);
    let pixels = image::open(value["image_source"].as_str().unwrap())
        .unwrap()
        .to_rgba8();
    let ink = pixels.get_pixel(300, 450).0;
    assert!(
        ink[0] > 180 && ink[1] < 80,
        "first real brush stroke: {ink:?}"
    );
}
