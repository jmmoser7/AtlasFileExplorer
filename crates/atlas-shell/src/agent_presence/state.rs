use eframe::egui::{Pos2, Rect};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Expansion {
    Chat(String),
    Profile(String),
    Add,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Popover {
    Hats,
    Provider,
    More,
    Advanced,
    Retirement,
}
#[derive(Default)]
pub(super) struct Edit {
    pub name: String,
    pub message: String,
    pub last_draft: String,
}
pub struct RosterState {
    pub(super) font_family: Option<eframe::egui::FontFamily>,
    pub(super) edits: HashMap<String, Edit>,
    pub(super) expansion: Option<Expansion>,
    pub(super) popover: Option<Popover>,
    pub(super) anchor: Rect,
    pub(super) canvas_anchor: Option<(Pos2, f32)>,
    pub(super) requested_provider: Option<String>,
    pub(super) add_name: String,
    pub(super) add_hat: String,
    pub(super) add_provider: String,
    pub(super) workspace_ready: bool,
    pub(super) retirement: String,
    pub(super) last_popup_rect: Rect,
    pub(super) last_popover_rect: Rect,
    pub(super) model_menu_open: bool,
    pub(super) last_model_popup_rect: Rect,
    pub(super) just_opened: bool,
    pub(super) roster_scroll: f32,
    pub(super) last_painted_pass: Option<u64>,
    pub(super) history_offsets: HashMap<String, usize>,
}
impl Default for RosterState {
    fn default() -> Self {
        Self {
            font_family: None,
            edits: HashMap::new(),
            expansion: None,
            popover: None,
            anchor: Rect::NOTHING,
            canvas_anchor: None,
            requested_provider: None,
            add_name: "Agent".into(),
            add_hat: "cap".into(),
            add_provider: String::new(),
            workspace_ready: true,
            retirement: "bow".into(),
            last_popup_rect: Rect::NOTHING,
            last_popover_rect: Rect::NOTHING,
            model_menu_open: false,
            last_model_popup_rect: Rect::NOTHING,
            just_opened: false,
            roster_scroll: 0.0,
            last_painted_pass: None,
            history_offsets: HashMap::new(),
        }
    }
}
impl RosterState {
    /// Apps report workspace availability; standalone consumers default to ready.
    pub fn set_workspace_ready(&mut self, ready: bool) {
        self.workspace_ready = ready;
    }
    /// Apps provide a registered regular UI family; the shell owns its use.
    /// An unavailable family falls back to proportional without changing app fonts.
    pub fn set_font_family(&mut self, family: Option<eframe::egui::FontFamily>) {
        self.font_family = family;
    }
    pub fn font_family(&self) -> Option<&eframe::egui::FontFamily> {
        self.font_family.as_ref()
    }
    pub fn open_chat(&mut self, id: &str) {
        self.model_menu_open = false;
        self.last_model_popup_rect = Rect::NOTHING;
        self.expansion = Some(Expansion::Chat(id.into()));
        self.canvas_anchor = None;
        self.popover = None;
        self.just_opened = true;
    }
    /// The anchor is the screen-space top-left of a world-sized attached chat.
    pub fn open_chat_at(&mut self, id: &str, anchor: Pos2, zoom: f32) {
        self.open_chat(id);
        self.update_chat_anchor(anchor, zoom);
    }
    pub fn update_chat_anchor(&mut self, anchor: Pos2, zoom: f32) {
        if anchor.is_finite() && zoom.is_finite() && zoom > 0.0 {
            self.canvas_anchor = Some((anchor, zoom));
        }
    }
    pub fn canvas_chat(&self) -> Option<&str> {
        self.canvas_anchor.and_then(|_| self.active_chat())
    }
    /// Select the only transcript rows an adapter needs to borrow/map this frame.
    /// The page and attached-chat limit have one owner in the transcript module.
    pub fn message_range(&self, id: &str, total: usize) -> std::ops::Range<usize> {
        super::transcript::history_window(
            total,
            self.history_offsets.get(id).copied().unwrap_or(0),
            self.canvas_chat() == Some(id),
        )
    }
    pub fn close_chat(&mut self) {
        self.model_menu_open = false;
        self.last_model_popup_rect = Rect::NOTHING;
        self.expansion = None;
        self.popover = None;
        self.canvas_anchor = None;
    }
    pub fn is_open(&self) -> bool {
        self.expansion.is_some()
    }
    pub fn captures_pointer(&self, ctx: &eframe::egui::Context) -> bool {
        let pointer = ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| self.occludes_point(ctx, p));
        let focused = ctx.memory(|m| m.focused());
        let editing = match self.expansion.as_ref() {
            Some(Expansion::Chat(id)) => {
                focused == Some(eframe::egui::Id::new(("presence-composer", id.as_str())))
            }
            Some(Expansion::Profile(id)) => {
                focused == Some(eframe::egui::Id::new(("presence-name", id.as_str())))
            }
            Some(Expansion::Add) => ctx.wants_keyboard_input(),
            None => false,
        };
        pointer || editing
    }
    /// A panel's own dismissal click stays claimed for the rest of this paint
    /// pass. On the next pass only an actually open surface occludes the board.
    pub fn occludes_point(&self, ctx: &eframe::egui::Context, point: Pos2) -> bool {
        (self.is_open() || self.last_painted_pass == Some(ctx.cumulative_pass_nr()))
            && (self.last_popup_rect.contains(point)
                || self.last_popover_rect.contains(point)
                || self.last_model_popup_rect.contains(point))
    }
    pub fn active_chat(&self) -> Option<&str> {
        match self.expansion.as_ref() {
            Some(Expansion::Chat(id)) => Some(id),
            _ => None,
        }
    }
    pub(super) fn toggle(&mut self, expansion: Expansion, anchor: Rect) {
        self.model_menu_open = false;
        self.last_model_popup_rect = Rect::NOTHING;
        self.expansion = if self.expansion.as_ref() == Some(&expansion) {
            None
        } else {
            Some(expansion)
        };
        self.anchor = anchor;
        self.canvas_anchor = None;
        self.popover = None;
        self.requested_provider = None;
        self.just_opened = true;
    }
    pub(super) fn toggle_popover(&mut self, popover: Popover) {
        self.model_menu_open = false;
        self.last_model_popup_rect = Rect::NOTHING;
        self.popover = if self.popover == Some(popover) {
            None
        } else {
            Some(popover)
        };
        self.requested_provider = None;
    }
}
