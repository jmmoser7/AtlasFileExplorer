//! Ordinary Slate frames and real egui pointer/key/wheel gestures. Fixture work
//! and visible history are synthetic; no provider is launched or claimed.
use super::super::*;
use super::{add, chrome_render::world_opaque_head, host, proposal_support::begin_command};
use crate::app::tests::{add_filled_rect, painted_strings, primary_button, Harness};
use eframe::egui::{Id, Pos2, Rect, Vec2};
use serde_json::json;
use std::time::Duration;

struct Gesture {
    h: Harness,
    who: String,
    time: f64,
    zoom: f32,
    underlay: slate_doc::NodeId,
}
impl Gesture {
    fn new(zoom: f32) -> Self {
        let mut h = host();
        h.app.ensure_work_tab();
        h.app.set_agent_programs_for_test(&[]);
        h.app.set_board_tool(crate::app::board::BoardTool::Select);
        h.frame();
        h.app.tab_mut().cam.z = zoom;
        let who = add(&mut h, "Dot");
        let p = h.app.presence.agents[0]
            .position(&h.app.presence.roster)
            .unwrap();
        let underlay = add_filled_rect(
            &mut h.app,
            p.x as f32 - 200.0,
            p.y as f32 - 200.0,
            1200.0,
            1200.0,
        );
        let target = add_filled_rect(&mut h.app, p.x as f32 + 600.0, p.y as f32, 60.0, 60.0);
        h.app.board_sel.clear();
        let c = h.app.presence.conversations.edit(&who).unwrap();
        c.push(
            "user",
            "Recent request",
            "You",
            None,
            "synthetic-history",
            "complete",
        );
        c.push(
            "assistant",
            "Recent answer",
            "Dot",
            None,
            "synthetic-history",
            "complete",
        );
        begin_command(
            &mut h,
            &who,
            "board.objects.move",
            &[target],
            json!({"dx":120,"dy":0}),
        );
        let time = h.ctx.input(|i| i.time);
        let mut f = Self {
            h,
            who,
            time,
            zoom,
            underlay,
        };
        f.paint();
        let before = f.h.app.presence.agents[0]
            .position(&f.h.app.presence.roster)
            .unwrap();
        std::thread::sleep(Duration::from_millis(20));
        f.paint();
        assert_ne!(
            f.h.app.presence.agents[0].position(&f.h.app.presence.roster),
            Some(before),
            "fixture was actually moving before the click"
        );
        f
    }
    fn paint(&mut self) -> egui::FullOutput {
        self.time += 1.0 / 60.0;
        let time = self.time;
        self.h.frame_output(|i| i.time = Some(time))
    }
    fn input(&mut self, prepare: impl FnOnce(&mut egui::RawInput)) {
        self.time += 1.0 / 60.0;
        let time = self.time;
        self.h.frame_with(|i| {
            i.time = Some(time);
            prepare(i);
        });
    }
    fn button(&mut self, at: Pos2, pressed: bool) {
        self.input(primary_button(at, pressed, false));
    }
    fn double_click(&mut self, at: Pos2) {
        self.button(at, true);
        assert!(self.h.app.presence.agents[0].paused);
        self.button(at, false);
        self.button(at, true);
        self.button(at, false);
    }
    fn head(&self, out: &egui::FullOutput) -> Rect {
        let p = self.h.app.presence.agents[0]
            .position(&self.h.app.presence.roster)
            .unwrap();
        let center = self
            .h
            .app
            .board_xf()
            .w2s(egui::pos2(p.x as f32, p.y as f32));
        world_opaque_head(out, center, self.zoom)
    }
    fn camera(&self) -> (Vec2, f32) {
        (self.h.app.tab().cam.offset, self.h.app.tab().cam.z)
    }
    fn wheel(&mut self, at: Pos2) {
        self.input(|i| {
            i.events.push(egui::Event::PointerMoved(at));
            i.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, 80.0),
                modifiers: egui::Modifiers::NONE,
            });
        });
    }
}
#[test]
fn actual_avatar_press_double_click_close_escape_and_wheel_keep_the_canvas_contract() {
    for zoom in [1.0, 0.5] {
        let mut f = Gesture::new(zoom);
        let out = f.paint();
        let head = f.head(&out);
        let scene = f.h.app.doc().scene.clone();
        let depth = f.h.app.tab().journal.undo_depth();
        let underlay = f.h.app.doc().scene.node(f.underlay).unwrap().clone();
        let before = f.h.app.presence.agents[0]
            .position(&f.h.app.presence.roster)
            .unwrap();
        assert!(f
            .h
            .app
            .board_xf()
            .rect_w2s(underlay.rect)
            .contains(head.center()));
        f.button(head.center(), true);
        assert!(
            f.h.app.presence.agents[0].paused,
            "real first press parks the visible moving avatar"
        );
        assert_eq!(
            f.h.app.presence.agents[0].position(&f.h.app.presence.roster),
            Some(before)
        );
        assert!(f.h.app.board_sel.is_empty());
        assert!(f.h.app.board_drag.is_none());
        let dragged = head.center() + Vec2::new(32.0, 0.0);
        f.input(|i| i.events.push(egui::Event::PointerMoved(dragged)));
        f.button(dragged, false);
        assert!(
            f.h.app.board_sel.is_empty(),
            "avatar press/drag never selects the object beneath it"
        );
        assert!(f.h.app.board_drag.is_none());
        assert_eq!(f.h.app.doc().scene.node(f.underlay), Some(&underlay));
        assert_eq!(f.h.app.doc().scene, scene);
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);

        let out = f.paint();
        let head = f.head(&out);
        f.double_click(head.center());
        let out = f.paint();
        let a = &f.h.app.presence.agents[0];
        assert!(a.paused);
        assert_eq!(a.activity, "listening");
        assert_eq!(f.h.app.presence.chrome.canvas_chat(), Some(f.who.as_str()));
        assert_eq!(f.h.app.presence.chrome.active_chat(), Some(f.who.as_str()));
        let text = painted_strings(&out);
        for latest in ["Recent request", "Recent answer"] {
            assert!(
                text.iter().any(|s| s.contains(latest)),
                "{latest} visible in attached chat at {zoom}"
            );
        }
        // Actual widget responses and their existing layer transform own the
        // control points; no hand-made close/composer hit geometry.
        let [close, composer] = [
            Id::new(("conversation-more", f.who.as_str())),
            Id::new(("presence-composer", f.who.as_str())),
        ]
        .map(|id| {
            let r = f.h.ctx.read_response(id).expect("actual attached widget");
            f.h.ctx
                .layer_transform_to_global(r.layer_id)
                .unwrap_or_default()
                * r.rect
        });
        let head = f.head(&out);
        assert!(
            close.top() > head.bottom(),
            "attached conversation sits beneath the actual painted avatar"
        );
        assert!(composer.is_positive());
        let camera = f.camera();
        f.wheel(composer.center());
        for _ in 0..24 {
            f.paint();
            assert_eq!(
                f.camera(),
                camera,
                "wheel over attached chat cannot move/zoom the human camera"
            );
        }

        f.button(close.center(), true);
        f.button(close.center(), false);
        f.paint();
        assert!(!f.h.app.presence.chrome.is_open());
        assert!(
            f.h.app.presence.agents[0].paused,
            "closing chat retains the explicit listening pause"
        );
        let out = f.paint();
        let head = f.head(&out);
        f.double_click(head.center());
        f.paint();
        assert_eq!(f.h.app.presence.chrome.canvas_chat(), Some(f.who.as_str()));
        f.input(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        f.paint();
        assert!(!f.h.app.presence.chrome.is_open());
        assert!(f.h.app.presence.agents[0].paused);
        f.input(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        f.paint();
        let camera = f.camera();
        // Positive control uses the canvas away from the avatar and dismissed chat.
        let board_point = f.h.app.canvas_rect.shrink(24.0).right_bottom();
        f.wheel(board_point);
        for _ in 0..24 {
            f.paint();
        }
        assert_ne!(
            f.camera(),
            camera,
            "positive control: the same wheel input reaches the board after chat dismissal"
        );
        assert_eq!(f.h.app.doc().scene, scene);
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
        assert!(f.h.app.board_sel.is_empty());
    }
}
