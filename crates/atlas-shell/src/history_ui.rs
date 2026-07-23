//! Shared journal-history overlay.
//!
//! A read-only floating panel anchored to the right edge of the window that
//! lists journaled commands newest-first: name, detail, author chip (empty
//! author = human — Constitution Art. VI keeps agents visible), and relative
//! time. Apps adapt their journal into plain [`HistoryRow`]s; no dependency
//! on any command or document crate (Constitution Art. X).
//!
//! Zero cost while closed: [`history_window`] returns immediately when
//! `*open` is `false` (Constitution Art. II).

use crate::theme::Palette;
use crate::tokens;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};

/// One journaled command, ready for display. Pass rows **newest first**.
pub struct HistoryRow {
    /// Command name (e.g. "Move nodes").
    pub name: String,
    /// Human-readable payload summary (e.g. "3 nodes · +120, −40").
    pub detail: String,
    /// Authoring agent name; empty string means the human.
    pub author: String,
    /// Relative timestamp text (e.g. "2 m ago"). Pre-formatted by the app.
    pub ago: String,
}

/// Show the history panel while `*open`. The panel's close button flips
/// `open` back to `false`; everything else is read-only.
pub fn history_window(ctx: &egui::Context, open: &mut bool, rows: &[HistoryRow]) {
    if !*open {
        return;
    }

    let t = tokens::current();
    let dark = ctx.style().visuals.dark_mode;
    let palette = Palette::for_mode(dark);
    let th = if dark { &t.dock.dark } else { &t.dock.light };

    let screen = ctx.screen_rect();
    // Right edge, below the top bar, above the bottom readouts.
    let top_offset = t.topbar.height + 18.0;
    let max_height = (screen.height() - top_offset - 60.0).max(160.0);

    egui::Area::new(egui::Id::new("atlas_shell_history_window"))
        .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-12.0, top_offset))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(th.popover_fill_color())
                .stroke(Stroke::new(1.0_f32, th.border_color()))
                .corner_radius(CornerRadius::same(
                    t.dock.popover_corner_radius.clamp(0.0, 255.0) as u8,
                ))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 14,
                    spread: 1,
                    color: Color32::from_black_alpha(70),
                })
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(t.dock.popover_width.max(220.0));

                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("History")
                                .size(13.0)
                                .strong()
                                .color(th.text_color()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").on_hover_text("Close (F2)").clicked() {
                                *open = false;
                            }
                            if ui
                                .small_button("Copy")
                                .on_hover_text("Copy the log as plain text")
                                .clicked()
                            {
                                ctx.copy_text(plain_text_log(rows));
                            }
                        });
                    });
                    ui.add_space(4.0);

                    egui::ScrollArea::vertical()
                        .max_height(max_height)
                        .show(ui, |ui| {
                            if rows.is_empty() {
                                ui.label(
                                    RichText::new("No commands yet")
                                        .small()
                                        .color(th.muted_text_color()),
                                );
                            }
                            for row in rows {
                                history_row(ui, row, &palette, th);
                            }
                        });
                });
        });
}

fn history_row(
    ui: &mut egui::Ui,
    row: &HistoryRow,
    palette: &Palette,
    th: &crate::tokens::DockThemeTokens,
) {
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(&row.name).size(12.0).color(th.text_color()));
        if !row.author.is_empty() {
            author_chip(ui, &row.author, palette.portal);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(&row.ago)
                    .size(10.0)
                    .color(th.muted_text_color()),
            );
        });
    });
    if !row.detail.is_empty() {
        ui.label(
            RichText::new(&row.detail)
                .size(10.5)
                .color(th.muted_text_color()),
        );
    }
    ui.add_space(2.0);
}

/// Small read-only agent-name chip, matching the `widgets::chip` look
/// (rounded pill, tinted fill) but non-interactive.
fn author_chip(ui: &mut egui::Ui, name: &str, base: Color32) {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(
            base.r(),
            base.g(),
            base.b(),
            60,
        ))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(name).size(9.5).color(base));
        });
}

/// The plain-text log placed on the clipboard by the Copy button.
fn plain_text_log(rows: &[HistoryRow]) -> String {
    let mut out = String::new();
    for row in rows {
        let author = if row.author.is_empty() {
            "human"
        } else {
            row.author.as_str()
        };
        out.push_str(&format!(
            "{} — {} [{}] ({})\n",
            row.name, row.detail, author, row.ago
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_log_marks_human_and_agents() {
        let rows = [
            HistoryRow {
                name: "Move nodes".into(),
                detail: "3 nodes".into(),
                author: String::new(),
                ago: "2 m ago".into(),
            },
            HistoryRow {
                name: "Add frame".into(),
                detail: "Frame A".into(),
                author: "layout-bot".into(),
                ago: "5 m ago".into(),
            },
        ];
        let log = plain_text_log(&rows);
        assert!(log.contains("Move nodes — 3 nodes [human] (2 m ago)"));
        assert!(log.contains("Add frame — Frame A [layout-bot] (5 m ago)"));
        assert_eq!(log.lines().count(), 2);
    }
}
