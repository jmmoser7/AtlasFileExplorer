//! Capture a frame into a raster and read its pixels.

use super::*;

/// Software rasterizer for egui's own tessellated output, so the board and
/// its HUDs can be inspected as real frames without a GPU or a screen grab.
pub(crate) struct FrameRaster {
    pub(crate) w: usize,
    pub(crate) h: usize,
    pub(crate) px: Vec<[f32; 4]>,
    pub(crate) textures:
        std::collections::HashMap<egui::TextureId, (usize, usize, Vec<egui::Color32>)>,
}

impl FrameRaster {
    pub(crate) fn new(w: usize, h: usize) -> Self {
        FrameRaster {
            w,
            h,
            px: vec![[0.0, 0.0, 0.0, 1.0]; w * h],
            textures: Default::default(),
        }
    }

    pub(crate) fn apply(&mut self, delta: &egui::TexturesDelta) {
        for (id, d) in &delta.set {
            let (w, h, pixels) = match &d.image {
                egui::ImageData::Color(c) => (c.size[0], c.size[1], c.pixels.clone()),
                egui::ImageData::Font(f) => (f.size[0], f.size[1], f.srgba_pixels(None).collect()),
            };
            match d.pos {
                None => {
                    self.textures.insert(*id, (w, h, pixels));
                }
                Some([x0, y0]) => {
                    if let Some((tw, _, dst)) = self.textures.get_mut(id) {
                        for y in 0..h {
                            for x in 0..w {
                                dst[(y0 + y) * *tw + x0 + x] = pixels[y * w + x];
                            }
                        }
                    }
                }
            }
        }
    }

    /// The font atlas as it stands now. Its first upload happened on harness
    /// frames this raster never saw, so text would otherwise sample white.
    pub(crate) fn sync_fonts(&mut self, ctx: &egui::Context) {
        let image = ctx.fonts(|f| f.image());
        let [w, h] = image.size;
        self.textures.insert(
            egui::TextureId::default(),
            (w, h, image.srgba_pixels(None).collect()),
        );
    }

    pub(crate) fn sample(&self, id: egui::TextureId, u: f32, v: f32) -> [f32; 4] {
        let Some((w, h, px)) = self.textures.get(&id) else {
            return [1.0; 4];
        };
        let (w, h) = (*w, *h);
        let fx = (u * w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
        let fy = (v * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let get = |x: usize, y: usize| {
            let c = px[y * w + x];
            [
                c.r() as f32 / 255.0,
                c.g() as f32 / 255.0,
                c.b() as f32 / 255.0,
                c.a() as f32 / 255.0,
            ]
        };
        let (a, b, c, d) = (get(x0, y0), get(x1, y0), get(x0, y1), get(x1, y1));
        let mut out = [0.0; 4];
        for k in 0..4 {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bot = c[k] + (d[k] - c[k]) * tx;
            out[k] = top + (bot - top) * ty;
        }
        out
    }

    pub(crate) fn draw(&mut self, prims: &[egui::ClippedPrimitive]) {
        for prim in prims {
            let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive else {
                continue;
            };
            let clip = prim.clip_rect;
            for tri in mesh.indices.as_chunks::<3>().0 {
                let v = [
                    &mesh.vertices[tri[0] as usize],
                    &mesh.vertices[tri[1] as usize],
                    &mesh.vertices[tri[2] as usize],
                ];
                let (p0, p1, p2) = (v[0].pos, v[1].pos, v[2].pos);
                let area = (p1.x - p0.x) * (p2.y - p0.y) - (p1.y - p0.y) * (p2.x - p0.x);
                if area.abs() < 1e-9 {
                    continue;
                }
                let minx = p0.x.min(p1.x).min(p2.x).max(clip.min.x).max(0.0).floor() as i64;
                let maxx =
                    p0.x.max(p1.x)
                        .max(p2.x)
                        .min(clip.max.x)
                        .min(self.w as f32)
                        .ceil() as i64;
                let miny = p0.y.min(p1.y).min(p2.y).max(clip.min.y).max(0.0).floor() as i64;
                let maxy =
                    p0.y.max(p1.y)
                        .max(p2.y)
                        .min(clip.max.y)
                        .min(self.h as f32)
                        .ceil() as i64;
                for y in miny..maxy {
                    for x in minx..maxx {
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let w0 = ((p1.x - px) * (p2.y - py) - (p1.y - py) * (p2.x - px)) / area;
                        let w1 = ((p2.x - px) * (p0.y - py) - (p2.y - py) * (p0.x - px)) / area;
                        let w2 = 1.0 - w0 - w1;
                        if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                            continue;
                        }
                        let lerp = |f: &dyn Fn(&egui::epaint::Vertex) -> f32| {
                            f(v[0]) * w0 + f(v[1]) * w1 + f(v[2]) * w2
                        };
                        let col = [
                            lerp(&|q| q.color.r() as f32 / 255.0),
                            lerp(&|q| q.color.g() as f32 / 255.0),
                            lerp(&|q| q.color.b() as f32 / 255.0),
                            lerp(&|q| q.color.a() as f32 / 255.0),
                        ];
                        let tex =
                            self.sample(mesh.texture_id, lerp(&|q| q.uv.x), lerp(&|q| q.uv.y));
                        let src = [
                            col[0] * tex[0],
                            col[1] * tex[1],
                            col[2] * tex[2],
                            col[3] * tex[3],
                        ];
                        let dst = &mut self.px[y as usize * self.w + x as usize];
                        for k in 0..4 {
                            dst[k] = src[k] + dst[k] * (1.0 - src[3]);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn save(&self, path: &std::path::Path) {
        let mut img = image::RgbImage::new(self.w as u32, self.h as u32);
        for (i, p) in self.px.iter().enumerate() {
            let q = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
            img.put_pixel(
                (i % self.w) as u32,
                (i / self.w) as u32,
                image::Rgb([q(p[0]), q(p[1]), q(p[2])]),
            );
        }
        img.save(path).unwrap();
    }

    /// Save only `area` (screen points, clamped to the raster), each pixel
    /// repeated `magnify` times so a close-up stays legible.
    pub(crate) fn save_crop(&self, path: &std::path::Path, area: ERect, magnify: u32) {
        let x0 = (area.min.x.max(0.0) as usize).min(self.w - 1);
        let y0 = (area.min.y.max(0.0) as usize).min(self.h - 1);
        let x1 = (area.max.x.ceil().max(0.0) as usize).clamp(x0 + 1, self.w);
        let y1 = (area.max.y.ceil().max(0.0) as usize).clamp(y0 + 1, self.h);
        let mut img = image::RgbImage::new((x1 - x0) as u32, (y1 - y0) as u32);
        let q = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        for y in y0..y1 {
            for x in x0..x1 {
                let p = self.px[y * self.w + x];
                img.put_pixel(
                    (x - x0) as u32,
                    (y - y0) as u32,
                    image::Rgb([q(p[0]), q(p[1]), q(p[2])]),
                );
            }
        }
        let magnify = magnify.max(1);
        if magnify > 1 {
            img = image::imageops::resize(
                &img,
                img.width() * magnify,
                img.height() * magnify,
                image::imageops::FilterType::Nearest,
            );
        }
        img.save(path).unwrap();
    }
}

/// Run one frame with `prepare`, feeding its textures into `raster`.
pub(crate) fn capture_frame(
    h: &mut Harness,
    raster: &mut FrameRaster,
    prepare: impl FnOnce(&mut egui::RawInput),
) -> egui::FullOutput {
    let mut input = egui::RawInput {
        screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0))),
        ..Default::default()
    };
    prepare(&mut input);
    let ctx = h.ctx.clone();
    let app = &mut h.app;
    let out = ctx.run(input, |c| app.update_app(c));
    raster.apply(&out.textures_delta);
    out
}

pub(crate) fn snapshot(
    h: &mut Harness,
    raster: &mut FrameRaster,
    out: egui::FullOutput,
    name: &str,
) {
    rasterize(h, raster, out);
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/brush-validate/frames");
    std::fs::create_dir_all(&dir).unwrap();
    raster.save(&dir.join(format!("{name}.png")));
}

/// Save a look-and-feel review frame to `target/review/<item>/<state>.png`,
/// where `item` is a request id from `docs/requests/` (e.g. `CT1`).
pub(crate) fn review_shot(
    h: &mut Harness,
    raster: &mut FrameRaster,
    out: egui::FullOutput,
    item: &str,
    state: &str,
) {
    raster.sync_fonts(&h.ctx);
    rasterize(h, raster, out);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/review")
        .join(item);
    std::fs::create_dir_all(&dir).unwrap();
    raster.save(&dir.join(format!("{state}.png")));
}

/// [`review_shot`] cropped to `area` and magnified, for close-ups of one object.
pub(crate) fn review_shot_crop(
    h: &mut Harness,
    raster: &mut FrameRaster,
    out: egui::FullOutput,
    item: &str,
    state: &str,
    area: ERect,
    magnify: u32,
) {
    raster.sync_fonts(&h.ctx);
    rasterize(h, raster, out);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/review")
        .join(item);
    std::fs::create_dir_all(&dir).unwrap();
    raster.save_crop(&dir.join(format!("{state}.png")), area, magnify);
}

/// Draw `out` into `raster` over black.
pub(crate) fn rasterize(h: &mut Harness, raster: &mut FrameRaster, out: egui::FullOutput) {
    // Tiled strokes render on worker threads; let them land first.
    let mut out = out;
    for _ in 0..200 {
        if !h.app.brush_tiles_enabled || h.app.brush_tiles.last.pending_jobs == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        let mods = h.ctx.input(|i| i.modifiers);
        out = capture_frame(h, raster, |i| i.modifiers = mods);
    }
    let prims = h.ctx.tessellate(out.shapes, out.pixels_per_point);
    for p in raster.px.iter_mut() {
        *p = [0.0, 0.0, 0.0, 1.0];
    }
    raster.draw(&prims);
}

// ---------- vertex picks on closed forms (P1.shape.vertex-style) ----------
