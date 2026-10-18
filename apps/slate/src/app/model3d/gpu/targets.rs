use super::surface::FrameCamera;
use super::*;

/// Preserve full-detail AO for normal portals; large views use a filtered
/// half-size field so pixel cost stays bounded on the reference Intel GPU.
fn ao_size(w: i32, h: i32) -> (i32, i32) {
    if w * h > 1_200_000 {
        ((w + 1) / 2, (h + 1) / 2)
    } else {
        (w, h)
    }
}

impl ModelEngine {
    pub(in crate::app::model3d) fn new_live_target(&self) -> Option<LiveTarget> {
        let gl = &self.gl;
        unsafe {
            self.gl_creates.fetch_add(12, Ordering::Relaxed);
            let target = LiveTarget {
                msaa_fbo: gl.create_framebuffer().ok()?,
                color_rb: gl.create_renderbuffer().ok()?,
                depth_rb: gl.create_renderbuffer().ok()?,
                resolve_fbo: gl.create_framebuffer().ok()?,
                resolve_tex: gl.create_texture().ok()?,
                depth_tex: gl.create_texture().ok()?,
                ao_fbo: gl.create_framebuffer().ok()?,
                ao_tex: gl.create_texture().ok()?,
                compose_fbo: gl.create_framebuffer().ok()?,
                compose_tex: gl.create_texture().ok()?,
                out_fbo: gl.create_framebuffer().ok()?,
                display: gl.create_texture().ok()?,
                egui_id: None,
                size: (0, 0),
            };
            self.size_live_target(&target, LIVE_TARGET_IDLE_PX, LIVE_TARGET_IDLE_PX);
            for (fbo, tex) in [
                (target.resolve_fbo, target.resolve_tex),
                (target.ao_fbo, target.ao_tex),
                (target.compose_fbo, target.compose_tex),
                (target.out_fbo, target.display),
            ] {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(tex),
                    0,
                );
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.resolve_fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::TEXTURE_2D,
                Some(target.depth_tex),
                0,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.msaa_fbo));
            for (attachment, rb) in [
                (glow::COLOR_ATTACHMENT0, target.color_rb),
                (glow::DEPTH_ATTACHMENT, target.depth_rb),
            ] {
                gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    attachment,
                    glow::RENDERBUFFER,
                    Some(rb),
                );
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            Some(target)
        }
    }

    fn size_live_target(&self, target: &LiveTarget, w: i32, h: i32) {
        let gl = &self.gl;
        unsafe {
            for (rb, format) in [
                (target.color_rb, glow::RGBA8),
                (target.depth_rb, glow::DEPTH_COMPONENT24),
            ] {
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
                gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, MSAA_SAMPLES, format, w, h);
            }
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            let display = if self.srgb_display {
                glow::SRGB8_ALPHA8
            } else {
                glow::RGBA8
            };
            for (tex, internal, format, kind) in [
                (
                    target.resolve_tex,
                    glow::RGBA8,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                ),
                (
                    target.compose_tex,
                    glow::RGBA8,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                ),
                (target.display, display, glow::RGBA, glow::UNSIGNED_BYTE),
                (target.ao_tex, glow::R8, glow::RED, glow::UNSIGNED_BYTE),
                (
                    target.depth_tex,
                    glow::DEPTH_COMPONENT24,
                    glow::DEPTH_COMPONENT,
                    glow::UNSIGNED_INT,
                ),
            ] {
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                let (tw, th) = if tex == target.ao_tex {
                    ao_size(w, h)
                } else {
                    (w, h)
                };
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    internal as i32,
                    tw,
                    th,
                    0,
                    format,
                    kind,
                    glow::PixelUnpackData::Slice(None),
                );
                let filter = if tex == target.depth_tex {
                    glow::NEAREST
                } else {
                    glow::LINEAR
                };
                for (param, value) in [
                    (glow::TEXTURE_MIN_FILTER, filter),
                    (glow::TEXTURE_MAG_FILTER, filter),
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                ] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, param, value as i32);
                }
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    pub(in crate::app::model3d) fn render_live(
        &self,
        target: &mut LiveTarget,
        model: &GpuModel,
        cam: &ModelCamera,
        size: (u32, u32, f32),
        adjust: Option<&ImageAdjust>,
    ) -> bool {
        self.render_frame(target, model, cam, size, adjust, false)
    }

    pub(super) fn render_frame(
        &self,
        target: &mut LiveTarget,
        model: &GpuModel,
        cam: &ModelCamera,
        (w, h, aspect): (u32, u32, f32),
        adjust: Option<&ImageAdjust>,
        capture_ground: bool,
    ) -> bool {
        self.render_passes.fetch_add(1, Ordering::Relaxed);
        let (w, h) = (w.clamp(16, 4096) as i32, h.clamp(16, 4096) as i32);
        if target.size != (w, h) {
            self.size_live_target(target, w, h);
            target.size = (w, h);
        }
        let ground = capture_ground || cam.display == ModelDisplay::Arctic;
        let camera = FrameCamera::new(model, cam, aspect, ground);
        let gl = &self.gl;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.msaa_fbo));
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                return false;
            }
            if self.srgb_framebuffer {
                gl.disable(glow::FRAMEBUFFER_SRGB);
            }
            self.draw_scene(model, &camera, w, h, ground);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(target.msaa_fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(target.resolve_fbo));
            gl.blit_framebuffer(
                0,
                0,
                w,
                h,
                0,
                0,
                w,
                h,
                glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT,
                glow::NEAREST,
            );
            let source = if cam.display == ModelDisplay::Arctic {
                self.ambient_occlusion(target, &camera, aspect, w, h);
                target.compose_tex
            } else {
                target.resolve_tex
            };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.out_fbo));
            gl.viewport(0, 0, w, h);
            gl.disable(glow::DEPTH_TEST);
            gl.use_program(Some(self.fx_program));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(source));
            gl.uniform_1_i32(Some(&self.u_fx_tex), 0);
            gl.uniform_1_f32(Some(&self.u_fx_flip), 1.0);
            self.bind_image_adjust_uniforms(&adjust.copied().unwrap_or_default());
            gl.bind_vertex_array(Some(self.bg_vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.use_program(None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        true
    }

    fn ambient_occlusion(
        &self,
        target: &LiveTarget,
        cam: &FrameCamera,
        aspect: f32,
        w: i32,
        h: i32,
    ) {
        let gl = &self.gl;
        let (aw, ah) = ao_size(w, h);
        unsafe {
            gl.disable(glow::DEPTH_TEST);
            gl.bind_vertex_array(Some(self.bg_vao));
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.ao_fbo));
            gl.viewport(0, 0, aw, ah);
            gl.use_program(Some(self.ao_program));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(target.depth_tex));
            gl.uniform_1_i32(self.uniform(self.ao_program, "u_depth_tex").as_ref(), 0);
            gl.uniform_4_f32(
                self.uniform(self.ao_program, "u_projection").as_ref(),
                cam.near,
                cam.far,
                (FOV_Y * 0.5).tan(),
                aspect,
            );
            gl.uniform_2_f32(
                self.uniform(self.ao_program, "u_size").as_ref(),
                aw as f32,
                ah as f32,
            );
            gl.uniform_1_f32(
                self.uniform(self.ao_program, "u_radius").as_ref(),
                cam.radius * 0.12,
            );
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.compose_fbo));
            gl.viewport(0, 0, w, h);
            gl.use_program(Some(self.compose_program));
            for (slot, name, tex) in [
                (0, "u_color_tex", target.resolve_tex),
                (1, "u_ao_tex", target.ao_tex),
                (2, "u_depth_tex", target.depth_tex),
            ] {
                gl.active_texture(glow::TEXTURE0 + slot);
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.uniform_1_i32(
                    self.uniform(self.compose_program, name).as_ref(),
                    slot as i32,
                );
            }
            gl.uniform_2_f32(
                self.uniform(self.compose_program, "u_size").as_ref(),
                aw as f32,
                ah as f32,
            );
            gl.uniform_2_f32(
                self.uniform(self.compose_program, "u_clip").as_ref(),
                cam.near,
                cam.far,
            );
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            for slot in 0..3 {
                gl.active_texture(glow::TEXTURE0 + slot);
                gl.bind_texture(glow::TEXTURE_2D, None);
            }
            gl.active_texture(glow::TEXTURE0);
        }
    }

    pub(in crate::app::model3d) fn read_target(
        &self,
        target: &LiveTarget,
    ) -> Option<egui::ColorImage> {
        unsafe {
            self.gl
                .bind_framebuffer(glow::READ_FRAMEBUFFER, Some(target.out_fbo));
            let image = self.read_rgba(target.size.0, target.size.1, false);
            self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            image
        }
    }
    pub(in crate::app::model3d) fn park_live_target(&self, target: &mut LiveTarget) {
        self.size_live_target(target, LIVE_TARGET_IDLE_PX, LIVE_TARGET_IDLE_PX);
        target.size = (LIVE_TARGET_IDLE_PX, LIVE_TARGET_IDLE_PX);
    }
    pub(super) fn free_target(&self, target: LiveTarget) {
        unsafe {
            for fbo in [
                target.msaa_fbo,
                target.resolve_fbo,
                target.ao_fbo,
                target.compose_fbo,
                target.out_fbo,
            ] {
                self.gl.delete_framebuffer(fbo);
            }
            for tex in [
                target.resolve_tex,
                target.depth_tex,
                target.ao_tex,
                target.compose_tex,
                target.display,
            ] {
                self.gl.delete_texture(tex);
            }
            self.gl.delete_renderbuffer(target.color_rb);
            self.gl.delete_renderbuffer(target.depth_rb);
        }
    }
}
