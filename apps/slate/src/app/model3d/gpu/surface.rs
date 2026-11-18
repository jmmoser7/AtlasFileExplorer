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

impl FrameCamera {
    /// Homogeneous clip planes in world space; a box is outside only
    /// when its most positive support point lies beyond one of them.
    fn clip_planes(&self) -> [[f32; 4]; 6] {
        std::array::from_fn(|plane| {
            let axis = plane / 2;
            let sign = if plane % 2 == 0 { 1.0 } else { -1.0 };
            std::array::from_fn(|column| {
                self.mvp[column * 4 + 3] + sign * self.mvp[column * 4 + axis]
            })
        })
    }
}

fn in_plane(plane: &[f32; 4], center: [f32; 3], half: [f32; 3], side: f32) -> bool {
    let mut distance = plane[3];
    let mut scale = plane[3].abs();
    for axis in 0..3 {
        let d = plane[axis] * center[axis];
        let radius = plane[axis].abs() * half[axis];
        distance += d + side * radius;
        scale += d.abs() + radius;
    }
    // Allow for rounding between the CPU test and vertex shader.
    distance >= -8.0 * f32::EPSILON * scale
}

impl GpuModel {
    pub(super) fn visible_surfaces(
        &self,
        cam: &FrameCamera,
        opaque_only: bool,
    ) -> Option<Vec<DrawRange>> {
        let planes = cam.clip_planes();
        let center = std::array::from_fn(|i| (self.bounds_min[i] + self.bounds_max[i]) * 0.5);
        let half = std::array::from_fn(|i| (self.bounds_max[i] - self.bounds_min[i]) * 0.5);
        if planes.iter().all(|p| in_plane(p, center, half, -1.0)) {
            return None;
        }
        let mut visible = Vec::new();
        for part in &self.bounded_draws {
            if (!opaque_only || part.draw.opacity >= 1.0)
                && planes
                    .iter()
                    .all(|p| in_plane(p, part.center, part.half, 1.0))
            {
                upload::append_draw(&mut visible, part.draw.clone(), false);
            }
        }
        Some(visible)
    }
}

impl ModelEngine {
    pub(super) fn draw_scene(
        &self,
        model: &GpuModel,
        cam: &FrameCamera,
        (w, h, output_h): (i32, i32, i32),
        navigating: bool,
        ground: bool,
        batch: refinement::Batch<'_>,
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
        let lod = model.navigation.as_ref().filter(|mesh| {
            let can_use_opaque = cam.pose.materials && mode == ModelDisplay::Shaded
                || model.opaque_indices == model.ground_offset / 4;
            let world_per_pixel =
                2.0 * depth_near.max(cam.near) * (FOV_Y * 0.5).tan() / output_h as f32;
            navigating
                && can_use_opaque
                && matches!(mode, ModelDisplay::Shaded | ModelDisplay::Arctic)
                && mesh.error <= 1.5 * world_per_pixel
        });
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
            if batch.clear {
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
            gl.uniform_1_i32(
                self.uniform(self.program, "u_navigating").as_ref(),
                i32::from(navigating),
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
            let uniform_surface = matches!(mode, ModelDisplay::Depth | ModelDisplay::Arctic)
                || (!cam.pose.materials && mode != ModelDisplay::Material);
            let shaded_materials = mode == ModelDisplay::Shaded && cam.pose.materials;
            let culled = if navigating
                && lod.is_none()
                && model.large_scene()
                && matches!(mode, ModelDisplay::Shaded | ModelDisplay::Arctic)
            {
                model.visible_surfaces(cam, shaded_materials)
            } else {
                None
            };
            let draws = if let Some(visible) = &culled {
                visible.as_slice()
            } else if let Some(lod) = lod {
                lod.draws.as_slice()
            } else if mode == ModelDisplay::Material {
                model.draws.as_slice()
            } else if shaded_materials && model.opaque_indices < model.ground_offset / 4 {
                let end = model
                    .surface_draws
                    .partition_point(|d| d.offset / 4 < model.opaque_indices);
                &model.surface_draws[..end]
            } else {
                model.surface_draws.as_slice()
            };
            gl.uniform_1_f32(self.uniform(self.program, "u_opacity").as_ref(), 1.0);
            let draws = if uniform_surface && culled.is_some() {
                for draw in draws {
                    gl.draw_elements(glow::TRIANGLES, draw.count, glow::UNSIGNED_INT, draw.offset);
                }
                &[][..]
            } else if uniform_surface {
                let count = lod.map_or(model.ground_offset / 4, |mesh| mesh.count);
                let offset = lod.map_or(0, |mesh| mesh.offset);
                if let Some((skip, n)) = batch.clip(0, i64::from(count)) {
                    gl.draw_elements(
                        glow::TRIANGLES,
                        n as i32,
                        glow::UNSIGNED_INT,
                        offset + 4 * skip as i32,
                    );
                }
                &[][..]
            } else if batch.layers.is_some() {
                let first =
                    draws.partition_point(|d| i64::from(d.offset / 4 + d.count) <= batch.start);
                let last = draws.partition_point(|d| i64::from(d.offset / 4) < batch.end);
                &draws[first..last]
            } else {
                draws
            };
            let draw_surface = |draw: &DrawRange, skip: i64, count: i64| {
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
                gl.draw_elements(
                    glow::TRIANGLES,
                    count as i32,
                    glow::UNSIGNED_INT,
                    draw.offset + 4 * skip as i32,
                );
            };
            for draw in draws {
                if let Some((skip, count)) =
                    batch.clip(i64::from(draw.offset / 4), i64::from(draw.count))
                {
                    draw_surface(draw, skip, count);
                }
            }
            let after_surfaces = i64::from(if shaded_materials {
                model.opaque_indices
            } else {
                model.ground_offset / 4
            });
            let ground_work = if ground {
                i64::from(h) * refinement::GROUND_ROW_INDICES
            } else {
                0
            };
            if let Some((start, len)) = batch.clip(after_surfaces, ground_work) {
                let per_row = refinement::GROUND_ROW_INDICES;
                let first = (start + per_row - 1) / per_row;
                let end = (start + len + per_row - 1) / per_row;
                gl.enable(glow::SCISSOR_TEST);
                gl.scissor(0, first as i32, w, (end - first) as i32);
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
            gl.disable(glow::SCISSOR_TEST);
            let alpha_start = after_surfaces + ground_work;
            if shaded_materials && !model.translucent.is_empty() && batch.end > alpha_start {
                // View-space z increases towards the eye. Opaque scene and
                // ground depths are already complete. Sort only the meshes
                // with authored partial opacity; preserve destination alpha.
                let sorted;
                let layers = if let Some(layers) = batch.layers {
                    layers
                } else {
                    sorted = model.sorted_layers(&cam.view);
                    &sorted
                };
                let first = layers.partition_point(|&(offset, i)| {
                    alpha_start + offset + i64::from(model.translucent[i].draw.count) <= batch.start
                });
                let last = layers.partition_point(|&(offset, _)| alpha_start + offset < batch.end);
                gl.uniform_1_f32(self.uniform(self.program, "u_ground").as_ref(), 0.0);
                gl.depth_mask(false);
                gl.enable(glow::BLEND);
                gl.blend_func_separate(
                    glow::SRC_ALPHA,
                    glow::ONE_MINUS_SRC_ALPHA,
                    glow::ZERO,
                    glow::ONE,
                );
                for &(offset, i) in &layers[first..last] {
                    let draw = &model.translucent[i].draw;
                    let Some((skip, count)) =
                        batch.clip(alpha_start + offset, i64::from(draw.count))
                    else {
                        continue;
                    };
                    gl.uniform_1_f32(
                        self.uniform(self.program, "u_opacity").as_ref(),
                        draw.opacity,
                    );
                    draw_surface(draw, skip, count);
                }
                gl.disable(glow::BLEND);
                gl.depth_mask(true);
            }
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.disable(glow::POLYGON_OFFSET_FILL);
            if mode == ModelDisplay::HiddenLine {
                if let Some((skip, count)) =
                    batch.clip(after_surfaces, i64::from(model.edge_count) * 3)
                {
                    self.draw_edges(model, cam, (w, h), (skip as i32 / 3, count as i32 / 3));
                }
            }
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
        }
    }

    fn draw_edges(
        &self,
        model: &GpuModel,
        cam: &FrameCamera,
        (w, h): (i32, i32),
        (first, count): (i32, i32),
    ) {
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
            if first != 0 {
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(model.edge_vbo));
                for slot in 0..5 {
                    gl.vertex_attrib_pointer_f32(
                        slot,
                        if slot == 4 { 1 } else { 3 },
                        glow::FLOAT,
                        false,
                        52,
                        first * 52 + slot as i32 * 12,
                    );
                }
            }
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, count);
            if first != 0 {
                for slot in 0..5 {
                    gl.vertex_attrib_pointer_f32(
                        slot,
                        if slot == 4 { 1 } else { 3 },
                        glow::FLOAT,
                        false,
                        52,
                        slot as i32 * 12,
                    );
                }
                gl.bind_buffer(glow::ARRAY_BUFFER, None);
            }
            gl.disable(glow::BLEND);
            gl.depth_mask(true);
            gl.depth_func(glow::LESS);
        }
    }
}
