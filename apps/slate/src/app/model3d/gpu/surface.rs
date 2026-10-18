use super::*;

pub(super) struct FrameCamera {
    pub pose: ModelCamera,
    pub view: [f32; 16],
    pub mvp: [f32; 16],
    pub near: f32,
    pub far: f32,
    pub radius: f32,
}

impl FrameCamera {
    pub fn new(model: &GpuModel, cam: &ModelCamera, aspect: f32, ground: bool) -> Self {
        let pose = resolve_camera(cam, model.bounds_min, model.bounds_max);
        let (_, radius) = bounds_sphere(model.bounds_min, model.bounds_max);
        let view = look_at(eye_of(&pose), pose.target);
        let near = (pose.distance - radius * 2.0)
            .max(pose.distance * 0.01)
            .max(radius * 1e-3);
        let far = pose.distance + radius * if ground { GROUND_EXTENT * 1.5 } else { 4.0 };
        let mvp = mat_mul(&perspective(aspect, near, far), &view);
        Self {
            pose,
            view,
            mvp,
            near,
            far,
            radius,
        }
    }
}

impl ModelEngine {
    pub(super) fn draw_scene(
        &self,
        model: &GpuModel,
        cam: &FrameCamera,
        w: i32,
        h: i32,
        ground: bool,
    ) {
        let gl = &self.gl;
        let mode = cam.pose.display;
        let light = v_norm(lighting::SUN);
        let transform = |v: [f32; 3]| {
            std::array::from_fn::<_, 3, _>(|i| {
                cam.view[i] * v[0] + cam.view[4 + i] * v[1] + cam.view[8 + i] * v[2]
            })
        };
        let light = transform(light);
        let up = transform([0.0, 0.0, 1.0]);
        let (mut depth_near, mut depth_far) =
            view_depth_range(&cam.view, model.bounds_min, model.bounds_max);
        if ground {
            let height = eye_of(&cam.pose)[2] - model.bounds_min[2];
            let down = cam.pose.pitch + FOV_Y * 0.5;
            let nearest = if height > 0.0 && down > 0.01 {
                height / down.sin() * (FOV_Y * 0.5).cos()
            } else {
                depth_near
            };
            depth_near = nearest.clamp(depth_near * 0.05, depth_near);
            depth_far *= 4.0;
        }
        unsafe {
            gl.viewport(0, 0, w, h);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::POLYGON_OFFSET_FILL);
            gl.depth_mask(true);
            let white = matches!(mode, ModelDisplay::Arctic | ModelDisplay::HiddenLine);
            if white {
                gl.clear_color(0.97, 0.97, 0.965, 1.0);
            } else {
                gl.clear_color(0.0, 0.0, 0.0, 1.0);
            }
            gl.clear_depth_f64(1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.disable(glow::DEPTH_TEST);
            if mode == ModelDisplay::Shaded {
                gl.use_program(Some(self.bg_program));
                gl.bind_vertex_array(Some(self.bg_vao));
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
            }
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LESS);
            gl.use_program(Some(self.program));
            for (name, matrix) in [
                ("u_mvp", &cam.mvp),
                ("u_view", &cam.view),
                ("u_shadow_matrix", &model.shadow.matrix),
            ] {
                gl.uniform_matrix_4_f32_slice(
                    self.uniform(self.program, name).as_ref(),
                    false,
                    matrix,
                );
            }
            for (name, value) in [
                ("u_light", light),
                ("u_up", up),
                ("u_depth", [0.0, depth_near, depth_far]),
            ] {
                gl.uniform_3_f32(
                    self.uniform(self.program, name).as_ref(),
                    value[0],
                    value[1],
                    value[2],
                );
            }
            let mode_id = match mode {
                ModelDisplay::Shaded => 0.0,
                ModelDisplay::Arctic => 1.0,
                ModelDisplay::Material => 2.0,
                ModelDisplay::Depth => 3.0,
                ModelDisplay::HiddenLine => 4.0,
            };
            gl.uniform_1_f32(self.uniform(self.program, "u_mode").as_ref(), mode_id);
            gl.uniform_1_f32(
                self.uniform(self.program, "u_materials").as_ref(),
                if cam.pose.materials { 1.0 } else { 0.0 },
            );
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(model.shadow.texture));
            gl.uniform_1_i32(self.uniform(self.program, "u_shadow_tex").as_ref(), 1);
            gl.uniform_1_i32(self.uniform(self.program, "u_base_tex").as_ref(), 0);
            gl.bind_vertex_array(Some(model.vao));
            gl.uniform_1_f32(self.uniform(self.program, "u_ground").as_ref(), 0.0);
            if mode == ModelDisplay::HiddenLine {
                // Offset fills by their depth slope; fixed clip-space bias alone
                // produces broken strokes on grazing and vertical faces.
                gl.enable(glow::POLYGON_OFFSET_FILL);
                gl.polygon_offset(1.0, 1.0);
            }
            for draw in &model.draws {
                for (name, value) in [
                    ("u_color", draw.color),
                    ("u_mask", draw.mask),
                    ("u_emission", draw.emission),
                ] {
                    gl.uniform_3_f32(
                        self.uniform(self.program, name).as_ref(),
                        value[0],
                        value[1],
                        value[2],
                    );
                }
                gl.uniform_1_f32(
                    self.uniform(self.program, "u_roughness").as_ref(),
                    draw.roughness,
                );
                gl.uniform_1_f32(
                    self.uniform(self.program, "u_metallic").as_ref(),
                    draw.metallic,
                );
                gl.uniform_1_f32(
                    self.uniform(self.program, "u_texture").as_ref(),
                    if draw.texture.is_some() { 1.0 } else { 0.0 },
                );
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, draw.texture);
                gl.draw_elements(glow::TRIANGLES, draw.count, glow::UNSIGNED_INT, draw.offset);
            }
            if ground {
                gl.uniform_1_f32(self.uniform(self.program, "u_ground").as_ref(), 1.0);
                gl.uniform_3_f32(
                    self.uniform(self.program, "u_color").as_ref(),
                    0.87,
                    0.87,
                    0.87,
                );
                gl.uniform_3_f32(
                    self.uniform(self.program, "u_mask").as_ref(),
                    0.59,
                    0.6,
                    0.59,
                );
                gl.uniform_3_f32(
                    self.uniform(self.program, "u_emission").as_ref(),
                    0.0,
                    0.0,
                    0.0,
                );
                gl.uniform_1_f32(self.uniform(self.program, "u_texture").as_ref(), 0.0);
                gl.uniform_1_f32(self.uniform(self.program, "u_metallic").as_ref(), 0.0);
                gl.uniform_1_f32(self.uniform(self.program, "u_roughness").as_ref(), 1.0);
                gl.draw_elements(glow::TRIANGLES, 6, glow::UNSIGNED_INT, model.ground_offset);
            }
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.disable(glow::POLYGON_OFFSET_FILL);
            if mode == ModelDisplay::HiddenLine {
                self.draw_edges(model, cam, w, h);
            }
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
        }
    }

    fn draw_edges(&self, model: &GpuModel, cam: &FrameCamera, w: i32, h: i32) {
        let gl = &self.gl;
        let eye = eye_of(&cam.pose);
        unsafe {
            gl.use_program(Some(self.edge_program));
            gl.uniform_matrix_4_f32_slice(
                self.uniform(self.edge_program, "u_mvp").as_ref(),
                false,
                &cam.mvp,
            );
            gl.uniform_3_f32(
                self.uniform(self.edge_program, "u_eye").as_ref(),
                eye[0],
                eye[1],
                eye[2],
            );
            gl.uniform_2_f32(
                self.uniform(self.edge_program, "u_size").as_ref(),
                w as f32,
                h as f32,
            );
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(false);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(
                glow::SRC_ALPHA,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );
            gl.bind_vertex_array(Some(model.edge_vao));
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, model.edge_count);
            gl.disable(glow::BLEND);
            gl.depth_mask(true);
            gl.depth_func(glow::LESS);
        }
    }
}
