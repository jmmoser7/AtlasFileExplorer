//! Retained software frames from real collaborator command admission and paint.
use super::*;
use crate::app::{
    preview::{PreviewEntry, PX_EXACT},
    tests::{capture_frame, picture_from, picture_item, rasterize, FrameRaster},
};
use slate_doc::NodeId;
use std::{collections::HashMap, io::Write, time::Duration};

pub(super) struct Fixture {
    pub h: Harness,
    pub who: String,
    pub images: [NodeId; 2],
    pub raster: FrameRaster,
    case: String,
    camera: (egui::Vec2, f32),
    rows: Vec<serde_json::Value>,
    image_textures: HashMap<NodeId, egui::TextureId>,
}

impl Fixture {
    pub fn new(source_x: f32) -> Self {
        Self::with_multicolour(source_x, false)
    }

    pub fn multicolour(source_x: f32) -> Self {
        Self::with_multicolour(source_x, true)
    }

    fn with_multicolour(source_x: f32, multicolour: bool) -> Self {
        let mut h = host();
        for provider in ["codex", "cursor"] {
            h.app.agents.set_catalog_for_test(provider, Vec::new());
        }
        h.app.ensure_work_tab();
        h.app.dark_mode = false;
        h.app.tab_mut().cam.z = 1.0;
        h.app.tab_mut().cam.offset = egui::vec2(0.0, 100.0);
        let images = [
            picture_from(&mut h, "moving.png", source_x),
            picture_from(&mut h, "target.png", 280.0),
        ];
        if multicolour {
            // Replace only fixture-owned pixels before any texture is loaded;
            // task execution never writes these sources or changes their links.
            for image in images {
                let item = picture_item(&h, image);
                let path = &h.app.doc().item(item).unwrap().path;
                image::RgbaImage::from_fn(64, 64, |x, y| {
                    image::Rgba([(x * 4) as u8, (y * 4) as u8, ((x + y) * 2) as u8, 255])
                })
                .save(path)
                .unwrap();
            }
        }
        h.app.board_sel.clear();
        h.frame();
        let who = add(&mut h, "Dot");
        let mut agent = h.app.presence.agents.remove(0);
        agent.request = atlas_agent::request_id();
        agent.grant.as_mut().unwrap().mutation_autonomy = true;
        h.app
            .presence
            .roster
            .session_mut(&agent.motion)
            .unwrap()
            .begin_request(id(&agent.request).unwrap())
            .unwrap();
        h.app.presence.agents.push(agent);
        // Upload the actual local PNG pixels through the existing ready-cache seam.
        // No worker wait or placeholder may stand in for the image being moved.
        let mut image_textures = HashMap::new();
        for image in images {
            let item = picture_item(&h, image);
            let (key, path, _, _) = h.app.resolved_item_preview(item).unwrap();
            let decoded = image::open(path).unwrap().to_rgba8();
            let pixels = egui::ColorImage::from_rgba_unmultiplied(
                [decoded.width() as usize, decoded.height() as usize],
                decoded.as_raw(),
            );
            let tex = h.ctx.load_texture(
                format!("motion-real-image-{}", image.0),
                pixels.clone(),
                egui::TextureOptions::LINEAR,
            );
            image_textures.insert(image, tex.id());
            h.app.preview_cache.insert(
                key,
                PreviewEntry {
                    tex,
                    bytes: pixels.pixels.len() * 4,
                    pixels: Arc::new(pixels),
                    px: PX_EXACT,
                    last_used: 0,
                },
            );
            assert!(h.app.item_texture(item, 160.0).is_some());
        }
        let camera = (h.app.tab().cam.offset, h.app.tab().cam.z);
        Self {
            h,
            who,
            images,
            raster: FrameRaster::new(1440, 900),
            case: format!("presence-motion-{}", atlas_agent::request_id()),
            camera,
            rows: Vec::new(),
            image_textures,
        }
    }

    pub fn invoke(
        &mut self,
        sequence: u64,
        command: &str,
        targets: &[NodeId],
        arguments: serde_json::Value,
    ) {
        let mut agent = self.h.app.presence.agents.remove(0);
        let call = BoardCall {
            id: atlas_agent::request_id(),
            session: agent.binding.clone(),
            request_id: agent.request.clone(),
            sequence,
            operation: BoardOperation::Invoke {
                command: command.into(),
                targets: targets.iter().map(|id| id.0.to_string()).collect(),
                arguments,
            },
        };
        assert!(self.h.app.presence_call(&mut agent, &call).is_none());
        assert!(agent.work.is_some(), "the real command must be admitted");
        self.h.app.presence.agents.push(agent);
    }

    pub fn step(&mut self) -> egui::FullOutput {
        self.h.app.presence.last_tick = Some(Instant::now() - Duration::from_millis(50));
        let out = capture_frame(&mut self.h, &mut self.raster, |_| {});
        assert_eq!(
            (self.h.app.tab().cam.offset, self.h.app.tab().cam.z),
            self.camera
        );
        out
    }

    pub fn sample(&self) -> MotionSample {
        let agent = &self.h.app.presence.agents[0];
        self.h
            .app
            .presence
            .roster
            .session(&agent.motion)
            .unwrap()
            .sample()
    }

    pub fn pending(&self) -> bool {
        self.h.app.presence.agents[0].work.is_some()
    }

    pub fn state(&self) -> serde_json::Value {
        let sample = self.sample();
        let previews = self.h.app.presence.agents[0]
            .work
            .as_ref()
            .map(|work| &work.previews);
        json!({"frame":self.h.app.frame_no,"phase":format!("{:?}",sample.phase),
            "avatar":[sample.position.x,sample.position.y],"progress":sample.progress,
            "previews":previews,"scene":self.h.app.doc().scene,
            "undoDepth":self.h.app.tab().journal.undo_depth(),
            "camera":[self.h.app.tab().cam.offset.x,self.h.app.tab().cam.offset.y,self.h.app.tab().cam.z]})
    }

    pub fn save(&mut self, phase: &str, out: egui::FullOutput) {
        let before = self.state();
        // The general review helper drains worker frames. Retain this exact
        // output instead, including the currently visible raster fallbacks.
        self.draw_exact(out);
        assert_eq!(
            self.state(),
            before,
            "phase labels must match the exact captured frame"
        );
        let path = self.directory().join(format!("{phase}.png"));
        std::fs::create_dir_all(self.directory()).unwrap();
        self.raster.save(&path);
        let row = json!({"phase":phase,"file":path.file_name().unwrap().to_string_lossy(),
            "bytes":std::fs::metadata(&path).unwrap().len(),"state":before});
        self.write_json(&format!("{phase}.json"), &row);
        self.rows.push(row);
    }

    pub fn draw(&mut self, out: &egui::FullOutput) {
        assert_eq!(self.h.app.brush_tiles.last.pending_jobs, 0);
        let before = self.state();
        rasterize(&mut self.h, &mut self.raster, out.clone());
        assert_eq!(self.state(), before);
    }

    pub fn draw_exact(&mut self, out: egui::FullOutput) {
        let before = self.state();
        self.raster.sync_fonts(&self.h.ctx);
        let prims = self.h.ctx.tessellate(out.shapes, out.pixels_per_point);
        self.raster.px.fill([0.0, 0.0, 0.0, 1.0]);
        self.raster.draw(&prims);
        assert_eq!(self.state(), before, "drawing must not advance any frames");
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.h.app.tab_mut().cam.z = zoom;
        self.camera = (self.h.app.tab().cam.offset, zoom);
    }

    pub fn blue(&self, node: &slate_doc::scene::Node) -> Option<bool> {
        let point = self.h.app.board_xf().w2s(egui::pos2(
            node.rect.x + node.rect.w * 0.2,
            node.rect.y + node.rect.h * 0.75,
        ));
        if !self.h.app.canvas_rect.contains(point) {
            return None;
        }
        let pixel = self.raster.px[point.y as usize * self.raster.w + point.x as usize];
        Some(
            pixel[..3]
                .iter()
                .zip([90.0, 140.0, 200.0])
                .all(|(actual, expected)| (actual * 255.0 - expected).abs() < 3.0),
        )
    }

    pub fn pixel(&self, world: egui::Pos2) -> [f32; 4] {
        let point = self.h.app.board_xf().w2s(world);
        assert!(self.h.app.canvas_rect.contains(point));
        self.raster.px[point.y as usize * self.raster.w + point.x as usize]
    }

    pub fn image_uvs(&self, out: &egui::FullOutput, image: NodeId) -> Vec<[f32; 2]> {
        fn walk(shape: &egui::Shape, texture: egui::TextureId, uvs: &mut Vec<[f32; 2]>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, texture, uvs)),
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                    uvs.extend(
                        mesh.vertices
                            .iter()
                            .map(|vertex| [vertex.uv.x, vertex.uv.y]),
                    );
                }
                _ => {}
            }
        }
        let mut uvs = Vec::new();
        for clipped in &out.shapes {
            walk(&clipped.shape, self.image_textures[&image], &mut uvs);
        }
        assert!(!uvs.is_empty(), "the real linked-image texture must paint");
        uvs
    }

    fn directory(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/review")
            .join(&self.case)
    }

    fn write_json(&self, name: &str, value: &serde_json::Value) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.directory().join(name))
            .unwrap();
        serde_json::to_writer_pretty(&mut file, value).unwrap();
        file.write_all(b"\n").unwrap();
    }

    pub fn record(&self, name: &str, value: serde_json::Value) {
        self.write_json(&format!("{name}.json"), &value);
    }

    pub fn finish(&self) {
        self.write_json("metadata.json", &json!({"status":"assertions-passed",
            "evidence":"headless software renderer only; native/GPU and human acceptance remain separate",
            "tickSeamMs":50,"phases":self.rows}));
        println!(
            "Retained witnessed-motion frames: {}",
            self.directory().display()
        );
    }
}
