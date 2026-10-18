//! Transient keyboard sequence state.
use atlas_commands::CommandId;
use std::time::Instant;

impl super::SlateApp {
    pub(crate) fn frame_input(&mut self, ctx: &eframe::egui::Context) {
        let _span = atlas_core::session_log::span("slate.input");
        self.desktop_sample_frame(ctx);
        let tag_edit = super::super::ui::tools::tags::editor_open(self, ctx);
        if (tag_edit || (!self.shape_property_keys(ctx) && !self.tip_numeric_keys(ctx)))
            && self.desktop_sample.is_none()
        {
            self.hotkeys(ctx);
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
