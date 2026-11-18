//! Cancellable live refinement on the software rasterizer. Scratch color
//! and depth stay in the retained MSAA target until the final resolve.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app::model3d) enum LiveDraw {
    Failed,
    Pending,
    Draft,
    Complete,
}

#[derive(PartialEq, Eq)]
struct Stamp {
    model: u64,
    camera: u64,
    size: (u32, u32, u32),
    filter: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Post {
    Resolve,
    Occlusion,
    Compose,
    Filter,
    Publish,
}

pub(super) const GROUND_ROW_INDICES: i64 = 384;

pub(super) struct Refinement {
    post: Post,
    row: i32,
    band: i32,
    stamp: Stamp,
    cursor: i64,
    total: i64,
    triangles: i64,
    layers: Vec<(i64, usize)>,
    visible: Option<Vec<(i64, i64)>>,
}

#[derive(Clone, Copy)]
pub(super) struct Batch<'a> {
    pub(super) start: i64,
    pub(super) end: i64,
    pub(super) complete: bool,
    pub(super) clear: bool,
    pub(super) layers: Option<&'a [(i64, usize)]>,
}

impl Batch<'_> {
    pub(super) const ALL: Self = Self {
        start: 0,
        end: i64::MAX,
        complete: true,
        clear: true,
        layers: None,
    };

    pub(super) fn clip(self, first: i64, count: i64) -> Option<(i64, i64)> {
        let start = first.max(self.start);
        let end = (first + count).min(self.end);
        (start < end).then_some((start - first, end - start))
    }
}

#[cfg(test)]
impl Refinement {
    pub(super) fn step(&self) -> (&'static str, i64, i64) {
        if self.cursor < self.total {
            return ("geometry", self.cursor, self.triangles);
        }
        let stage = match self.post {
            Post::Resolve => "resolve",
            Post::Occlusion => "occlusion",
            Post::Compose => "compose",
            Post::Filter => "filter",
            Post::Publish => "publish",
        };
        (stage, i64::from(self.row), i64::from(self.band))
    }
}

impl GpuModel {
    pub(super) fn sorted_layers(&self, view: &[f32; 16]) -> Vec<(i64, usize)> {
        let mut parts: Vec<_> = self
            .translucent
            .iter()
            .enumerate()
            .map(|(i, part)| {
                let p = part.center;
                (view[2] * p[0] + view[6] * p[1] + view[10] * p[2], i)
            })
            .collect();
        parts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut first = 0i64;
        parts
            .into_iter()
            .map(|(_, i)| {
                let offset = first;
                first += self.translucent[i].draw.count as i64;
                (offset, i)
            })
            .collect()
    }
}

impl ModelEngine {
    // Allocate a second display only on targets that refine. Matching the
    // front buffer's color encoding permits an atomic handle swap on completion.
    fn ensure_standby_display(&self, target: &mut LiveTarget) -> bool {
        if target.standby.is_some() {
            return true;
        }
        let gl = &self.gl;
        unsafe {
            let Ok(fbo) = gl.create_framebuffer() else {
                return false;
            };
            let Ok(texture) = gl.create_texture() else {
                gl.delete_framebuffer(fbo);
                return false;
            };
            self.gl_creates.fetch_add(2, Ordering::Relaxed);
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            let format = if self.srgb_display {
                glow::SRGB8_ALPHA8
            } else {
                glow::RGBA8
            };
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                format as i32,
                target.size.0,
                target.size.1,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            for (parameter, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, value as i32);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            let complete =
                gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            if !complete {
                gl.delete_framebuffer(fbo);
                gl.delete_texture(texture);
                return false;
            }
            target.standby = Some(StandbyDisplay {
                fbo,
                texture,
                egui_id: None,
            });
        }
        true
    }

    pub(in crate::app::model3d) fn refines_live(&self, model: &GpuModel) -> bool {
        self.software && model.large_scene()
    }

    pub(in crate::app::model3d) fn refine_live(
        &self,
        target: &mut LiveTarget,
        model: &GpuModel,
        cam: &ModelCamera,
        size: (u32, u32, f32),
        adjust: Option<&ImageAdjust>,
    ) -> LiveDraw {
        let stamp = Stamp {
            model: model.generation,
            camera: cam.cache_hash(),
            size: (size.0, size.1, size.2.to_bits()),
            filter: adjust.map_or(0, |value| value.cache_hash()),
        };
        let mut job = target
            .refinement
            .take()
            .filter(|job| job.stamp == stamp)
            .unwrap_or_else(|| {
                let shaded = cam.display == ModelDisplay::Shaded && cam.materials;
                let ground = cam.display == ModelDisplay::Arctic;
                let geometry = if shaded {
                    model.opaque_indices
                } else {
                    model.ground_offset / 4
                };
                let layers = if shaded && !model.translucent.is_empty() {
                    model.sorted_layers(&surface::FrameCamera::new(model, cam, size.2, ground).view)
                } else {
                    Vec::new()
                };
                let alpha = layers.last().map_or(0, |&(offset, i)| {
                    offset + model.translucent[i].draw.count as i64
                });
                let edge = if cam.display == ModelDisplay::HiddenLine {
                    i64::from(model.edge_count) * 3
                } else {
                    0
                };
                let total = i64::from(geometry)
                    + if ground {
                        i64::from(size.1) * GROUND_ROW_INDICES
                    } else {
                        0
                    }
                    + alpha
                    + edge;
                let camera = surface::FrameCamera::new(model, cam, size.2, ground);
                let visible = model.visible_surfaces(&camera, shaded).map(|draws| {
                    let mut spans: Vec<_> = draws
                        .into_iter()
                        .map(|draw| {
                            let first = i64::from(draw.offset / 4);
                            (first, first + i64::from(draw.count))
                        })
                        .collect();
                    // Transparent surfaces, ground and analytic edges follow the opaque range.
                    if i64::from(geometry) < total {
                        spans.push((i64::from(geometry), total));
                    }
                    spans
                });
                Refinement {
                    visible,
                    stamp,
                    cursor: 0,
                    // Even a fully transparent scene must clear the scratch target.
                    total: total.max(3),
                    triangles: 8,
                    layers,
                    post: Post::Resolve,
                    row: 0,
                    band: 8,
                }
            });
        let start = Instant::now();
        let geometry = job.cursor < job.total;
        if geometry {
            // Skip whole out-of-frustum ranges, without losing the initial clear.
            // Short visible slices also keep an empty region from training the
            // budget on millions of invisible triangles before a detailed part.
            let (first, last) = job
                .visible
                .as_ref()
                .map_or((job.cursor, job.total), |spans| {
                    let next = spans.partition_point(|&(_, end)| end <= job.cursor);
                    spans
                        .get(next)
                        .copied()
                        .map_or((job.total, job.total), |(a, b)| (a.max(job.cursor), b))
                });
            let end = (first + job.triangles * 3).min(last);
            let batch = Batch {
                start: first,
                end,
                complete: false,
                clear: job.cursor == 0,
                layers: Some(&job.layers),
            };
            if !self.render_batch(target, model, cam, size, adjust, FrameQuality::Final, batch) {
                return LiveDraw::Failed;
            }
            job.cursor = end;
        } else {
            let (w, h) = target.size;
            let arctic = cam.display == ModelDisplay::Arctic;
            let camera = surface::FrameCamera::new(model, cam, size.2, arctic);
            let gl = &self.gl;
            let rows = if job.post == Post::Occlusion {
                targets::ao_size(w, h).1
            } else {
                h
            };
            let end = (job.row + job.band).min(rows);
            unsafe {
                gl.disable(glow::SCISSOR_TEST);
                if self.srgb_framebuffer {
                    gl.disable(glow::FRAMEBUFFER_SRGB);
                }
                gl.disable(glow::BLEND);
                if job.post == Post::Publish {
                    let staged = target.standby.as_mut().unwrap();
                    std::mem::swap(&mut staged.fbo, &mut target.out_fbo);
                    std::mem::swap(&mut staged.texture, &mut target.display);
                    std::mem::swap(&mut staged.egui_id, &mut target.egui_id);
                    return LiveDraw::Complete;
                }
                gl.enable(glow::SCISSOR_TEST);
                gl.scissor(0, job.row, w, end - job.row);
                match job.post {
                    Post::Resolve => {
                        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(target.msaa_fbo));
                        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(target.resolve_fbo));
                        gl.blit_framebuffer(
                            0,
                            job.row,
                            w,
                            end,
                            0,
                            job.row,
                            w,
                            end,
                            glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT,
                            glow::NEAREST,
                        );
                    }
                    Post::Occlusion | Post::Compose => {
                        self.ambient_occlusion(target, &camera, size.2, (w, h), Some(job.post))
                    }
                    Post::Filter => {
                        if !self.ensure_standby_display(target) {
                            gl.disable(glow::SCISSOR_TEST);
                            return LiveDraw::Failed;
                        }
                        let source = if arctic {
                            target.compose_tex
                        } else {
                            target.resolve_tex
                        };
                        self.filter_frame(
                            source,
                            target.standby.as_ref().unwrap().fbo,
                            (w, h),
                            adjust,
                        );
                    }
                    Post::Publish => unreachable!(),
                }
                gl.disable(glow::SCISSOR_TEST);
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.bind_vertex_array(None);
                gl.use_program(None);
            }
            job.row = end;
            if job.row == rows {
                job.row = 0;
                job.post = match job.post {
                    Post::Resolve if arctic => Post::Occlusion,
                    Post::Occlusion => Post::Compose,
                    Post::Resolve | Post::Compose => Post::Filter,
                    Post::Filter | Post::Publish => Post::Publish,
                };
            }
        }
        // Complete this bounded submission before yielding. A driver backlog
        // of old batches would otherwise stall the next camera movement.
        unsafe { self.gl.finish() };
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let growth = (6.0 / ms.max(0.1)).clamp(0.25, 2.0);
        if geometry {
            let limit = if job.visible.is_some() { 64 } else { 8192 };
            job.triangles = ((job.triangles as f64 * growth) as i64).clamp(1, limit);
        } else {
            job.band = ((job.band as f64 * growth) as i32).clamp(1, 64);
        }
        target.refinement = Some(job);
        LiveDraw::Pending
    }
}
