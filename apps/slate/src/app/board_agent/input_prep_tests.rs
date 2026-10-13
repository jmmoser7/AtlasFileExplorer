//! Request capture, tab identity, and Stop while image inputs are pending.

use super::*;

#[test]
fn authored_image_input_is_captured_as_a_reference_before_worker_completion() {
    use slate_doc::scene::{ConnectorEnd, Crop, ImageNode, Side};

    let (mut h, generator, _) = super::test_support::generator_with_note("clip_input", "portrait");
    let workbook = h.base.join("Board.slate");
    h.app.tab_mut().path = Some(workbook);
    let folder = h.base.join("assets");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("authored.png");
    image::RgbImage::new(12, 8).save(&path).unwrap();
    let item = h.app.doc_mut().add_item(
        "assets/authored.png".into(),
        "authored.png",
        1,
        0,
        "authored-key",
    );
    let mut image_node = ImageNode::new(item);
    image_node.adjust.brightness = 1.1;
    image_node.crop = Crop {
        x: 0.1,
        y: 0.1,
        w: 0.8,
        h: 0.8,
    };
    let image_node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(-300.0, 300.0, 120.0, 80.0),
        NodeKind::Image(image_node),
    );
    let image_id = image_node.id;
    let wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: image_id,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: generator,
            side: Side::Bottom,
            t: 0.4,
        },
    );
    h.app.add_nodes(vec![image_node, wire]);

    let inputs = h.app.agent_input_snapshot(generator).unwrap();
    let source = path.to_string_lossy().into_owned();
    assert!(inputs.wired.iter().any(|item| {
        item.images
            .iter()
            .any(|source| std::path::Path::new(source) == path)
    }));
    let mut prep = h.app.begin_agent_input_preparation(inputs).unwrap();
    assert_eq!(prep.replacements.len(), 1);
    assert_eq!(prep.replacements[0].0, image_id);
    assert!(prep.inputs.wired.iter().any(|item| {
        item.images
            .iter()
            .any(|source| std::path::Path::new(source) == path)
    }));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match h.app.poll_agent_input_preparation(&mut prep) {
            Ok(true) => break,
            Ok(false) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            result => panic!("relative workbook image preparation did not finish: {result:?}"),
        }
    }
    let prepared = prep
        .inputs
        .wired
        .iter()
        .find(|input| input.node == image_id.0)
        .and_then(|input| input.images.first())
        .expect("the authored image wire still has its prepared reference")
        .clone();
    assert_ne!(prepared, source);
    assert!(std::path::Path::new(&prepared).is_file());
}

#[test]
fn a_generation_queued_for_another_tab_is_never_dispatched() {
    let (mut h, generator, _) =
        super::test_support::generator_with_note("stale_generation", "portrait");
    let request = h.app.generation_request(generator, false).unwrap();
    let inputs = h
        .app
        .begin_agent_input_preparation(request.inputs.clone())
        .unwrap();
    let stale_doc = h.app.tab().id.wrapping_add(1);
    h.app
        .agents
        .comfy_queue
        .entry(generator)
        .or_default()
        .push_back(super::runtime::QueuedGeneration {
            doc: stale_doc,
            request,
            inputs,
        });

    h.app.pump_comfy_queue();
    assert!(h.app.agents.dispatched.is_empty());
    assert!(!h.app.agents.comfy_queue.contains_key(&generator));
}

#[test]
fn copied_layer_input_waits_for_bake_before_a_plain_image_can_be_dispatched() {
    use slate_doc::scene::{ConnectorEnd, ImageNode, Side};

    let (mut h, generator, _) = super::test_support::generator_with_note("bake_gate", "portrait");
    let workbook = h.base.join("Board.slate");
    h.app.tab_mut().path = Some(workbook.clone());
    let path = h.base.join("assets").join("pending-layer.png");
    let item = h.app.doc_mut().add_item(
        std::path::PathBuf::from("assets/pending-layer.png"),
        "pending-layer.png",
        0,
        0,
        "pending-layer",
    );
    let image_node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(-300.0, 300.0, 120.0, 80.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let image_id = image_node.id;
    let wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: image_id,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: generator,
            side: Side::Bottom,
            t: 0.4,
        },
    );
    h.app.add_nodes(vec![image_node, wire]);
    let token = crate::app::board_image_layers::LayerBakeReadiness::new();
    let tab = h.app.tab().id;
    h.app
        .layer_bakes
        .register_readiness(tab, item, token.clone());

    let inputs = h.app.agent_input_snapshot(generator).unwrap();
    let mut prep = h.app.begin_agent_input_preparation(inputs).unwrap();
    assert_eq!(prep.replacements.len(), 1);
    assert!(!path.exists());
    assert!(matches!(
        h.app.poll_agent_input_preparation(&mut prep),
        Ok(false)
    ));

    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::new(12, 8).save(&path).unwrap();
    token.resolve(&Ok((12, 1)));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match h.app.poll_agent_input_preparation(&mut prep) {
            Ok(true) => break,
            Ok(false) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            result => panic!("copied layer input did not become ready: {result:?}"),
        }
    }
    let prepared = prep
        .inputs
        .wired
        .iter()
        .find(|input| input.node == image_id.0)
        .and_then(|input| input.images.first())
        .expect("the copied image wire remains captured");
    assert_eq!(std::path::Path::new(prepared), path.as_path());
}

#[test]
fn failed_layer_bake_fails_input_preparation_with_a_clear_error() {
    let (h, generator, _) = super::test_support::generator_with_note("bake_failure", "portrait");
    let token = crate::app::board_image_layers::LayerBakeReadiness::new();
    token.resolve(&Err("Disk full".into()));
    let mut prep = super::input_prep::InputPreparation {
        inputs: atlas_agent::InputSnapshot::default(),
        replacements: vec![],
        bake_dependencies: vec![token],
    };
    let error = h
        .app
        .poll_agent_input_preparation(&mut prep)
        .expect_err("failed bake must stop dispatch");
    assert!(error.contains("copied paint layer could not be baked"));
    assert!(error.contains("Disk full"));
    assert!(!h.app.agent_is_running(generator));
}

#[test]
fn stop_removes_a_chat_send_waiting_for_image_preparation() {
    let (mut h, portal, _) = super::test_support::generator_with_note("cancel_input", "portrait");
    h.app.agents.chat_preparing.insert(
        portal,
        super::runtime::PendingChatSend {
            doc: h.app.tab().id,
            portal,
            composer: portal,
            prompt: "portrait".into(),
            workspace: h.base.clone(),
            typing_here: false,
            inputs: super::input_prep::InputPreparation {
                inputs: atlas_agent::InputSnapshot::default(),
                replacements: vec![],
                bake_dependencies: vec![crate::app::board_image_layers::LayerBakeReadiness::new()],
            },
        },
    );

    assert!(h.app.cancel_preparing_agent_send(portal));
    assert!(!h.app.agents.chat_preparing.contains_key(&portal));
    assert!(!h.app.agent_is_running(portal));
}
