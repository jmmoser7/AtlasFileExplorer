//! Focused page input. Only changed pointer samples reach the browser, so a
//! parked cursor does not synthesize work or hold the interactive cadence open.

use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) struct PointerSample(f32, f32, u8);

impl WebRuntime {
    pub(super) fn note_page_input(&mut self) {
        self.last_page_input = Some(Instant::now());
    }

    pub(super) fn recently_interactive(&self) -> bool {
        self.pointer_down != 0
            || self
                .last_page_input
                .is_some_and(|t| t.elapsed().as_secs_f32() < INTERACTIVE_HOLD_SECS)
    }

    pub(super) fn focused_frame_interval(&self, id: NodeId) -> Duration {
        if self.focused == Some(id) && self.recently_interactive() {
            Duration::from_secs_f32(1.0 / INTERACTIVE_FPS)
        } else if self.focused == Some(id) {
            Duration::from_secs_f32(1.0 / FOCUSED_FPS)
        } else {
            Duration::from_secs_f32(1.0 / IDLE_FPS)
        }
    }

    pub(super) fn pointer_move(&mut self, x: f32, y: f32, buttons: u8) -> Option<WebInput> {
        let sample = PointerSample(x, y, buttons);
        (self.pointer_sample.replace(sample) != Some(sample)).then_some(WebInput::Move {
            x,
            y,
            buttons,
        })
    }

    pub(super) fn reset_page_input(&mut self) {
        self.pointer_inside = false;
        self.pointer_down = 0;
        self.pointer_sample = None;
        self.last_page_input = None;
    }
}

impl SlateApp {
    pub(super) fn web_input_page(
        &mut self,
        ui: &egui::Ui,
        id: NodeId,
        web: &WebPortalRef,
        css_rect: slate_doc::scene::WorldRect,
        layout: &PortalChromeLayout,
        pointer: Option<Pos2>,
    ) -> bool {
        let page = layout.body;
        let inside = pointer.is_some_and(|p| page_owns_point(layout, p))
            && self.portal_owns_pointer(ui.ctx(), id);
        let dragging = self.web.pointer_down != 0;
        if !inside && !dragging {
            if std::mem::take(&mut self.web.pointer_inside) {
                self.web.host.send_input(id, WebInput::Leave);
                self.web.reset_page_input();
            }
            return false;
        }
        self.web.pointer_inside = true;

        let (css_w, css_h) = css_size(web, css_rect);
        let p = pointer.unwrap_or(page.center());
        let u = ((p.x - page.left()) / page.width().max(1.0)).clamp(0.0, 1.0);
        let v = ((p.y - page.top()) / page.height().max(1.0)).clamp(0.0, 1.0);
        let x = u * css_w as f32;
        let y = v * css_h as f32;

        let mut events = Vec::new();
        let mut buttons = self.web.pointer_down;
        ui.input(|i| {
            for (button, bit) in [
                (egui::PointerButton::Primary, 1u8),
                (egui::PointerButton::Secondary, 2),
                (egui::PointerButton::Middle, 4),
            ] {
                if i.pointer.button_pressed(button) {
                    buttons |= bit;
                }
                if i.pointer.button_released(button) {
                    buttons &= !bit;
                }
            }
            if let Some(event) = self.web.pointer_move(x, y, buttons) {
                events.push(event);
            }
            for (button, code) in [
                (egui::PointerButton::Primary, 0u8),
                (egui::PointerButton::Secondary, 1),
                (egui::PointerButton::Middle, 2),
            ] {
                if i.pointer.button_pressed(button) {
                    events.push(WebInput::Down { x, y, button: code });
                }
                if i.pointer.button_released(button) {
                    events.push(WebInput::Up { x, y, button: code });
                }
            }
            if !i.modifiers.command {
                let d = i.raw_scroll_delta;
                if d.y != 0.0 {
                    events.push(WebInput::Wheel {
                        x,
                        y,
                        delta: d.y,
                        horizontal: false,
                    });
                }
                if d.x != 0.0 {
                    events.push(WebInput::Wheel {
                        x,
                        y,
                        delta: d.x,
                        horizontal: true,
                    });
                }
            }
        });
        self.web.pointer_down = buttons;
        events.extend(self.web_keyboard_events(ui));
        let interacting = events.iter().any(page_input_is_interactive);
        for event in events {
            self.web.host.send_input(id, event);
        }
        if interacting {
            self.web.note_page_input();
            ui.ctx().request_repaint();
        }
        ui.ctx().set_cursor_icon(
            self.web
                .host
                .cursor(id)
                .unwrap_or(egui::CursorIcon::Default),
        );
        true
    }

    /// Keys the focused page is allowed to hear. Esc is Slate's, always: it is
    /// how focus is peeled back off the page (D22).
    fn web_keyboard_events(&self, ui: &egui::Ui) -> Vec<WebInput> {
        let mut events = Vec::new();
        ui.input(|i| {
            for event in &i.events {
                match event {
                    egui::Event::Key { key, pressed, .. } if *key != egui::Key::Escape => {
                        events.push(WebInput::Key {
                            key: *key,
                            pressed: *pressed,
                        });
                    }
                    egui::Event::Text(t) => {
                        events.extend(t.chars().map(WebInput::Text));
                    }
                    _ => {}
                }
            }
        });
        events
    }
}

fn page_input_is_interactive(event: &WebInput) -> bool {
    match event {
        WebInput::Wheel { .. }
        | WebInput::Down { .. }
        | WebInput::Up { .. }
        | WebInput::Key { .. }
        | WebInput::Text(_) => true,
        WebInput::Move { .. } => true,
        WebInput::Leave => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{only_portal, web_board, with_fake_host, FakeWebHost, Harness};

    fn focused_page(tag: &str) -> (Harness, FakeWebHost, NodeId, Pos2) {
        let mut h = web_board(tag);
        let host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/dashboard", Pos2::ZERO);
        let (id, _) = only_portal(&h);
        h.app.web_allow_origin(id);
        h.frame();
        h.app.web_focus(id);
        let node = h.app.doc().scene.node(id).unwrap();
        let point = h.app.board_xf().rect_w2s(node.rect).center();
        (h, host, id, point)
    }

    fn moves(host: &FakeWebHost) -> Vec<WebInput> {
        host.inputs()
            .into_iter()
            .filter(|event| matches!(event, WebInput::Move { .. }))
            .collect()
    }

    #[test]
    fn moving_hover_is_interactive_but_a_parked_pointer_sends_nothing() {
        let (mut h, host, id, point) = focused_page("web_pointer_dedupe");
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
        assert_eq!(moves(&host).len(), 1);
        let active_at = h.app.web.last_page_input;
        assert!(active_at.is_some());
        assert_eq!(
            h.app.web.focused_frame_interval(id),
            Duration::from_secs_f32(1.0 / INTERACTIVE_FPS)
        );
        for _ in 0..4 {
            h.frame();
        }
        assert_eq!(
            moves(&host).len(),
            1,
            "repaints do not synthesize browser mousemoves"
        );
        assert_eq!(
            h.app.web.last_page_input, active_at,
            "parked input does not extend the burst"
        );
        h.frame_with(|input| {
            input
                .events
                .push(egui::Event::PointerMoved(point + egui::vec2(3.0, 0.0)))
        });
        assert_eq!(moves(&host).len(), 2);
        assert!(h.app.web.last_page_input > active_at);
    }

    #[test]
    fn button_changes_forward_once_without_repeating_a_held_drag() {
        let (mut h, host, _, point) = focused_page("web_pointer_buttons");
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
        for pressed in [true, false] {
            h.frame_with(|input| {
                input.events.push(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                })
            });
            h.frame();
        }
        assert_eq!(
            moves(&host).len(),
            3,
            "entry, down, and up each change the sample once"
        );
        assert_eq!(
            host.sent(|event| match event {
                WebInput::Move { buttons, .. } => Some(*buttons),
                _ => None,
            }),
            vec![0, 1, 0]
        );
        assert_eq!(
            host.sent(|event| matches!(event, WebInput::Down { .. }).then_some(()))
                .len(),
            1
        );
        assert_eq!(
            host.sent(|event| matches!(event, WebInput::Up { .. }).then_some(()))
                .len(),
            1
        );
    }

    #[test]
    fn holding_a_button_keeps_capture_fast_without_synthetic_moves() {
        let mut web = WebRuntime::default();
        web.focused = Some(NodeId(1));
        web.pointer_down = 1;
        web.last_page_input = None;
        assert_eq!(
            web.focused_frame_interval(NodeId(1)),
            Duration::from_secs_f32(1.0 / INTERACTIVE_FPS)
        );
        assert!(web.pointer_move(10.0, 20.0, 1).is_some());
        assert!(web.pointer_move(10.0, 20.0, 1).is_none());
        web.pointer_down = 0;
        assert_eq!(
            web.focused_frame_interval(NodeId(1)),
            Duration::from_secs_f32(1.0 / FOCUSED_FPS)
        );
    }

    #[test]
    fn pointer_tracking_resets_on_leave_focus_and_source_changes() {
        let (mut h, host, id, point) = focused_page("web_pointer_reset");
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
        h.frame_with(|input| {
            input
                .events
                .push(egui::Event::PointerMoved(Pos2::new(-100.0, -100.0)))
        });
        h.frame();
        assert_eq!(
            host.sent(|event| matches!(event, WebInput::Leave).then_some(()))
                .len(),
            1
        );
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
        assert_eq!(
            moves(&host).len(),
            2,
            "re-entry forwards the first sample again"
        );
        h.app.web_blur();
        h.app.web_focus(id);
        h.frame();
        assert_eq!(
            moves(&host).len(),
            3,
            "refocus forwards the current pointer"
        );
        h.app
            .bind_web_source(id, "https://example.com/other-dashboard".into());
        h.frame();
        assert_eq!(
            moves(&host).len(),
            4,
            "a new browser receives its first sample"
        );
    }
}
