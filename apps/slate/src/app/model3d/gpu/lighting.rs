use super::*;

pub(super) struct Shadow {
    pub texture: glow::Texture,
    pub fbo: glow::Framebuffer,
    pub matrix: [f32; 16],
}
pub(super) const SHADOW_SIZE: i32 = 2048;
pub(super) const SUN: [f32; 3] = [-0.45, -0.55, 0.85];

impl ModelEngine {
    pub(super) fn make_shadow(
        &self,
        vao: glow::VertexArray,
        draws: &[DrawRange],
        min: [f32; 3],
        max: [f32; 3],
    ) -> Option<Shadow> {
        let (center, radius) = bounds_sphere(min, max);
        let light = v_norm(SUN);
        let eye = std::array::from_fn(|i| center[i] + light[i] * radius * 4.0);
        let view = look_at(eye, center);
        let r = radius * 1.12;
        let near = radius * 0.05;
        let far = radius * 8.0;
        let ortho = [
            1.0 / r,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0 / r,
            0.0,
            0.0,
            0.0,
            0.0,
            -2.0 / (far - near),
            0.0,
            0.0,
            0.0,
            -(far + near) / (far - near),
            1.0,
        ];
        let matrix = mat_mul(&ortho, &view);
        let gl = &self.gl;
        unsafe {
            self.gl_creates.fetch_add(2, Ordering::Relaxed);
            let fbo = gl.create_framebuffer().ok()?;
            let texture = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::DEPTH_COMPONENT24 as i32,
                SHADOW_SIZE,
                SHADOW_SIZE,
                0,
                glow::DEPTH_COMPONENT,
                glow::UNSIGNED_INT,
                glow::PixelUnpackData::Slice(None),
            );
            for (param, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, param, value as i32);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            gl.draw_buffer(glow::NONE);
            gl.read_buffer(glow::NONE);
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.delete_framebuffer(fbo);
                gl.delete_texture(texture);
                return None;
            }
            gl.viewport(0, 0, SHADOW_SIZE, SHADOW_SIZE);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_mask(true);
            gl.depth_func(glow::LESS);
            gl.enable(glow::POLYGON_OFFSET_FILL);
            gl.polygon_offset(1.5, 2.0);
            gl.clear_depth_f64(1.0);
            gl.clear(glow::DEPTH_BUFFER_BIT);
            gl.use_program(Some(self.shadow_program));
            gl.uniform_matrix_4_f32_slice(
                self.uniform(self.shadow_program, "u_mvp").as_ref(),
                false,
                &matrix,
            );
            gl.bind_vertex_array(Some(vao));
            for draw in draws {
                gl.draw_elements(glow::TRIANGLES, draw.count, glow::UNSIGNED_INT, draw.offset);
            }
            gl.disable(glow::POLYGON_OFFSET_FILL);
            gl.disable(glow::DEPTH_TEST);
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            Some(Shadow {
                texture,
                fbo,
                matrix,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadow_camera_contains_model_corners() {
        let min = [-8.0, -3.0, -1.0];
        let max = [8.0, 3.0, 6.0];
        let (center, radius) = bounds_sphere(min, max);
        let l = v_norm(SUN);
        let view = look_at(
            std::array::from_fn(|i| center[i] + l[i] * radius * 4.0),
            center,
        );
        for x in [min[0], max[0]] {
            for y in [min[1], max[1]] {
                for z in [min[2], max[2]] {
                    let p = [x, y, z];
                    for row in 0..2 {
                        let v = view[row] * p[0]
                            + view[4 + row] * p[1]
                            + view[8 + row] * p[2]
                            + view[12 + row];
                        assert!(v.abs() < radius * 1.12, "shadow receiver must fit");
                    }
                }
            }
        }
    }
}
