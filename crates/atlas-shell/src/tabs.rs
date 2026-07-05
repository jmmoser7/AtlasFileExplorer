//! Browser-style tab strip — the only UI above the tab workspace.
//!
//! Data-driven so every app in the ecosystem paints identical tabs: the app
//! supplies [`TabSpec`]s and reacts to the returned [`TabAction`]. All
//! geometry and colors live here; apps must not paint their own tab chrome.
//!
//! The bar is tabs-only: no title row, no buttons. Undo/redo lives on
//! Ctrl+Z / Ctrl+Y in each app's hotkeys.

use crate::theme::Palette;
use crate::widgets::trunc;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Layout, Margin, Pos2, Rect,
    Sense, Stroke, StrokeKind, Ui, Vec2,
};

/// Height of the tab-bar row (70% of the original 34px strip).
const TAB_BAR_H: f32 = 24.0;
const TAB_TOP_RADIUS: u8 = 6;
/// Radius of the concave shoulder curves on an active tab.
const TAB_SHOULDER_R: f32 = 4.0;
const TAB_H_PAD: f32 = 10.0;
const TAB_CLOSE_W: f32 = 16.0;
/// Width of an empty tab before content is chosen (no label text).
const TAB_EMPTY_W: f32 = 88.0;
const TAB_WIDTH_SCALE: f32 = 2.0;
/// Truncation width for tab titles.
const TAB_TITLE_CHARS: usize = 36;

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
    /// Empty tabs render without a label and invite content selection:
    /// clicking the active empty tab yields [`TabAction::ActivateEmpty`]
    /// instead of a switch.
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
    /// `None` for empty tabs (blank label until content is chosen).
    title: Option<String>,
}

fn tab_paint_rect(rect: Rect, active: bool, bar_h: f32) -> Rect {
    let mut h = bar_h;
    if active {
        // Overlap the canvas by 1px so the active tab reads as connected.
        h += 1.0;
    }
    Rect::from_min_size(
        Pos2::new(rect.min.x, rect.max.y - h),
        Vec2::new(rect.width(), h),
    )
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

/// Paint the hoverable close "×" for one tab slot.
fn paint_close_x(ui: &Ui, painter: &egui::Painter, slot: &TabSlot, palette: &Palette) {
    let cx = egui::Rect::from_center_size(
        Pos2::new(slot.paint.right() - TAB_H_PAD, slot.paint.center().y),
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

/// Renders the tab strip row; returns the user's action, if any.
pub fn tab_strip(
    ui: &mut Ui,
    palette: &Palette,
    tabs: &[TabSpec],
    active_tab: usize,
    busy: bool,
) -> Option<TabAction> {
    let colors = TabChromeColors::from_palette(palette);
    let mut action: Option<TabAction> = None;
    let mut slots: Vec<TabSlot> = Vec::new();

    ui.set_min_height(TAB_BAR_H);
    ui.with_layout(Layout::left_to_right(Align::BOTTOM), |ui| {
        for (i, spec) in tabs.iter().enumerate() {
            let active = i == active_tab;
            let title = if spec.is_empty {
                None
            } else {
                Some(trunc(&spec.title, TAB_TITLE_CHARS))
            };

            let font = FontId::proportional(12.5);
            let text_w = title
                .as_ref()
                .map(|t| {
                    ui.painter()
                        .layout_no_wrap(t.clone(), font.clone(), Color32::WHITE)
                        .size()
                        .x
                })
                .unwrap_or(0.0);
            let base_w = if spec.is_empty {
                TAB_EMPTY_W + if spec.closable { TAB_CLOSE_W } else { 0.0 }
            } else {
                text_w + TAB_H_PAD * 2.0 + if spec.closable { TAB_CLOSE_W } else { 0.0 }
            };
            let w = base_w * TAB_WIDTH_SCALE;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, TAB_BAR_H), Sense::click());
            let hovered = resp.hovered() && !active;
            let paint = tab_paint_rect(rect, active, TAB_BAR_H);

            slots.push(TabSlot {
                paint,
                active,
                hovered,
                closable: spec.closable,
                title,
            });

            if spec.closable {
                let cx = egui::Rect::from_center_size(
                    Pos2::new(rect.right_center().x - 12.0, paint.center().y),
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

        ui.add_space(1.0);
        let (prect, presp) = ui.allocate_exact_size(Vec2::new(28.0, TAB_BAR_H), Sense::click());
        let presp = presp.on_hover_cursor(CursorIcon::PointingHand);
        let plus_center = prect.center();
        let plus_hover = presp.hovered();
        if plus_hover {
            ui.painter()
                .circle_filled(plus_center, 9.0, colors.inactive_hover);
        }
        ui.painter().text(
            plus_center,
            Align2::CENTER_CENTER,
            "+",
            FontId::proportional(13.0),
            if plus_hover { palette.ink } else { palette.sub },
        );
        if presp.on_hover_text("New tab").clicked() {
            action = Some(TabAction::New);
        }

        if busy {
            ui.add_space(6.0);
            ui.spinner();
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
        let Some(title) = &slot.title else {
            // Empty tab: blank label, just the close affordance.
            if slot.closable && (slot.hovered || slot.active) {
                paint_close_x(ui, &painter, slot, palette);
            }
            continue;
        };

        let text_color = if slot.active {
            palette.ink
        } else if slot.hovered {
            palette.ink.gamma_multiply(0.92)
        } else {
            palette.sub
        };
        painter.text(
            Pos2::new(slot.paint.left() + TAB_H_PAD, slot.paint.center().y),
            Align2::LEFT_CENTER,
            title.clone(),
            FontId::proportional(12.5),
            text_color,
        );

        if slot.closable && (slot.hovered || slot.active) {
            paint_close_x(ui, &painter, slot, palette);
        }
    }

    action
}

/// State the app feeds into [`top_bar`] each frame.
pub struct TopBarModel<'a> {
    /// Salts the panel id — in linked sessions two apps share one egui
    /// Context (two viewports), and panel state must not collide. Not
    /// rendered: the bar is tabs-only.
    pub app_title: &'a str,
    /// Show a spinner after the tabs (e.g. a picker dialog is open).
    pub busy: bool,
    pub tabs: &'a [TabSpec],
    pub active_tab: usize,
}

/// Tabs-only top bar. Identical chrome for every Atlas app; returns the
/// user's tab action, if any.
pub fn top_bar(
    ctx: &egui::Context,
    palette: &Palette,
    model: TopBarModel<'_>,
) -> Option<TabAction> {
    let colors = TabChromeColors::from_palette(palette);
    let mut action = None;

    egui::TopBottomPanel::top(egui::Id::new(("topbar", model.app_title)))
        .frame(egui::Frame::new().fill(colors.bar).inner_margin(Margin {
            left: 8,
            right: 8,
            top: 2,
            bottom: 0,
        }))
        .show(ctx, |ui| {
            action = tab_strip(ui, palette, model.tabs, model.active_tab, model.busy);
        });

    action
}
