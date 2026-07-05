//! Browser-style top chrome — title row + tab strip.
//!
//! Data-driven so every app in the ecosystem paints identical tabs: the app
//! supplies [`TabSpec`]s and reacts to the returned [`TabAction`] /
//! [`TopBarResponse`]. All geometry and colors live here; apps must not
//! paint their own tab chrome.

use crate::theme::Palette;
use crate::widgets::trunc;
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
/// Truncation width for tab titles.
const TAB_TITLE_CHARS: usize = 18;

#[derive(Clone, Copy)]
pub struct TabChromeColors {
    pub bar: Color32,
    pub inactive: Color32,
    pub inactive_hover: Color32,
    pub active: Color32,
    pub divider: Color32,
}

impl TabChromeColors {
    pub fn from_palette(p: &Palette) -> Self {
        if p.bg.r() > 128 {
            Self {
                bar: Color32::from_rgb(0xe4, 0xe7, 0xeb),
                inactive: Color32::from_rgb(0xd2, 0xd6, 0xdc),
                inactive_hover: Color32::from_rgb(0xea, 0xec, 0xf0),
                active: p.bg,
                divider: p.border_strong.gamma_multiply(0.55),
            }
        } else {
            Self {
                bar: Color32::from_rgb(0x14, 0x17, 0x1b),
                inactive: Color32::from_rgb(0x1a, 0x1e, 0x24),
                inactive_hover: Color32::from_rgb(0x22, 0x27, 0x2e),
                active: p.bg,
                divider: p.border.gamma_multiply(0.65),
            }
        }
    }
}

/// What the app wants shown for one tab.
pub struct TabSpec {
    pub title: String,
    pub tooltip: String,
    pub closable: bool,
    /// Empty tabs invite content selection: clicking the active empty tab
    /// yields [`TabAction::ActivateEmpty`] instead of a switch.
    pub is_empty: bool,
}

pub enum TabAction {
    Switch(usize),
    Close(usize),
    New,
    /// Active empty tab clicked — the app opens its "choose content" flow
    /// (folder picker in File Atlas, workbook picker in Slate).
    ActivateEmpty,
}

struct TabSlot {
    paint: Rect,
    active: bool,
    hovered: bool,
    closable: bool,
    title: String,
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

/// Renders the tab strip row; returns the user's action, if any.
pub fn tab_strip(
    ui: &mut Ui,
    palette: &Palette,
    tabs: &[TabSpec],
    active_tab: usize,
) -> Option<TabAction> {
    let colors = TabChromeColors::from_palette(palette);
    let mut action: Option<TabAction> = None;
    let mut slots: Vec<TabSlot> = Vec::new();

    ui.set_min_height(TAB_BAR_H);
    ui.with_layout(Layout::left_to_right(Align::BOTTOM), |ui| {
        for (i, spec) in tabs.iter().enumerate() {
            let active = i == active_tab;
            let title = trunc(&spec.title, TAB_TITLE_CHARS);

            let font = FontId::proportional(12.5);
            let text_w = ui
                .painter()
                .layout_no_wrap(title.clone(), font.clone(), Color32::WHITE)
                .size()
                .x;
            let w = text_w + TAB_H_PAD * 2.0 + if spec.closable { TAB_CLOSE_W } else { 0.0 };
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
                paint,
                active,
                hovered,
                closable: spec.closable,
                title,
            });

            if spec.closable {
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
                    action = Some(if active && spec.is_empty {
                        TabAction::ActivateEmpty
                    } else {
                        TabAction::Switch(i)
                    });
                }
            } else if resp.clicked() {
                action = Some(if active && spec.is_empty {
                    TabAction::ActivateEmpty
                } else {
                    TabAction::Switch(i)
                });
            }
            resp.on_hover_text(spec.tooltip.clone());
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

    action
}

/// State the app feeds into [`top_bar`] each frame.
pub struct TopBarModel<'a> {
    /// App name shown in the title row ("File Atlas", "Slate", …).
    pub app_title: &'a str,
    /// Show a spinner next to the title (e.g. a picker dialog is open).
    pub busy: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub tabs: &'a [TabSpec],
    pub active_tab: usize,
}

/// What the user did in the top bar this frame.
#[derive(Default)]
pub struct TopBarResponse {
    pub undo_clicked: bool,
    pub redo_clicked: bool,
    pub tab_action: Option<TabAction>,
}

impl TopBarResponse {
    fn none() -> Self {
        Self {
            undo_clicked: false,
            redo_clicked: false,
            tab_action: None,
        }
    }
}

/// Title row + undo/redo + tab strip. Identical chrome for every Atlas app.
pub fn top_bar(ctx: &egui::Context, palette: &Palette, model: TopBarModel<'_>) -> TopBarResponse {
    let colors = TabChromeColors::from_palette(palette);
    let mut out = TopBarResponse::none();

    // Salt the panel id with the app title: in linked sessions two apps share
    // one egui Context (two viewports), and panel state must not collide.
    egui::TopBottomPanel::top(egui::Id::new(("topbar", model.app_title)))
        .frame(egui::Frame::new().fill(colors.bar).inner_margin(Margin {
            left: 10,
            right: 10,
            top: 6,
            bottom: 0,
        }))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new(model.app_title)
                        .size(16.0)
                        .color(palette.ink),
                );
                if model.busy {
                    ui.add_space(8.0);
                    ui.spinner();
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Journal: kept in code but hidden from chrome until its
                    // permanent home is decided (likely a tools-rail panel).
                    ui.add_enabled_ui(model.can_redo, |ui| {
                        if ui.button("Redo").clicked() {
                            out.redo_clicked = true;
                        }
                    });
                    ui.add_enabled_ui(model.can_undo, |ui| {
                        if ui.button("Undo").clicked() {
                            out.undo_clicked = true;
                        }
                    });
                });
            });

            ui.add_space(4.0);
            out.tab_action = tab_strip(ui, palette, model.tabs, model.active_tab);
        });

    out
}
