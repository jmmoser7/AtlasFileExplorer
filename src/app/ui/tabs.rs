//! Browser-style tab strip — the only UI above the tab workspace.

use super::super::AtlasApp;
use super::widgets::trunc;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Sense, Stroke, StrokeKind, Ui, Vec2,
};

impl AtlasApp {
    pub(super) fn tab_strip(&mut self, ui: &mut Ui) {
        let palette = self.palette();
        enum TabAction {
            Switch(usize),
            Close(usize),
            New,
            OpenPicker,
        }
        let mut action: Option<TabAction> = None;

        for i in 0..self.tabs.len() {
            let active = i == self.active_tab;
            let closable = self.tabs.len() > 1 || self.tabs[i].root.is_some();
            let is_empty = self.tabs[i].root.is_none();
            let title = if is_empty && active {
                "Select a folder…".to_string()
            } else {
                trunc(&self.tabs[i].title(), 18)
            };

            let font = FontId::proportional(12.5);
            let text_w = ui
                .painter()
                .layout_no_wrap(title.clone(), font.clone(), Color32::WHITE)
                .size()
                .x;
            let w = text_w + 26.0 + if closable { 16.0 } else { 0.0 };
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 24.0), Sense::click());
            let hovered = resp.hovered();

            let fill = if active {
                palette.card_hover
            } else if hovered {
                palette.card
            } else {
                palette.card.gamma_multiply(0.6)
            };
            let cr = CornerRadius {
                nw: 6,
                ne: 6,
                sw: 0,
                se: 0,
            };
            ui.painter().rect_filled(rect, cr, fill);
            if active {
                ui.painter().rect_stroke(
                    rect,
                    cr,
                    Stroke::new(1.0, palette.accent.gamma_multiply(0.7)),
                    StrokeKind::Inside,
                );
            }
            let text_color = if active { palette.ink } else { palette.sub };
            ui.painter().text(
                rect.left_center() + Vec2::new(10.0, 0.0),
                Align2::LEFT_CENTER,
                title,
                font,
                text_color,
            );

            if closable {
                let cx = egui::Rect::from_center_size(
                    rect.right_center() - Vec2::new(12.0, 0.0),
                    Vec2::splat(14.0),
                );
                let over_x = ui
                    .ctx()
                    .pointer_latest_pos()
                    .map(|p| cx.contains(p))
                    .unwrap_or(false);
                if hovered || active {
                    ui.painter().text(
                        cx.center(),
                        Align2::CENTER_CENTER,
                        "×",
                        FontId::proportional(13.0),
                        if over_x { palette.ink } else { palette.sub },
                    );
                }
                if resp.clicked() && over_x {
                    action = Some(TabAction::Close(i));
                } else if resp.clicked() {
                    action = Some(if active && is_empty {
                        TabAction::OpenPicker
                    } else {
                        TabAction::Switch(i)
                    });
                }
            } else if resp.clicked() {
                action = Some(if active && is_empty {
                    TabAction::OpenPicker
                } else {
                    TabAction::Switch(i)
                });
            }
            if let Some(root) = &self.tabs[i].root {
                resp.on_hover_text(root.to_string_lossy());
            } else {
                resp.on_hover_text("Click to choose a folder for this tab");
            }
            ui.add_space(2.0);
        }

        let (prect, presp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
        if presp.hovered() {
            ui.painter()
                .circle_filled(prect.center(), 10.0, palette.card_hover);
        }
        ui.painter().text(
            prect.center(),
            Align2::CENTER_CENTER,
            "+",
            FontId::proportional(15.0),
            palette.sub,
        );
        if presp.on_hover_text("New tab").clicked() {
            action = Some(TabAction::New);
        }

        match action {
            Some(TabAction::Switch(i)) => self.switch_tab(i),
            Some(TabAction::Close(i)) => self.close_tab(i),
            Some(TabAction::New) => self.new_tab(),
            Some(TabAction::OpenPicker) => self.open_folder_dialog(),
            None => {}
        }
    }
}

pub fn top_bar(app: &mut AtlasApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("topbar").show(ctx, |ui| {
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("File Atlas").size(16.0));
            ui.add_space(10.0);
            app.tab_strip(ui);
            if app.picker_rx.is_some() {
                ui.spinner();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Journal: kept in code but hidden from chrome until its
                // permanent home is decided (likely a tools-rail panel).
                ui.add_enabled_ui(app.journal.can_redo(), |ui| {
                    if ui.button("Redo").clicked() {
                        app.redo();
                    }
                });
                ui.add_enabled_ui(app.journal.can_undo(), |ui| {
                    if ui.button("Undo").clicked() {
                        app.undo();
                    }
                });
            });
        });
        ui.add_space(4.0);
    });
}
