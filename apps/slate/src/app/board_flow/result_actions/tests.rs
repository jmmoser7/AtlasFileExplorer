//! Golden interaction traces through the real board loop and shared actions.
use super::*;
use crate::app::tests::Harness;
mod expanded;
mod held_hover;

pub(super) fn fixture(tag: &str) -> (Harness, NodeId) {
    let (mut h, host) = super::super::tests::generator_with_output(tag);
    let mut session = atlas_agent::AgentSession {
        activity: vec![],
        approval: None,
        conversation: String::new(),
        artifacts: vec![],
        status: atlas_agent::AgentStatus::Idle,
        provider: "comfy".into(),
        turns: vec![],
        updated_at: 0,
        bundle: Default::default(),
        request: String::new(),
        usage: None,
    };
    for index in 1..5 {
        let path = h.base.join(format!("result-{index}.png"));
        image::RgbaImage::from_pixel(20, 10, image::Rgba([index * 40, 90, 170, 192]))
            .save(&path)
            .unwrap();
        session.bundle.images.push(atlas_agent::ImageOutput {
            id: format!("result-{index}"),
            request: "run".into(),
            source: path.to_string_lossy().into_owned(),
            ..Default::default()
        });
    }
    h.app.set_agent_session_for_test(host, session);
    h.app.agent_blur();
    h.app.board_sel = std::iter::once(host).collect();
    h.app.tab_mut().cam.offset = egui::vec2(240.0, 240.0);
    h.app.tab_mut().cam.z = 1.0;
    let shown = h.app.agent_active_output(host).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.fitted_agent_image_for_test(host) != Some(shown.as_str()) {
        h.frame();
        assert!(
            std::time::Instant::now() < deadline,
            "result preview did not fit: shown={shown}, fitted={:?}, host={:?}",
            h.app.fitted_agent_image_for_test(host),
            h.app.doc().scene.node(host).unwrap().rect
        );
        std::thread::yield_now();
    }
    // Fitting happens while painting; paint the next frame at the settled size.
    h.frame();
    (h, host)
}

fn point(h: &Harness, host: NodeId, index: usize) -> Pos2 {
    let body = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(host).unwrap().rect);
    let placement = h.app.result_strip_placement(host, body);
    let images = h.app.agent_images(host);
    let runs: Vec<_> = images.iter().map(|image| image.request.as_str()).collect();
    atlas_shell::home::album_index_rects(
        placement,
        &runs,
        images.len(),
        h.app.agent_shown_index(host).unwrap_or(0),
        h.app.board_xf().z,
    )
    .into_iter()
    .find(|(i, _)| *i == index)
    .unwrap()
    .1
    .center()
}

fn click(h: &mut Harness, at: Pos2, shift: bool, secondary: bool) {
    let modifiers = egui::Modifiers {
        shift,
        ..Default::default()
    };
    h.frame_with(|input| {
        input.modifiers = modifiers;
        input.events.push(egui::Event::PointerMoved(at));
    });
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.modifiers = modifiers;
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button: if secondary {
                    egui::PointerButton::Secondary
                } else {
                    egui::PointerButton::Primary
                },
                pressed,
                modifiers,
            });
        });
    }
    h.frame();
}

#[test]
fn result_strip_golden_trace_preserves_host_and_range_across_repeated_picks() {
    let (mut h, host) = fixture("result-strip-golden");
    for (index, shift, expected) in [
        (1, false, vec![1]),
        (3, true, vec![1, 2, 3]),
        (4, false, vec![4]),
        (0, false, vec![0]),
    ] {
        let at = point(&h, host, index);
        click(&mut h, at, shift, false);
        assert_eq!(
            h.app.board_sel.iter().copied().collect::<Vec<_>>(),
            [host],
            "thumbnail owns press and release before board pick"
        );
        assert_eq!(h.app.agent_shown_index(host), Some(index));
        let images = h.app.agent_images(host);
        let selection = h.app.result_selection(host);
        let actual: Vec<_> = images
            .iter()
            .enumerate()
            .filter_map(|(i, image)| selection.contains(&image.id).then_some(i))
            .collect();
        assert_eq!(actual, expected, "golden range selection");
        let menu = h
            .app
            .agents
            .flow
            .results
            .menu
            .as_ref()
            .expect("click opens result actions");
        assert_eq!(menu.address.outputs.len(), expected.len());
    }
    let before = h.app.tab().journal.undo_depth();
    let at = point(&h, host, 0);
    click(&mut h, at, false, true);
    let address = h
        .app
        .agents
        .flow
        .results
        .menu
        .as_ref()
        .unwrap()
        .address
        .clone();
    assert!(h.app.dispatch_image_result_action(
        &h.ctx,
        "portal.agent.result.copy",
        Some(&serde_json::to_string(&address).unwrap())
    ));
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        before,
        "copy changes no scene commands"
    );
    assert_eq!(h.app.board_clipboard.len(), 1);
    assert!(
        h.app.os_clipboard.last_write().unwrap().png.is_some(),
        "copy has usable image pixels"
    );
}

#[test]
fn external_fallback_chrome_cannot_own_a_shifted_thumbnail_pick() {
    let (mut h, host) = fixture("result-strip-external-fallback-clearance");
    h.app.patch_nodes(&[host], |node| {
        node.rect = slate_doc::scene::WorldRect::new(-60.0, -40.0, 120.0, 80.0);
        node.rotation_deg = 17.0;
    });
    h.app.shape_properties.panel = Some(crate::app::board_properties::Panel::Agent);
    for _ in 0..12 {
        h.frame();
    }
    let body = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(host).unwrap().rect);
    let (tab, chrome_host, chrome_bottom) = h.app.shape_properties.result_chrome_bottom.unwrap();
    assert_eq!((tab, chrome_host), (h.app.tab().id, host));
    assert!(chrome_bottom > body.bottom());
    let images = h.app.agent_images(host);
    let runs: Vec<_> = images.iter().map(|image| image.request.as_str()).collect();
    let old = atlas_shell::home::album_index_rects(
        body,
        &runs,
        images.len(),
        h.app.agent_shown_index(host).unwrap_or(0),
        h.app.board_xf().z,
    );
    assert!(old.iter().any(|(_, rect)| rect.center().y <= chrome_bottom));
    let placement = h.app.result_strip_placement(host, body);
    assert!(placement.bottom() >= chrome_bottom);

    let expected_id = h.app.agent_images(host)[1].id.clone();
    let at = point(&h, host, 1);
    click(&mut h, at, false, false);
    assert_eq!(h.app.agent_shown_index(host), Some(1));
    assert_eq!(
        h.app.agent_active_output(host).as_deref(),
        Some(expected_id.as_str())
    );
    assert_eq!(
        h.app.shape_properties.panel,
        Some(crate::app::board_properties::Panel::Agent),
        "thumbnail click keeps the generator editor open instead of activating fallback controls"
    );
    let menu = h
        .app
        .agents
        .flow
        .results
        .menu
        .as_ref()
        .expect("thumbnail click opens its actions");
    assert_eq!(menu.address.outputs, vec![expected_id]);

    h.app.board_sel.clear();
    h.frame();
    let (fade_tab, fade_host, fade_bottom) = h
        .app
        .shape_properties
        .result_chrome_bottom
        .expect("freshly painted fading chrome still occupies its current footprint");
    assert_eq!((fade_tab, fade_host), (tab, host));
    assert!(fade_bottom > body.bottom());
    assert!(h.app.result_strip_placement(host, body).bottom() >= fade_bottom);
    let mut time = h.ctx.input(|input| input.time);
    for _ in 0..30 {
        time += 1.0 / 60.0;
        h.frame_with(|input| input.time = Some(time));
    }
    assert_eq!(h.app.shape_properties.result_chrome_bottom, None);
    h.app.shape_properties.result_chrome_bottom =
        Some((fade_tab.wrapping_add(1), fade_host, fade_bottom));
    assert_eq!(
        h.app.result_strip_placement(host, body),
        body,
        "the old tab/selection footprint does not displace the next strip"
    );
}

#[test]
fn painted_strip_corridor_follows_host_and_zoom_without_rescaling_its_spacing() {
    let (mut h, host) = fixture("result-strip-retained-geometry");
    let body = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(host).unwrap().rect);
    let zoom = h.app.board_xf().z;
    let offset = (point(&h, host, 1) - body.center_bottom()) / zoom;
    let cached = h.app.agents.flow.results.strips[&(h.app.tab().id, host)];
    let ctx = egui::Context::default();
    for (size, next_zoom) in [
        (body.size(), zoom),
        (body.size() * 0.5, zoom * 0.5),
        (body.size() * 2.0, zoom),
    ] {
        let moved = Rect::from_min_size(body.min + egui::vec2(300.0, -50.0), size);
        let at = moved.center_bottom() + offset * next_zoom;
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::PointerMoved(at)],
                ..Default::default()
            },
            |ctx| assert!(h.app.result_strip_hovered(ctx, host, moved, next_zoom)),
        );
        assert_eq!(
            h.app.agents.flow.results.strips[&(h.app.tab().id, host)],
            cached,
            "hover transforms the retained paint record without rebuilding it"
        );
    }
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::PointerMoved(point(&h, host, 1))],
            ..Default::default()
        },
        |ctx| assert!(h.app.result_strip_hovered(ctx, host, body, zoom)),
    );
    h.app.patch_nodes(&[host], |node| node.hidden = true);
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        assert!(!h.app.result_strip_hovered(ctx, host, body, zoom));
    });
    h.app.remember_result_strip(host, body, Rect::NOTHING, zoom);
    assert!(!h
        .app
        .agents
        .flow
        .results
        .strips
        .contains_key(&(h.app.tab().id, host)));
}

#[test]
fn result_actions_reject_rebound_or_closed_workbook_identity() {
    let (mut h, host) = fixture("result-stale-actions");
    let address = ResultAddress {
        tab: h.app.tab().id,
        host,
        binding: h.app.result_binding(host).unwrap(),
        outputs: vec![h.app.agent_images(host)[1].id.clone()],
    };
    assert!(h.app.result_address_valid(&address));
    let mut missing = address.clone();
    missing.outputs.push("missing-result".into());
    assert!(!h.app.result_address_valid(&missing));
    let mut closed = address.clone();
    closed.tab += 1;
    assert!(!h.app.result_address_valid(&closed));
    h.app.patch_nodes(&[host], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().session = "different".into()
    });
    assert!(!h.app.result_address_valid(&address));
    assert!(!h.app.dispatch_image_result_action(
        &h.ctx,
        "portal.agent.result.copy",
        Some(&serde_json::to_string(&address).unwrap())
    ));
}

#[test]
fn batch_copy_to_canvas_is_one_undo_and_keeps_each_output_identity() {
    let (mut h, host) = fixture("result-copy-batch-undo");
    let images = h.app.agent_images(host);
    let placements: Vec<_> = images
        .iter()
        .skip(1)
        .take(3)
        .enumerate()
        .map(|(index, image)| {
            (
                image.id.clone(),
                Pos2::new(800.0 + index as f32 * 500.0, 0.0),
            )
        })
        .collect();
    let before = h.app.doc().scene.nodes.len();
    let depth = h.app.tab().journal.undo_depth();
    let ids = h.app.copy_agent_results_to_canvas(host, &placements);
    assert_eq!(ids.len(), 3);
    assert_eq!(h.app.doc().scene.nodes.len(), before + 3);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    for (index, id) in ids.iter().enumerate() {
        let NodeKind::Image(image) = &h.app.doc().scene.node(*id).unwrap().kind else {
            unreachable!()
        };
        assert!(image.agent.is_none());
        assert_eq!(
            h.app.doc().item(image.item).unwrap().path,
            PathBuf::from(&images[index + 1].source)
        );
    }
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), before);
    assert!(ids.iter().all(|id| h.app.doc().scene.node(*id).is_none()));
}

#[test]
fn copying_a_painted_result_pastes_an_independent_layer_stack() {
    use slate_doc::image_paint::{ImageResultLayers, PaintLayer, PaintLayerId};
    use slate_doc::scene::{ShapeNode, WorldRect};
    let (mut h, host) = fixture("result-copy-painted-stack");
    let output = h.app.agent_images(host)[1].id.clone();
    let mark = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.1, 0.1, 0.5, 0.5),
        NodeKind::Shape(ShapeNode {
            shape: slate_doc::scene::ShapeKind::Rect,
            fill: Some(slate_doc::scene::Rgba::opaque(250, 30, 20)),
            stroke: Default::default(),
            corner: Default::default(),
            sides: 4,
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let original_mark = mark.id;
    let mut layer = PaintLayer::new(PaintLayerId(1));
    layer.nodes.push(mark);
    let mut stack = ImageResultLayers::new(ImageResultId::Output(output.clone()));
    stack.layers.push(layer);
    h.app.patch_nodes(&[host], |node| {
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        image.result_layers.push(stack.clone());
    });
    let address = ResultAddress {
        tab: h.app.tab().id,
        host,
        binding: h.app.result_binding(host).unwrap(),
        outputs: vec![output],
    };
    let pick = h.app.agent_shown_index(host);
    let before = h.app.tab().journal.undo_depth();
    assert!(h.app.dispatch_image_result_action(
        &h.ctx,
        "portal.agent.result.copy",
        Some(&serde_json::to_string(&address).unwrap())
    ));
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    assert_eq!(
        h.app.agent_shown_index(host),
        pick,
        "copy does not alter the displayed result"
    );
    assert_eq!(h.app.board_paste(None, Some(Pos2::new(900.0, 900.0))), 1);
    let pasted = *h.app.board_sel.iter().next().unwrap();
    let NodeKind::Image(image) = &h.app.doc().scene.node(pasted).unwrap().kind else {
        unreachable!()
    };
    assert_eq!(image.paint_layers.len(), 1);
    assert_eq!(image.paint_layers[0].nodes.len(), 1);
    assert_ne!(image.paint_layers[0].nodes[0].id, original_mark);
    assert_ne!(image.paint_layers[0].nodes[0].id, pasted);
}
