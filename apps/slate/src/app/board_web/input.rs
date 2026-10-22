//! Focused page input. Only changed pointer samples reach the browser, so a
//! parked cursor does not synthesize work or hold the interactive cadence open.

use super::*;

/// Input forwarded to the one portal holding input focus (D22). Composition
/// hosting gives the app no HWND for the page, so every event is explicit.
#[derive(Debug, Clone, PartialEq)]
pub enum WebInput {
    /// Pointer position in page CSS pixels. `buttons` is the bitmask of what is
    /// currently held (1 = left, 2 = right, 4 = middle) — without it a page
    /// sees a bare hover and drag-selecting text does nothing.
    Move {
        x: f32,
        y: f32,
        buttons: u8,
    },
    Down {
        x: f32,
        y: f32,
        button: u8,
    },
    Up {
        x: f32,
        y: f32,
        button: u8,
    },
    /// Wheel in page units; `horizontal` distinguishes the two axes.
    Wheel {
        x: f32,
        y: f32,
        delta: f32,
        horizontal: bool,
    },
    /// The pointer left the frame, so hover states can clear.
    Leave,
    Key {
        key: egui::Key,
        pressed: bool,
        modifiers: egui::Modifiers,
    },
    /// One complete platform text or paste payload; never split Unicode scalars.
    Text(String),
}

/// Whether a screen point belongs to the live page rather than Slate's frame
/// band or chrome (D17/D22). Chrome takes priority where the inset page hit
/// region overlaps it; native scrollbar tracks remain reachable at the edge.
pub(crate) fn page_owns_point(layout: &PortalChromeLayout, p: Pos2) -> bool {
    if layout.pointer_on_chrome(p) {
        return false;
    }
    let page = layout.body;
    let scrollbar_width = atlas_shell::tabs::portal_scrollbar_width(
        layout.bar.or(layout.reveal).map_or(0.0, |r| r.height()),
    );
    layout.page.contains(p)
        || (page.contains(p)
            && (p.x >= page.right() - scrollbar_width || p.y >= page.bottom() - scrollbar_width))
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct PointerSample(f32, f32, u8);

pub(super) fn screen_point_for_css(
    layout: &PortalChromeLayout,
    css_width: f32,
    css_height: f32,
    x: f32,
    y: f32,
) -> Pos2 {
    let page = layout.body;
    Pos2::new(
        page.left() + page.width() * x / css_width.max(1.0),
        page.top() + page.height() * y / css_height.max(1.0),
    )
}

pub(super) fn css_point_for_screen(
    layout: &PortalChromeLayout,
    css_width: f32,
    css_height: f32,
    point: Pos2,
) -> (f32, f32) {
    let page = layout.body;
    let u = ((point.x - page.left()) / page.width().max(1.0)).clamp(0.0, 1.0);
    let v = ((point.y - page.top()) / page.height().max(1.0)).clamp(0.0, 1.0);
    (u * css_width, v * css_height)
}

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
        let (x, y) = css_point_for_screen(layout, css_w as f32, css_h as f32, p);

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
        if ui.ctx().wants_keyboard_input() || self.palette_state.open {
            return Vec::new();
        }
        ui.input(|i| keyboard_events(&i.events))
            .into_iter()
            .filter(|event| {
                !matches!(event, WebInput::Key { key, modifiers, .. }
                if self.slate_owns_web_key(*key, *modifiers))
            })
            .collect()
    }
}

/// The platform already supplies committed text. Raw keys carry shortcuts and
/// editing; a paste payload replaces its shortcut so it cannot also read the
/// browser clipboard and insert twice.
fn keyboard_events(input: &[egui::Event]) -> Vec<WebInput> {
    let pasted = input
        .iter()
        .any(|event| matches!(event, egui::Event::Paste(_)));
    let mut output = Vec::new();
    let mut clipboard_sent = [false; 2];
    for event in input {
        match event {
            egui::Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } if *key != egui::Key::Escape
                && !(pasted
                    && ((*key == egui::Key::V && (modifiers.command || modifiers.ctrl))
                        || (*key == egui::Key::Insert && modifiers.shift))) =>
            {
                output.push(WebInput::Key {
                    key: *key,
                    pressed: *pressed,
                    modifiers: *modifiers,
                });
            }
            egui::Event::Text(text)
                if !(pasted
                    && input.iter().any(
                        |other| matches!(other, egui::Event::Paste(paste) if paste == text),
                    ))
                    && !text.is_empty() =>
            {
                output.push(WebInput::Text(text.clone()))
            }
            egui::Event::Paste(text) if !text.is_empty() => {
                output.push(WebInput::Text(text.clone()))
            }
            egui::Event::Copy | egui::Event::Cut => {
                let key = if matches!(event, egui::Event::Copy) {
                    egui::Key::C
                } else {
                    egui::Key::X
                };
                // Platforms can supply either the shortcut or a clipboard
                // event. Invoke the page action once, without reading bytes.
                let duplicate =
                    std::mem::replace(&mut clipboard_sent[usize::from(key == egui::Key::X)], true);
                if !duplicate
                    && !input.iter().any(|other| {
                        matches!(other,
                    egui::Event::Key { key: other, pressed: true, modifiers, .. }
                        if *other == key && (modifiers.ctrl || modifiers.command))
                    })
                {
                    for pressed in [true, false] {
                        output.push(WebInput::Key {
                            key,
                            pressed,
                            modifiers: egui::Modifiers::CTRL,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    output
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
mod tests;
