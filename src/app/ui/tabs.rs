//! Browser-style tab strip — the only UI above the tab workspace.

use super::super::AtlasApp;
use super::widgets::trunc;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Layout, Margin, Pos2, Rect,
    Sense, Stroke, StrokeKind, Ui, Vec2,
};

/// Height of the recessed tab-bar row beneath the title.
const TAB_BAR_H: f32 = 34.0;
/// Resting height for background tabs.
const TAB_INACTIVE_H: f32 = 26.0;
/// Height when an inactive tab is hovered — it "reaches" upward.
const TAB_HOVER_H: f32 = 31.0;
const TAB_TOP_RADIUS: u8 = 8;
/// Radius of the concave shoulder curves on an active tab.
const TAB_SHOULDER_R: f32 = 6.0;
const TAB_H_PAD: f32 = 10.0;
const TAB_CLOSE_W: f32 = 16.0;

#[derive(Clone, Copy)]
struct TabChromeColors {
    bar: Color32,
    inactive: Color32,
    inactive_hover: Color32,
    active: Color32,
    divider: Color32,
}

impl TabChromeColors {
    fn from_palette(bg: Color32, border: Color32, border_strong: Color32) -> Self {
        if bg.r() > 128 {
            Self {
                bar: Color32::from_rgb(0xe4, 0xe7, 0xeb),
                inactive: Color32::from_rgb(0xd2, 0xd6, 0xdc),
                inactive_hover: Color32::from_rgb(0xea, 0xec, 0xf0),
                active: bg,
                divider: border_strong.gamma_multiply(0.55),
            }
        } else {
            Self {
                bar: Color32::from_rgb(0x14, 0x17, 0x1b),
                inactive: Color32::from_rgb(0x1a, 0x1e, 0x24),
                inactive_hover: Color32::from_rgb(0x22, 0x27, 0x2e),
                active: bg,
                divider: border.gamma_multiply(0.65),
            }
        }
    }
}

struct TabSlot {
    index: usize,
    rect: Rect,
    paint: Rect,
    active: bool,
    hovered: bool,
    closable: bool,
    is_empty: bool,
    title: String,
    tooltip: String,
}

fn tab_visual(active: bool, hovered: bool, bar_h: f32) -> (f32, f32) {
    if active {
        (bar_h, 0.0)
    } else if hovered {
        (TAB_HOVER_H, 1.0)
    } else {
        (TAB_INACTIVE_H, 3.0)
    }
}

fn paint_chrome_tab(
    painter: &egui::Painter,
    rect: Rect,
    fill: Color32,
    active: bool,
    colors: TabChromeColors,
) {
    let cr = CornerRadius {
        nw: TAB_TOP_RADIUS,
        ne: TAB_TOP_RADIUS,
        sw: 0,
        se: 0,
    };
    painter.rect_filled(rect, cr, fill);

    if active {
        // Concave shoulders where the active tab meets the tab bar.
        painter.circle_filled(
            Pos2::new(rect.left(), rect.bottom()),
            TAB_SHOULDER_R,
            colors.bar,
        );
        painter.circle_filled(
            Pos2::new(rect.right(), rect.bottom()),
            TAB_SHOULDER_R,
            colors.bar,
        );
        // Soft top highlight so the tab reads as raised.
        painter.rect_stroke(
            rect,
            cr,
            Stroke::new(1.0, Color32::from_white_alpha(18)),
            StrokeKind::Inside,
        );
    } else {
        painter.line_segment(
            [rect.right_top(), rect.right_bottom()],
            Stroke::new(1.0, colors.divider),
        );
    }
}

impl AtlasApp {
    pub(super) fn tab_strip(&mut self, ui: &mut Ui) {
        let palette = self.palette();
        let colors =
            TabChromeColors::from_palette(palette.bg, palette.border, palette.border_strong);
        enum TabAction {
            Switch(usize),
            Close(usize),
            New,
            OpenPicker,
        }
        let mut action: Option<TabAction> = None;
        let mut slots: Vec<TabSlot> = Vec::new();

        ui.set_min_height(TAB_BAR_H);
        ui.with_layout(Layout::left_to_right(Align::BOTTOM), |ui| {
            for i in 0..self.tabs.len() {
                let active = i == self.active_tab;
                let closable = self.tabs.len() > 1 || self.tabs[i].root.is_some();
                let is_empty = self.tabs[i].root.is_none();
                let title = if is_empty && active {
                    "Select a folder…".to_string()
                } else {
                    trunc(&self.tabs[i].title(), 18)
                };
                let tooltip = if let Some(root) = &self.tabs[i].root {
                    root.to_string_lossy().into_owned()
                } else {
                    "Click to choose a folder for this tab".to_string()
                };

                let font = FontId::proportional(12.5);
                let text_w = ui
                    .painter()
                    .layout_no_wrap(title.clone(), font.clone(), Color32::WHITE)
                    .size()
                    .x;
                let w = text_w + TAB_H_PAD * 2.0 + if closable { TAB_CLOSE_W } else { 0.0 };
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(w.max(56.0), TAB_BAR_H), Sense::click());
                let hovered = resp.hovered() && !active;
                let (mut h, bottom_inset) = tab_visual(active, hovered, TAB_BAR_H);
                if active {
                    // Overlap the canvas by 1px so the active tab reads as connected.
                    h += 1.0;
                }
                let paint = Rect::from_min_size(
                    Pos2::new(rect.min.x, rect.max.y - bottom_inset - h),
                    Vec2::new(rect.width(), h),
                );

                slots.push(TabSlot {
                    index: i,
                    rect,
                    paint,
                    active,
                    hovered,
                    closable,
                    is_empty,
                    title,
                    tooltip: tooltip.clone(),
                });

                if closable {
                    let cx = egui::Rect::from_center_size(
                        rect.right_center() - Vec2::new(12.0, bottom_inset + h * 0.5),
                        Vec2::splat(14.0),
                    );
                    let over_x = ui
                        .ctx()
                        .pointer_latest_pos()
                        .map(|p| cx.contains(p))
                        .unwrap_or(false);
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
                resp.on_hover_text(tooltip);
            }

            ui.add_space(4.0);
            let (prect, presp) = ui.allocate_exact_size(Vec2::new(28.0, TAB_BAR_H), Sense::click());
            let presp = presp.on_hover_cursor(CursorIcon::PointingHand);
            let plus_center = Pos2::new(prect.center().x, prect.max.y - TAB_INACTIVE_H * 0.5 - 3.0);
            let plus_hover = presp.hovered();
            if plus_hover {
                ui.painter()
                    .circle_filled(plus_center, 11.0, colors.inactive_hover);
            }
            ui.painter().text(
                plus_center,
                Align2::CENTER_CENTER,
                "+",
                FontId::proportional(15.0),
                if plus_hover { palette.ink } else { palette.sub },
            );
            if presp.on_hover_text("New tab").clicked() {
                action = Some(TabAction::New);
            }
        });

        let painter = ui.painter().clone();
        let inactive: Vec<_> = slots.iter().filter(|s| !s.active).collect();
        let active = slots.iter().find(|s| s.active);

        for slot in &inactive {
            let fill = if slot.hovered {
                colors.inactive_hover
            } else {
                colors.inactive
            };
            paint_chrome_tab(&painter, slot.paint, fill, false, colors);
        }
        if let Some(slot) = active {
            paint_chrome_tab(&painter, slot.paint, colors.active, true, colors);
        }

        for slot in &slots {
            let font = FontId::proportional(12.5);
            let text_y = slot.paint.center().y;
            let text_color = if slot.active {
                palette.ink
            } else if slot.hovered {
                palette.ink.gamma_multiply(0.92)
            } else {
                palette.sub
            };
            painter.text(
                Pos2::new(slot.paint.left() + TAB_H_PAD, text_y),
                Align2::LEFT_CENTER,
                slot.title.clone(),
                font.clone(),
                text_color,
            );

            if slot.closable && (slot.hovered || slot.active) {
                let cx = egui::Rect::from_center_size(
                    Pos2::new(slot.paint.right() - TAB_H_PAD, text_y),
                    Vec2::splat(14.0),
                );
                let over_x = ui
                    .ctx()
                    .pointer_latest_pos()
                    .map(|p| cx.contains(p))
                    .unwrap_or(false);
                painter.text(
                    cx.center(),
                    Align2::CENTER_CENTER,
                    "×",
                    FontId::proportional(13.0),
                    if over_x { palette.ink } else { palette.sub },
                );
            }
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
    let palette = app.palette();
    let colors = TabChromeColors::from_palette(palette.bg, palette.border, palette.border_strong);

    egui::TopBottomPanel::top("topbar")
        .frame(egui::Frame::new().fill(colors.bar).inner_margin(Margin {
            left: 10,
            right: 10,
            top: 6,
            bottom: 0,
        }))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new("File Atlas")
                        .size(16.0)
                        .color(palette.ink),
                );
                if app.picker_rx.is_some() {
                    ui.add_space(8.0);
                    ui.spinner();
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
            app.tab_strip(ui);
        });
}
