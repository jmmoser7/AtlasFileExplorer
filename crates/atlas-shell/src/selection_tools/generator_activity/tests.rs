use super::*;

#[test]
fn collapsed_or_empty_activity_reserves_no_space() {
    let ctx = egui::Context::default();
    let entry = [ActivityEntry {
        id: "tool-1",
        label: "Running",
        text: "Writing the dashboard",
    }];
    assert_eq!(height(&ctx, &entry, 300.0, CardFold::Collapsed), 0.0);
    assert_eq!(height(&ctx, &[], 300.0, CardFold::Open), 0.0);
}

#[test]
fn retained_activity_uses_the_header_without_an_extra_summary_row() {
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    let entry = [ActivityEntry {
        id: "completed",
        label: "Completed",
        text: "Created the dashboard",
    }];
    let mut state = ActivityState::default();
    let chrome = GeneratorChrome {
        activity: Some(ActivityView {
            entries: &entry,
            state: &mut state,
            running: false,
        }),
        ..Default::default()
    };
    let measure = |status, chrome: &GeneratorChrome<'_>| {
        generator_layout(
            &ctx,
            300.0,
            "Auto",
            None,
            GeneratorContent::new(1, 0, status, None, chrome),
        )
        .height
    };
    let base = measure(None, &GeneratorChrome::default());
    assert_eq!(measure(None, &chrome), base + 6.0);
    assert_eq!(measure(Some("Ready"), &chrome), base + 6.0);
}

fn summary_frame(
    ctx: &egui::Context,
    state: &mut ActivityState,
    events: Vec<egui::Event>,
) -> GeneratorResponse {
    let mut response = GeneratorResponse::default();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut prompt = String::new();
                let entry = [ActivityEntry {
                    id: "completed",
                    label: "Completed",
                    text: "Created output/index.html",
                }];
                let mut chrome = GeneratorChrome {
                    activity: Some(ActivityView {
                        entries: &entry,
                        state,
                        running: false,
                    }),
                    ..Default::default()
                };
                let measured = generator_layout(
                    ctx,
                    400.0,
                    "Auto",
                    None,
                    GeneratorContent::new(1, 0, None, None, &chrome),
                );
                let panel =
                    Rect::from_min_size(Pos2::new(100.0, 58.0), Vec2::new(400.0, measured.height));
                let measured = measured.at(panel, 1.0);
                let folded = header_feedback(
                    ui,
                    measured.feedback,
                    measured.fold,
                    chrome.activity.as_mut(),
                    false,
                    1.0,
                    Palette::dark(),
                );
                response = generator_body(
                    ui,
                    measured,
                    GeneratorBody {
                        prompt: &mut prompt,
                        hint: "Describe the dashboard",
                        take_focus: false,
                        history: &[],
                        selected_history: None,
                        status: None,
                        approval: None,
                        approval_interactive: false,
                        activity: chrome.activity.take(),
                    },
                    1.0,
                    Palette::dark(),
                    "retained-activity",
                );
                response.activity_fold = folded;
            });
        },
    );
    response
}

#[test]
fn completed_activity_can_expand_without_a_lifecycle_status() {
    let ctx = egui::Context::default();
    let mut state = ActivityState::default();
    let point = Pos2::new(421.0, 65.2);
    summary_frame(&ctx, &mut state, vec![egui::Event::PointerMoved(point)]);
    for pressed in [true, false] {
        let response = summary_frame(
            &ctx,
            &mut state,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            response.activity_fold,
            (!pressed).then_some(CardFold::Partial)
        );
    }
    assert_eq!(state.fold, CardFold::Partial);
}

#[test]
fn follow_holds_the_readers_position_until_they_return_to_the_bottom() {
    let mut state = FollowScroll::default();
    let requested = requested_offset(&state, 200.0, 80.0);
    assert_eq!(requested, 120.0);
    settle_follow(&mut state, requested, 40.0, 200.0, 80.0);
    assert!(!state.follow);
    assert_eq!(requested_offset(&state, 300.0, 80.0), 40.0);
    settle_follow(&mut state, 40.0, 220.0, 300.0, 80.0);
    assert!(state.follow);
    assert_eq!(state.offset, 220.0);
}

#[test]
fn selectable_public_transcript_keeps_its_world_wrapping_across_zoom() {
    let ctx = egui::Context::default();
    let mut heights = Vec::new();
    for zoom in [0.45, 0.525, 0.9, 1.0, 1.25, 2.0] {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect = Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(240.0, 300.0) * zoom);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                let row = transcript_row(&mut child, Id::new("public-transcript"), "Preparing the reference image, then writing an interactive gradient with editable domain and title.", TEXT_UNITS, zoom, Palette::dark().ink, true, false);
                heights.push(row.height() / zoom);
            });
        });
    }
    for height in &heights {
        assert!((height - heights[0]).abs() < 0.03, "{heights:?}");
    }
}

fn fold_frame(
    ctx: &egui::Context,
    state: &mut ActivityState,
    events: Vec<egui::Event>,
) -> Option<CardFold> {
    let mut chosen = None;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(400.0, 24.0));
                chosen = paint_fold_controls(ui, rect, state.fold, 1.0, Palette::dark());
                if let Some(fold) = chosen {
                    state.fold = fold;
                }
            });
        },
    );
    chosen
}

#[test]
fn activity_chevrons_dispatch_the_shared_fold_states() {
    let ctx = egui::Context::default();
    let mut state = ActivityState {
        fold: CardFold::Partial,
        ..Default::default()
    };
    let point = Pos2::new(107.0, 102.8);
    fold_frame(&ctx, &mut state, vec![egui::Event::PointerMoved(point)]);
    for pressed in [true, false] {
        let choice = fold_frame(
            &ctx,
            &mut state,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(choice, (!pressed).then_some(CardFold::Collapsed));
    }
    assert_eq!(state.fold, CardFold::Collapsed);
}

#[test]
fn transcript_rows_reserve_raster_ink_and_repeat_without_height_growth() {
    fn edges(r: Rect) -> [f32; 4] {
        [r.left(), r.top(), r.right(), r.bottom()]
    }
    // Match egui-wgpu::ScissorRect::new (and Glow's positive-coordinate
    // set_clip_rect): global points -> physical pixels -> round -> clamp.
    fn scissor(clip: Rect, pixels_per_point: f32, target: [u32; 2]) -> Rect {
        let [left, top, right, bottom] =
            edges(clip).map(|edge| (edge * pixels_per_point).round() as u32);
        let left = left.clamp(0, target[0]);
        let top = top.clamp(0, target[1]);
        let right = right.clamp(left, target[0]);
        let bottom = bottom.clamp(top, target[1]);
        Rect::from_min_max(
            Pos2::new(left as f32, top as f32),
            Pos2::new(right as f32, bottom as f32),
        )
    }
    // Quantizing the scissor never quantizes ink or hides the original crop.
    for ppp in [1.0, 1.5, 2.0] {
        let clip = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(200.0, 610.0));
        let ink = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(200.0, 610.4));
        assert!(
            !scissor(clip, ppp, [4800, 4800]).contains_rect(ink * ppp),
            "the real 0.4-point crop must remain detectable at ppp={ppp}"
        );
    }
    let text = "Pair turn 1: ".to_owned() + &"A complete assistant paragraph continues after the divider and must be fully available, with no truncation or fixed-height viewport after Full. ".repeat(9);
    for zoom in [0.49, 0.5, 0.71, 0.72, 0.99, 1.0, 1.01, 1.49, 1.5, 1.99, 2.0] {
        let ctx = egui::Context::default();
        let mut previous = None;
        let mut saw_overhang = false;
        for interactive in [false, true] {
            for frame in 0..4 {
                let mut row = Rect::NOTHING;
                let mut raster_diagnostic = None;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(2400.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        let parent = Rect::from_min_size(
                            Pos2::new(100.0, 100.0),
                            Vec2::new(648.0 * zoom, 1800.0),
                        );
                        // Use the same registered canvas parent as a real transcript.
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let mut ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .id_salt("ink-row-parent")
                                    .max_rect(parent),
                            );
                            ui.set_clip_rect(parent);
                            let layout = canvas_text::world_layout(
                                ctx,
                                &text,
                                canvas_scale::font(14.0, 1.0),
                                648.0,
                                egui::Align::LEFT,
                            );
                            let raster = canvas_text::world_text(ctx, &layout, zoom);
                            let union = Rect::from_min_size(Pos2::ZERO, raster.size())
                                .union(raster.galley().mesh_bounds * raster.scale());
                            raster_diagnostic = Some((
                                edges(parent),
                                raster.scale(),
                                edges(raster.galley().rect),
                                edges(raster.galley().mesh_bounds),
                                (raster.scale() - 1.0).abs(),
                                (raster.scale() - 1.0).abs() < 1e-4,
                                edges(union),
                                union.height(),
                            ));
                            saw_overhang |= raster.galley().mesh_bounds.bottom() * raster.scale()
                                > layout.height * zoom;
                            row = transcript_row(
                                &mut ui,
                                Id::new("ink-row"),
                                &text,
                                14.0,
                                zoom,
                                Color32::WHITE,
                                interactive,
                                false,
                            );
                        });
                    },
                );
                // A newly created selectable Area deliberately paints invisibly
                // during its initial sizing pass (egui::Area::content_ui).
                // Settle that native lifecycle, then check three real paint frames.
                if frame == 0 {
                    continue;
                }
                fn text_ink(shape: &egui::Shape) -> Option<(Rect, Pos2, Rect, Rect)> {
                    match shape {
                        egui::Shape::Text(t) => Some((
                            t.visual_bounding_rect(),
                            t.pos,
                            t.galley.rect,
                            t.galley.mesh_bounds,
                        )),
                        egui::Shape::Vec(v) => v.iter().find_map(text_ink),
                        _ => None,
                    }
                }
                let ((ink, origin, logical, mesh), clip) = output
                    .shapes
                    .iter()
                    .find_map(|s| text_ink(&s.shape).map(|ink| (ink, s.clip_rect)))
                    .unwrap_or_else(|| panic!("registered row paints text: zoom={zoom}, interactive={interactive}, row={row:?}, shapes={:?}", output.shapes));
                let clip_edges = edges(clip);
                let ink_edges = edges(ink);
                let signed_room = [
                    ink.left() - clip.left(),
                    ink.top() - clip.top(),
                    clip.right() - ink.right(),
                    clip.bottom() - ink.bottom(),
                ];
                let ulps: Vec<i64> = ink_edges
                    .iter()
                    .zip(clip_edges.iter())
                    .map(|(ink, clip)| i64::from(ink.to_bits()) - i64::from(clip.to_bits()))
                    .collect();
                // FullOutput shapes and clips are already in global points;
                // canvas zoom is included. Only display ppp remains to apply.
                let ppp = output.pixels_per_point;
                let target = [(2400.0 * ppp).round() as u32; 2];
                let parent_edges = raster_diagnostic.as_ref().unwrap().0;
                let parent = Rect::from_min_max(
                    Pos2::new(parent_edges[0], parent_edges[1]),
                    Pos2::new(parent_edges[2], parent_edges[3]),
                );
                let physical_clip = scissor(clip, ppp, target);
                let row_edges = edges(row);
                let row_room = [
                    ink.left() - row.left(),
                    ink.top() - row.top(),
                    row.right() - ink.right(),
                    row.bottom() - ink.bottom(),
                ];
                let row_ulps: Vec<i64> = ink_edges
                    .iter()
                    .zip(row_edges.iter())
                    .map(|(ink, row)| i64::from(ink.to_bits()) - i64::from(row.to_bits()))
                    .collect();
                let relative_ink = ink.translate(-origin.to_vec2());
                let diagnostic = format!(
                    "zoom={zoom:.9}, interactive={interactive}, frame={frame}; clip={clip_edges:.9?}; ink={ink_edges:.9?}; clip_inside_room={signed_room:.9?}; ink_minus_clip_ulps={ulps:?}; row={row_edges:.9?}; row_inside_room={row_room:.9?}; ink_minus_row_ulps={row_ulps:?}; row_size=({:.9},{:.9}); origin=({:.9},{:.9}); relative_ink={:.9?}; logical={:.9?}; mesh={:.9?}; raster(parent,scale,logical,mesh,scale_distance,unscaled_paint_branch,union,height)={raster_diagnostic:.9?}; row_bits={:?}; ink_bits={:?}; clip_bits={:?}; physical_clip={physical_clip:?}, ppp={ppp:.9}, target={target:?}",
                    row.width(), row.height(), origin.x, origin.y, edges(relative_ink), edges(logical), edges(mesh),
                    row_edges.map(f32::to_bits), ink_edges.map(f32::to_bits), clip_edges.map(f32::to_bits));
                let union_edges = raster_diagnostic.as_ref().unwrap().6;
                let measured_local = Rect::from_min_max(
                    Pos2::new(union_edges[0], union_edges[1]),
                    Pos2::new(union_edges[2], union_edges[3]),
                );
                let measured_height = raster_diagnostic.as_ref().unwrap().7;
                // The row is a layout reservation, not a paint scissor. Compare
                // actual emitted local ink before translation: Rect::translate
                // rebuilds min+size, so its global bounding box can differ by
                // one ULP even when the original local ink fits exactly.
                assert!(
                    measured_local.top() <= mesh.top() && mesh.bottom() <= measured_local.bottom(),
                    "strict local row ink reservation: {diagnostic}"
                );
                assert_eq!(
                    row.bottom(),
                    row.top() + measured_height,
                    "the row must reserve the complete measured union height: {diagnostic}"
                );
                assert_eq!(
                    origin,
                    row.min - Vec2::new(0.0, measured_local.top()),
                    "paint origin must place negative top ink inside the reservation: {diagnostic}"
                );
                assert_eq!(
                    physical_clip,
                    scissor(parent, ppp, target),
                    "inverse/forward layer clipping must preserve the actual parent scissor: {diagnostic}"
                );
                assert!(
                    clip.top() <= ink.top() && ink.bottom() <= clip.bottom(),
                    "strict unrounded vertical ink containment: {diagnostic}"
                );
                assert!(
                    physical_clip.contains_rect(ink * ppp),
                    "physical scissor contains unrounded ink: {diagnostic}"
                );
                let reservation = (measured_local, measured_height, row, origin);
                if let Some(previous) = previous {
                    assert_eq!(reservation, previous,
                        "identical frames and plain/selectable modes must retain the same local measurement, reservation, and placement: {diagnostic}");
                }
                previous = Some(reservation);
            }
        }
        if zoom == 1.0 {
            assert!(
                saw_overhang,
                "the original failing rung must exercise real raster overhang"
            );
        }
    }
}
