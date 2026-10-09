//! Brush-shift preview cost and wheel-notch rebuilds.

use super::*;

/// tip18 on real frames: the Shift preview is visible on screen, both from
/// the press point and continuing an earlier stroke.
#[test]
fn brush_shift_drag_preview_reaches_the_screen() {
    let mut h = brush_board("brush_shift_raster");
    if let Ok(z) = std::env::var("SLATE_TEST_PPP") {
        h.ctx.set_pixels_per_point(z.parse().unwrap());
        h.frame_with(|i| i.max_texture_side = Some(8192));
        h.frame();
    }
    h.app.board_colors.fg.0 = [255, 40, 40, 255];
    h.app.brush_opacity = 1.0;
    h.app.brush_softness = 0.0;
    h.app.brush_width = 16.0;
    let mut raster = FrameRaster::new(1440, 900);
    capture_frame(&mut h, &mut raster, |_| {});
    let xf = h.app.board_xf();
    let lit = |raster: &FrameRaster, s: Pos2| {
        let p = raster.px[s.y as usize * raster.w + s.x as usize];
        p[0] > 0.6 && p[1] < 0.4
    };
    let shift = egui::Modifiers::SHIFT;
    for (round, (a, b)) in [
        (Pos2::new(100.0, 100.0), Pos2::new(300.0, 100.0)),
        (Pos2::new(250.0, 300.0), Pos2::new(450.0, 300.0)),
    ]
    .into_iter()
    .enumerate()
    {
        let (sa, sb) = (xf.w2s(a), xf.w2s(b));
        capture_frame(&mut h, &mut raster, |i| {
            i.modifiers = shift;
            i.events.push(egui::Event::PointerMoved(sa));
            i.events.push(egui::Event::PointerButton {
                pos: sa,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: shift,
            });
        });
        capture_frame(&mut h, &mut raster, |i| {
            i.modifiers = shift;
            i.events
                .push(egui::Event::PointerMoved(sa + (sb - sa) * 0.5));
        });
        let out = capture_frame(&mut h, &mut raster, |i| {
            i.modifiers = shift;
            i.events.push(egui::Event::PointerMoved(sb));
        });
        snapshot(&mut h, &mut raster, out, &format!("shift-preview-{round}"));
        let from = if round == 0 {
            a
        } else {
            Pos2::new(300.0, 100.0)
        };
        // A Shift drag takes 45° steps from where the segment starts.
        let (from, end) = (xf.w2s(from), xf.w2s(board_snap::ortho_snap_point(from, b)));
        let mid = from + (end - from) * 0.5;
        assert!(
            lit(&raster, mid),
            "round {round}: the preview is on screen at {mid:?}"
        );
        capture_frame(&mut h, &mut raster, |i| {
            i.modifiers = shift;
            i.events.push(egui::Event::PointerButton {
                pos: sb,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: shift,
            });
        });
    }
}

#[test]
#[ignore]
fn brush_shift_drag_frame_times_on_a_busy_board() {
    let mut h = brush_board("brush_shift_busy");
    h.frame_with(|i| i.max_texture_side = Some(8192));
    h.app.brush_width = 40.0;
    h.app.brush_softness = 0.6;
    h.app.brush_opacity = 0.6;
    for k in 0..70 {
        let y0 = -400.0 + k as f32 * 12.0;
        let pts: Vec<Pos2> = (0..=200)
            .map(|i| {
                let t = i as f32 / 200.0;
                Pos2::new(-600.0 + t * 1200.0, y0 + (t * 20.0).sin() * 40.0)
            })
            .collect();
        h.app.finish_freehand_brush(pts);
    }
    for _ in 0..50 {
        h.frame();
        if h.app.brush_tiles.last.pending_jobs == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let xf = h.app.board_xf();
    let shift = egui::Modifiers::SHIFT;
    let a = xf.w2s(Pos2::new(0.0, 0.0));
    let timed = |label: &str, h: &mut Harness, ev: Vec<egui::Event>| {
        let t = std::time::Instant::now();
        h.frame_with(|i| {
            i.modifiers = shift;
            i.events = ev;
        });
        eprintln!("{label}: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    };
    timed("hover", &mut h, vec![egui::Event::PointerMoved(a)]);
    timed(
        "press",
        &mut h,
        vec![egui::Event::PointerButton {
            pos: a,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: shift,
        }],
    );
    for k in 1..=5 {
        let p = a + EVec2::new(30.0 * k as f32, 10.0 * k as f32);
        timed(
            &format!("move {k}"),
            &mut h,
            vec![egui::Event::PointerMoved(p)],
        );
    }
    let end = a + EVec2::new(150.0, 50.0);
    timed(
        "release",
        &mut h,
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: shift,
        }],
    );
    for k in 0..3 {
        timed(&format!("after {k}"), &mut h, vec![]);
    }
}

/// tip18 frame times at the user's brush (207 wide, pencil, softness 0.09)
/// on a 1.5 px/pt display at 150 %: a freehand stroke, then six chained
/// Shift segments. Prints press, move, release, and next-frame times.
#[test]
#[ignore]
fn brush_shift_chain_frame_times_at_a_big_brush() {
    let mut h = brush_board("tip18_chain");
    h.ctx.set_pixels_per_point(1.5);
    h.frame_with(|i| i.max_texture_side = Some(8192));
    h.app.brush_width = 207.0;
    h.app.brush_softness = 0.09;
    h.app.brush_texture = slate_doc::scene::BrushTexture::Pencil;
    h.app.tab_mut().cam.z = 1.5;
    h.frame();
    let xf = h.app.board_xf();
    let c = xf.s2w(Pos2::new(720.0, 450.0));
    press_drag_release_frames(
        &mut h,
        &[
            c + EVec2::new(-300.0, -200.0),
            c + EVec2::new(-150.0, -150.0),
            c + EVec2::new(0.0, -200.0),
        ],
        egui::Modifiers::NONE,
        |_| {},
    );
    let shift = egui::Modifiers::SHIFT;
    for seg in 0..6 {
        let xf = h.app.board_xf();
        let a = xf.w2s(c + EVec2::new(-200.0 + 60.0 * seg as f32, 100.0));
        let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: shift,
        };
        let t = std::time::Instant::now();
        board::brush_prof::begin();
        h.frame_with(|i| {
            i.modifiers = shift;
            i.events.push(egui::Event::PointerMoved(a));
            i.events.push(button(a, true));
        });
        let laps: Vec<String> = board::brush_prof::take()
            .into_iter()
            .filter(|(_, ms)| *ms > 2.0)
            .map(|(n, ms)| format!("{n}={ms:.0}"))
            .collect();
        let press = t.elapsed().as_secs_f64() * 1000.0;
        let t = std::time::Instant::now();
        let b = a + EVec2::new(if seg % 2 == 0 { 200.0 } else { -200.0 }, 120.0);
        h.frame_with(|i| {
            i.modifiers = shift;
            i.events.push(egui::Event::PointerMoved(b));
        });
        let mv = t.elapsed().as_secs_f64() * 1000.0;
        let t = std::time::Instant::now();
        h.frame_with(|i| {
            i.modifiers = shift;
            i.events.push(button(b, false));
        });
        let rel = t.elapsed().as_secs_f64() * 1000.0;
        let t = std::time::Instant::now();
        h.frame();
        let after = t.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "seg{seg}: press {press:.1} {laps:?} move {mv:.1} release {rel:.1} after {after:.1} ms"
        );
    }
}

/// tip18 at the user's brush: a Shift drag's move frames stamp nothing on
/// the frame loop (Art. II), whatever the segment's length, yet the
/// preview follows the pointer on every one of them.
#[test]
fn brush_shift_move_frames_stamp_nothing_on_the_frame_loop() {
    let (mut h, c) = big_brush_board("tip18_budget");
    let xf = h.app.board_xf();
    let shift = egui::Modifiers::SHIFT;
    let press = xf.w2s(c + EVec2::new(-200.0, 100.0));
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerMoved(press));
        i.events.push(egui::Event::PointerButton {
            pos: press,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: shift,
        });
    });
    assert!(h.app.brush_straight.is_some());
    let from = h.app.brush_line_anchor().expect("the stroke's end").pos;
    let (allocs, copies, asks) = (
        board_path::line_tex_allocs_on_this_thread(),
        board_path::base_copies_on_this_thread().0,
        h.app.brush_tiles.line_tags_issued(),
    );
    for k in 1..=8 {
        let s = press + EVec2::new(45.0 * k as f32, 25.0 * k as f32);
        let before = board_path::stamp_px_on_this_thread();
        h.frame_with(|i| {
            i.modifiers = shift;
            i.events.push(egui::Event::PointerMoved(s));
        });
        let spent = board_path::stamp_px_on_this_thread() - before;
        assert_eq!(spent, 0, "move {k} stamped {spent} px on the frame loop");
        let end = board_snap::ortho_snap_point(from, h.app.board_xf().s2w(s));
        let canvas = h.app.brush_live.as_ref().expect("live canvas");
        let shown = canvas.live_line_end().expect("the preview shows a segment");
        assert!(
            near_px(Pos2::new(shown[0], shown[1]), end),
            "move {k}: the preview shows {shown:?}, the pointer asks {end:?}"
        );
    }
    // Once the pointer stops, the preview becomes the exact stamp.
    let mut waited = 0;
    while !h.app.brush_live.as_ref().is_some_and(|c| c.line_exact()) {
        waited += 1;
        assert!(waited < 400, "the exact stamp never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.frame_with(|i| i.modifiers = shift);
    }
    // The preview's pixels go into the one canvas-sized line texture, each
    // job copying at most its own box of the canvas as its base.
    assert_eq!(
        board_path::line_tex_allocs_on_this_thread(),
        allocs,
        "a move or a landed stamp allocated a texture"
    );
    let asked = h.app.brush_tiles.line_tags_issued() - asks;
    assert!(asked > 0, "the moves asked for no stamp");
    assert!(
        board_path::base_copies_on_this_thread().0 - copies <= asked,
        "a job copied its base more than once"
    );
    // The release takes that stamp into the canvas; nothing stamps here.
    let before = board_path::stamp_px_on_this_thread();
    let last = press + EVec2::new(45.0 * 8.0, 25.0 * 8.0);
    h.frame_with(|i| {
        i.modifiers = shift;
        i.events.push(egui::Event::PointerButton {
            pos: last,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: shift,
        });
    });
    h.frame();
    assert_eq!(
        board_path::stamp_px_on_this_thread(),
        before,
        "the release stamped"
    );
    let canvas = h.app.brush_live.as_ref().expect("parked canvas");
    assert!(canvas.settled(), "the canvas holds the committed segment");
}

/// The user's brush after one wheel notch: the camera changed, so the next
/// Shift press rebuilds the live canvas. The rebuild stamps nothing on the
/// frame loop (Art. II): the workers stamp the stroke, the scene keeps
/// painting it until they land, and then the canvas shows the chain.
#[test]
fn brush_shift_after_a_wheel_notch_rebuilds_off_the_frame_loop_at_the_users_brush() {
    let (mut h, mut raster, c) = big_brush_raster_board("r10_wheel_rebuild");
    let z = h.app.tab().cam.z;
    capture_frame(&mut h, &mut raster, |inp| {
        inp.events = vec![
            egui::Event::PointerMoved(Pos2::new(720.0, 450.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: EVec2::new(0.0, -120.0),
                modifiers: egui::Modifiers::NONE,
            },
        ];
    });
    let out = capture_frame(&mut h, &mut raster, |_| {});
    rasterize(&mut h, &mut raster, out);
    assert!(
        (h.app.tab().cam.z - z).abs() > z * 0.01,
        "the wheel moved the camera"
    );
    let xf = h.app.board_xf();
    let chain = [
        c + EVec2::new(-225.0, -175.0),
        c + EVec2::new(-75.0, -175.0),
    ];
    for w in chain {
        assert!(
            red_around(&raster, &xf, w),
            "{w:?}: the stroke is dark before the press"
        );
    }
    let shift = egui::Modifiers::SHIFT;
    let at = xf.w2s(c + EVec2::new(-200.0, 100.0));
    let (stamps, px, asks) = (
        board_path::stamps_on_this_thread(),
        board_path::stamp_px_on_this_thread(),
        h.app.brush_tiles.line_tags_issued(),
    );
    let t = std::time::Instant::now();
    let out = capture_frame(&mut h, &mut raster, |inp| {
        inp.modifiers = shift;
        inp.events = vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: shift,
            },
        ];
    });
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let press = (
        board_path::stamps_on_this_thread() - stamps,
        board_path::stamp_px_on_this_thread() - px,
    );
    eprintln!("wheel-notch rebuild: press frame {ms:.1} ms, frame-thread (stamps, px) {press:?}");
    assert!(
        h.app.brush_straight.is_some(),
        "the press started a Shift segment"
    );
    assert_eq!(
        press,
        (0, 0),
        "the rebuild stamped on the frame loop at the press"
    );
    draw_now(&h, &mut raster, out);
    for w in chain {
        assert!(
            red_around(&raster, &xf, w),
            "{w:?}: the stroke vanished at the press"
        );
    }
    wait_brush_live(&mut h, &mut raster, shift, |c| {
        c.settled() && c.line_exact()
    });
    assert_eq!(
        (
            board_path::stamps_on_this_thread(),
            board_path::stamp_px_on_this_thread()
        ),
        (stamps, px),
        "the rebuild stamped on the frame loop"
    );
    let id = h.app.doc().scene.nodes[0].id;
    assert!(
        canvas_covers(&h, id),
        "the canvas does not hold the stroke in the scene's place"
    );
    assert!(
        h.app.brush_tiles.line_tags_issued() >= asks + 2,
        "the workers were not asked for the stroke and the segment"
    );
    let out = capture_frame(&mut h, &mut raster, |inp| inp.modifiers = shift);
    draw_now(&h, &mut raster, out);
    let end = h.app.brush_line_anchor().expect("the stroke's end").pos;
    let seg = h
        .app
        .brush_live
        .as_ref()
        .and_then(|c| c.live_line_end())
        .expect("a segment");
    let mid = Pos2::new((end.x + seg[0]) * 0.5, (end.y + seg[1]) * 0.5);
    for w in chain.into_iter().chain([mid]) {
        assert!(
            red_around(&raster, &xf, w),
            "{w:?} is dark once the canvas holds the chain"
        );
    }
}

/// [`brush_shift_after_a_wheel_notch_rebuilds_off_the_frame_loop_at_the_users_brush`]
/// at 50 % opacity with a smooth tip: once the rebuilt canvas holds the
/// stroke, the scene leaves it out, so the chain's body and the joint with
/// the segment paint once, at the brush's opacity.
#[test]
fn brush_shift_after_a_wheel_notch_at_half_opacity_paints_the_chain_once() {
    let (mut h, c) = big_brush_setup("r12_wheel_rebuild_half");
    h.app.brush_opacity = 0.5;
    h.app.brush_texture = slate_doc::scene::BrushTexture::Smooth;
    let mut raster = FrameRaster::new(1440, 900);
    capture_frame(&mut h, &mut raster, |_| {});
    raster_drag(
        &mut h,
        &mut raster,
        egui::Modifiers::NONE,
        &big_brush_stroke(c),
        false,
    );
    settle_brush_live(&mut h, &mut raster);
    wheel_notch(&mut h, &mut raster);
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let xf = h.app.board_xf();
    let id = h.app.doc().scene.nodes[0].id;
    let shift = egui::Modifiers::SHIFT;
    let at = xf.w2s(p(-200.0, 100.0));
    capture_frame(&mut h, &mut raster, |inp| {
        inp.modifiers = shift;
        inp.events = vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: shift,
            },
        ];
    });
    wait_brush_live(&mut h, &mut raster, shift, |c| {
        c.settled() && c.line_exact()
    });
    assert!(
        canvas_covers(&h, id),
        "the canvas does not hold the stroke in the scene's place"
    );
    // Give a scene copy the stroke's own raster would paint time to land.
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        capture_frame(&mut h, &mut raster, |inp| inp.modifiers = shift);
    }
    let out = capture_frame(&mut h, &mut raster, |inp| inp.modifiers = shift);
    draw_now(&h, &mut raster, out);
    let bg = {
        let s = xf.w2s(p(250.0, -250.0));
        raster.px[s.y as usize * raster.w + s.x as usize][0]
    };
    let end = h.app.brush_line_anchor().expect("the stroke's end").pos;
    for (what, w) in [
        ("body", p(-225.0, -175.0)),
        ("body", p(-75.0, -175.0)),
        ("joint", end),
    ] {
        let a = red_alpha(&raster, &xf, w, bg);
        assert!((a - 0.5).abs() < 0.06, "the chain's {what} at {w:?} is {a}");
    }
}
