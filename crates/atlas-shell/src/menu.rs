//! Shared dropdown and right-click menu language (see `MENUS.md`).
//!
//! One look for every menu in File Atlas and Slate: filleted panel, no
//! border, soft shadow, inset section rules, icon + label + optional
//! chevron. Apps build rows through this module; they must not invent a
//! second menu chrome.

use crate::tokens::{self, MenuThemeTokens, MenuTokens};
use eframe::egui::{
    self, text::LayoutJob, Align2, Color32, CornerRadius, FontId, Frame, Margin, Pos2, Rect,
    RichText, Sense, Shadow, Stroke, StrokeKind, TextFormat, Ui, Vec2,
};

/// Line-art glyph that sits to the left of a menu label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MenuIcon {
    #[default]
    None,
    File,
    Folder,
    Open,
    Copy,
    Tag,
    Link,
    Details,
    Rename,
    NewFolder,
    Trash,
    TrashX,
    Duplicate,
    Front,
    Back,
    Group,
    Ungroup,
    Lock,
    Hide,
    Maximize,
    Restore,
    Tab,
    Url,
    Paste,
    Cursor,
    Chat,
    Enter,
    Check,
    Settings,
    View,
    Image,
    Search,
}

/// One painted row. Prefer the `item*` helpers; use this for mixed states.
pub struct Row<'a> {
    pub icon: MenuIcon,
    pub label: &'a str,
    pub shortcut: Option<&'a str>,
    pub chevron: bool,
    pub danger: bool,
    pub selected: bool,
    pub enabled: bool,
    pub checked: Option<bool>,
    pub swatch: Option<Color32>,
}

impl<'a> Row<'a> {
    pub fn new(icon: MenuIcon, label: &'a str) -> Self {
        Self {
            icon,
            label,
            shortcut: None,
            chevron: false,
            danger: false,
            selected: false,
            enabled: true,
            checked: None,
            swatch: None,
        }
    }

    pub fn shortcut(mut self, shortcut: &'a str) -> Self {
        if !shortcut.is_empty() {
            self.shortcut = Some(shortcut);
        }
        self
    }

    pub fn chevron(mut self) -> Self {
        self.chevron = true;
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }

    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }

    pub fn checked(mut self, on: bool) -> Self {
        self.checked = Some(on);
        self
    }

    pub fn swatch(mut self, color: Color32) -> Self {
        self.swatch = Some(color);
        self
    }
}

/// Apply the shared menu language to egui's built-in menus, combo boxes,
/// and `Frame::popup` / `Frame::menu`. Call after [`crate::theme::Palette`]
/// visuals so those do not overwrite the panel chrome.
///
/// Only menu-panel fields are touched. Widget / button styling stays on
/// [`crate::theme`] so inspector buttons and the rest of the chrome do not
/// pick up menu hover fills.
pub fn apply_style(ctx: &egui::Context, dark: bool) {
    let tokens = tokens::current().menu;
    let theme = tokens.theme(dark);
    ctx.style_mut(|style| {
        let r = tokens.corner_radius.clamp(0.0, 255.0) as u8;
        style.visuals.menu_corner_radius = CornerRadius::same(r);
        style.visuals.popup_shadow = Shadow {
            offset: [
                tokens.shadow_offset_x.clamp(-127.0, 127.0) as i8,
                tokens.shadow_offset_y.clamp(-127.0, 127.0) as i8,
            ],
            blur: tokens.shadow_blur.clamp(0.0, 255.0) as u8,
            spread: tokens.shadow_spread.clamp(0.0, 255.0) as u8,
            color: Color32::from_black_alpha((tokens.shadow_opacity.clamp(0.0, 1.0) * 255.0) as u8),
        };
        let pad = tokens.panel_padding.clamp(0.0, 127.0) as i8;
        style.spacing.menu_margin = Margin::same(pad);
        style.visuals.window_stroke = if tokens.border_width <= 0.01 {
            Stroke::NONE
        } else {
            Stroke::new(tokens.border_width, theme.border_color())
        };
        style.visuals.window_fill = theme.fill_color();
    });
}

/// Local styling for an egui-built menu body (`menu_button`, `context_menu`).
/// Safe to call inside the menu closure — it does not leak to the rest of
/// the frame.
pub fn prepare(ui: &mut Ui, dark: bool) {
    let tokens = tokens();
    let theme = tokens.theme(dark);
    ui.set_min_width(tokens.min_width);
    ui.spacing_mut().item_spacing = Vec2::new(0.0, tokens.row_gap);
    ui.spacing_mut().button_padding = Vec2::new(
        tokens.row_pad_x,
        ((tokens.row_height - tokens.text_size) * 0.5).max(4.0),
    );
    let r = tokens.hover_radius.clamp(0.0, 255.0) as u8;
    ui.visuals_mut().widgets.hovered.weak_bg_fill = theme.hover_color();
    ui.visuals_mut().widgets.active.weak_bg_fill = theme.hover_color();
    ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::NONE;
    ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
    ui.visuals_mut().widgets.active.bg_stroke = Stroke::NONE;
    ui.visuals_mut().widgets.hovered.corner_radius = CornerRadius::same(r);
    ui.visuals_mut().widgets.inactive.corner_radius = CornerRadius::same(r);
    ui.visuals_mut().widgets.active.corner_radius = CornerRadius::same(r);
    ui.visuals_mut().widgets.inactive.fg_stroke.color = theme.text_color();
    ui.visuals_mut().widgets.hovered.fg_stroke.color = theme.text_color();
    ui.visuals_mut().widgets.active.fg_stroke.color = theme.text_color();
    ui.visuals_mut().widgets.noninteractive.fg_stroke.color = theme.muted_color();
    ui.visuals_mut().button_frame = false;
    ui.visuals_mut().override_text_color = Some(theme.text_color());
}

/// Frame for a hand-built menu panel (right-click, portal flyout).
pub fn frame(dark: bool) -> Frame {
    let tokens = tokens::current().menu;
    let theme = tokens.theme(dark);
    let stroke = if tokens.border_width <= 0.01 {
        Stroke::NONE
    } else {
        Stroke::new(tokens.border_width, theme.border_color())
    };
    Frame::new()
        .fill(theme.fill_color())
        .stroke(stroke)
        .corner_radius(CornerRadius::same(
            tokens.corner_radius.clamp(0.0, 255.0) as u8
        ))
        .shadow(Shadow {
            offset: [
                tokens.shadow_offset_x.clamp(-127.0, 127.0) as i8,
                tokens.shadow_offset_y.clamp(-127.0, 127.0) as i8,
            ],
            blur: tokens.shadow_blur.clamp(0.0, 255.0) as u8,
            spread: tokens.shadow_spread.clamp(0.0, 255.0) as u8,
            color: Color32::from_black_alpha((tokens.shadow_opacity.clamp(0.0, 1.0) * 255.0) as u8),
        })
        .inner_margin(Margin::same(tokens.panel_padding.clamp(0.0, 127.0) as i8))
}

pub fn tokens() -> MenuTokens {
    tokens::current().menu
}

pub fn theme(dark: bool) -> MenuThemeTokens {
    tokens().theme(dark).clone()
}

pub fn is_dark(ui: &Ui) -> bool {
    ui.visuals().dark_mode
}

/// Quiet caption at the top of a menu ("3 file(s)", app name).
pub fn heading(ui: &mut Ui, text: impl AsRef<str>, dark: bool) {
    let tokens = tokens();
    let theme = tokens.theme(dark);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), tokens.row_height * 0.72),
        Sense::hover(),
    );
    paint_label(
        ui,
        Pos2::new(rect.left() + tokens.row_pad_x, rect.center().y),
        text.as_ref(),
        tokens.text_size * 0.85,
        theme.muted_color(),
        tokens.letter_spacing,
        Align2::LEFT_CENTER,
    );
}

/// Wrapping muted note (hints, empty states).
pub fn note(ui: &mut Ui, text: impl AsRef<str>, dark: bool) {
    let tokens = tokens();
    let theme = tokens.theme(dark);
    ui.add(
        egui::Label::new(
            RichText::new(text.as_ref())
                .size(tokens.shortcut_text_size)
                .color(theme.muted_color()),
        )
        .wrap(),
    );
}

/// Inset hairline between sections — does not touch the panel edge.
pub fn separator(ui: &mut Ui, dark: bool) {
    let tokens = tokens();
    let theme = tokens.theme(dark);
    ui.add_space(tokens.divider_gap * 0.5);
    let y = ui.cursor().top();
    let left = ui.max_rect().left() + tokens.divider_inset;
    let right = ui.max_rect().right() - tokens.divider_inset;
    if right > left {
        ui.painter().line_segment(
            [Pos2::new(left, y), Pos2::new(right, y)],
            Stroke::new(tokens.divider_thickness, theme.divider_color()),
        );
    }
    ui.add_space(tokens.divider_gap * 0.5 + tokens.divider_thickness);
}

pub fn item(ui: &mut Ui, icon: MenuIcon, label: impl AsRef<str>, dark: bool) -> egui::Response {
    row(ui, Row::new(icon, label.as_ref()), dark)
}

pub fn item_shortcut(
    ui: &mut Ui,
    icon: MenuIcon,
    label: impl AsRef<str>,
    shortcut: impl AsRef<str>,
    dark: bool,
) -> egui::Response {
    row(
        ui,
        Row::new(icon, label.as_ref()).shortcut(shortcut.as_ref()),
        dark,
    )
}

pub fn item_danger(
    ui: &mut Ui,
    icon: MenuIcon,
    label: impl AsRef<str>,
    dark: bool,
) -> egui::Response {
    row(ui, Row::new(icon, label.as_ref()).danger(), dark)
}

pub fn submenu(
    ui: &mut Ui,
    icon: MenuIcon,
    label: impl AsRef<str>,
    selected: bool,
    dark: bool,
) -> egui::Response {
    row(
        ui,
        Row::new(icon, label.as_ref()).chevron().selected(selected),
        dark,
    )
}

pub fn toggle(ui: &mut Ui, on: bool, label: impl AsRef<str>, dark: bool) -> egui::Response {
    let icon = if on { MenuIcon::Check } else { MenuIcon::None };
    row(ui, Row::new(icon, label.as_ref()).checked(on), dark)
}

pub fn item_swatch(
    ui: &mut Ui,
    color: Color32,
    label: impl AsRef<str>,
    selected: bool,
    dark: bool,
) -> egui::Response {
    row(
        ui,
        Row::new(MenuIcon::None, label.as_ref())
            .swatch(color)
            .checked(selected),
        dark,
    )
}

pub fn row(ui: &mut Ui, spec: Row<'_>, dark: bool) -> egui::Response {
    let tokens = tokens();
    let theme = tokens.theme(dark);
    ui.add_space(tokens.row_gap);
    let sense = if spec.enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), tokens.row_height), sense);
    if spec.enabled && (response.hovered() || spec.selected) {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(tokens.hover_radius.clamp(0.0, 255.0) as u8),
            theme.hover_color(),
        );
    }
    let fade = if spec.enabled { 1.0 } else { 0.38 };
    let color = if spec.danger {
        theme.danger_color()
    } else {
        theme.text_color()
    }
    .gamma_multiply(fade);
    let icon_color = if spec.danger {
        theme.danger_color()
    } else {
        theme.icon_color()
    }
    .gamma_multiply(fade);
    let icon_c = Pos2::new(
        rect.left() + tokens.row_pad_x + tokens.icon_size * 0.5,
        rect.center().y,
    );
    if let Some(swatch) = spec.swatch {
        ui.painter()
            .circle_filled(icon_c, tokens.icon_size * 0.28, swatch.gamma_multiply(fade));
        if spec.checked == Some(true) {
            ui.painter().circle_stroke(
                icon_c,
                tokens.icon_size * 0.42,
                Stroke::new(tokens.icon_stroke, icon_color),
            );
        }
    } else {
        let icon = if spec.checked == Some(true) {
            MenuIcon::Check
        } else {
            spec.icon
        };
        paint_icon(
            ui.painter(),
            icon,
            icon_c,
            tokens.icon_size,
            tokens.icon_stroke,
            icon_color,
        );
    }
    let text_x = rect.left() + tokens.row_pad_x + tokens.icon_size + tokens.icon_gap;
    paint_label(
        ui,
        Pos2::new(text_x, rect.center().y),
        spec.label,
        tokens.text_size,
        color,
        tokens.letter_spacing,
        Align2::LEFT_CENTER,
    );
    if let Some(shortcut) = spec.shortcut {
        paint_label(
            ui,
            Pos2::new(rect.right() - tokens.row_pad_x, rect.center().y),
            shortcut,
            tokens.shortcut_text_size,
            theme.muted_color().gamma_multiply(fade),
            0.0,
            Align2::RIGHT_CENTER,
        );
    } else if spec.chevron {
        paint_chevron(
            ui.painter(),
            Pos2::new(
                rect.right() - tokens.row_pad_x - tokens.chevron_size * 0.35,
                rect.center().y,
            ),
            tokens.chevron_size,
            tokens.icon_stroke,
            theme.muted_color().gamma_multiply(fade),
        );
    }
    response
}

fn paint_label(
    ui: &Ui,
    pos: Pos2,
    text: &str,
    size: f32,
    color: Color32,
    tracking: f32,
    align: Align2,
) {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: FontId::proportional(size),
            color,
            extra_letter_spacing: tracking,
            ..Default::default()
        },
    );
    let galley = ui.fonts(|f| f.layout_job(job));
    let rect = align.anchor_size(pos, galley.size());
    ui.painter().galley(rect.min, galley, color);
}

fn paint_chevron(painter: &egui::Painter, c: Pos2, size: f32, stroke: f32, color: Color32) {
    let s = size * 0.28;
    painter.line_segment(
        [c + Vec2::new(-s, -s * 1.4), c + Vec2::new(s, 0.0)],
        Stroke::new(stroke, color),
    );
    painter.line_segment(
        [c + Vec2::new(s, 0.0), c + Vec2::new(-s, s * 1.4)],
        Stroke::new(stroke, color),
    );
}

fn paint_icon(
    painter: &egui::Painter,
    icon: MenuIcon,
    c: Pos2,
    size: f32,
    stroke_w: f32,
    color: Color32,
) {
    if matches!(icon, MenuIcon::None) {
        return;
    }
    let s = Stroke::new(stroke_w, color);
    let h = size * 0.42;
    match icon {
        MenuIcon::None => {}
        MenuIcon::File => {
            let r = Rect::from_center_size(c + Vec2::new(-0.5, 0.0), Vec2::new(h * 1.15, h * 1.45));
            painter.rect_stroke(r, 1.2, s, StrokeKind::Inside);
            painter.line_segment(
                [
                    r.left_top() + Vec2::new(2.0, h * 0.45),
                    r.right_top() + Vec2::new(-2.0, h * 0.45),
                ],
                s,
            );
        }
        MenuIcon::Folder => {
            let tab =
                Rect::from_min_size(c + Vec2::new(-h, -h * 0.55), Vec2::new(h * 0.85, h * 0.35));
            let body =
                Rect::from_center_size(c + Vec2::new(0.0, h * 0.15), Vec2::new(h * 2.0, h * 1.15));
            painter.rect_stroke(tab, 1.0, s, StrokeKind::Inside);
            painter.rect_stroke(body, 1.4, s, StrokeKind::Inside);
        }
        MenuIcon::Open => {
            let r = Rect::from_center_size(c, Vec2::splat(h * 1.5));
            painter.rect_stroke(r, 1.2, s, StrokeKind::Inside);
            painter.arrow(
                c + Vec2::new(-h * 0.15, h * 0.15),
                Vec2::new(h * 0.7, -h * 0.7),
                s,
            );
        }
        MenuIcon::Copy | MenuIcon::Duplicate => {
            let a = Rect::from_center_size(c + Vec2::new(-1.4, -1.4), Vec2::splat(h * 1.15));
            let b = Rect::from_center_size(c + Vec2::new(1.4, 1.4), Vec2::splat(h * 1.15));
            painter.rect_stroke(a, 1.0, s, StrokeKind::Inside);
            painter.rect_stroke(b, 1.0, s, StrokeKind::Inside);
        }
        MenuIcon::Tag => {
            let pts = [
                c + Vec2::new(-h * 0.2, -h),
                c + Vec2::new(h, -h * 0.15),
                c + Vec2::new(h * 0.2, h),
                c + Vec2::new(-h, h * 0.15),
            ];
            for i in 0..4 {
                painter.line_segment([pts[i], pts[(i + 1) % 4]], s);
            }
            painter.circle_filled(c + Vec2::new(-h * 0.25, -h * 0.35), 1.2, color);
        }
        MenuIcon::Link => {
            painter.circle_stroke(c + Vec2::new(-h * 0.35, 0.0), h * 0.45, s);
            painter.circle_stroke(c + Vec2::new(h * 0.35, 0.0), h * 0.45, s);
        }
        MenuIcon::Details => {
            painter.circle_stroke(c, h, s);
            painter.line_segment(
                [c + Vec2::new(0.0, -h * 0.15), c + Vec2::new(0.0, h * 0.45)],
                s,
            );
            painter.circle_filled(c + Vec2::new(0.0, -h * 0.45), 1.15, color);
        }
        MenuIcon::Rename => {
            painter.line_segment(
                [
                    c + Vec2::new(-h, h * 0.7),
                    c + Vec2::new(h * 0.15, -h * 0.55),
                ],
                s,
            );
            painter.line_segment(
                [
                    c + Vec2::new(h * 0.15, -h * 0.55),
                    c + Vec2::new(h * 0.55, -h * 0.15),
                ],
                s,
            );
        }
        MenuIcon::NewFolder => {
            paint_icon(
                painter,
                MenuIcon::Folder,
                c + Vec2::new(-1.0, 0.0),
                size * 0.9,
                stroke_w,
                color,
            );
            painter.line_segment(
                [
                    c + Vec2::new(h * 0.55, -h * 0.15),
                    c + Vec2::new(h * 0.55, h * 0.55),
                ],
                s,
            );
            painter.line_segment(
                [
                    c + Vec2::new(h * 0.2, h * 0.2),
                    c + Vec2::new(h * 0.9, h * 0.2),
                ],
                s,
            );
        }
        MenuIcon::Trash => {
            painter.line_segment(
                [
                    c + Vec2::new(-h * 0.7, -h * 0.45),
                    c + Vec2::new(h * 0.7, -h * 0.45),
                ],
                s,
            );
            painter.line_segment(
                [
                    c + Vec2::new(-h * 0.25, -h * 0.7),
                    c + Vec2::new(h * 0.25, -h * 0.7),
                ],
                s,
            );
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, h * 0.2), Vec2::new(h * 1.2, h * 1.15)),
                1.0,
                s,
                StrokeKind::Inside,
            );
        }
        MenuIcon::TrashX => {
            paint_icon(
                painter,
                MenuIcon::Trash,
                c + Vec2::new(-1.2, 0.0),
                size * 0.9,
                stroke_w,
                color,
            );
            painter.line_segment(
                [
                    c + Vec2::new(h * 0.35, -h * 0.15),
                    c + Vec2::new(h * 0.95, h * 0.45),
                ],
                s,
            );
            painter.line_segment(
                [
                    c + Vec2::new(h * 0.95, -h * 0.15),
                    c + Vec2::new(h * 0.35, h * 0.45),
                ],
                s,
            );
        }
        MenuIcon::Front => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(-1.5, 1.5), Vec2::splat(h * 1.2)),
                1.0,
                s,
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(1.5, -1.5), Vec2::splat(h * 1.2)),
                1.0,
                s,
                StrokeKind::Inside,
            );
        }
        MenuIcon::Back => {
            paint_icon(painter, MenuIcon::Front, c, size, stroke_w, color);
        }
        MenuIcon::Group => {
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::new(h * 1.8, h * 1.25)),
                2.0,
                s,
                StrokeKind::Inside,
            );
            painter.line_segment(
                [c + Vec2::new(0.0, -h * 0.4), c + Vec2::new(0.0, h * 0.4)],
                s,
            );
        }
        MenuIcon::Ungroup => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(-h * 0.55, 0.0), Vec2::new(h * 0.85, h * 1.1)),
                1.2,
                s,
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(h * 0.55, 0.0), Vec2::new(h * 0.85, h * 1.1)),
                1.2,
                s,
                StrokeKind::Inside,
            );
        }
        MenuIcon::Lock => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, h * 0.25), Vec2::new(h * 1.15, h * 0.9)),
                1.2,
                s,
                StrokeKind::Inside,
            );
            painter.circle_stroke(c + Vec2::new(0.0, -h * 0.35), h * 0.38, s);
        }
        MenuIcon::Hide | MenuIcon::View => {
            painter.line_segment([c + Vec2::new(-h, 0.0), c + Vec2::new(0.0, -h * 0.55)], s);
            painter.line_segment([c + Vec2::new(0.0, -h * 0.55), c + Vec2::new(h, 0.0)], s);
            painter.line_segment([c + Vec2::new(h, 0.0), c + Vec2::new(0.0, h * 0.55)], s);
            painter.line_segment([c + Vec2::new(0.0, h * 0.55), c + Vec2::new(-h, 0.0)], s);
            painter.circle_stroke(c, h * 0.28, s);
            if matches!(icon, MenuIcon::Hide) {
                painter.line_segment(
                    [
                        c + Vec2::new(-h * 0.85, h * 0.7),
                        c + Vec2::new(h * 0.85, -h * 0.7),
                    ],
                    s,
                );
            }
        }
        MenuIcon::Maximize => {
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(h * 1.35)),
                1.0,
                s,
                StrokeKind::Inside,
            );
        }
        MenuIcon::Restore => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(1.2, -1.2), Vec2::splat(h * 1.05)),
                1.0,
                s,
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(-1.0, 1.0), Vec2::splat(h * 1.05)),
                1.0,
                s,
                StrokeKind::Inside,
            );
        }
        MenuIcon::Tab => {
            let r = Rect::from_center_size(c, Vec2::new(h * 1.7, h * 1.1));
            painter.rect_stroke(r, 2.0, s, StrokeKind::Inside);
            painter.line_segment(
                [
                    r.left_top() + Vec2::new(0.0, 3.5),
                    r.right_top() + Vec2::new(0.0, 3.5),
                ],
                s,
            );
        }
        MenuIcon::Url => {
            painter.line_segment(
                [
                    c + Vec2::new(-h * 0.7, h * 0.4),
                    c + Vec2::new(h * 0.7, -h * 0.4),
                ],
                s,
            );
            painter.circle_stroke(c + Vec2::new(-h * 0.55, h * 0.35), 1.6, s);
            painter.circle_stroke(c + Vec2::new(h * 0.55, -h * 0.35), 1.6, s);
        }
        MenuIcon::Paste => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, h * 0.1), Vec2::new(h * 1.2, h * 1.35)),
                1.2,
                s,
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, -h * 0.55), Vec2::new(h * 0.7, h * 0.35)),
                1.0,
                s,
                StrokeKind::Inside,
            );
        }
        MenuIcon::Cursor => {
            painter.line_segment(
                [
                    c + Vec2::new(-h * 0.5, -h),
                    c + Vec2::new(-h * 0.5, h * 0.6),
                ],
                s,
            );
            painter.line_segment(
                [
                    c + Vec2::new(-h * 0.5, -h),
                    c + Vec2::new(h * 0.55, h * 0.15),
                ],
                s,
            );
        }
        MenuIcon::Chat => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, -h * 0.1), Vec2::new(h * 1.7, h * 1.15)),
                2.0,
                s,
                StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    c + Vec2::new(-h * 0.2, h * 0.45),
                    c + Vec2::new(-h * 0.55, h * 0.9),
                ],
                s,
            );
        }
        MenuIcon::Enter => {
            painter.arrow(c + Vec2::new(h * 0.5, -h * 0.2), Vec2::new(-h, 0.0), s);
            painter.line_segment(
                [
                    c + Vec2::new(h * 0.5, -h * 0.2),
                    c + Vec2::new(h * 0.5, h * 0.55),
                ],
                s,
            );
        }
        MenuIcon::Check => {
            painter.line_segment(
                [
                    c + Vec2::new(-h * 0.55, 0.05),
                    c + Vec2::new(-h * 0.1, h * 0.5),
                ],
                s,
            );
            painter.line_segment(
                [
                    c + Vec2::new(-h * 0.1, h * 0.5),
                    c + Vec2::new(h * 0.65, -h * 0.5),
                ],
                s,
            );
        }
        MenuIcon::Settings => {
            painter.circle_stroke(c, h * 0.35, s);
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::TAU / 6.0;
                painter.line_segment(
                    [
                        c + Vec2::angled(a) * h * 0.5,
                        c + Vec2::angled(a) * h * 0.95,
                    ],
                    s,
                );
            }
        }
        MenuIcon::Image => {
            let r = Rect::from_center_size(c, Vec2::new(h * 1.6, h * 1.2));
            painter.rect_stroke(r, 1.4, s, StrokeKind::Inside);
            painter.circle_filled(c + Vec2::new(-h * 0.35, -h * 0.2), 1.4, color);
            painter.line_segment(
                [
                    c + Vec2::new(-h * 0.6, h * 0.35),
                    c + Vec2::new(0.0, -h * 0.05),
                ],
                s,
            );
            painter.line_segment(
                [
                    c + Vec2::new(0.0, -h * 0.05),
                    c + Vec2::new(h * 0.65, h * 0.4),
                ],
                s,
            );
        }
        MenuIcon::Search => {
            painter.circle_stroke(c + Vec2::new(-h * 0.15, -h * 0.15), h * 0.55, s);
            painter.line_segment(
                [
                    c + Vec2::new(h * 0.25, h * 0.25),
                    c + Vec2::new(h * 0.75, h * 0.75),
                ],
                s,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_frame_has_no_border_by_default() {
        let f = frame(true);
        assert_eq!(f.stroke.width, 0.0);
        assert!(f.corner_radius.nw >= 4);
    }

    #[test]
    fn menu_tokens_match_the_style_reference() {
        let t = tokens::current().menu;
        assert!(t.border_width <= 0.01);
        assert!(t.corner_radius >= 4.0);
        assert!(t.divider_inset > 0.0);
        assert!(t.icon_size > 0.0);
        assert!(t.letter_spacing >= 0.0);
    }
}
