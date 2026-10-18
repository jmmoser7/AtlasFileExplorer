//! Opt-in validation against Slate's real Windows GL framebuffer, including MSAA.

use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

struct CaptureProbe {
    capture: GpuCapture,
    gl: Arc<glow::Context>,
    before: Arc<egui_glow::CallbackFn>,
    after: Arc<egui_glow::CallbackFn>,
    sentinel_pack: glow::Buffer,
    sentinel_read: glow::Framebuffer,
    completed: Arc<AtomicBool>,
    started: Instant,
    count: usize,
    first_buffer: Option<usize>,
    reused: bool,
}

impl CaptureProbe {
    fn new(cc: &eframe::CreationContext<'_>, completed: Arc<AtomicBool>) -> Self {
        let gl = cc.gl.clone().expect("GL renderer");
        let sentinel_pack = unsafe { gl.create_buffer().unwrap() };
        let sentinel_read = unsafe { gl.create_framebuffer().unwrap() };
        let before_gl = Arc::clone(&gl);
        let before = Arc::new(egui_glow::CallbackFn::new(move |_, _| unsafe {
            assert!(before_gl.get_parameter_i32(glow::SAMPLES) >= 4);
            before_gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(sentinel_read));
            before_gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(sentinel_pack));
            before_gl.pixel_store_i32(glow::PACK_ALIGNMENT, 8);
            before_gl.pixel_store_i32(glow::PACK_ROW_LENGTH, 4);
            before_gl.pixel_store_i32(glow::PACK_SKIP_ROWS, 1);
            before_gl.pixel_store_i32(glow::PACK_SKIP_PIXELS, 2);
        }));
        let after_gl = Arc::clone(&gl);
        let after = Arc::new(egui_glow::CallbackFn::new(move |_, painter| unsafe {
            assert_eq!(
                after_gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING),
                Some(sentinel_read),
            );
            assert_eq!(
                after_gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING),
                painter.intermediate_fbo(),
            );
            assert_eq!(
                after_gl.get_parameter_buffer(glow::PIXEL_PACK_BUFFER_BINDING),
                Some(sentinel_pack),
            );
            assert_eq!(after_gl.get_parameter_i32(glow::PACK_ALIGNMENT), 8);
            assert_eq!(after_gl.get_parameter_i32(glow::PACK_ROW_LENGTH), 4);
            assert_eq!(after_gl.get_parameter_i32(glow::PACK_SKIP_ROWS), 1);
            assert_eq!(after_gl.get_parameter_i32(glow::PACK_SKIP_PIXELS), 2);
            assert_eq!(after_gl.get_error(), glow::NO_ERROR);
        }));
        Self {
            capture: GpuCapture::new(Arc::clone(&gl)),
            gl,
            before,
            after,
            sentinel_pack,
            sentinel_read,
            completed,
            started: Instant::now(),
            count: 0,
            first_buffer: None,
            reused: false,
        }
    }

    fn accept(&mut self, frame: CapturedFrame) {
        assert_eq!([frame.w, frame.h], [200, 100]);
        assert_eq!(frame.generation, 41);
        for (x, y, expected) in [
            (25, 25, [255, 0, 0, 255]),
            (175, 25, [0, 255, 0, 255]),
            (25, 75, [0, 0, 255, 255]),
            (175, 75, [255, 255, 255, 255]),
        ] {
            let offset = (y * frame.w as usize + x) * 4;
            assert_eq!(&frame.rgba[offset..offset + 4], &expected);
        }
        let pointer = frame.rgba.as_ptr() as usize;
        self.reused |= self.first_buffer == Some(pointer);
        self.first_buffer.get_or_insert(pointer);
        self.count += 1;
        // Drop returns the exact allocation to the bounded pool.
    }
}

impl eframe::App for CaptureProbe {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        assert!(self.started.elapsed() < std::time::Duration::from_secs(15));
        assert!(self.capture.take_error().is_none());
        if let Some(frame) = self.capture.take_frame() {
            self.accept(frame);
        }
        if self.count >= 3 {
            assert!(self.reused, "capture reuses its CPU allocation");
            self.completed.store(true, Ordering::Release);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = egui::Rect::from_min_size(egui::pos2(40.0, 40.0), egui::vec2(320.0, 160.0));
            let painter = ui.painter();
            let center = rect.center();
            for (rect, color) in [
                (
                    egui::Rect::from_min_max(rect.min, center),
                    egui::Color32::RED,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(center.x, rect.top()),
                        egui::pos2(rect.right(), center.y),
                    ),
                    egui::Color32::GREEN,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left(), center.y),
                        egui::pos2(center.x, rect.bottom()),
                    ),
                    egui::Color32::BLUE,
                ),
                (
                    egui::Rect::from_min_max(center, rect.max),
                    egui::Color32::WHITE,
                ),
            ] {
                painter.rect_filled(rect, 0.0, color);
            }
            painter.add(egui::PaintCallback {
                rect,
                callback: self.before.clone(),
            });
            self.capture.request(41, [200, 100], Some([200, 100]));
            self.capture.paint(painter, rect);
            painter.add(egui::PaintCallback {
                rect,
                callback: self.after.clone(),
            });
            // Represents chrome drawn after contents. It must not be captured.
            painter.rect_filled(rect, 0.0, egui::Color32::YELLOW);
        });
        ctx.request_repaint();
    }

    fn on_exit(&mut self, _: Option<&glow::Context>) {
        self.capture.destroy();
        unsafe {
            self.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            self.gl.delete_buffer(self.sentinel_pack);
            self.gl.delete_framebuffer(self.sentinel_read);
        }
    }
}

#[test]
#[ignore = "requires a visible Windows GL window"]
fn native_capture_msaa_crop_and_color_orientation() {
    let completed = Arc::new(AtomicBool::new(false));
    let report = Arc::clone(&completed);
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        multisampling: atlas_shell::NATIVE_MSAA_SAMPLES,
        run_and_return: true,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([480.0, 320.0])
            .with_title("Slate portal capture probe"),
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Slate portal capture probe",
        options,
        Box::new(move |cc| Ok(Box::new(CaptureProbe::new(cc, report)))),
    )
    .unwrap();
    assert!(completed.load(Ordering::Acquire));
}
