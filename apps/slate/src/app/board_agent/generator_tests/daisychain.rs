//! A generated album keeps its identities through downstream input capture.
use super::*;

fn chain(h: &mut crate::app::tests::Harness, source: NodeId) -> NodeId {
    let detail = serde_json::json!({"source":source.0,"kind":"image","prompt":"evening light"});
    assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
    *h.app.board_sel.iter().next().unwrap()
}

fn solid_layer(h: &mut crate::app::tests::Harness, color: [u8; 4]) -> slate_doc::PaintLayer {
    use slate_doc::scene::{Corner, Rgba, ShapeKind, ShapeNode, Stroke, WorldRect};
    let child = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 1.0, 1.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: Some(Rgba(color)),
            stroke: Stroke::none(),
            corner: Corner::Square,
            sides: 4,
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let mut layer = slate_doc::PaintLayer::new(slate_doc::image_paint::PaintLayerId(child.id.0));
    layer.nodes.push(child);
    layer
}

fn finish_preparation(
    h: &crate::app::tests::Harness,
    mut preparation: crate::app::board_agent::input_prep::InputPreparation,
    source: NodeId,
) -> image::RgbaImage {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if h.app
            .poll_agent_input_preparation(&mut preparation)
            .unwrap()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "captured pinned image preparation finishes"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let path = preparation
        .inputs
        .wired
        .iter()
        .find(|item| item.node == source.0)
        .and_then(|item| item.images.first())
        .unwrap();
    image::open(path).unwrap().to_rgba8()
}

#[test]
fn a_pinned_picture_prepares_its_pixels_while_the_host_shows_another_edited_result() {
    let (mut h, source) = generator_with_output("pinned-edited-sibling");
    let original = h.app.agent_shown_path(source).unwrap();
    image::RgbImage::from_fn(8, 8, |x, y| {
        image::Rgb([20 + x as u8 * 20, 40 + y as u8 * 10, 80])
    })
    .save(&original)
    .unwrap();
    h.app.pick_agent_result(source, 0);
    let target = chain(&mut h, source);
    album::append_result(&mut h, source, "second");
    h.app.pick_agent_result(source, 1);
    let sibling = h.app.agent_shown_path(source).unwrap();
    image::RgbImage::from_pixel(20, 10, image::Rgb([0, 0, 255]))
        .save(sibling)
        .unwrap();
    let other_layers = solid_layer(&mut h, [0, 255, 0, 255]);
    h.app.patch_nodes(&[source], |node| {
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        image.crop = Crop {
            x: 0.25,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        image.flip_x = true;
        image.adjust.brightness = 0.8;
        image.paint_layers = vec![other_layers.clone()];
    });
    let request = h.app.generation_request(target, false).unwrap();
    assert_eq!(
        std::path::Path::new(&request.inputs.wired[0].images[0]),
        original
    );
    let preparation = h.app.begin_agent_input_preparation(request.inputs).unwrap();
    let prepared = finish_preparation(&h, preparation, source);
    assert_eq!(
        prepared.dimensions(),
        (4, 8),
        "crop applies to the pinned 8px source"
    );
    let pixel = prepared.get_pixel(0, 0);
    assert!(
        pixel[0] > 80 && pixel[0] < 110,
        "captured source's mirror/filter survive: {pixel:?}"
    );
    assert!(
        pixel[1] < 100 && pixel[2] < 100,
        "neither the sibling's blue pixels nor its legacy green layer leaks into the pin: {pixel:?}"
    );
    assert_eq!(h.app.agent_active_output(source).as_deref(), Some("second"));
}

#[test]
fn a_pinned_picture_keeps_its_own_layer_stack_and_submission_crop_after_further_edits() {
    use slate_doc::image_paint::{ImageResultId, ImageResultLayers};
    let (mut h, source) = generator_with_output("pinned-own-stack-capture");
    h.app.pick_agent_result(source, 0);
    let target = chain(&mut h, source);
    album::append_result(&mut h, source, "second");
    h.app.pick_agent_result(source, 1);
    let red = solid_layer(&mut h, [255, 0, 0, 255]);
    let blue = solid_layer(&mut h, [0, 0, 255, 255]);
    h.app.patch_nodes(&[source], |node| {
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        let mut first = ImageResultLayers::new(ImageResultId::Output("img-1".into()));
        first.add_layer(red.clone());
        let mut second = ImageResultLayers::new(ImageResultId::Output("second".into()));
        second.add_layer(blue.clone());
        image.result_layers = vec![first, second];
        image.crop = Crop {
            x: 0.25,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        image.flip_x = true;
    });
    let request = h.app.generation_request(target, false).unwrap();
    let preparation = h.app.begin_agent_input_preparation(request.inputs).unwrap();
    h.app.patch_nodes(&[source], |node| {
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        image.result_layers[0].layers.clear();
        image.crop = Crop::full();
        image.flip_x = false;
    });
    let prepared = finish_preparation(&h, preparation, source);
    assert_eq!(
        prepared.dimensions(),
        (4, 8),
        "capture retains the submitted crop"
    );
    assert_eq!(
        prepared.get_pixel(2, 4).0,
        [255, 0, 0, 255],
        "captured A uses its red layer, even after edits and while B's blue stack is shown"
    );
    assert_eq!(h.app.agent_active_output(source).as_deref(), Some("second"));
}

#[test]
fn a_picked_generated_picture_remains_available_to_its_pinned_daisychain() {
    let (mut h, source) = generator_with_output("picked-daisychain");
    album::append_result(&mut h, source, "second");
    album::append_result(&mut h, source, "third");
    h.app.pick_agent_result(source, 1);
    let selected = h.app.agent_shown_path(source).unwrap();
    let target = chain(&mut h, source);
    let request = h.app.generation_request(target, false).unwrap();
    let primary = request
        .inputs
        .on(atlas_agent::InputSlot::Media)
        .next()
        .unwrap();
    assert_eq!(primary.images, [selected.to_string_lossy()]);
    assert!(std::path::Path::new(&primary.images[0]).is_file());

    h.app.pick_agent_result(source, 2);
    let next = h.app.generation_request(target, false).unwrap();
    assert_eq!(
        next.inputs.wired[0].images, primary.images,
        "an authored output pin retains its result"
    );
    assert_eq!(
        request.inputs.wired[0].images, primary.images,
        "submitted input is immutable"
    );
}

#[test]
fn an_unpinned_daisychain_follows_the_shown_result_and_then_the_newest() {
    let (mut h, source) = generator_with_output("active-daisychain");
    album::append_result(&mut h, source, "second");
    h.app.pick_agent_result(source, 0);
    let target = chain(&mut h, source);
    let wire = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|node| {
            let NodeKind::Connector(connector) = &node.kind else {
                return None;
            };
            let binding = connector.binding.as_ref()?;
            (slate_doc::agent_inputs::endpoint_node(binding.target_end(connector)) == Some(target))
                .then_some(node.id)
        })
        .unwrap();
    h.app.patch_nodes(&[wire], |node| {
        let NodeKind::Connector(connector) = &mut node.kind else {
            unreachable!()
        };
        connector.binding.as_mut().unwrap().output = None;
    });
    let first = h.app.generation_request(target, false).unwrap();
    h.app.pick_agent_result(source, 1);
    let picked = h.app.generation_request(target, false).unwrap();
    assert_ne!(first.inputs.wired[0].images, picked.inputs.wired[0].images);
    assert_eq!(
        picked.inputs.wired[0].images,
        [h.app.agent_shown_path(source).unwrap().to_string_lossy()]
    );
    h.app.patch_nodes(&[source], |node| {
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        image.item = slate_doc::ItemId::NONE;
    });
    album::append_result(&mut h, source, "third");
    let newest = h.app.generation_request(target, false).unwrap();
    assert!(newest.inputs.wired[0].images[0].ends_with("third.png"));
    assert_eq!(
        picked.inputs.wired[0].images[0],
        h.base.join("second.png").to_string_lossy()
    );
}

#[test]
fn a_missing_explicit_output_pin_is_reported_instead_of_substituting_another_image() {
    let (mut h, source) = generator_with_output("missing-daisychain-pin");
    h.app.pick_agent_result(source, 0);
    let target = chain(&mut h, source);
    let wire = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|node| {
            let NodeKind::Connector(connector) = &node.kind else {
                return None;
            };
            connector
                .binding
                .as_ref()
                .is_some_and(|binding| {
                    slate_doc::agent_inputs::endpoint_node(binding.target_end(connector))
                        == Some(target)
                })
                .then_some(node.id)
        })
        .unwrap();
    h.app.patch_nodes(&[wire], |node| {
        let NodeKind::Connector(connector) = &mut node.kind else {
            unreachable!()
        };
        connector.binding.as_mut().unwrap().output = Some("unavailable".into());
    });
    assert!(h
        .app
        .generation_request(target, false)
        .unwrap_err()
        .contains("pinned image output is unavailable"));
}
