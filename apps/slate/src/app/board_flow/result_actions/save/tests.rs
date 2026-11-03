use super::*;

#[test]
fn batch_save_preserves_pixels_and_numbers_repeated_existing_destinations() {
    let (mut h, host) = super::super::tests::fixture("result-batch-save");
    let address = ResultAddress {
        references: Vec::new(),
        target: None,
        witness: None,
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
        worker::save(&snapshot, &destinations).unwrap().paths,
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
        worker::save(&snapshot, &destinations).unwrap().paths,
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
    let item = h.app.add_paths(std::slice::from_ref(&path))[0];
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
        // Ink is normalized to the full uncropped image. The right-half
        // crop maps this mark to x=2..8, y=1..6 in the saved 10x10 image.
        WorldRect::new(0.6, 0.1, 0.3, 0.5),
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
        worker::save(&snapshot, std::slice::from_ref(&destination))
            .unwrap()
            .paths
            .as_slice(),
        std::slice::from_ref(&destination)
    );
    assert_eq!(
        worker::save(&snapshot, std::slice::from_ref(&destination))
            .unwrap()
            .paths,
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
    assert!(worker::save(&snapshot, std::slice::from_ref(&source))
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
    finish_save(&mut h);
    assert_eq!(
        std::fs::read(destination).unwrap(),
        std::fs::read(&source).unwrap()
    );
    assert_eq!(h.app.doc().scene.node(id), Some(&original));
    assert_eq!(h.app.image_item_path(id), Some(source));
}

fn finish_save(h: &mut crate::app::tests::Harness) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.agents.flow.results.save.worker.is_some() {
        h.app.result_save_input(&h.ctx);
        assert!(
            std::time::Instant::now() < deadline,
            "image save worker did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
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
        worker::save(&snapshot, std::slice::from_ref(&destination))
            .unwrap()
            .paths
            .as_slice(),
        std::slice::from_ref(&destination)
    );
    assert_eq!(
        std::fs::read(destination).unwrap(),
        std::fs::read(output.source).unwrap()
    );
}

#[test]
fn repeated_single_save_defaults_skip_existing_suffixes_and_keep_cancelled_suggestion() {
    let (mut h, id, _) = ordinary_image("save-next-default");
    let snapshot = h.app.board_image_save_snapshot(id).unwrap();
    assert_eq!(
        h.app.agents.flow.results.save.single_save_path("png"),
        PathBuf::from("render scene.png")
    );
    for number in [1, 3] {
        std::fs::write(
            h.base.join(format!("render scene_{number:03}.png")),
            b"keep existing",
        )
        .unwrap();
    }
    let occupied_webp = h.base.join("render scene_004.webp");
    std::fs::write(&occupied_webp, b"keep existing webp").unwrap();
    let first = h.base.join("render scene.png");
    adopt_pick(&mut h, snapshot.clone(), Some(first));
    finish_save(&mut h);
    let next = h.app.agents.flow.results.save.single_save_path("png");
    assert_eq!(next, h.base.join("render scene_002.png"));
    adopt_pick(&mut h, snapshot.clone(), None);
    assert_eq!(h.app.agents.flow.results.save.single_save_path("png"), next);
    adopt_pick(&mut h, snapshot, Some(next.clone()));
    finish_save(&mut h);
    assert!(next.is_file());
    assert_eq!(
        h.app.agents.flow.results.save.single_save_path("png"),
        h.base.join("render scene_004.png")
    );
    let next_webp = h.app.agents.flow.results.save.single_save_path("webp");
    assert_eq!(next_webp, h.base.join("render scene_003.webp"));
    assert!(
        !next_webp.exists(),
        "alternate-format suggestion must have been probed"
    );
    assert_eq!(std::fs::read(occupied_webp).unwrap(), b"keep existing webp");
    for number in [1, 3] {
        assert_eq!(
            std::fs::read(h.base.join(format!("render scene_{number:03}.png"))).unwrap(),
            b"keep existing"
        );
    }
}

#[test]
fn an_unavailable_next_number_does_not_fail_an_already_published_save() {
    let (h, id, _) = ordinary_image("save-advisory-overflow");
    let snapshot = h.app.board_image_save_snapshot(id).unwrap();
    let destination = h.base.join("photo_18446744073709551615.png");
    let saved = worker::save(&snapshot, std::slice::from_ref(&destination)).unwrap();
    assert_eq!(saved.paths.as_slice(), std::slice::from_ref(&destination));
    assert!(saved.next_single.is_empty());
    assert!(destination.is_file());
}

#[test]
fn saved_model_output_copies_bytes_and_public_provenance_without_replacing_user_json() {
    use slate_doc::scene::{HiddenOutputRef, ModelOutputRef, SourceUri};
    let (mut h, host) = super::super::tests::fixture("save-model-output-reference");
    let source = h.base.join("immutable-model.3dm");
    std::fs::write(&source, b"saved immutable model bytes").unwrap();
    let item = h.app.item_for_ready_path(&source, 27, 0);
    let mut agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(host).unwrap())
        .unwrap()
        .clone();
    agent.provider = "codex".into();
    agent.seed = None;
    agent.bundle = None;
    agent.generator.model_outputs = vec![ModelOutputRef {
        id: "native-ticket".into(),
        source: SourceUri {
            locator: source.to_string_lossy().into(),
        },
        size: 27,
        mtime: 0,
    }];
    agent.generator.shown_model = Some("native-ticket".into());
    let mut image = ImageNode::new(item);
    image.agent = Some(Box::new(agent));
    h.app
        .patch_nodes(&[host], |node| node.kind = NodeKind::Image(image.clone()));
    let address = ResultAddress {
        tab: h.app.tab().id,
        host,
        binding: h.app.result_binding(host).unwrap(),
        outputs: vec!["native-ticket".into()],
        references: vec![HiddenOutputRef::Model {
            ticket: "native-ticket".into(),
        }],
        target: Some("native-ticket".into()),
        witness: None,
    };
    let (doc, nodes) = h.app.result_payload(&address);
    let snapshot = SaveSnapshot {
        target: SaveTarget::Results(address),
        doc,
        nodes,
        workbook: None,
        extensions: vec!["3dm".into()],
    };
    let destination = h.base.join("model-copy.3dm");
    let saved = worker::save(&snapshot, std::slice::from_ref(&destination)).unwrap();
    assert_eq!(
        std::fs::read(&saved.paths[0]).unwrap(),
        std::fs::read(&source).unwrap()
    );
    let sidecar = atlas_core::workbook_assets::sidecar_path(&saved.paths[0]);
    let provenance: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sidecar).unwrap()).unwrap();
    assert_eq!(provenance["source_run"], "native-ticket");
    assert_eq!(provenance["prompt"], "");
    let collision = h.base.join("user-data.3dm");
    let user_json = atlas_core::workbook_assets::sidecar_path(&collision);
    std::fs::write(&user_json, b"user data kept intact").unwrap();
    assert!(worker::save(&snapshot, &[collision]).is_err());
    assert_eq!(std::fs::read(user_json).unwrap(), b"user data kept intact");
}

#[test]
fn text_result_choice_and_save_keep_authored_words_and_exclude_unlinked_review_reply() {
    use slate_doc::scene::HiddenOutputRef;
    use slate_doc::text_answers::{TextAnswerRef, TextOutputRef, TextReview};
    let (mut h, host) = super::super::tests::fixture("save-public-text-review");
    let mut agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(host).unwrap())
        .unwrap()
        .clone();
    let response = TextAnswerRef {
        session: agent.session.clone(),
        through: 2,
    };
    agent.generator.text_response_only = true;
    agent.generator.text_reviews = vec![TextReview {
        id: "review-request".into(),
        group: "request-group".into(),
        source: "Public authored words".into(),
        source_revision: None,
        selection: None,
        question: "Revise it".into(),
        text: "PRIVATE_UNLINKED_PROPOSAL".into(),
        edit: true,
        output: TextOutputRef {
            review_id: Some("review-request".into()),
            source: slate_doc::SourceUri {
                locator: h.base.join("private-history.json").to_string_lossy().into(),
            },
            response: response.clone(),
        },
    }];
    let mut text = slate_doc::scene::TextNode::agent_note(agent);
    text.text = "Public authored words".into();
    h.app
        .patch_nodes(&[host], |node| node.kind = NodeKind::Text(text.clone()));
    let reference = HiddenOutputRef::Text {
        review_id: Some("review-request".into()),
        response,
    };
    let choice = h
        .app
        .prepare_result_output_mutation(host, &reference, false)
        .unwrap();
    assert!(h.app.commit_scene(vec![choice]));
    assert_eq!(
        h.app.text_answer_source(host).unwrap().0,
        "Public authored words"
    );
    let address = ResultAddress {
        tab: h.app.tab().id,
        host,
        binding: h.app.result_binding(host).unwrap(),
        outputs: vec!["review-request".into()],
        references: vec![reference],
        target: Some("review-request".into()),
        witness: None,
    };
    let (doc, nodes) = h.app.result_payload(&address);
    assert!(nodes
        .iter()
        .all(|node| slate_doc::agent_chat::agent(node).is_none()));
    let snapshot = SaveSnapshot {
        target: SaveTarget::Results(address),
        doc,
        nodes,
        workbook: None,
        extensions: vec!["txt".into()],
    };
    let destination = h.base.join("public-text.txt");
    worker::save(&snapshot, std::slice::from_ref(&destination)).unwrap();
    assert_eq!(
        std::fs::read_to_string(destination).unwrap(),
        "Public authored words"
    );
    assert_eq!(
        h.app.text_review(host).unwrap().text,
        "PRIVATE_UNLINKED_PROPOSAL"
    );
}

#[path = "tests/html.rs"]
mod html;
