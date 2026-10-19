//! Full-app paint/input evidence; fixture data and I/O stay in test-owned stores.
#[path = "chrome_render_capture.rs"]
mod capture;
#[path = "chrome_render_scenarios.rs"]
mod scenarios;
use super::super::*;
use crate::app::tests::{
    capture_frame, painted_strings, painted_textures, primary_button, FrameRaster,
};
use eframe::egui::{self, Id, Pos2, Rect};
use scenarios::Declarations;
pub(super) use scenarios::Scenario;
use serde_json::json;
use std::time::Duration;

pub(super) struct Fixture {
    pub h: Harness,
    pub who: String,
    raster: FrameRaster,
    case: String,
    store: PathBuf,
    time: f64,
    rows: Vec<serde_json::Value>,
    camera: (egui::Vec2, f32),
    declarations: Declarations,
    started: Instant,
    retained: usize,
    viewport: Option<Rect>,
}

impl Fixture {
    pub fn new() -> Self {
        Self::for_scenario(Scenario::Chrome)
    }

    pub fn for_scenario(scenario: Scenario) -> Self {
        let mut h = host();
        // All recognized catalog routes are injected before the first app frame.
        h.app.set_agent_programs_for_test(&["codex", "cursor"]);
        if scenario == Scenario::Harness {
            for provider in ["codex", "cursor"] {
                h.app
                    .pin_pack_for_test(provider, atlas_ai::packs::PackHealth::Ok);
            }
        }
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
        h.app.settings.presence_reduced_motion = scenario == Scenario::Chrome;
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
            declarations: Declarations::new(scenario),
            started: Instant::now(),
            retained: 0,
            viewport: None,
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
        let viewport = self.viewport;
        let out = capture_frame(&mut self.h, &mut self.raster, |input| {
            input.time = Some(time);
            if let Some(rect) = viewport {
                input.screen_rect = Some(rect);
            }
            prepare(input);
        });
        self.assert_local();
        out
    }

    pub fn viewport(&self) -> Rect {
        self.viewport.unwrap_or_else(screen)
    }

    pub fn resize_viewport(&mut self, width: usize, height: usize) {
        assert_eq!(self.retained, 0, "serialize retained outputs before resize");
        let textures = self.raster.textures.clone();
        self.raster.resize(width, height);
        assert_eq!(
            self.raster.textures, textures,
            "resize retains actual uploaded textures"
        );
        self.viewport = Some(Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(width as f32, height as f32),
        ));
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
            self.viewport().contains_rect(rect),
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
            self.viewport().contains_rect(panel),
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
            assert!(agent.runtime.is_none() && agent.cursor.is_none());
            assert!(agent.work.is_none() && agent.launched.is_empty());
            self.declarations.assert_agent(agent);
        }
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
    let observed = painted_strings(out);
    assert!(
        observed.iter().any(|s| s.contains(text)),
        "not painted: {text}; painted strings: {observed:?}"
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
