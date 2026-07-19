//! Slate chrome: thin adapters between `SlateApp` state and the shared
//! `atlas-shell` chrome (which owns all painting).

mod advanced;
mod inspector;
mod menubar;
mod readouts;
mod tools;

use super::SlateApp;
use eframe::egui::Context;

impl SlateApp {
    pub(super) fn draw_top_bar(&mut self, ctx: &Context) {
        menubar::top_bar(self, ctx);
    }

    pub(super) fn draw_tools_rail(&mut self, ctx: &Context) {
        tools::floating_tools_dock(self, ctx);
    }

    pub(super) fn draw_readout_bar(&mut self, ctx: &Context) {
        readouts::status_bar(self, ctx);
    }

    pub(super) fn draw_advanced_window(&mut self, ctx: &Context) {
        advanced::window(self, ctx);
    }
}
