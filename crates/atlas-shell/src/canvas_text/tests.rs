use super::*;

#[test]
fn scaled_selection_copies_text_at_multiple_zooms() {
    for scale in [0.5, 1.0, 2.0] {
        let ctx = egui::Context::default();
        let mut time = 0.0;
        let mut frame = |events| {
            time += 0.1;
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let galley = ui.painter().layout_no_wrap(
                            "Selectable reply".into(),
                            FontId::proportional(20.0),
                            Color32::WHITE,
                        );
                        Scaled::from_galley(galley, scale).selectable(
                            ui,
                            egui::Id::new("reply"),
                            Pos2::new(50.0, 50.0),
                            Color32::WHITE,
                        );
                    });
                },
            )
        };
        frame(vec![]);
        frame(vec![]);
        let first = Pos2::new(50.0, 50.0 + 10.0 * scale);
        let last = Pos2::new(50.0 + 160.0 * scale, first.y);
        frame(vec![
            egui::Event::PointerMoved(first),
            egui::Event::PointerButton {
                pos: first,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ]);
        frame(vec![egui::Event::PointerMoved(last)]);
        frame(vec![egui::Event::PointerButton {
            pos: last,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        let output = frame(vec![egui::Event::Copy]);
        assert!(
            output
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c,egui::OutputCommand::CopyText(s) if s=="Selectable reply")),
            "copy failed at scale {scale}: {:?}",
            output.platform_output.commands
        );
    }
}

/// The property the whole pattern is about: double the host, double the
/// type. Anything that clamps breaks this at one end or the other.
#[test]
fn text_scales_one_for_one_with_its_host() {
    for zoom in [0.05_f32, 0.5, 1.0, 4.0, 40.0] {
        let single = authored_px(24.0, zoom);
        let double = authored_px(24.0, zoom * 2.0);
        assert!(
            (double - single * 2.0).abs() < 1e-3,
            "authored text stopped tracking the host at z={zoom}: {single} → {double}"
        );
    }

    // Above the floor, where the fraction is what decides the size — the
    // floor region below it is the one place clamping is correct.
    for host_h in [50.0_f32, 100.0, 1000.0, 10_000.0] {
        let single = derived_px(host_h, 0.36, consts::LABEL_FLOOR_PX);
        let double = derived_px(host_h * 2.0, 0.36, consts::LABEL_FLOOR_PX);
        assert!(
            (double - single * 2.0).abs() < 1e-3,
            "derived label stopped tracking the host at h={host_h}: {single} → {double}"
        );
    }
}

/// A floor is a legibility aid at the small end; there is no ceiling at
/// the large end, which is the half people get wrong.
#[test]
fn the_floor_is_the_only_clamp() {
    assert_eq!(
        derived_px(1.0, 0.36, consts::LABEL_FLOOR_PX),
        consts::LABEL_FLOOR_PX,
        "a tiny host should still yield a readable label"
    );
    assert!(
        derived_px(100_000.0, 0.36, consts::LABEL_FLOOR_PX) > 30_000.0,
        "a huge host must keep growing its label — no ceiling"
    );
    assert!(
        authored_px(24.0, 500.0) > 10_000.0,
        "authored text must keep growing with the camera"
    );
}

#[test]
fn text_too_small_to_read_is_dropped_rather_than_clamped() {
    assert!(!legible(authored_px(24.0, 0.01)));
    assert!(legible(authored_px(24.0, 1.0)));
}

/// Width of a word over a zoom sweep, divided by the zoom: how much the
/// text drifts against the host it belongs to overall (`spread`), and the
/// worst single frame-to-frame lurch (`jump`), which is what reads as
/// judder. A rigid canvas object gives zero for both.
fn sweep(paint: impl Fn(&Painter, f32) -> Vec2) -> (f32, f32) {
    let ctx = egui::Context::default();
    let (mut spread, mut jump) = (0.0_f32, 0.0_f32);
    let _ = ctx.run(Default::default(), |ctx| {
        let painter = ctx.layer_painter(egui::LayerId::debug());
        let (mut lo, mut hi) = (f32::MAX, 0.0_f32);
        let mut prev: Option<f32> = None;
        // A fine sweep, as a scroll-wheel zoom would produce.
        for i in 0..=400 {
            let z = 0.5 + i as f32 * 0.00875;
            let per_zoom = paint(&painter, z).x / z;
            lo = lo.min(per_zoom);
            hi = hi.max(per_zoom);
            if let Some(p) = prev {
                jump = jump.max((per_zoom - p).abs() / p);
            }
            prev = Some(per_zoom);
        }
        spread = (hi - lo) / lo;
    });
    (spread, jump)
}

/// The bug the painting half of this module exists to fix: a size that
/// varies smoothly does not paint smoothly, because egui rasterizes at
/// whole physical pixels. Text laid out directly visibly steps against its
/// host; the same text through [`layout_no_wrap`] holds its proportion.
#[test]
fn zooming_does_not_make_text_climb_a_staircase() {
    let word = "Climate";
    for base in [8.0_f32, 11.0, 14.0, 24.0] {
        let (raw_spread, raw_jump) = sweep(|painter, z| {
            painter
                .layout_no_wrap(
                    word.to_owned(),
                    FontId::proportional(base * z),
                    Color32::WHITE,
                )
                .size()
        });
        let (spread, jump) = sweep(|painter, z| {
            layout_no_wrap(
                painter,
                word.to_owned(),
                FontId::proportional(base * z),
                Color32::WHITE,
            )
            .size()
        });

        assert!(
            raw_jump > 0.05,
            "egui stopped quantizing text at {base}px ({:.1}% worst jump) \
             — if that is genuinely fixed upstream, the ladder can go",
            raw_jump * 100.0
        );
        assert!(
            jump < 0.04 && spread < 0.07,
            "canvas text at {base}px lurched {:.1}% between frames and \
             drifted {:.1}% across the sweep (raw egui: {:.1}% / {:.1}%)",
            jump * 100.0,
            spread * 100.0,
            raw_jump * 100.0,
            raw_spread * 100.0
        );
    }
}

/// The ladder must be coarse enough to be worth having, never blow a
/// glyph up, and never sample one so far down that it shimmers.
#[test]
fn the_ladder_is_coarse_but_never_visibly_soft() {
    let mut sizes = std::collections::BTreeSet::new();
    for i in 0..4000 {
        let px = 4.0 + i as f32 * 0.1;
        let scale = px / raster_px(px);
        if px <= MAX_RASTER_PX {
            assert!(
                scale <= 1.001,
                "{px}px would be blown up from {}px — blurry",
                raster_px(px)
            );
            assert!(
                scale >= 1.0 / MAX_MINIFY - 0.001,
                "{px}px would be sampled down {:.1}x from {}px — shimmery",
                1.0 / scale,
                raster_px(px)
            );
        }
        sizes.insert(raster_px(px).to_bits());
    }
    assert!(
        sizes.len() < 40,
        "{} rasterized sizes over the zoom range is atlas churn",
        sizes.len()
    );
}

pub(super) const ZOOM_SWEEP: [f32; 10] = [0.1, 0.25, 0.37, 0.5, 0.73, 1.0, 1.5, 2.3, 4.0, 8.0];

pub(super) fn with_ctx(mut body: impl FnMut(&egui::Context)) {
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| body(ctx));
}

pub(super) fn row_text(galley: &Galley) -> Vec<String> {
    galley.rows.iter().map(|row| row.text()).collect()
}

fn breaks_at(
    ctx: &egui::Context,
    text: &str,
    size: f32,
    wrap: f32,
    align: egui::Align,
    zoom: f32,
) -> Vec<String> {
    let font = FontId::proportional(size);
    let layout = world_layout(ctx, text, font, wrap, align);
    let galley = zoom_galley(ctx, &layout, Color32::WHITE, zoom);
    let chars: usize = galley
        .rows
        .iter()
        .map(|row| row.char_count_including_newline())
        .sum();
    assert_eq!(chars, text.chars().count(), "caret map dropped a character");
    row_text(&galley)
}

#[test]
fn line_breaks_do_not_change_with_zoom() {
    let samples = [
        (
            "Short.\n\nA longer paragraph with several words that need to wrap inside a narrow column of the board.\nsupercalifragilisticexpialidocious\n尾声",
            11.0,
            160.0,
        ),
        (
            "看板文字在缩放时不得改换行位置即使是连续汉字也一样保持稳定",
            18.0,
            140.0,
        ),
        (
            "alpha bravo charlie delta echo foxtrot golf hotel india\n\nsecond paragraph stays put while the camera moves",
            24.0,
            220.0,
        ),
        ("one two three four five six seven eight nine ten", 8.0, 70.0),
        ("Title that is wider than its box", 48.0, 180.0),
    ];
    with_ctx(|ctx| {
        for (text, size, wrap) in samples {
            for align in [egui::Align::LEFT, egui::Align::Center, egui::Align::RIGHT] {
                let expected = breaks_at(ctx, text, size, wrap, align, 1.0);
                let cjk = text.chars().any(|ch| ch > '\u{2E80}');
                if !cjk {
                    assert!(expected.len() > 1, "sample did not wrap: {text}");
                }
                for zoom in ZOOM_SWEEP {
                    let got = breaks_at(ctx, text, size, wrap, align, zoom);
                    assert_eq!(
                        got, expected,
                        "breaks changed at zoom {zoom} size {size} align {align:?}\n{text}"
                    );
                }
            }
        }
        let mono = "fn main() {\n    println!(\"hi\");\n}\n";
        let expected = {
            let font = FontId::monospace(13.0);
            let layout = world_layout(ctx, mono, font.clone(), 90.0, egui::Align::LEFT);
            let galley = zoom_galley(ctx, &layout, Color32::WHITE, 1.0);
            row_text(&galley)
        };
        for zoom in ZOOM_SWEEP {
            let font = FontId::monospace(13.0);
            let layout = world_layout(ctx, mono, font.clone(), 90.0, egui::Align::LEFT);
            let galley = zoom_galley(ctx, &layout, Color32::WHITE, zoom);
            assert_eq!(row_text(&galley), expected);
        }
    });
}

#[test]
fn a_world_point_stays_on_the_same_line() {
    let text = "alpha bravo charlie delta echo foxtrot golf\nsecond paragraph stays put";
    with_ctx(|ctx| {
        let layout = world_layout(
            ctx,
            text,
            FontId::proportional(16.0),
            150.0,
            egui::Align::LEFT,
        );
        assert!(
            layout.lines.len() >= 2,
            "expected a wrapped first paragraph"
        );
        let line = &layout.lines[1];
        let world = egui::vec2(line.x + 2.0, line.y + line.height * 0.5);
        for zoom in ZOOM_SWEEP {
            let galley = zoom_galley(ctx, &layout, Color32::WHITE, zoom);
            let cursor = galley.cursor_from_pos(world * zoom);
            assert_eq!(cursor.rcursor.row, 1, "caret left its line at zoom {zoom}");
        }
    });
}

#[test]
fn zoom_and_pan_do_not_reshape() {
    let text = "cached paragraph that wraps more than once across the column";
    with_ctx(|ctx| {
        clear_world_layout_cache();
        let before = world_layout_shapes();
        let font = FontId::proportional(20.0);
        let layout = world_layout(ctx, text, font.clone(), 120.0, egui::Align::LEFT);
        assert_eq!(world_layout_shapes(), before + 1);
        for zoom in ZOOM_SWEEP {
            let _ = world_layout(ctx, text, font.clone(), 120.0, egui::Align::LEFT);
            let _ = zoom_galley(ctx, &layout, Color32::WHITE, zoom);
        }
        assert_eq!(
            world_layout_shapes(),
            before + 1,
            "pure zoom reshaped the paragraph"
        );
    });
}

#[test]
fn glyphs_scale_uniformly_and_rungs_do_not_reshape() {
    let text = "Glyph positions scale with the camera; words never slide inside a line.";
    with_ctx(|ctx| {
        clear_world_layout_cache();
        let layout = world_layout(
            ctx,
            text,
            FontId::proportional(14.0),
            180.0,
            egui::Align::Center,
        );
        let shapes = world_layout_shapes();
        let world: Vec<Vec<Pos2>> = layout
            .lines
            .iter()
            .map(|line| {
                let x = line.glyph_x.iter().map(|gx| line.x + gx);
                x.map(|x| egui::pos2(x, line.baseline)).collect()
            })
            .collect();
        for zoom in [0.25, 0.5, 0.8, 1.0, 1.5, 2.0, 3.0, 4.0] {
            let scaled = world_text(ctx, &layout, zoom);
            let galley = scaled.galley();
            for (row, want) in galley.rows.iter().zip(&world) {
                assert_eq!(row.glyphs.len(), want.len(), "zoom {zoom}");
                for (glyph, want) in row.glyphs.iter().zip(want) {
                    let got = glyph.pos.to_vec2() * scaled.scale();
                    let err = (got - want.to_vec2() * zoom).length();
                    assert!(err < 0.5, "glyph off by {err}px at zoom {zoom}");
                }
            }
        }
        let builds = rung_galley_builds();
        for step in 0..40 {
            let _ = world_text(ctx, &layout, 1.0 + step as f32 * 0.001);
        }
        assert!(
            rung_galley_builds() - builds <= 1,
            "a small zoom crossed many rungs"
        );
        assert_eq!(world_layout_shapes(), shapes, "painting reshaped the text");
    });
}

#[test]
fn a_still_camera_reuses_the_screen_galley() {
    let text = "a note that repaints every frame while nothing moves";
    with_ctx(|ctx| {
        clear_world_layout_cache();
        let font = FontId::proportional(18.0);
        let paint = |text: &str, zoom: f32| {
            let layout = world_layout(ctx, text, font.clone(), 140.0, egui::Align::LEFT);
            zoom_galley(ctx, &layout, Color32::WHITE, zoom)
        };
        let first = paint(text, 1.5);
        let builds = zoom_galley_builds();
        let again = paint(text, 1.5);
        assert!(Arc::ptr_eq(&first, &again), "a repaint rebuilt the galley");
        assert_eq!(zoom_galley_builds(), builds);
        let _ = paint(text, 2.0);
        assert_eq!(zoom_galley_builds(), builds + 1, "a new zoom must rebuild");
        let edited = paint("a note that was edited", 1.5);
        assert!(!Arc::ptr_eq(&first, &edited));
        for i in 0..(raster::ZOOM_CACHE_HARD_CAP + 8) {
            let _ = paint(text, 1.0 + i as f32 * 0.001);
        }
        raster::CACHES
            .with(|caches| assert!(caches.borrow().zoom_len() <= raster::ZOOM_CACHE_HARD_CAP));
    });
}
