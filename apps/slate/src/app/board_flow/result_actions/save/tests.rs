use super::*;

#[test]
fn batch_save_preserves_pixels_and_numbers_repeated_existing_destinations() {
    let (mut h, host) = super::super::tests::fixture("result-batch-save");
    let address = ResultAddress {
        tab: h.app.tab().id,
        host,
        binding: h.app.result_binding(host).unwrap(),
        outputs: h
            .app
            .agent_images(host)
            .iter()
            .skip(1)
            .take(2)
            .map(|image| image.id.clone())
            .collect(),
    };
    let (doc, nodes) = h.app.result_payload(&address);
    let snapshot = SaveSnapshot {
        target: SaveTarget::Results(address),
        doc,
        nodes,
        workbook: None,
        extensions: vec!["png".into(); 2],
    };
    let destinations = [
        h.base.join("render scene_001.png"),
        h.base.join("render scene_002.png"),
    ];
    assert_eq!(
        worker::save(&snapshot, &destinations).unwrap(),
        destinations
    );
    for (index, destination) in destinations.iter().enumerate() {
        let actual = image::open(destination).unwrap().to_rgba8();
        assert_eq!(actual.dimensions(), (20, 10));
        assert_eq!(actual.get_pixel(0, 0)[3], 192);
        assert_eq!(actual.get_pixel(0, 0)[0], (index as u8 + 1) * 40);
    }
    let first_bytes = std::fs::read(&destinations[0]).unwrap();
    let second_bytes = std::fs::read(&destinations[1]).unwrap();
    assert_eq!(
        worker::save(&snapshot, &destinations).unwrap(),
        [
            h.base.join("render scene_003.png"),
            h.base.join("render scene_004.png"),
        ]
    );
    assert_eq!(std::fs::read(&destinations[0]).unwrap(), first_bytes);
    assert_eq!(std::fs::read(&destinations[1]).unwrap(), second_bytes);
    assert_eq!(
        std::fs::read(h.base.join("render scene_003.png")).unwrap(),
        first_bytes
    );
    assert_eq!(
        std::fs::read(h.base.join("render scene_004.png")).unwrap(),
        second_bytes
    );
    let NodeKind::Image(image) = &snapshot.nodes[0].kind else {
        unreachable!()
    };
    let original = crate::app::image_composite::item_file(&snapshot.doc, None, image.item).unwrap();
    assert!(
        worker::save(&snapshot, &[original, h.base.join("other.png")])
            .unwrap_err()
            .contains("source images")
    );
}

fn ordinary_image(tag: &str) -> (crate::app::tests::Harness, NodeId, PathBuf) {
    let mut h = crate::app::tests::Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    let path = h.base.join("photo.png");
    image::RgbaImage::from_pixel(20, 10, image::Rgba([10, 20, 30, 192]))
        .save(&path)
        .unwrap();
    let item = h.app.add_paths(&[path.clone()])[0];
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    (h, id, path)
}

#[test]
fn ordinary_image_save_keeps_edited_pixels_and_protects_original_source() {
    use slate_doc::image_paint::{PaintLayer, PaintLayerId};
    use slate_doc::scene::{Crop, Rgba, ShapeKind, ShapeNode, WorldRect};
    let (mut h, id, source) = ordinary_image("save-edited-ordinary-image");
    let original_bytes = std::fs::read(&source).unwrap();
    let mark = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.1, 0.1, 0.5, 0.5),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: Some(Rgba::opaque(250, 30, 20)),
            stroke: Default::default(),
            corner: Default::default(),
            sides: 4,
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let mut layer = PaintLayer::new(PaintLayerId(1));
    layer.nodes.push(mark);
    h.app.patch_nodes(&[id], |node| {
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        image.crop = Crop {
            x: 0.5,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        image.adjust.invert = 1.0;
        image.paint_layers.push(layer.clone());
    });
    let snapshot = h.app.board_image_save_snapshot(id).unwrap();
    let destination = h.base.join("edited.png");
    assert_eq!(
        worker::save(&snapshot, &[destination.clone()]).unwrap(),
        [destination.clone()]
    );
    assert_eq!(
        worker::save(&snapshot, &[destination.clone()]).unwrap(),
        [h.base.join("edited_001.png")]
    );
    let actual = image::open(destination).unwrap().to_rgba8();
    assert_eq!(actual.dimensions(), (10, 10), "the displayed crop is saved");
    assert_eq!(
        actual.get_pixel(9, 9).0,
        [245, 235, 225, 192],
        "filters and alpha survive"
    );
    assert_eq!(
        actual.get_pixel(3, 3).0,
        [250, 30, 20, 255],
        "visible paint is saved"
    );
    assert!(worker::save(&snapshot, &[source.clone()])
        .unwrap_err()
        .contains("source images"));
    assert_eq!(std::fs::read(source).unwrap(), original_bytes);
}

fn adopt_pick(
    h: &mut crate::app::tests::Harness,
    snapshot: SaveSnapshot,
    destination: Option<PathBuf>,
) {
    let (tx, rx) = crossbeam_channel::bounded(1);
    tx.send(SavePick {
        snapshot,
        names: None,
        destination,
    })
    .unwrap();
    let mut picker = h.app.dialogs.picker();
    assert!(picker.adopt(rx));
    h.app.agents.flow.results.save.picker = Some(picker);
    h.app.result_save_input(&h.ctx);
}

#[test]
fn cancelled_or_stale_ordinary_image_picker_does_not_start_a_save() {
    let (mut h, id, source) = ordinary_image("save-stale-ordinary-image");
    let destination = h.base.join("chosen.png");
    let snapshot = h.app.board_image_save_snapshot(id).unwrap();
    assert!(snapshot.valid(&h.app));
    adopt_pick(&mut h, snapshot.clone(), None);
    assert!(h.app.agents.flow.results.save.worker.is_none());
    h.app.patch_nodes(&[id], |node| {
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        image.adjust.invert = 1.0;
    });
    assert!(!snapshot.valid(&h.app));
    adopt_pick(&mut h, snapshot, Some(destination.clone()));
    assert!(h.app.agents.flow.results.save.worker.is_none());
    assert!(!destination.exists());
    assert!(source.exists());
    let snapshot = h.app.board_image_save_snapshot(id).unwrap();
    h.app.tab_mut().path = Some(h.base.join("rebound.slate"));
    assert!(!snapshot.valid(&h.app));
    adopt_pick(&mut h, snapshot, Some(destination.clone()));
    assert!(h.app.agents.flow.results.save.worker.is_none());
    assert!(!destination.exists());
}

#[test]
fn ordinary_image_picker_runs_the_shared_worker_without_relinking_its_source() {
    let (mut h, id, source) = ordinary_image("save-ordinary-picker-worker");
    let original = h.app.doc().scene.node(id).unwrap().clone();
    let snapshot = h.app.board_image_save_snapshot(id).unwrap();
    let destination = h.base.join("chosen.png");
    adopt_pick(&mut h, snapshot, Some(destination.clone()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.agents.flow.results.save.worker.is_some() {
        h.app.result_save_input(&h.ctx);
        assert!(
            std::time::Instant::now() < deadline,
            "image save worker did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(
        std::fs::read(destination).unwrap(),
        std::fs::read(&source).unwrap()
    );
    assert_eq!(h.app.doc().scene.node(id), Some(&original));
    assert_eq!(h.app.image_item_path(id), Some(source));
}

#[test]
fn canvas_copy_uses_the_same_save_pipeline_as_ordinary_images() {
    let (mut h, host) = super::super::tests::fixture("save-canvas-copy");
    let output = h.app.agent_images(host)[1].clone();
    let copy = h
        .app
        .copy_agent_results_to_canvas(host, &[(output.id.clone(), Pos2::new(800.0, 0.0))])[0];
    assert!(h.app.board_image_save_available(copy));
    let snapshot = h.app.board_image_save_snapshot(copy).unwrap();
    let destination = h.base.join("copied.png");
    assert_eq!(
        worker::save(&snapshot, &[destination.clone()]).unwrap(),
        [destination.clone()]
    );
    assert_eq!(
        std::fs::read(destination).unwrap(),
        std::fs::read(output.source).unwrap()
    );
}
