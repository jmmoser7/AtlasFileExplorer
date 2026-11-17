//! Transient keyboard sequence state.
use atlas_commands::CommandId;
use eframe::egui;
use std::time::Instant;

impl super::SlateApp {
    pub(crate) fn frame_input(&mut self, ctx: &egui::Context) {
        let _span = atlas_core::session_log::span("slate.input");
        let space_interrupted = ctx.input_mut(|input| {
            let interrupted = space_input_interrupted(input);
            if interrupted {
                // The OS window menu may receive Space's key-up instead.
                input.keys_down.remove(&egui::Key::Space);
            }
            interrupted
        });
        if space_interrupted {
            self.space_tap = SpaceTap::default();
        }
        if ctx.memory(|memory| memory.top_modal_layer().is_some()) {
            return;
        }
        self.desktop_sample_frame(ctx);
        let tag_edit = super::super::ui::tools::tags::editor_open(self, ctx);
        if (tag_edit
            || (!self.dismiss_generator_popup_for_escape(ctx)
                && !self.shape_property_keys(ctx)
                && !self.tip_numeric_keys(ctx)))
            && self.desktop_sample.is_none()
            && !self.presence_captures_shortcut(ctx)
        {
            self.hotkeys(ctx, space_interrupted);
        }
        self.drop_stale_portal_chrome();
    }

    pub(super) fn selected_slate_enter(&mut self, ctx: &eframe::egui::Context) -> bool {
        if self.board_tool != super::board::BoardTool::Select
            || !ctx.input(|i| i.key_pressed(eframe::egui::Key::Enter))
        {
            return false;
        }
        let ids: Vec<_> = self.board_sel.iter().copied().collect();
        let slate = ids.len() == 1 && self.doc().scene.node(ids[0]).is_some_and(|node| {
            matches!(&node.kind, slate_doc::NodeKind::Portal(p) if p.kind == slate_doc::PortalKind::Slate)
        });
        if slate {
            ctx.input_mut(|i| {
                i.consume_key(eframe::egui::Modifiers::NONE, eframe::egui::Key::Enter)
            });
            self.open_slate_portal(ids[0]);
        }
        slate
    }
}

pub(in crate::app) fn space_input_interrupted(input: &egui::InputState) -> bool {
    !input.focused
        || input.modifiers.alt
        || input.raw.events.iter().any(|event| {
            matches!(event, egui::Event::WindowFocused(false))
                || matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Space,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.alt
                )
        })
}

/// Native input can batch a primary press and a displaced release without a
/// matching move. egui only measures click-distance on move samples, so give
/// it the final sample before it assigns the release to a widget.
pub(in crate::app) fn supply_brush_release_motion(events: &mut Vec<egui::Event>) {
    use egui::{Event, PointerButton::Primary};

    let mut last = None;
    let mut moves = Vec::new();
    for (index, event) in events.iter().enumerate() {
        match *event {
            Event::PointerButton {
                pos,
                button: Primary,
                pressed: true,
                ..
            } => last = Some(pos),
            Event::PointerMoved(pos) if last.is_some() => last = Some(pos),
            Event::PointerButton {
                pos,
                button: Primary,
                pressed: false,
                ..
            } => {
                if last.take().is_some_and(|p| p != pos) {
                    moves.push((index, Event::PointerMoved(pos)));
                }
            }
            Event::PointerGone | Event::WindowFocused(false) => last = None,
            _ => {}
        }
    }
    for (index, event) in moves.into_iter().rev() {
        events.insert(index, event);
    }
}

/// Tracks a live Space press so release can decide tap (repeat) vs hold (pan).
#[derive(Default)]
pub struct SpaceTap {
    pub(super) pressed_at: Option<Instant>,
    /// A pointer button went down while Space was held — it was a pan chord.
    pub(super) pointer_used: bool,
}

/// Pending bare-letter shortcut waiting out the type-to-command ambiguity
/// window. If more printable characters arrive before [`BARE_LETTER_HOLD`],
/// the buffer opens the canvas palette instead of firing `pending_id`.
#[derive(Clone, Debug)]
pub struct BareLetterHold {
    pub pending_id: CommandId,
    pub buffer: String,
    pub started_at: Instant,
}

#[cfg(test)]
mod credential_modal_tests;
