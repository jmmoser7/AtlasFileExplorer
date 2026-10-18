use super::*;

impl ModelEngine {
    pub(super) fn bind_image_adjust_uniforms(&self, adjust: &ImageAdjust) {
        let mat = super::super::super::imagefx::gl_color_mat3(adjust);
        let (scale, offset) = super::super::super::imagefx::filter_tone(adjust);
        let (overlay, invert) = super::super::super::imagefx::filter_shader_overlay(adjust);
        let gl = &self.gl;
        unsafe {
            gl.uniform_matrix_3_f32_slice(Some(&self.u_fx_mat), false, &mat);
            gl.uniform_1_f32(Some(&self.u_fx_scale), scale);
            gl.uniform_1_f32(Some(&self.u_fx_offset), offset);
            gl.uniform_1_f32(Some(&self.u_fx_invert), invert);
            gl.uniform_4_f32(
                Some(&self.u_fx_overlay),
                overlay[0],
                overlay[1],
                overlay[2],
                overlay[3],
            );
        }
    }

    pub(super) fn read_rgba(&self, w: i32, h: i32, flip_rows: bool) -> Option<egui::ColorImage> {
        self.readbacks.fetch_add(1, Ordering::Relaxed);
        let gl = &self.gl;
        unsafe {
            let mut buf = vec![0u8; (w * h * 4) as usize];
            gl.read_pixels(
                0,
                0,
                w,
                h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut buf)),
            );
            Some(color_image_from_readback(&buf, w, h, flip_rows))
        }
    }

    pub(super) fn filter_resolved_texture(
        &self,
        src_tex: glow::Texture,
        w: i32,
        h: i32,
        adjust: &ImageAdjust,
        flip_readback: bool,
    ) -> Option<egui::ColorImage> {
        let gl = &self.gl;
        unsafe {
            self.gl_creates.fetch_add(2, Ordering::Relaxed);
            let out_fbo = gl.create_framebuffer().ok()?;
            let out_tex = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(out_tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                w,
                h,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(out_fbo));
            gl.framebuffer_texture_2d(
                glow::DRAW_FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(out_tex),
                0,
            );
            gl.viewport(0, 0, w, h);
            gl.disable(glow::DEPTH_TEST);
            gl.use_program(Some(self.fx_program));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(src_tex));
            gl.uniform_1_i32(Some(&self.u_fx_tex), 0);
            gl.uniform_1_f32(Some(&self.u_fx_flip), 0.0);
            self.bind_image_adjust_uniforms(adjust);
            gl.bind_vertex_array(Some(self.bg_vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(out_fbo));
            let img = self.read_rgba(w, h, flip_readback);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            gl.delete_framebuffer(out_fbo);
            gl.delete_texture(out_tex);
            img
        }
    }

    /// GPU photo-filter pass for a CPU [`ColorImage`] (posters, exports).
    pub fn filter_color_image(
        &self,
        src: &egui::ColorImage,
        adjust: &ImageAdjust,
    ) -> egui::ColorImage {
        if adjust.is_identity() {
            return src.clone();
        }
        let w = src.width() as i32;
        let h = src.height() as i32;
        let mut rgba = Vec::with_capacity(src.pixels.len() * 4);
        for p in &src.pixels {
            rgba.extend_from_slice(&p.to_srgba_unmultiplied());
        }
        let gl = &self.gl;
        unsafe {
            self.gl_creates.fetch_add(1, Ordering::Relaxed);
            let tex = match gl.create_texture() {
                Ok(t) => t,
                Err(_) => return super::super::super::imagefx::adjusted(src, adjust),
            };
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                w,
                h,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&rgba)),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
            let out = self
                .filter_resolved_texture(tex, w, h, adjust, false)
                .unwrap_or_else(|| super::super::super::imagefx::adjusted(src, adjust));
            gl.delete_texture(tex);
            out
        }
    }
}
