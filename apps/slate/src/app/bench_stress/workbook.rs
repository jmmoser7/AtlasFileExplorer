//! Measure the actual workbook draw list, separately from the Windows compositor.

use super::*;
use crate::app::canvas::background;
use eframe::egui_glow::Painter;
use eframe::glow::{self, HasContext};
use std::sync::Arc;

#[test]
#[ignore = "requires SLATE_BENCH_WORKBOOK and a native graphics context"]
fn bench_workbook_composition() {
    let path = PathBuf::from(std::env::var_os("SLATE_BENCH_WORKBOOK").expect("workbook path"));
    let document = slate_doc::SlateDoc::load_from(&path).unwrap();
    let mut h = Harness::new("workbook_composition");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let tab = h.app.tab_mut();
    tab.cam.offset = Vec2::new(document.view.cam_x, document.view.cam_y);
    tab.cam.z = document.view.zoom;
    tab.doc = document;
    tab.path = Some(path);
    tab.read_only = true;

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default().with_visible(false),
        event_loop_builder: Some(Box::new(|builder| {
            #[cfg(windows)]
            {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                builder.with_any_thread(true);
            }
            #[cfg(not(windows))]
            let _ = builder;
        })),
        ..Default::default()
    };
    struct Close;
    impl eframe::App for Close {
        fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    eframe::run_native(
        "Slate workbook composition audit",
        options,
        Box::new(move |cc| {
            let ctx = h.ctx.clone();
            let gl = cc.gl.as_ref().unwrap();
            assert!(gl.version().major >= 3);
            h.app.gl = Some(gl.clone());
            let software = background::supported(gl);
            h.app.software_board_clear = software;
            compare(gl, &ctx, software, |ppp| {
                let output = h.frame_output(|input| {
                    input
                        .viewports
                        .get_mut(&egui::ViewportId::ROOT)
                        .unwrap()
                        .native_pixels_per_point = Some(ppp);
                });
                (output, h.app.canvas_rect, h.app.palette().bg)
            });
            Ok(Box::new(Close))
        }),
    )
    .unwrap();
}

fn compare(
    gl: &Arc<glow::Context>,
    ctx: &egui::Context,
    software: bool,
    mut frame: impl FnMut(f32) -> (egui::FullOutput, egui::Rect, egui::Color32),
) {
    // Both painters replay the same tessellation and texture history. A fenced
    // offscreen draw measures completed software/GPU work without SwapBuffers.
    let mut painters =
        [true, false].map(|dither| Painter::new(gl.clone(), "", None, dither).unwrap());
    let ppp = std::env::var("SLATE_BENCH_PIXELS_PER_POINT")
        .ok()
        .map(|v| v.parse::<f32>().expect("pixel scale"))
        .unwrap_or(2.0);
    assert!((0.5..=4.0).contains(&ppp));
    let size = [(1440.0 * ppp).round() as u32, (900.0 * ppp).round() as u32];
    let (fbo, color) = unsafe {
        let fbo = gl.create_framebuffer().unwrap();
        let color = gl.create_renderbuffer().unwrap();
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
        gl.renderbuffer_storage(
            glow::RENDERBUFFER,
            glow::RGBA8,
            size[0] as i32,
            size[1] as i32,
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::RENDERBUFFER,
            Some(color),
        );
        assert_eq!(
            gl.check_framebuffer_status(glow::FRAMEBUFFER),
            glow::FRAMEBUFFER_COMPLETE
        );
        println!(
            "workbook renderer={}",
            gl.get_parameter_string(glow::RENDERER)
        );
        (fbo, color)
    };
    let mut last = None;
    for _ in 0..48 {
        let (out, rect, bg) = frame(ppp);
        for p in &mut painters {
            for (id, delta) in &out.textures_delta.set {
                p.set_texture(*id, delta);
            }
            for id in &out.textures_delta.free {
                p.free_texture(*id);
            }
        }
        assert_eq!(out.pixels_per_point, ppp);
        last = Some((out, rect, bg));
        // Give asynchronous source decoding and thumbnail queues a chance to run.
        std::thread::sleep(Duration::from_millis(20));
    }
    let (out, rect, bg) = last.unwrap();
    let current = out.shapes;
    let index = current
        .iter()
        .position(|s| match &s.shape {
            egui::Shape::Callback(c) => software && c.rect == rect,
            egui::Shape::Rect(r) => !software && r.rect == rect && r.fill == bg,
            _ => false,
        })
        .expect("the actual app must emit the designated board background");
    let mut single = current.clone();
    single[index].shape = egui::Shape::rect_filled(rect, 0., bg);
    let mut duplicate = single.clone();
    duplicate.insert(
        index + 1,
        egui::epaint::ClippedShape {
            clip_rect: rect,
            shape: egui::Shape::rect_filled(rect, 0., bg),
        },
    );
    let mut cleared = current;
    if !software {
        cleared[index] = egui::epaint::ClippedShape {
            clip_rect: rect,
            shape: background::clear(rect, bg).into(),
        };
    }
    let captured = [duplicate, single, cleared].map(|shapes| ctx.tessellate(shapes, ppp));
    println!(
        "workbook {} clipped primitives, scale {}",
        captured[1].len(),
        ppp
    );
    let mut snapshots: [Vec<u8>; 4] = Default::default();
    for pass in 0..12 {
        // Rotate draw order to share shader warmup and background load.
        for next in 0..4 {
            let variant = (pass + next) % 4;
            let dither = variant < 3;
            let started = Instant::now();
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.disable(glow::SCISSOR_TEST);
                gl.clear_color(0., 0., 0., 1.);
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
            painters[usize::from(!dither)].paint_primitives(size, ppp, &captured[variant.min(2)]);
            unsafe { gl.finish() };
            let ms = started.elapsed().as_secs_f64() * 1000.;
            println!("workbook draw pass={pass} dither={dither} variant={variant} ms={ms:.3}");
            if pass == 11 {
                snapshots[variant].resize((size[0] * size[1] * 4) as usize, 0);
                unsafe {
                    gl.read_pixels(
                        0,
                        0,
                        size[0] as i32,
                        size[1] as i32,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelPackData::Slice(Some(&mut snapshots[variant])),
                    );
                }
            }
        }
    }
    for variant in 1..4 {
        let mut deltas = [0usize; 256];
        for (&a, &b) in snapshots[0].iter().zip(&snapshots[variant]) {
            deltas[a.abs_diff(b) as usize] += 1;
        }
        println!(
            "workbook variant={variant} channel errors 0={} 1={} 2={} >2={} max={}",
            deltas[0],
            deltas[1],
            deltas[2],
            deltas[3..].iter().sum::<usize>(),
            deltas.iter().rposition(|v| *v > 0).unwrap()
        );
        if variant < 3 {
            assert_eq!(
                deltas[0],
                snapshots[0].len(),
                "the replacement board background changed pixels"
            );
        }
    }
    for p in &mut painters {
        p.destroy();
    }
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.delete_renderbuffer(color);
        gl.delete_framebuffer(fbo);
    }
}
