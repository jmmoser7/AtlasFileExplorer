//! Curve-style and visual verification frame tests.

use super::*;

#[test]
#[ignore]
fn texture_and_pen_style_validation_image() {
    use board_tip_hud::{apply_curve_style, CurveStyle};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/brush-validate");
    std::fs::create_dir_all(&dir).unwrap();
    let mut img = image::RgbaImage::from_pixel(700, 560, image::Rgba([16, 17, 20, 255]));
    let mut put = |x: u32, y: u32, rgb: [u8; 3], a: f32| {
        if x >= 700 || y >= 560 || a <= 0.0 {
            return;
        }
        let p = img.get_pixel_mut(x, y);
        for (c, &v) in rgb.iter().enumerate() {
            p.0[c] = (v as f32 * a + p.0[c] as f32 * (1.0 - a)).round() as u8;
        }
    };
    // Brush textures: one wavy stroke each, same size and softness.
    for (row, texture) in slate_doc::scene::BrushTexture::ALL.iter().enumerate() {
        let y0 = 40.0 + row as f32 * 56.0;
        let pts: Vec<vector_ink::TipPoint> = (0..=60)
            .map(|i| {
                let x = 30.0 + i as f32 * 5.0;
                vector_ink::TipPoint {
                    pos: [x, y0 + (i as f32 * 0.3).sin() * 12.0],
                    tip: vector_ink::StampStyle {
                        diameter: 34.0,
                        softness: 0.35,
                        rgba: [150, 255, 170, 255],
                        grain: texture.grain(),
                    },
                }
            })
            .collect();
        let stamp = vector_ink::stamp_tipped(&[pts], 1.0).unwrap();
        for y in 0..stamp.height {
            for x in 0..stamp.width {
                let i = ((y * stamp.width + x) * 4) as usize;
                let a = stamp.rgba[i + 3] as f32 / 255.0;
                let wx = stamp.origin[0] + x as f32 + 0.5;
                let wy = stamp.origin[1] + y as f32 + 0.5;
                if wx >= 0.0 && wy >= 0.0 {
                    put(wx as u32, wy as u32, [150, 255, 170], a);
                }
            }
        }
    }
    // Pen styles: a bent open curve per style, outline filled by sampling.
    let styles = [
        CurveStyle::Square,
        CurveStyle::Round,
        CurveStyle::Arrow,
        CurveStyle::TaperStart,
        CurveStyle::TaperBoth,
    ];
    for (row, style) in styles.iter().enumerate() {
        let y0 = 40.0 + row as f32 * 100.0;
        let mut stroke = board_path::default_curve_stroke(slate_doc::scene::Rgba::BLACK);
        stroke.width = 14.0;
        apply_curve_style(&mut stroke, *style);
        let mut bez = vector_ink::kurbo::BezPath::new();
        bez.move_to((380.0, (y0 + 30.0) as f64));
        bez.line_to((500.0, (y0 - 10.0) as f64));
        bez.line_to((640.0, (y0 + 30.0) as f64));
        let ink = board_path::stroke_style_world(&stroke, 1.0);
        let body = if stroke.arrow_end {
            slate_doc::geom::trim_end(&bez, slate_doc::geom::arrow_trim(stroke.width))
        } else {
            bez.clone()
        };
        let outline = vector_ink::stroke_outline(&body, &ink, 0.25);
        let contours = vector_ink::flatten_contours(&outline, 0.25);
        let arrow = stroke
            .arrow_end
            .then(|| slate_doc::geom::path_end_arrow(&bez, stroke.width))
            .flatten()
            .map(|(tip, into)| slate_doc::geom::arrow_head(tip, into, stroke.width));
        for y in (y0 as u32).saturating_sub(40)..(y0 as u32 + 60) {
            for x in 360..690 {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let inside = vector_ink::point_in_polygon(&contours, p)
                    || arrow.is_some_and(|t| vector_ink::point_in_polygon(&vec![t.to_vec()], p));
                if inside {
                    put(x, y, [230, 230, 240], 1.0);
                }
            }
        }
    }
    img.save(dir.join("styles.png")).unwrap();
}

/// Arrow and taper styles as each curve tool commits them, for eyeballing.
#[test]
#[ignore]
fn curve_style_visual_frames() {
    use board_tip_hud::{CurveStyle, TipChoice};
    let mut h = Harness::new("visual_styles");
    let mut raster = FrameRaster::new(1440, 900);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    capture_frame(&mut h, &mut raster, |_| {});
    h.app.tab_mut().cam.z = 1.0;
    capture_frame(&mut h, &mut raster, |_| {});
    let c = h.app.canvas_rect.center();
    let xf = h.app.board_xf();
    let w = |x: f32, y: f32| xf.s2w(Pos2::new(c.x + x, c.y + y));
    let arm = |h: &mut Harness, tool: board::BoardTool, style: CurveStyle| {
        h.app.set_board_tool(tool);
        h.app.apply_tip_choice(TipChoice::Curve(style));
        let tip = h.app.active_tip();
        h.app.set_active_tip(10.0, tip.1, 1.0);
    };
    let styles = [
        CurveStyle::Arrow,
        CurveStyle::TaperStart,
        CurveStyle::TaperBoth,
    ];
    for (row, style) in styles.iter().enumerate() {
        let y = -300.0 + row as f32 * 220.0;
        // Freehand pen: a hand that slows down and hooks a little at the end.
        arm(&mut h, board::BoardTool::Pen, *style);
        let mut pts: Vec<Pos2> = (0..=80)
            .map(|k| {
                let t = k as f32 / 80.0;
                let e = 1.0 - (1.0 - t).powi(3);
                w(-650.0 + e * 260.0, y + (e * 5.0).sin() * 30.0)
            })
            .collect();
        let end = *pts.last().unwrap();
        pts.push(end + egui::vec2(0.6, 1.2));
        pts.push(end + egui::vec2(0.8, 2.2));
        h.app.finish_freehand_pen(pts);
        // Line.
        arm(&mut h, board::BoardTool::Line, *style);
        h.app.commit_line(w(-330.0, y + 30.0), w(-110.0, y - 30.0));
        // Arc through three clicks.
        arm(&mut h, board::BoardTool::Arc, *style);
        for p in [w(-60.0, y + 40.0), w(160.0, y + 40.0), w(50.0, y - 40.0)] {
            h.app.path_tool_click(p);
        }
        // Bezier with dragged handles.
        arm(&mut h, board::BoardTool::BezierSpan, *style);
        for (a, b) in [
            ((220.0, y + 30.0), (260.0, y - 40.0)),
            ((420.0, y), (470.0, y + 50.0)),
        ] {
            let press = w(a.0, a.1);
            h.app.bezier_anchor_press(press);
            h.app.bezier_anchor_release(press, w(b.0, b.1), false);
        }
        h.app.finish_path_draft();
        // Polyline.
        arm(&mut h, board::BoardTool::Polyline, *style);
        for p in [w(520.0, y + 30.0), w(600.0, y - 30.0), w(690.0, y + 30.0)] {
            h.app.path_tool_click(p);
        }
        h.app.finish_path_draft();
    }
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.board_sel.clear();
    let out = capture_frame(&mut h, &mut raster, |_| {});
    snapshot(&mut h, &mut raster, out, "10-curve-styles");
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/brush-validate/frames");
    let full = image::open(dir.join("10-curve-styles.png")).unwrap();
    for (row, name) in ["arrow", "taper-start", "taper-both"].iter().enumerate() {
        let y = (c.y - 360.0 + row as f32 * 220.0).max(0.0) as u32;
        let x = (c.x - 690.0).max(0.0) as u32;
        full.crop_imm(x, y, 720, 140)
            .save(dir.join(format!("10-{name}-left.png")))
            .unwrap();
        full.crop_imm(x + 700, y, 720, 140)
            .save(dir.join(format!("10-{name}-right.png")))
            .unwrap();
    }
    let html = slate_artifact::render_html(h.app.doc(), &slate_artifact::AssetMap::default());
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/brush-validate/frames");
    std::fs::write(dir.join("10-curve-styles.html"), html).unwrap();
}

#[test]
#[ignore]
fn visual_verification_frames() {
    use board_tip_hud::{palette_slot, CurveStyle, TipChoice};
    let mut h = Harness::new("visual");
    let mut raster = FrameRaster::new(1440, 900);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    capture_frame(&mut h, &mut raster, |_| {});
    capture_frame(&mut h, &mut raster, |_| {});
    let c = h.app.canvas_rect.center();
    let xf = h.app.board_xf();
    let w = |x: f32, y: f32| xf.s2w(Pos2::new(c.x + x, c.y + y));

    // Five textured brush strokes.
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.board_colors.fg.0 = [150, 255, 170, 255];
    for (i, t) in slate_doc::scene::BrushTexture::ALL.iter().enumerate() {
        h.app.brush_texture = *t;
        h.app.brush_width = 26.0 / xf.z;
        h.app.brush_softness = 0.35;
        let y0 = -330.0 + i as f32 * 52.0;
        let pts: Vec<Pos2> = (0..=40)
            .map(|k| w(-650.0 + k as f32 * 9.0, y0 + (k as f32 * 0.35).sin() * 12.0))
            .collect();
        h.app.finish_freehand_brush(pts);
    }
    // A spot erase through the first three strokes.
    let out = capture_frame(&mut h, &mut raster, |_| {});
    snapshot(&mut h, &mut raster, out, "00-before-erase");
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 22.0 / xf.z;
    h.app.eraser_softness = 0.0;
    h.app.eraser_opacity = 1.0;
    h.app.board_drag = Some(h.app.begin_erase(w(-470.0, -360.0), false));
    for k in 1..=12 {
        h.app
            .update_erase(w(-470.0 + k as f32 * 2.0, -360.0 + k as f32 * 12.0));
    }
    if let Some(board::BoardDrag::Erase {
        touched,
        points,
        spot,
        ..
    }) = h.app.board_drag.take()
    {
        h.app.finish_erase(touched, points, spot);
    }
    // Five pen curves, one per style.
    h.app.set_board_tool(board::BoardTool::Pen);
    let styles = [
        CurveStyle::Square,
        CurveStyle::Round,
        CurveStyle::Arrow,
        CurveStyle::TaperStart,
        CurveStyle::TaperBoth,
    ];
    let mut arrow_id = None;
    for (i, s) in styles.iter().enumerate() {
        h.app.set_board_tool(board::BoardTool::Pen);
        h.app.apply_tip_choice(TipChoice::Curve(*s));
        let width = h.app.active_tip();
        h.app.set_active_tip(12.0 / xf.z, width.1, 1.0);
        let y0 = -330.0 + i as f32 * 60.0;
        let pts = [
            w(200.0, y0 + 20.0),
            w(330.0, y0 - 15.0),
            w(470.0, y0 + 20.0),
        ];
        let (rect, data) = board_path::points_to_path_data(&pts, false);
        h.app
            .commit_path_node(slate_doc::StrokeTool::Pen, rect, data, false);
        if *s == CurveStyle::Arrow {
            arrow_id = h.app.doc().scene.nodes.last().map(|n| n.id);
        }
    }
    let out = capture_frame(&mut h, &mut raster, |_| {});
    snapshot(&mut h, &mut raster, out, "01-strokes");

    // Brush: Alt+right-drag size circle with the texture row.
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_texture = slate_doc::scene::BrushTexture::Pencil;
    let alt = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let press = Pos2::new(c.x - 350.0, c.y + 180.0);
    let rmb = |pos: Pos2, down: bool, m: egui::Modifiers| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed: down,
        modifiers: m,
    };
    capture_frame(&mut h, &mut raster, |i| {
        i.modifiers = alt;
        i.events.push(egui::Event::PointerMoved(press));
        i.events.push(rmb(press, true, alt));
    });
    let mut last = press;
    for k in 1..=5 {
        last = press + egui::vec2(8.0 * k as f32, -2.0 * k as f32);
        let p = last;
        capture_frame(&mut h, &mut raster, |i| {
            i.modifiers = alt;
            i.events.push(egui::Event::PointerMoved(p));
        });
    }
    let out = capture_frame(&mut h, &mut raster, |i| i.modifiers = alt);
    snapshot(&mut h, &mut raster, out, "02-brush-size-hud");
    let r = h.app.active_tip().0 * 0.5 * h.app.tab().cam.z;
    let before = (h.app.brush_width, h.app.brush_softness);
    let slot = palette_slot(press, r, 4, 5);
    for p in [
        last + egui::vec2(0.0, r + 12.0),
        slot + egui::vec2(-20.0, -8.0),
        slot,
    ] {
        capture_frame(&mut h, &mut raster, |i| {
            i.modifiers = alt;
            i.events.push(egui::Event::PointerMoved(p));
        });
    }
    let out = capture_frame(&mut h, &mut raster, |i| i.modifiers = alt);
    snapshot(&mut h, &mut raster, out, "03-brush-palette-hover");
    assert_eq!(
        h.app.brush_width, before.0,
        "reaching the style row must not scrub the size"
    );
    capture_frame(&mut h, &mut raster, |i| {
        i.modifiers = alt;
        i.events.push(rmb(slot, false, alt));
    });
    let _ = last;

    // Color wheel with recent colors, then the ring gap, then outside.
    let recents: Vec<[u8; 3]> = (0..14)
        .map(|k| board_color::hsv_to_rgb([k as f32 / 14.0, 0.8, 0.95]))
        .collect();
    h.app.tab_mut().doc.view.recent_colors = Some(recents);
    let wp = Pos2::new(c.x + 50.0, c.y + 150.0);
    capture_frame(&mut h, &mut raster, |i| {
        i.modifiers = ctrl;
        i.events.push(egui::Event::PointerMoved(wp));
        i.events.push(rmb(wp, true, ctrl));
    });
    let center = match h.app.brush_hud {
        Some(board_color::BrushHud::Wheel { center, .. }) => center,
        other => panic!("wheel open, got {other:?}"),
    };
    for (name, p) in [
        ("04-wheel", wp + egui::vec2(3.0, 2.0)),
        (
            "05-wheel-gap",
            center + egui::vec2(0.0, -(board_color::WHEEL_HUE_OUTER + 8.0)),
        ),
        (
            "06-wheel-outside",
            center + egui::vec2(0.0, -(board_color::WHEEL_BACKDROP_RADIUS + 30.0)),
        ),
    ] {
        capture_frame(&mut h, &mut raster, |i| {
            i.modifiers = ctrl;
            i.events.push(egui::Event::PointerMoved(p));
        });
        let out = capture_frame(&mut h, &mut raster, |i| i.modifiers = ctrl);
        snapshot(&mut h, &mut raster, out, name);
    }
    capture_frame(&mut h, &mut raster, |i| {
        i.modifiers = ctrl;
        i.events.push(rmb(wp, false, ctrl));
    });
    h.app.cancel_brush_hud();

    // Pen: size circle with the curve-style row.
    h.app.set_board_tool(board::BoardTool::Pen);
    let pp = Pos2::new(c.x + 380.0, c.y + 190.0);
    capture_frame(&mut h, &mut raster, |i| {
        i.modifiers = alt;
        i.events.push(egui::Event::PointerMoved(pp));
        i.events.push(rmb(pp, true, alt));
    });
    let pr = h.app.active_tip().0 * 0.5 * h.app.tab().cam.z;
    let pslot = palette_slot(pp, pr, 2, 5);
    capture_frame(&mut h, &mut raster, |i| {
        i.modifiers = alt;
        i.events.push(egui::Event::PointerMoved(pslot));
    });
    let out = capture_frame(&mut h, &mut raster, |i| i.modifiers = alt);
    snapshot(&mut h, &mut raster, out, "07-pen-size-hud");
    capture_frame(&mut h, &mut raster, |i| {
        i.modifiers = alt;
        i.events.push(rmb(pslot, false, alt));
    });

    // Direct Select on the arrow curve, middle anchor selected.
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(arrow_id);
    h.app.direct.anchors.insert(1);
    let out = capture_frame(&mut h, &mut raster, |i| {
        i.events.push(egui::Event::PointerMoved(Pos2::new(
            c.x + 330.0,
            c.y - 200.0,
        )));
    });
    snapshot(&mut h, &mut raster, out, "08-direct-select");
}
