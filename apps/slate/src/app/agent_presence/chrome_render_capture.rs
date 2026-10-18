//! The original Fixture capture implementation, extracted before extension.
use super::*;
use crate::app::tests::{painted_strings, review_shot};
use std::io::Write;

pub(in super::super) struct RetainedFrame {
    out: egui::FullOutput,
    geometry: serde_json::Value,
    state: serde_json::Value,
    wall_ms: f64,
    textures: HashMap<egui::TextureId, (usize, usize, Vec<egui::Color32>)>,
    font_size: [usize; 2],
    font_pixels: Vec<f32>,
}
impl Fixture {
    /// Inspect only the actual app output; this does not construct a pose.
    pub fn mesh_vertices(out: &egui::FullOutput, area: Rect, dark: bool) -> Vec<[u32; 3]> {
        Self::mesh_vertices_at_least_alpha(out, area, dark, 129)
    }
    /// Zero includes transparent fringe vertices, keeping opacity-only motion's
    /// geometry fixed so its actual alpha delta can be measured without a cutoff.
    pub fn mesh_vertices_at_least_alpha(
        out: &egui::FullOutput,
        area: Rect,
        dark: bool,
        min_alpha: u8,
    ) -> Vec<[u32; 3]> {
        fn walk(
            shape: &egui::Shape,
            area: Rect,
            dark: bool,
            min_alpha: u8,
            into: &mut Vec<[u32; 3]>,
        ) {
            match shape {
                egui::Shape::Vec(children) => children
                    .iter()
                    .for_each(|s| walk(s, area, dark, min_alpha, into)),
                egui::Shape::Mesh(mesh) => into.extend(
                    mesh.vertices
                        .iter()
                        .filter(|v| {
                            area.contains(v.pos)
                                && v.color.a() >= min_alpha
                                && (!dark
                                    || (v.color.r() < 25 && v.color.g() < 25 && v.color.b() < 25))
                        })
                        .map(|v| {
                            [
                                v.pos.x.to_bits(),
                                v.pos.y.to_bits(),
                                u32::from_le_bytes(v.color.to_array()),
                            ]
                        }),
                ),
                _ => {}
            }
        }
        let mut vertices = Vec::new();
        for clipped in &out.shapes {
            walk(
                &clipped.shape,
                area.intersect(clipped.clip_rect),
                dark,
                min_alpha,
                &mut vertices,
            );
        }
        vertices.sort_unstable();
        vertices
    }
    pub(super) fn conversations(&self) -> serde_json::Value {
        json!({"self":self.h.app.presence_conversation("self"),
            "agent":self.h.app.presence_conversation(&self.who),
            "savedParticipants":self.h.app.settings.collaborators.iter().map(|p|
                (&p.id,self.h.app.presence_conversation(&p.id))).collect::<Vec<_>>()})
    }

    fn state(&self) -> serde_json::Value {
        json!({"frame":self.h.app.frame_no,"viewport":rect_json(self.viewport()),"scene":self.h.app.doc().scene,
            "undoDepth":self.h.app.tab().journal.undo_depth(),"conversations":self.conversations(),
            "activeChat":self.h.app.presence.chrome.active_chat(),
            "canvasChat":self.h.app.presence.chrome.canvas_chat(),
            "popupOpen":self.h.app.presence.chrome.is_open(),
            "human":self.h.app.settings.presence_identity,
            "agent":self.h.app.presence.agents.first().map(|a|&a.profile),
            "participants":self.h.app.presence.agents.iter().map(|a|json!({
                "profile":a.profile,"parent":a.parent,"paused":a.paused,"activity":a.activity,
                "status":a.status,"binding":a.binding,"grant":a.grant,"request":a.request,
                "departing":a.departure.is_some()})).collect::<Vec<_>>(),
            "savedProfiles":self.h.app.settings.collaborators,
            "peopleSetupOpen":self.h.app.presence.human_chat.setup.open,
            "agentState":self.h.app.presence.agents.first().map(|a|json!({
                "paused":a.paused,"activity":a.activity,"status":a.status,
                "position":self.h.app.presence.roster.session(&a.motion)
                    .map(|s|{let p=s.sample().position;[p.x,p.y]})})),
            "camera":[self.h.app.tab().cam.offset.x,self.h.app.tab().cam.offset.y,self.h.app.tab().cam.z],
            "eguiTime":self.h.ctx.input(|i|i.time),"declarations":self.declarations,
            "helperResults":self.h.app.presence.helper_results.iter().map(|r|json!({
                "id":r.id,"parent":r.parent_id,"request":r.request_id,"outcome":r.outcome,
                "status":r.status,"result":r.result,"expired":r.expired})).collect::<Vec<_>>()})
    }

    pub fn save(&mut self, phase: &str, out: egui::FullOutput, geometry: serde_json::Value) {
        let frame = self.retain(out, geometry);
        self.save_retained(phase, frame);
    }

    pub fn retain(&mut self, out: egui::FullOutput, geometry: serde_json::Value) -> RetainedFrame {
        assert!(
            self.retained < 3,
            "at most three real-time outputs may await serialization"
        );
        self.retained += 1;
        let font = self.h.ctx.fonts(|f| f.image());
        RetainedFrame {
            out,
            geometry,
            state: self.state(),
            wall_ms: self.started.elapsed().as_secs_f64() * 1000.0,
            textures: self.raster.textures.clone(),
            font_size: font.size,
            font_pixels: font.pixels,
        }
    }

    pub fn save_retained(&mut self, phase: &str, frame: RetainedFrame) {
        let RetainedFrame {
            out,
            geometry,
            state,
            wall_ms,
            textures,
            font_size,
            font_pixels,
        } = frame;
        let font = self.h.ctx.fonts(|f| f.image());
        assert_eq!(
            font.size, font_size,
            "retained frame must keep its exact font atlas"
        );
        assert_eq!(
            font.pixels, font_pixels,
            "later glyph uploads cannot stand in for captured textures"
        );
        let pixels_per_point = out.pixels_per_point;
        let labels = painted_strings(&out);
        assert!(
            !labels.iter().any(|s| s.contains("An open board")),
            "Welcome overlaps a live collaborator"
        );
        assert_eq!(
            self.h.app.brush_tiles.last.pending_jobs, 0,
            "capture cannot advance pending work"
        );
        let before = self.state();
        let path = self.directory().join(format!("{phase}.png"));
        assert!(
            !path.exists(),
            "phase evidence must never overwrite a previous frame"
        );
        let current_textures = std::mem::replace(&mut self.raster.textures, textures);
        review_shot(&mut self.h, &mut self.raster, out, &self.case, phase);
        self.raster.textures = current_textures;
        assert_eq!(
            self.state(),
            before,
            "saving advanced or changed the labeled UI state"
        );
        assert_eq!(
            image::image_dimensions(&path).unwrap(),
            (self.raster.w as u32, self.raster.h as u32)
        );
        let row = json!({"phase":phase,"file":path.file_name().unwrap().to_string_lossy(),
            "bytes":std::fs::metadata(&path).unwrap().len(),"geometry":geometry,"labels":labels,"state":state,
            "observedWallMs":wall_ms,"fontAtlas":font_size,"pixelsPerPoint":pixels_per_point,
            "outputDimensions":[self.raster.w,self.raster.h]});
        self.write_json(&format!("{phase}.json"), &row);
        self.rows.push(row);
        self.retained -= 1;
    }

    pub(super) fn directory(&self) -> PathBuf {
        review_directory().join(&self.case)
    }

    pub(super) fn write_json(&self, name: &str, value: &serde_json::Value) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.directory().join(name))
            .unwrap();
        serde_json::to_writer_pretty(&mut file, value).unwrap();
        file.write_all(b"\n").unwrap();
    }

    pub fn finish(&self) {
        self.finish_kind(false);
    }
    pub fn finish_management(&self) {
        self.finish_kind(true);
    }
    fn finish_kind(&self, management: bool) {
        self.write_json("metadata.json", &json!({"status":"assertions-passed",
            "evidence":"actual full-app headless software paint; native GPU and human acceptance remain separate",
            "management":management,
            "reducedMotion":true,"catalogs":"injected and launch-pinned before first frame",
            "history":if management {"synthetic test-owned agent history; no provider send"} else {"self sent locally; agent history synthetic; both saved and reloaded in test-owned temporary store"},
            "hatWheel":if management {"not activated in management states"} else {"real shared album input changes fixture hat in memory; cfg(test) never saves human preferences"},
            "omitted":["OS profile-photo picker", "actual provider message or harness switch"],"phases":self.rows}));
        println!(
            "Retained full-app presence chrome: {}",
            self.directory().display()
        );
    }
}
