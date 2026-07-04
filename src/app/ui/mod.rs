//! Top-level chrome: browser tabs + global undo/redo only.
//! Everything else lives inside the active tab workspace.

mod activity_heatmap;
mod advanced;
mod readouts;
mod sidebar;
mod tabs;
mod tools;
mod widgets;

pub use widgets::group_digits;

use super::AtlasApp;
use eframe::egui::Context;

impl AtlasApp {
    pub(super) fn draw_top_chrome(&mut self, ctx: &Context) {
        tabs::top_bar(self, ctx);
    }

    pub(super) fn draw_tools_rail(&mut self, ctx: &Context) {
        tools::left_panel(self, ctx);
    }

    pub(super) fn draw_readout_bar(&mut self, ctx: &Context) {
        readouts::status_bar(self, ctx);
    }

    pub(super) fn draw_advanced_window(&mut self, ctx: &Context) {
        advanced::window(self, ctx);
    }
}
