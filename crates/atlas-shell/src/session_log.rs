//! Advanced-window readout for [`atlas_core::session_log`].
//!
//! Painting lives here so File Atlas and Slate cannot drift. Apps pass the
//! log and apply the returned actions (mark / reveal).

use atlas_core::session_log::SessionLog;
use eframe::egui::{self, RichText};

use crate::theme::Palette;

pub struct SessionLogActions {
    pub mark: bool,
    pub reveal: bool,
}

/// Shared "Session log" section for both Advanced windows.
pub fn section(ui: &mut egui::Ui, log: &SessionLog, palette: &Palette) -> SessionLogActions {
    let mut actions = SessionLogActions {
        mark: false,
        reveal: false,
    };

    ui.label(RichText::new("Session log").small().strong());
    ui.label(
        RichText::new(
            "Records frame stalls and named work while you use the app, so a \
             later debug session can read what happened instead of reconstructing \
             it. F4 marks this moment.",
        )
        .small()
        .color(palette.sub),
    );
    ui.add_space(4.0);

    let recording = if log.persist_enabled() {
        "Writing to disk"
    } else {
        "In memory only (this run is not writing)"
    };
    ui.label(RichText::new(recording).small().color(palette.sub));

    if let Some(path) = log.log_path() {
        ui.label(
            RichText::new(path.display().to_string())
                .small()
                .color(palette.sub),
        );
    }

    if !log.is_empty() {
        ui.label(
            RichText::new(format!(
                "p95 {:.0} ms · max {:.0} ms · app max {:.0} ms · {} dropped / {} frames",
                log.p95_ms(),
                log.max_ms(),
                log.app_max_ms(),
                log.dropped(),
                log.frames()
            ))
            .small()
            .color(palette.ink),
        );
    }

    let stalls = log.last_stalls();
    if let Some(last) = stalls.last() {
        ui.label(
            RichText::new(format!(
                "Last stall: {:.0} ms app / {:.0} ms delivered · {} · {} entries / {} nodes",
                last.app_ms, last.delivered_ms, last.spans, last.entries, last.nodes
            ))
            .small()
            .color(palette.sub),
        );
    }

    ui.horizontal(|ui| {
        if ui
            .small_button("Mark this moment")
            .on_hover_text("F4 — drops a bookmark into the log")
            .clicked()
        {
            actions.mark = true;
        }
        ui.add_enabled_ui(log.persist_enabled(), |ui| {
            if ui.small_button("Open log folder").clicked() {
                actions.reveal = true;
            }
        });
    });

    actions
}
