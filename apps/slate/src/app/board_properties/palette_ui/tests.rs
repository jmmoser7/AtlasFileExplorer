use super::*;
use crate::app::tests::Harness;

mod fallback;
mod media_snapshot;
mod pointer_ownership;

impl SlateApp {
    /// Full shared geometry for an observed, active single-image presentation.
    /// Tests keep reveal clipping separate from the content's stable layout.
    pub(crate) fn observed_image_palette_layout_for_test(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        kind: chrome::ImagePanelKind,
    ) -> (bool, chrome::ImagePresentationLayout, Rect) {
        let state = self
            .shape_properties
            .last_chrome
            .as_ref()
            .expect("observed image chrome");
        assert_eq!(
            state.host,
            Some((self.tab().id, id)),
            "current tab and host"
        );
        assert!(self.board_sel.contains(&id), "active image fixture");
        let xf = self.board_xf();
        let host = self.doc().scene.node(id).expect("current image host");
        let image = xf.rect_w2s(host.rect.rotated_bounds(host.rotation_deg));
        let presentation = if self.shape_properties.panel.is_some() {
            chrome::BlisterPresentation::Overlay
        } else {
            chrome::BlisterPresentation::Collapsed
        };
        let blister = chrome::blister_layout(image, xf.z, state.count, presentation);
        let strip = if state.image {
            blister.band
        } else {
            chrome::strip_rect_below(
                xf.w2s(state.bottom),
                state.count,
                xf.z,
                chrome::strip_fade(ctx, Id::new("selection_property_strip"), true),
            )
        };
        let mut layout = if state.image {
            chrome::image_presentation_layout(image, xf.z, kind)
        } else {
            chrome::image_editor_presentation_layout(image, strip, xf.z, kind, self.canvas_rect)
        };
        if state.image {
            layout.blister = blister;
        }
        (state.image, layout, strip)
    }
}

pub(super) fn record_text_button(
    app: &SlateApp,
    ui: &egui::Ui,
    response: &egui::Response,
    item: StripItem,
    live: bool,
    grip_under: bool,
) {
    let ctx = ui.ctx();
    let trace_id = Id::new("text-button-test-trace");
    let stored = ctx.data(|d| d.get_temp::<Vec<String>>(trace_id)).is_some();
    let print = std::env::var_os("SLATE_TEST_CHROME_TRACE").is_some();
    if !stored && !print {
        return;
    }
    let interaction = ctx.interaction_snapshot(|s| {
        format!(
            "clicked={:?} dragged={:?} hovered={:?} contains={:?}",
            s.clicked, s.dragged, s.hovered, s.contains_pointer
        )
    });
    let pointer = ctx.input(|i| {
        format!(
            "time={} pos={:?} origin={:?} down={} pressed={} released={} events={:?}",
            i.time,
            i.pointer.latest_pos(),
            i.pointer.press_origin(),
            i.pointer.primary_down(),
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.events
        )
    });
    let owner = ctx.pointer_latest_pos().and_then(|p| ctx.layer_id_at(p));
    let tooltip_id = response.id.with(0usize);
    let tooltip = egui::containers::AreaState::load(ctx, tooltip_id);
    let owner_area = owner.and_then(|layer| egui::containers::AreaState::load(ctx, layer.id));
    let response_area = egui::containers::AreaState::load(ctx, response.layer_id.id);
    let line = format!(
        "{pointer}; item={item:?} response id={:?} layer={:?} response_area={response_area:?} rect={:?} interact_rect={:?} clip={:?} enabled={} hovered={} contains={} down={} clicked={} live={live} grip_under={grip_under}; owner={owner:?} owner_area={owner_area:?} text_tooltip_id={tooltip_id:?} text_tooltip={tooltip:?} panel={:?} selected={:?}; {interaction}",
        response.id,
        response.layer_id,
        response.rect,
        response.interact_rect,
        ui.clip_rect(),
        response.enabled(),
        response.hovered(),
        response.contains_pointer(),
        response.is_pointer_button_down_on(),
        response.clicked(),
        app.shape_properties.panel,
        app.board_sel
    );
    if print {
        eprintln!("[SLATE_TEST_CHROME_TRACE] {line}");
    }
    if stored {
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<Vec<String>>(trace_id)
                .push(line)
        });
    }
}

fn frame(h: &mut Harness, time: &mut f64) -> egui::FullOutput {
    frame_with_events(h, time, vec![])
}

fn frame_with_events(
    h: &mut Harness,
    time: &mut f64,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    *time += 1.0 / 60.0;
    let xf = h.app.board_xf();
    h.ctx.clone().run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
            time: Some(*time),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                h.app.shape_properties_ui(ui, &xf);
            });
        },
    )
}

#[test]
fn a_generator_popup_keeps_its_visible_hits_while_the_image_apron_opens() {
    for (host, width, height, attached_route) in [
        ("attached", 680.0, 380.0, true),
        ("external", 360.0, 240.0, false),
    ] {
        for zoom in [0.25, 0.8, 1.0, 2.0] {
            let mut h = Harness::new(&format!("image-generator-popup-reveal-{host}-{zoom}"));
            h.app.leave_home();
            h.app.ensure_work_tab();
            h.app.set_board_tool(BoardTool::Select);
            let id = crate::app::board_flow::tests::place(&mut h, "photo.png", 0.0);
            let rect = WorldRect::new(-width * 0.5, -height * 0.5, width, height);
            h.app.doc_mut().scene.node_mut(id).unwrap().rect = rect;
            h.app.tab_mut().cam.z = zoom;
            h.app.board_sel = std::iter::once(id).collect();
            h.frame();

            // Anchor the apron near the top of the screen at every zoom. This
            // moves only the camera; the host's world-space eligibility is fixed.
            let canvas = h.app.canvas_rect;
            let xf = h.app.board_xf();
            let screen_anchor = Pos2::new(canvas.left() + 40.0, canvas.top() + 60.0);
            let world_anchor = Pos2::new(rect.x, rect.y + rect.h);
            h.app.tab_mut().cam.offset =
                world_anchor.to_vec2() - (screen_anchor - xf.center) / zoom;

            let mut time = h.ctx.input(|i| i.time);
            for _ in 0..12 {
                frame(&mut h, &mut time);
            }
            let width_units = if attached_route {
                width
            } else {
                chrome::EDITOR_WIDTH
            };
            let (actual_attached, _, _) = h.app.observed_image_palette_layout_for_test(
                &h.ctx,
                id,
                chrome::ImagePanelKind::Custom {
                    main_height: h.app.image_agent_body_height_for_width(
                        &h.ctx.layer_painter(egui::LayerId::background()),
                        id,
                        width_units,
                    ),
                },
            );
            assert_eq!(
                actual_attached, attached_route,
                "observed image editor route for {host} {width}x{height} at zoom {zoom}"
            );

            h.app.open_agent_model_menu_for_test(id);
            h.app.shape_properties.panel = Some(Panel::Agent);
            frame(&mut h, &mut time); // The new popup's sizing pass is invisible.
            let output = frame(&mut h, &mut time);
            let xf = h.app.board_xf();
            let image = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
            let canvas = h.app.canvas_rect;
            if atlas_shell::canvas_scale::too_small(12.0 * zoom) {
                assert!(h.app.shape_properties.chrome_hits.is_empty());
                assert!(
                    output.shapes.iter().all(|shape| {
                        !matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains('·'))
                    }),
                    "a hidden popup exposes no model-row text at this zoom"
                );
                assert_eq!(h.app.shape_properties.panel, Some(Panel::Agent));
                assert_eq!(
                    h.app
                        .shape_properties
                        .last_chrome
                        .as_ref()
                        .and_then(|chrome| chrome.host),
                    Some((h.app.tab().id, id))
                );
                assert_eq!(h.app.board_sel, std::iter::once(id).collect());
                continue;
            }
            let (point, row_clip) = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    let egui::Shape::Text(text) = &shape.shape else {
                        return None;
                    };
                    let row_clip = shape.clip_rect;
                    let point = text.visual_bounding_rect().center();
                    (text.galley.text().contains('·')
                        && row_clip.is_positive()
                        && row_clip.top() > image.bottom()
                        && row_clip.height() < 220.0 * xf.z
                        && row_clip.contains(point)
                        && canvas.contains(point))
                    .then_some((point, row_clip))
                })
                .max_by(|a, b| a.0.y.total_cmp(&b.0.y))
                .expect("the model popup paints a visible row above the opening apron's clip");
            assert!(
                row_clip.contains(point),
                "the selected row point is clipped in"
            );
            assert!(
                canvas.contains(point),
                "the selected row point is on the canvas"
            );
            assert!(
                h.app
                    .shape_properties
                    .chrome_hits
                    .iter()
                    .any(|hit| hit.contains(point)),
                "a visible popup row must retain its full hit rectangle during reveal"
            );
            frame_with_events(&mut h, &mut time, vec![egui::Event::PointerMoved(point)]);
            for pressed in [true, false] {
                frame_with_events(
                    &mut h,
                    &mut time,
                    vec![egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
                assert_eq!(
                    h.app.shape_properties.panel,
                    Some(Panel::Agent),
                    "the popup extension is not an outside-apron dismissal"
                );
                assert!(h.app.board_sel.contains(&id));
            }
        }
    }
}

#[test]
fn a_picked_vertex_strip_stays_beside_the_vertex_instead_of_the_whole_shape() {
    let mut h = Harness::new("picked_vertex_strip_anchor");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    let id = super::super::tests::rectangle(&mut h, WorldRect::new(0.0, 0.0, 200.0, 300.0), 0.0);
    h.app.board_sel.insert(id);
    h.app.direct.grip_points.pick(id, 0, false);
    let mut time = 0.0;
    let _ = frame(&mut h, &mut time);
    let picked = h.app.picked_vertex_bounds().expect("picked corner bounds");
    for _ in 0..12 {
        let _ = frame(&mut h, &mut time);
    }
    let chrome = h.app.shape_properties.last_chrome.as_ref().unwrap();
    assert!(chrome.picked_vertices);
    assert!((chrome.bottom.y - (picked.y + picked.h)).abs() < 0.001);
    assert!(
        chrome.bottom.y < 50.0,
        "strip followed the whole rectangle's bottom"
    );
}

#[test]
fn deselecting_a_rectangular_portal_keeps_its_fading_palette_below_the_portal() {
    let mut h = Harness::new("portal-blister-dismissal");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-340.0, -190.0, 680.0, 380.0),
        NodeKind::Portal(scene::PortalNode::unbound_web("Page")),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_sel.insert(id);
    h.frame();
    let mut time = 0.0;
    for _ in 0..12 {
        let _ = frame(&mut h, &mut time);
    }
    let host = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let upper = Rect::from_min_max(
        Pos2::new(host.left() - 50.0, host.top() - 100.0),
        Pos2::new(host.right() + 50.0, host.top() - 2.0),
    );
    let chrome_state = h.app.shape_properties.last_chrome.as_ref().unwrap();
    assert!(chrome_state.blister);
    let lower = chrome::blister_layout(
        host,
        h.app.board_xf().z,
        chrome_state.count,
        chrome::BlisterPresentation::Collapsed,
    )
    .band;
    h.app.board_sel.clear();
    let mut fading_frames = 0;
    for _ in 0..12 {
        let out = frame(&mut h, &mut time);
        if h.app.shape_properties.last_chrome.is_some() {
            fading_frames += 1;
            let bounds: Vec<_> = out
                .shapes
                .iter()
                .map(|shape| shape.shape.visual_bounding_rect())
                .filter(Rect::is_positive)
                .collect();
            assert!(
                !bounds.iter().any(|rect| upper.contains_rect(*rect)),
                "a fading bottom blister must never become the old upper toolbar"
            );
            assert!(
                bounds.iter().any(|rect| lower.contains_rect(*rect)),
                "the dismissal still paints the bottom blister"
            );
        }
    }
    assert!(fading_frames > 0, "exercise the dismissal animation");
    assert!(h.app.shape_properties.last_chrome.is_none());
}

#[test]
fn deselecting_an_image_never_falls_back_to_the_generic_toolbar() {
    let (mut h, _) =
        crate::app::board_flow::tests::generator_with_output("image-blister-dismissal");
    let id = crate::app::board_flow::tests::place(&mut h, "photo.png", 0.0);
    h.app.doc_mut().scene.node_mut(id).unwrap().rect = WorldRect::new(-340.0, -190.0, 680.0, 380.0);
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_sel = std::iter::once(id).collect();
    h.frame();
    let mut time = 0.0;
    for _ in 0..12 {
        let _ = frame(&mut h, &mut time);
    }
    assert!(h.app.shape_properties.last_chrome.as_ref().unwrap().image);
    h.app.board_sel.clear();
    for _ in 0..12 {
        let out = frame(&mut h, &mut time);
        assert!(h.app.shape_properties.chrome_hits.is_empty());
        assert!(
            out.shapes.iter().all(|shape| {
                let bounds = shape.shape.visual_bounding_rect();
                !bounds.is_positive() || bounds.width() > 1000.0 || bounds.height() > 800.0
            }),
            "dismissal paints only the panel background, never a generic toolbar"
        );
    }
}
