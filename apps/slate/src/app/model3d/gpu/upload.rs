use super::*;

/// Each run covers contiguous indices. Only the diagnostic material view
/// distinguishes the generated per-object mask colors.
pub(super) fn append_draw(runs: &mut Vec<DrawRange>, draw: DrawRange, use_mask: bool) {
    if let Some(last) = runs.last_mut() {
        if last.offset + last.count * 4 == draw.offset
            && (!use_mask || last.mask == draw.mask)
            && last.opacity == draw.opacity
            && last.color == draw.color
            && last.roughness == draw.roughness
            && last.metallic == draw.metallic
            && last.emission == draw.emission
            && last.texture == draw.texture
        {
            last.count += draw.count;
            return;
        }
    }
    runs.push(draw);
}

impl ModelEngine {
    pub fn upload(&self, scene: &PreviewScene) -> Option<GpuModel> {
        let gl = &self.gl;
        // Keep admission to one allocation per packed stream even on models
        // containing thousands of separate Rhino objects.
        let (vertices, indices, edges) = scene
            .meshes
            .iter()
            .filter(|m| !m.positions.is_empty() && !m.indices.is_empty())
            .fold((4, 6, 0), |(v, i, e), m| {
                (
                    v + m.positions.len(),
                    i + m.indices.len(),
                    e + m.feature_edges.len(),
                )
            });
        let mut vertices = Vec::<f32>::with_capacity(vertices * 8);
        let mut indices = Vec::<u32>::with_capacity(indices);
        let mut edges = Vec::<f32>::with_capacity(edges * 13);
        let mut draws = Vec::<DrawRange>::new();
        let mut surface_draws = Vec::new();
        let mut bounded_draws = Vec::new();
        let build_navigation = scene
            .meshes
            .iter()
            .any(|mesh| !mesh.navigation_indices.is_empty());
        let mut navigation_indices = Vec::new();
        let mut navigation_draws = Vec::new();
        let mut navigation_error = 0.0f32;
        let mut translucent = Vec::new();
        let mut opaque_indices = 0;
        let mut texture_cache = HashMap::new();
        let mut order: Vec<_> = (0..scene.meshes.len()).collect();
        order.sort_by_key(|&i| {
            let m = &scene.meshes[i];
            (
                m.material.as_ref().is_some_and(|m| m.base_color[3] < 1.0),
                m.color,
                m.material.as_ref().map(|m| {
                    (
                        m.base_color.map(f32::to_bits),
                        m.roughness.to_bits(),
                        m.metallic.to_bits(),
                    )
                }),
            )
        });
        for i in order {
            let mesh = &scene.meshes[i];
            if mesh.positions.is_empty() || mesh.indices.is_empty() {
                continue;
            }
            let offset = (indices.len() * 4) as i32;
            let base = (vertices.len() / 8) as u32;
            let (mut min, mut max) = (mesh.positions[0], mesh.positions[0]);
            for (j, p) in mesh.positions.iter().enumerate() {
                for axis in 0..3 {
                    min[axis] = min[axis].min(p[axis]);
                    max[axis] = max[axis].max(p[axis]);
                }
                vertices.extend_from_slice(p);
                vertices.extend_from_slice(mesh.normals.get(j).unwrap_or(&[0.0, 0.0, 1.0]));
                vertices.extend_from_slice(mesh.texcoords.get(j).unwrap_or(&[0.0, 0.0]));
            }
            indices.extend(mesh.indices.iter().map(|n| n + base));
            let rgb = mesh
                .color
                .map(|c| c.map(|v| v as f32 / 255.0))
                .unwrap_or(DEFAULT_PART_COLOR);
            let mat = mesh.material.as_ref();
            let color = mat
                .map(|m| [m.base_color[0], m.base_color[1], m.base_color[2]])
                .unwrap_or_else(|| rgb.map(|c| c.powf(2.2)));
            let texture = mat
                .and_then(|m| m.base_color_texture.as_ref())
                .and_then(|src| {
                    let model_preview::TextureSource::Decoded {
                        rgba,
                        width,
                        height,
                    } = src
                    else {
                        return None;
                    };
                    *texture_cache
                        .entry((rgba.as_ptr() as usize, *width, *height))
                        .or_insert_with(|| self.upload_texture(src))
                });
            let draw = DrawRange {
                offset,
                count: mesh.indices.len() as i32,
                color,
                opacity: mat.map_or(1.0, |m| m.base_color[3].clamp(0.0, 1.0)),
                mask: mesh.color.map(|_| rgb).unwrap_or_else(|| mask_palette(i)),
                roughness: mat.map_or(0.75, |m| m.roughness),
                metallic: mat.map_or(0.0, |m| m.metallic),
                emission: mat.map_or([0.0; 3], |m| m.emissive),
                texture,
            };
            let center = std::array::from_fn(|axis| (min[axis] + max[axis]) * 0.5);
            bounded_draws.push(BoundedDraw {
                center,
                half: std::array::from_fn(|axis| (max[axis] - min[axis]) * 0.5),
                draw: draw.clone(),
            });
            if draw.opacity < 1.0 {
                if draw.opacity > 0.0 {
                    translucent.push(TranslucentDraw {
                        draw: draw.clone(),
                        center,
                    });
                }
            } else {
                opaque_indices += draw.count;
                if build_navigation {
                    let lod = if mesh.navigation_indices.is_empty() {
                        &mesh.indices
                    } else {
                        &mesh.navigation_indices
                    };
                    let mut range = draw.clone();
                    range.offset = (navigation_indices.len() * 4) as i32;
                    range.count = lod.len() as i32;
                    append_draw(&mut navigation_draws, range, false);
                    navigation_indices.extend(lod.iter().map(|index| index + base));
                    if !mesh.navigation_indices.is_empty() {
                        navigation_error = navigation_error.max(mesh.navigation_error);
                    }
                }
            }
            append_draw(&mut surface_draws, draw.clone(), false);
            append_draw(&mut draws, draw, true);
            for edge in &mesh.feature_edges {
                edges.extend_from_slice(&edge.positions[0]);
                edges.extend_from_slice(&edge.positions[1]);
                edges.extend_from_slice(&edge.normals[0]);
                edges.extend_from_slice(&edge.normals[1]);
                edges.push(u8::from(edge.boundary) as f32 * 2.0 + u8::from(edge.crease) as f32);
            }
        }
        if indices.is_empty() {
            return None;
        }
        let ground_offset = (indices.len() * 4) as i32;
        let base = (vertices.len() / 8) as u32;
        let (center, radius) = bounds_sphere(scene.bounds_min, scene.bounds_max);
        let extent = radius * GROUND_EXTENT;
        for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            vertices.extend_from_slice(&[
                center[0] + x * extent,
                center[1] + y * extent,
                scene.bounds_min[2] - radius * 0.002,
                0.0,
                0.0,
                1.0,
                0.0,
                0.0,
            ]);
        }
        indices.extend([0, 1, 2, 0, 2, 3].map(|i| i + base));
        let navigation = (!navigation_indices.is_empty()).then(|| {
            let offset = (indices.len() * 4) as i32;
            let count = navigation_indices.len() as i32;
            for draw in &mut navigation_draws {
                draw.offset += offset;
            }
            indices.append(&mut navigation_indices);
            NavigationMesh {
                draws: navigation_draws,
                offset,
                count,
                error: navigation_error,
            }
        });
        unsafe {
            self.gl_creates.fetch_add(5, Ordering::Relaxed);
            let vao = gl.create_vertex_array().ok()?;
            let vbo = gl.create_buffer().ok()?;
            let ebo = gl.create_buffer().ok()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck_f32_slice(&vertices),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytemuck_u32_slice(&indices),
                glow::STATIC_DRAW,
            );
            for (slot, n, offset) in [(0, 3, 0), (1, 3, 12), (2, 2, 24)] {
                gl.vertex_attrib_pointer_f32(slot, n, glow::FLOAT, false, 32, offset);
                gl.enable_vertex_attrib_array(slot);
            }
            let edge_vao = gl.create_vertex_array().ok()?;
            let edge_vbo = gl.create_buffer().ok()?;
            gl.bind_vertex_array(Some(edge_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(edge_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck_f32_slice(&edges),
                glow::STATIC_DRAW,
            );
            for slot in 0..5 {
                gl.vertex_attrib_pointer_f32(
                    slot,
                    if slot == 4 { 1 } else { 3 },
                    glow::FLOAT,
                    false,
                    52,
                    slot as i32 * 12,
                );
                gl.enable_vertex_attrib_array(slot);
                gl.vertex_attrib_divisor(slot, 1);
            }
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            let shadow =
                self.make_shadow(vao, opaque_indices, scene.bounds_min, scene.bounds_max)?;
            static GENERATION: AtomicU64 = AtomicU64::new(1);
            Some(GpuModel {
                generation: GENERATION.fetch_add(1, Ordering::Relaxed),
                vao,
                vbo,
                ebo,
                draws,
                surface_draws,
                bounded_draws,
                opaque_indices,
                navigation,
                translucent,
                textures: texture_cache.into_values().flatten().collect(),
                ground_offset,
                edge_vao,
                edge_vbo,
                edge_count: (edges.len() / 13) as i32,
                shadow,
                bounds_min: scene.bounds_min,
                bounds_max: scene.bounds_max,
            })
        }
    }

    fn upload_texture(&self, source: &model_preview::TextureSource) -> Option<glow::Texture> {
        let model_preview::TextureSource::Decoded {
            rgba,
            width,
            height,
        } = source
        else {
            return None;
        };
        let gl = &self.gl;
        unsafe {
            let tex = gl.create_texture().ok()?;
            self.gl_creates.fetch_add(1, Ordering::Relaxed);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::SRGB8_ALPHA8 as i32,
                *width as i32,
                *height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(rgba)),
            );
            gl.generate_mipmap(glow::TEXTURE_2D);
            for (param, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::REPEAT),
                (glow::TEXTURE_WRAP_T, glow::REPEAT),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, param, value as i32);
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
            Some(tex)
        }
    }

    pub fn free(&self, model: GpuModel) {
        let gl = &self.gl;
        unsafe {
            for tex in model.textures {
                gl.delete_texture(tex);
            }
            gl.delete_vertex_array(model.vao);
            gl.delete_buffer(model.vbo);
            gl.delete_buffer(model.ebo);
            gl.delete_vertex_array(model.edge_vao);
            gl.delete_buffer(model.edge_vbo);
            gl.delete_texture(model.shadow.texture);
            gl.delete_framebuffer(model.shadow.fbo);
        }
    }
}
