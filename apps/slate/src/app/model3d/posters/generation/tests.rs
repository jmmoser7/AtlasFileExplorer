use super::*;
use crate::app::tests::Harness;

fn info(node: u64) -> ModelNodeInfo {
    ModelNodeInfo {
        node: NodeId(node),
        cache_key: "same-source".into(),
        path: PathBuf::from("same-model.obj"),
        cam: ModelCamera {
            distance: 10.0,
            ..Default::default()
        },
        rect: slate_doc::scene::WorldRect {
            x: 0.0,
            y: 0.0,
            w: 320.0,
            h: 200.0,
        },
    }
}

#[test]
fn a_generated_pose_satisfies_hundreds_of_queued_duplicate_nodes() {
    let mut space = ModelSpace::default();
    let first = info(1);
    let (name, _) = space.model_poster_identity(&first).unwrap();
    assert!(!space.poster_already_resident(&first));
    space.cache_poster_pixels(
        name,
        Arc::new(egui::ColorImage::new([1, 1], egui::Color32::RED)),
    );
    for node in 2..=500 {
        let mut duplicate = info(node);
        duplicate.rect.x = node as f32 * 400.0;
        duplicate.rect.y = -200.0;
        assert!(space.poster_already_resident(&duplicate));
    }
    assert_eq!(space.poster_pixels.len(), 1);
    assert!(
        space.models.is_empty(),
        "cached duplicates require no original model parse"
    );
}

#[test]
fn a_cached_pose_does_not_satisfy_different_camera_aspect_or_source() {
    let mut space = ModelSpace::default();
    let first = info(1);
    let (name, _) = space.model_poster_identity(&first).unwrap();
    space.cache_poster_pixels(
        name,
        Arc::new(egui::ColorImage::new([1, 1], egui::Color32::RED)),
    );
    let mut changed = info(2);
    changed.cam.yaw += 0.2;
    assert!(!space.poster_already_resident(&changed));
    changed = info(3);
    changed.cam.display = ModelDisplay::Arctic;
    assert!(!space.poster_already_resident(&changed));
    changed = info(4);
    changed.cam.materials = !changed.cam.materials;
    assert!(!space.poster_already_resident(&changed));
    changed = info(5);
    changed.rect.w *= 2.0;
    assert!(!space.poster_already_resident(&changed));
    changed = info(6);
    changed.cache_key = "changed-source".into();
    assert!(!space.poster_already_resident(&changed));
}

#[test]
fn autofit_duplicates_share_the_resolved_camera_identity_after_parse() {
    let mut space = ModelSpace::default();
    let mut automatic = info(1);
    automatic.cam.distance = 0.0;
    assert!(!space.poster_already_resident(&automatic));
    let bounds = ([-1.0; 3], [1.0; 3]);
    space.bounds.insert(automatic.cache_key.clone(), bounds);
    let (name, _) = space.model_poster_identity(&automatic).unwrap();
    space.cache_poster_pixels(
        name,
        Arc::new(egui::ColorImage::new([1, 1], egui::Color32::RED)),
    );
    assert!(space.poster_already_resident(&automatic));
    let mut resolved = info(2);
    resolved.cam = resolve_camera(&automatic.cam, bounds.0, bounds.1);
    assert!(space.poster_already_resident(&resolved));
}

#[test]
fn a_gpu_only_cached_pose_also_satisfies_duplicate_requests() {
    let mut space = ModelSpace::default();
    let ctx = egui::Context::default();
    let first = info(1);
    let (name, _) = space.model_poster_identity(&first).unwrap();
    space.cache_poster_texture(
        name,
        ctx.load_texture(
            "poster",
            egui::ColorImage::new([1, 1], egui::Color32::RED),
            egui::TextureOptions::LINEAR,
        ),
    );
    assert!(space.poster_already_resident(&info(2)));
    assert!(space.poster_pixels.is_empty());
}

#[test]
fn a_second_queued_node_completes_without_gl_or_another_decode_request() {
    let mut h = Harness::new("poster-queued-duplicate");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let source = h.base.join("same-model.obj");
    std::fs::write(&source, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let item = h.app.add_paths(&[source])[0];
    h.app.place_items_on_board(&[item, item], egui::Pos2::ZERO);
    let nodes: Vec<_> = h.app.doc().scene.nodes.iter().map(|node| node.id).collect();
    assert_eq!(nodes.len(), 2);
    let first = h.app.model_node_info(nodes[0]).unwrap();
    h.app
        .model3d
        .bounds
        .insert(first.cache_key.clone(), ([0.0; 3], [1.0; 3]));
    let (name, _) = h.app.model3d.model_poster_identity(&first).unwrap();
    // Represent the first node's completed render, while both NodeId requests
    // are still queued. The second must share this exact pose without GL.
    h.app.model3d.cache_poster_pixels(
        name,
        Arc::new(egui::ColorImage::new([1, 1], egui::Color32::RED)),
    );
    let document = h.app.tab().id;
    for node in &nodes {
        h.app.model3d.want_poster.insert((document, *node));
    }
    let slot = h.app.next_preview_slot;
    assert!(h.app.gl.is_none());
    assert!(matches!(
        h.app.generate_poster(nodes[1]),
        PosterGeneration::Complete
    ));
    assert_eq!(h.app.next_preview_slot, slot);
    assert!(h.app.model3d.models.is_empty());
    assert_eq!(h.app.model3d.gl_counts(), (0, 0, 0));
    assert_eq!(h.app.model3d.poster_pixels.len(), 1);
}
