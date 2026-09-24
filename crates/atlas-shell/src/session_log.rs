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

/// Input and immediate-repaint cue for [`atlas_core::session_log::is_stall`].
pub fn frame_wake(ctx: &egui::Context) -> atlas_core::session_log::FrameWake {
    let had_input = ctx.input(|input| {
        input
            .raw
            .events
            .iter()
            .any(|event| !matches!(event, egui::Event::Screenshot { .. }))
    });
    atlas_core::session_log::FrameWake {
        had_input,
        eager: ctx.requested_repaint_last_pass(),
    }
}

/// Short `file:line reason` list from [`egui::Context::repaint_causes`].
pub fn repaint_cause_summary(ctx: &egui::Context) -> String {
    let causes = ctx.repaint_causes();
    let mut summary = String::new();
    for (i, cause) in causes.iter().take(6).enumerate() {
        if i > 0 {
            summary.push_str(" | ");
        }
        let file = cause.file.rsplit(['/', '\\']).next().unwrap_or(cause.file);
        summary.push_str(file);
        summary.push(':');
        summary.push_str(&cause.line.to_string());
        if !cause.reason.is_empty() {
            summary.push(' ');
            for ch in cause.reason.chars().take(80) {
                summary.push(ch);
            }
        }
    }
    if causes.len() > 6 {
        summary.push_str(" | …");
    }
    summary
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
