//! Top-level chrome: unified top bar (icon portal, menus, tabs, window controls).
//! Everything else lives inside the active tab workspace.

pub(crate) mod activity_heatmap;
mod advanced;
mod menubar;
mod readouts;
mod tools;

pub use atlas_shell::widgets::group_digits;

use super::AtlasApp;
use eframe::egui::Context;

impl AtlasApp {
    pub(super) fn draw_top_bar(&mut self, ctx: &Context) {
        menubar::top_bar(self, ctx);
    }

    pub(super) fn draw_tools_rail(&mut self, ctx: &Context) {
        tools::floating_tools_dock(self, ctx);
    }

    pub(super) fn draw_readout_bar(&mut self, ctx: &Context) {
        readouts::status_bar(self, ctx);
    }

    /// Temporary pre-warm progress dashboard (only while a run is active).
    pub(super) fn draw_prewarm_dashboard(&mut self, ctx: &Context) {
        readouts::prewarm_dashboard(self, ctx);
    }

    pub(super) fn draw_advanced_window(&mut self, ctx: &Context) {
        advanced::window(self, ctx);
    }
}
