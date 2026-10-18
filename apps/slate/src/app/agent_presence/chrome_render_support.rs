//! Full-app paint/input evidence; fixture data and I/O stay in test-owned stores.
use super::super::*;
use crate::app::tests::{
    capture_frame, painted_strings, painted_textures, primary_button, review_shot, FrameRaster,
};
use eframe::egui::{self, Id, Pos2, Rect};
use serde_json::json;
use std::{io::Write, time::Duration};

pub(super) struct Fixture {
    pub h: Harness,
    pub who: String,
    raster: FrameRaster,
    case: String,
    store: PathBuf,
    time: f64,
    rows: Vec<serde_json::Value>,
    camera: (egui::Vec2, f32),
}

impl Fixture {
    pub fn new() -> Self {
        let mut h = host();
        // All recognized catalog routes are injected before the first app frame.
        h.app.set_agent_programs_for_test(&["codex", "cursor"]);
        for key in [
            "codex",
            "cursor",
            "ollama",
            "comfy",
            "openai-image",
            "openai-text",
        ] {
            h.app.agents.set_catalog_for_test(key, Vec::new());
        }
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.dark_mode = false;
        h.app.settings.presence_reduced_motion = true;
        h.app.settings.recovery.enabled = false;
        h.app.tab_mut().cam.z = 1.0;
        h.app.tab_mut().cam.offset = egui::Vec2::ZERO;
        let store = h.base.join("chrome-conversations");
        h.app.presence.conversations =
            conversation_store::ConversationStore::at_directory(store.clone());
        wait_for_store(&mut h);
        let case = format!("presence-chrome-{}", atlas_agent::request_id());
        let directory = review_directory().join(&case);
        std::fs::create_dir_all(review_directory()).unwrap();
        std::fs::create_dir(&directory).expect("capture directory must be exclusive");
        let camera = (h.app.tab().cam.offset, h.app.tab().cam.z);
        let mut f = Self {
            h,
            who: atlas_agent::request_id(),
            raster: FrameRaster::new(1440, 900),
            case,
            store,
            time: 0.0,
            rows: Vec::new(),
            camera,
        };
        f.step(|_| {});
        f.h.app
            .join_presence(
                StoredAgent {
                    id: f.who.clone(),
                    name: "Dot".into(),
                    hat: "cap".into(),
                    provider: "local".into(),
                    retired: false,
                },
                None,
            )
            .unwrap();
        let thread = f.h.app.presence.conversations.edit(&f.who).unwrap();
        // Deliberately synthetic visible history, not evidence of a provider reply.
        for (role, text, author) in [
            ("user", "How is the interface feeling?", "You"),
            ("assistant", "Ready to help organize this board.", "Dot"),
        ] {
            thread.push(role, text, author, None, "chrome-fixture", "complete");
        }
        f.h.app.presence.conversations.save(&f.who);
        f
    }

    pub fn step(&mut self, prepare: impl FnOnce(&mut egui::RawInput)) -> egui::FullOutput {
        self.time += 1.0 / 60.0;
        let time = self.time;
        let out = capture_frame(&mut self.h, &mut self.raster, |input| {
            input.time = Some(time);
            prepare(input);
        });
        self.assert_local();
        out
    }

    pub fn settle(&mut self) -> egui::FullOutput {
        for _ in 0..8 {
            self.step(|_| {});
        }
        self.step(|_| {})
    }

    pub fn target(&self, id: Id) -> Rect {
        let response = self
            .h
            .ctx
            .read_response(id)
            .unwrap_or_else(|| panic!("missing target {id:?}"));
        let transform = self
            .h
            .ctx
            .layer_transform_to_global(response.layer_id)
            .unwrap_or_default();
        let rect = transform * response.rect;
        let hit = transform * response.interact_rect;
        assert!(
            rect.is_positive() && hit.contains(rect.center()),
            "clipped target {id:?}: {rect:?} hit {hit:?}"
        );
        assert!(
            screen().contains_rect(rect),
            "offscreen target {id:?}: {rect:?}"
        );
        rect
    }

    pub fn click(&mut self, id: Id) -> egui::FullOutput {
        let pos = self.target(id).center();
        self.step(primary_button(pos, true, false));
        self.step(primary_button(pos, false, false));
        self.settle()
    }

    pub fn type_into(&mut self, id: Id, text: &str) -> egui::FullOutput {
        self.click(id);
        assert_eq!(self.h.ctx.memory(|m| m.focused()), Some(id));
        self.step(|input| input.events.push(egui::Event::Text(text.into())));
        self.settle()
    }

    pub fn wheel(&mut self, id: Id) -> egui::FullOutput {
        let pos = self.target(id).center();
        let before_hat = self.hat();
        self.step(|input| {
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -3.0),
                modifiers: egui::Modifiers::NONE,
            });
        });
        let mut previous = Vec::new();
        let mut stable = 0;
        for _ in 0..180 {
            let out = self.step(|_| {});
            let geometry = album_geometry(&out);
            stable = if geometry == previous { stable + 1 } else { 0 };
            previous = geometry;
            // The picker keeps its neighbors visible at rest. Observe its actual
            // painted mesh settling, rather than copying the private flow state.
            if self.hat() != before_hat
                && stable >= 3
                && !previous.is_empty()
                && self
                    .h
                    .ctx
                    .input(|i| i.time_since_last_scroll() > 2.0 / 60.0)
            {
                return out;
            }
        }
        panic!("album wheel did not settle");
    }

    pub fn hats_ready(&mut self) -> egui::FullOutput {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let out = self.step(|_| {});
            let mut textures = painted_textures(&out);
            textures.retain(|id| *id != egui::TextureId::default());
            let count = textures
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
                .len();
            if count >= 3 {
                return self.settle();
            }
            assert!(
                Instant::now() < deadline,
                "real embedded hat artwork never painted"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn chat(&self, out: &egui::FullOutput, who: &str) -> Rect {
        assert_eq!(self.h.app.presence.chrome.active_chat(), Some(who));
        assert_eq!(self.h.app.presence.chrome.canvas_chat(), None);
        let header = self.target(Id::new(("conversation-profile", who)));
        let composer = self.target(Id::new(("presence-composer", who)));
        let send = self.target(Id::new(("presence-send", who)));
        let panel = self.enclosure(out, header.union(composer).union(send), 0.0);
        assert!(panel.top() < header.top() && panel.bottom() > composer.bottom());
        panel
    }

    pub fn address_at_zoom(&mut self, zoom: f32, at: egui::Vec2) -> egui::FullOutput {
        let agent = &self.h.app.presence.agents[0];
        let position = self
            .h
            .app
            .presence
            .roster
            .session(&agent.motion)
            .unwrap()
            .sample()
            .position;
        self.h.app.tab_mut().cam.z = zoom;
        let target = self.h.app.canvas_rect.min + at;
        let before = self.h.app.board_xf().s2w(target);
        self.h.app.tab_mut().cam.offset +=
            egui::pos2(position.x as f32, position.y as f32) - before;
        self.camera = (self.h.app.tab().cam.offset, self.h.app.tab().cam.z);
        self.h.app.presence_address(&self.h.ctx, &self.who).unwrap();
        self.settle()
    }

    pub fn complete_target(&self, id: Id) -> Rect {
        let rect = self.target(id);
        let response = self.h.ctx.read_response(id).unwrap();
        let transform = self
            .h
            .ctx
            .layer_transform_to_global(response.layer_id)
            .unwrap_or_default();
        assert!(
            (transform * response.interact_rect).contains_rect(rect.shrink(0.01)),
            "whole target must remain interactive: {id:?} {rect:?}"
        );
        rect
    }

    pub fn enclosure(&self, out: &egui::FullOutput, inside: Rect, extra_height: f32) -> Rect {
        fn walk(shape: &egui::Shape, into: &mut Vec<Rect>, fill: egui::Color32) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, into, fill)),
                egui::Shape::Rect(rect) if rect.fill == fill => into.push(rect.rect),
                _ => {}
            }
        }
        let mut rects = Vec::new();
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut rects, self.h.app.palette().card);
        }
        let panel = rects
            .into_iter()
            .filter(|r| {
                r.expand(0.5).contains_rect(inside) && r.height() >= inside.height() + extra_height
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()))
            .expect("one painted card encloses the actual controls");
        assert!(
            screen().contains_rect(panel),
            "enclosing surface leaves viewport: {panel:?}"
        );
        panel
    }

    pub fn reload_histories(&mut self) {
        let expected = self.conversations();
        let receipts: Vec<_> = ["self", self.who.as_str()]
            .into_iter()
            .map(|id| self.h.app.presence.conversations.save_with_receipt(id))
            .collect();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut saved = Vec::new();
        while receipts.iter().any(|id| !saved.contains(id)) {
            let store = &mut self.h.app.presence.conversations;
            store.poll();
            saved.extend(store.take_receipts());
            assert!(
                store.take_failed_receipts().is_empty(),
                "fixture histories failed to persist"
            );
            assert!(
                Instant::now() < deadline,
                "fixture histories were not acknowledged"
            );
            std::thread::yield_now();
        }
        self.h.app.presence.conversations =
            conversation_store::ConversationStore::at_directory(self.store.clone());
        wait_for_store(&mut self.h);
        assert_eq!(
            self.conversations(),
            expected,
            "same identities reload real durable histories"
        );
    }

    pub fn hat(&self) -> String {
        self.h.app.presence.agents[0].profile.hat.clone()
    }

    fn assert_local(&self) {
        assert_eq!(
            (self.h.app.tab().cam.offset, self.h.app.tab().cam.z),
            self.camera
        );
        for key in [
            "codex",
            "cursor",
            "ollama",
            "comfy",
            "openai-image",
            "openai-text",
        ] {
            assert!(self.h.app.agents.catalog_ready(key));
            assert!(self.h.app.agents.catalog_error(key).is_none());
        }
        for agent in &self.h.app.presence.agents {
            assert!(agent.request.is_empty() && !agent.inflight);
            assert!(agent.runtime.is_none() && agent.cursor.is_none());
            assert!(agent.work.is_none() && agent.read.is_none());
            assert!(agent.pending_harness.is_none() && agent.switch_offer.is_none());
            assert_eq!(agent.profile.provider, "local");
        }
    }

    fn conversations(&self) -> serde_json::Value {
        json!({"self":self.h.app.presence_conversation("self"),
            "agent":self.h.app.presence_conversation(&self.who),
            "savedParticipants":self.h.app.settings.collaborators.iter().map(|p|
                (&p.id,self.h.app.presence_conversation(&p.id))).collect::<Vec<_>>()})
    }

    fn state(&self) -> serde_json::Value {
        json!({"frame":self.h.app.frame_no,"scene":self.h.app.doc().scene,
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
            "camera":[self.h.app.tab().cam.offset.x,self.h.app.tab().cam.offset.y,self.h.app.tab().cam.z]})
    }

    pub fn save(&mut self, phase: &str, out: egui::FullOutput, geometry: serde_json::Value) {
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
        review_shot(&mut self.h, &mut self.raster, out, &self.case, phase);
        assert_eq!(
            self.state(),
            before,
            "saving advanced or changed the labeled UI state"
        );
        assert_eq!(image::image_dimensions(&path).unwrap(), (1440, 900));
        let row = json!({"phase":phase,"file":path.file_name().unwrap().to_string_lossy(),
            "bytes":std::fs::metadata(&path).unwrap().len(),"geometry":geometry,"labels":labels,"state":before});
        self.write_json(&format!("{phase}.json"), &row);
        self.rows.push(row);
    }

    fn directory(&self) -> PathBuf {
        review_directory().join(&self.case)
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

fn wait_for_store(h: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        h.app.presence.conversations.poll();
        let status = h.app.presence.conversations.status();
        if status.is_none() {
            return;
        }
        assert_eq!(status.as_deref(), Some("Loading previous messages…"));
        assert!(
            Instant::now() < deadline,
            "temporary conversation store never loaded"
        );
        std::thread::yield_now();
    }
}

fn review_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/review")
}
pub(super) fn screen() -> Rect {
    Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))
}
pub(super) fn rect_json(rect: Rect) -> serde_json::Value {
    json!([rect.min.x, rect.min.y, rect.max.x, rect.max.y])
}
pub(super) fn has(out: &egui::FullOutput, text: &str) {
    assert!(
        painted_strings(out).iter().any(|s| s.contains(text)),
        "not painted: {text}"
    );
}

fn album_geometry(out: &egui::FullOutput) -> Vec<(egui::TextureId, Vec<(u32, u32)>)> {
    fn walk(shape: &egui::Shape, into: &mut Vec<(egui::TextureId, Vec<(u32, u32)>)>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, into)),
            egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
                into.push((
                    mesh.texture_id,
                    mesh.vertices
                        .iter()
                        .map(|v| (v.pos.x.to_bits(), v.pos.y.to_bits()))
                        .collect(),
                ));
            }
            _ => {}
        }
    }
    let mut geometry = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut geometry);
    }
    geometry
}
