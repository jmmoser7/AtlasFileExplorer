//! A clipboard picture stored under assets/pasted wires into a generator.

use super::*;

#[test]
fn a_pasted_image_resolves_as_a_generator_input() {
    let mut h = Harness::new("paste_generator");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let book = h.base.join("Book.slate");
    h.app.tab_mut().path = Some(book.clone());
    h.app.ai.config.workspace_dir = Some(h.base.clone());

    let pasted =
        atlas_core::workbook_assets::paste_dir(Some(&book), &atlas_core::index::data_dir())
            .join("paste-input.png");
    if let Some(dir) = pasted.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    image::RgbaImage::from_pixel(40, 20, image::Rgba([9, 9, 9, 255]))
        .save(&pasted)
        .unwrap();
    let item = h.app.item_for_path(&pasted).unwrap();
    let stored = h.app.doc().item(item).unwrap().path.clone();
    assert!(
        stored
            .to_string_lossy()
            .replace('\\', "/")
            .starts_with("assets/pasted/"),
        "pasted picture is a workbook locator, got {}",
        stored.display()
    );
    let picture = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(-400.0, 0.0, 200.0, 100.0),
        NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let picture = h.app.add_nodes(vec![picture])[0];

    h.app.place_agent_portal_at(Pos2::new(200.0, 0.0));
    let generator = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .rev()
        .find(|n| matches!(n.kind, NodeKind::Portal(_)))
        .unwrap()
        .id;
    h.app.set_agent_program(generator, "comfy");
    h.app.patch_nodes(&[generator], |n| {
        if let Some(agent) = slate_doc::agent_chat::agent_mut(n) {
            agent.instruction = "repaint this".into();
        }
    });
    let anchored = |node, side, t| slate_doc::scene::ConnectorEnd::Anchored { node, side, t };
    h.app
        .add_connector(
            anchored(picture, slate_doc::scene::Side::Right, 0.5),
            anchored(generator, slate_doc::scene::Side::Left, 0.25),
        )
        .unwrap();

    let request = h.app.generation_request(generator, false).unwrap();
    let image = request
        .inputs
        .on(atlas_agent::InputSlot::Media)
        .next()
        .and_then(|item| item.images.first().cloned())
        .expect("the pasted picture is a media input");
    let path = std::path::PathBuf::from(&image);
    assert!(
        path.is_file(),
        "generator input {image} is not a file the engine can open"
    );
    assert!(image.replace('\\', "/").ends_with("paste-input.png"));

    h.app.patch_nodes(&[picture], |n| {
        if let NodeKind::Image(img) = &mut n.kind {
            img.crop = slate_doc::scene::Crop {
                x: 0.5,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            };
        }
    });
    let request = h.app.generation_request(generator, false).unwrap();
    let image = request
        .inputs
        .on(atlas_agent::InputSlot::Media)
        .next()
        .and_then(|item| item.images.first().cloned())
        .expect("the cropped picture is a media input");
    let fed = image::open(&image).expect("the cropped input is a readable file");
    assert_eq!(
        (fed.width(), fed.height()),
        (20, 20),
        "a cropped pasted picture feeds its visible window"
    );
}
