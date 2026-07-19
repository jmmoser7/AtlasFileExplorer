//! Unified top chrome — one thin strip for the whole window header.
//!
//! Replaces the old two-row layout (title bar + tab strip) with a single
//! Chrome-style bar: app icon portal (floating navigation on hover or click),
//! inline browser tabs, a draggable caption area, and the standard window
//! buttons. The portal overlays the workspace and never displaces tabs.
//!
//! Data-driven so every app in the ecosystem paints identical chrome: the app
//! supplies [`MenuSpec`]s, [`TabSpec`]s (via [`crate::tabs`]), and reacts to
//! the returned [`UnifiedTopBarResult`]. All geometry and colors live here.

use crate::tabs::{self, TabAction, TabChromeColors, TabSpec};
use crate::theme::Palette;
use crate::tokens::{PortalMenuThemeTokens, PortalMenuTokens, TopBarTokens};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Margin, Pos2, Rect, Sense, Shadow,
    Stroke, StrokeKind, Vec2, ViewportCommand,
};

/// Thickness of the invisible resize border on an undecorated window.
const RESIZE_EDGE: f32 = 5.0;
/// Corner zones get a larger diagonal grab area.
const RESIZE_CORNER: f32 = 12.0;

/// One clickable row inside a menu.
pub struct MenuItem {
    pub id: &'static str,
    pub label: String,
    pub shortcut: &'static str,
    pub enabled: bool,
    pub checked: Option<bool>,
    pub separator_before: bool,
}

impl MenuItem {
    pub fn new(id: &'static str, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            shortcut: "",
            enabled: true,
            checked: None,
            separator_before: false,
        }
    }

    pub fn shortcut(mut self, s: &'static str) -> Self {
        self.shortcut = s;
        self
    }

    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }

    pub fn checked(mut self, state: bool) -> Self {
        self.checked = Some(state);
        self
    }

    pub fn separated(mut self) -> Self {
        self.separator_before = true;
        self
    }
}

/// One top-level menu (title + its dropdown items).
pub struct MenuSpec {
    pub title: &'static str,
    pub items: Vec<MenuItem>,
}

/// Which app's mark to paint at the left edge of the top bar.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppIcon {
    Atlas,
    Slate,
}

impl AppIcon {
    pub fn paint(self, painter: &egui::Painter, rect: Rect, palette: &Palette) {
        let c = rect.center();
        let s = rect.height().min(rect.width()) * 0.5;
        match self {
            AppIcon::Atlas => {
                let root = Pos2::new(c.x - s * 0.55, c.y);
                let leaves = [
                    Pos2::new(c.x + s * 0.55, c.y - s * 0.6),
                    Pos2::new(c.x + s * 0.65, c.y),
                    Pos2::new(c.x + s * 0.55, c.y + s * 0.6),
                ];
                let line = Stroke::new(1.3_f32, palette.sub);
                for leaf in leaves {
                    painter.line_segment([root, leaf], line);
                }
                painter.circle_filled(root, s * 0.28, palette.accent);
                for leaf in leaves {
                    painter.circle_filled(leaf, s * 0.18, palette.ink);
                }
            }
            AppIcon::Slate => {
                let page = Rect::from_center_size(
                    Pos2::new(c.x - s * 0.1, c.y),
                    Vec2::new(s * 1.3, s * 1.6),
                );
                painter.rect_stroke(
                    page,
                    2.0,
                    Stroke::new(1.4_f32, palette.ink),
                    StrokeKind::Inside,
                );
                for i in 0..2 {
                    let y = page.min.y + page.height() * (0.3 + 0.22 * i as f32);
                    painter.line_segment(
                        [
                            Pos2::new(page.min.x + s * 0.25, y),
                            Pos2::new(page.max.x - s * 0.25, y),
                        ],
                        Stroke::new(1.1_f32, palette.sub),
                    );
                }
                let d = Pos2::new(page.max.x + s * 0.05, page.max.y - s * 0.05);
                let r = s * 0.32;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        Pos2::new(d.x, d.y - r),
                        Pos2::new(d.x + r, d.y),
                        Pos2::new(d.x, d.y + r),
                        Pos2::new(d.x - r, d.y),
                    ],
                    palette.accent,
                    Stroke::NONE,
                ));
            }
        }
    }
}

/// State the app feeds into [`unified_top_bar`] each frame.
pub struct UnifiedTopBarModel<'a> {
    /// Salts panel ids — in linked sessions two apps share one egui Context.
    pub app_title: &'a str,
    pub icon: AppIcon,
    pub menus: &'a [MenuSpec],
    pub busy: bool,
    pub tabs: &'a [TabSpec],
    pub active_tab: usize,
}

/// Actions returned from the unified top bar.
pub struct UnifiedTopBarResult {
    pub menu_clicked: Option<&'static str>,
    pub tab_action: Option<TabAction>,
}

#[derive(Clone, Default)]
struct PortalMenuState {
    open: bool,
    pinned: bool,
    active_menu: Option<usize>,
    last_inside_time: f64,
}

struct PortalMenuResponse {
    clicked: Option<&'static str>,
    main_rect: Rect,
    submenu_rect: Rect,
}

fn paint_bar_background(painter: &egui::Painter, rect: Rect, colors: TabChromeColors) {
    tabs::paint_vertical_gradient(painter, rect, colors.bar_top, colors.bar);
}

fn portal_theme<'a>(palette: &Palette, portal: &'a PortalMenuTokens) -> &'a PortalMenuThemeTokens {
    if palette.bg.r() > 128 {
        &portal.light
    } else {
        &portal.dark
    }
}

fn portal_frame(portal: &PortalMenuTokens, theme: &PortalMenuThemeTokens) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.fill_color())
        .stroke(Stroke::new(1.0_f32, theme.border_color()))
        .corner_radius(CornerRadius::same(
            portal.corner_radius.clamp(0.0, 255.0) as u8
        ))
        .shadow(Shadow {
            offset: [
                portal.shadow_offset_x.clamp(-127.0, 127.0) as i8,
                portal.shadow_offset_y.clamp(-127.0, 127.0) as i8,
            ],
            blur: portal.shadow_blur.clamp(0.0, 255.0) as u8,
            spread: portal.shadow_spread.clamp(0.0, 255.0) as u8,
            color: Color32::from_black_alpha((portal.shadow_opacity.clamp(0.0, 1.0) * 255.0) as u8),
        })
        .inner_margin(Margin::same(portal.panel_padding.clamp(0.0, 127.0) as i8))
}

fn portal_header(
    ui: &mut egui::Ui,
    title: &str,
    portal: &PortalMenuTokens,
    theme: &PortalMenuThemeTokens,
) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), portal.row_height),
        Sense::hover(),
    );
    ui.painter().text(
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(portal.header_text_size),
        theme.text_color(),
    );
}

fn portal_separator(ui: &mut egui::Ui, portal: &PortalMenuTokens, theme: &PortalMenuThemeTokens) {
    ui.add_space(portal.separator_gap * 0.5);
    let y = ui.cursor().top();
    ui.painter().line_segment(
        [
            Pos2::new(ui.max_rect().left(), y),
            Pos2::new(ui.max_rect().right(), y),
        ],
        Stroke::new(1.0_f32, theme.border_color().gamma_multiply(0.65)),
    );
    ui.add_space(portal.separator_gap * 0.5);
}

fn portal_category_row(
    ui: &mut egui::Ui,
    title: &str,
    has_submenu: bool,
    selected: bool,
    portal: &PortalMenuTokens,
    theme: &PortalMenuThemeTokens,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), portal.row_height),
        Sense::click(),
    );
    if response.hovered() || selected {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same((portal.corner_radius * 0.42).clamp(0.0, 255.0) as u8),
            theme.hover_color(),
        );
    }
    let text_color = if has_submenu {
        theme.text_color()
    } else {
        theme.muted_text_color()
    };
    ui.painter().text(
        Pos2::new(rect.left() + 3.0, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(portal.row_text_size),
        text_color,
    );
    if has_submenu {
        ui.painter().text(
            Pos2::new(rect.right() - 3.0, rect.center().y),
            Align2::RIGHT_CENTER,
            "›",
            FontId::proportional(portal.chevron_text_size),
            theme.muted_text_color(),
        );
    }
    response
}

fn portal_item_row(
    ui: &mut egui::Ui,
    item: &MenuItem,
    portal: &PortalMenuTokens,
    theme: &PortalMenuThemeTokens,
) -> egui::Response {
    let sense = if item.enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), portal.row_height), sense);
    if item.enabled && response.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same((portal.corner_radius * 0.42).clamp(0.0, 255.0) as u8),
            theme.hover_color(),
        );
    }
    let text_color = if item.enabled {
        theme.text_color()
    } else {
        theme.muted_text_color().gamma_multiply(0.65)
    };
    let prefix = match item.checked {
        Some(true) => "✓  ",
        Some(false) => "    ",
        None => "",
    };
    ui.painter().text(
        Pos2::new(rect.left() + 3.0, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{prefix}{}", item.label),
        FontId::proportional(portal.row_text_size),
        text_color,
    );
    if !item.shortcut.is_empty() {
        ui.painter().text(
            Pos2::new(rect.right() - 3.0, rect.center().y),
            Align2::RIGHT_CENTER,
            item.shortcut,
            FontId::proportional(portal.shortcut_text_size),
            theme.muted_text_color(),
        );
    }
    response
}

fn show_portal_menu(
    ctx: &egui::Context,
    palette: &Palette,
    metrics: &TopBarTokens,
    model: &UnifiedTopBarModel<'_>,
    anchor: Rect,
    state: &mut PortalMenuState,
) -> PortalMenuResponse {
    let portal = &metrics.portal;
    let theme = portal_theme(palette, portal);
    let mut active_row_rect = Rect::NOTHING;

    let main = egui::Area::new(egui::Id::new(("portal_main", model.app_title)))
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::new(
            anchor.left() + portal.panel_offset_x,
            anchor.bottom() + portal.panel_gap,
        ))
        .show(ctx, |ui| {
            ui.set_width(portal.width);
            portal_frame(portal, theme).show(ui, |ui| {
                ui.set_width((portal.width - portal.panel_padding * 2.0).max(1.0));
                portal_header(ui, model.app_title, portal, theme);
                portal_separator(ui, portal, theme);

                for (index, menu) in model.menus.iter().enumerate() {
                    if menu.title == "Preferences" {
                        portal_separator(ui, portal, theme);
                    }
                    let selected = state.active_menu == Some(index);
                    let response = portal_category_row(
                        ui,
                        menu.title,
                        !menu.items.is_empty(),
                        selected,
                        portal,
                        theme,
                    );
                    if selected {
                        active_row_rect = response.rect;
                    }
                    if response.hovered() {
                        state.active_menu = if menu.items.is_empty() {
                            None
                        } else {
                            Some(index)
                        };
                        active_row_rect = response.rect;
                    }
                }
            });
        });

    let mut clicked = None;
    let mut submenu_rect = Rect::NOTHING;
    if let Some(index) = state.active_menu {
        if let Some(menu) = model.menus.get(index) {
            if !menu.items.is_empty() && active_row_rect.is_positive() {
                let submenu =
                    egui::Area::new(egui::Id::new(("portal_submenu", model.app_title, index)))
                        .order(egui::Order::Foreground)
                        .fixed_pos(Pos2::new(
                            main.response.rect.right() + portal.submenu_gap,
                            active_row_rect.top() - portal.panel_padding,
                        ))
                        .show(ctx, |ui| {
                            ui.set_width(portal.submenu_width);
                            portal_frame(portal, theme).show(ui, |ui| {
                                ui.set_width(
                                    (portal.submenu_width - portal.panel_padding * 2.0).max(1.0),
                                );
                                for item in &menu.items {
                                    if item.separator_before {
                                        portal_separator(ui, portal, theme);
                                    }
                                    if portal_item_row(ui, item, portal, theme).clicked() {
                                        clicked = Some(item.id);
                                    }
                                }
                            });
                        });
                submenu_rect = submenu.response.rect;
            }
        }
    }

    PortalMenuResponse {
        clicked,
        main_rect: main.response.rect,
        submenu_rect,
    }
}

/// Renders the unified top bar as the topmost panel.
pub fn unified_top_bar(
    ctx: &egui::Context,
    palette: &Palette,
    model: UnifiedTopBarModel<'_>,
) -> UnifiedTopBarResult {
    let metrics = crate::tokens::current().topbar;
    let colors = TabChromeColors::from_palette(palette, &metrics);
    let mut result = UnifiedTopBarResult {
        menu_clicked: None,
        tab_action: None,
    };

    let portal_state_id = egui::Id::new(("icon_portal_menu", model.app_title));
    let mut portal_state = ctx.data_mut(|data| {
        data.get_temp::<PortalMenuState>(portal_state_id)
            .unwrap_or_default()
    });
    let locked_preview = crate::tuning::portal_preview_menu();
    if let Some(menu_index) = locked_preview {
        portal_state.open = true;
        portal_state.pinned = true;
        portal_state.active_menu = Some(menu_index);
    }
    #[cfg(feature = "ui-tuner")]
    if std::env::var_os("ATLAS_TUNER_PORTAL_OPEN").is_some() {
        portal_state.open = true;
        portal_state.pinned = true;
        portal_state.active_menu = Some(0);
    }
    let mut portal_anchor = Rect::NOTHING;
    let now = ctx.input(|input| input.time);

    egui::TopBottomPanel::top(egui::Id::new(("unified_topbar", model.app_title)))
        .exact_height(metrics.height)
        .frame(
            egui::Frame::new()
                .fill(colors.bar)
                .inner_margin(Margin::ZERO),
        )
        .show(ctx, |ui| {
            let bar = ui.max_rect();
            paint_bar_background(ui.painter(), bar, colors);

            // ---- window buttons (right) ----
            let close = Rect::from_min_max(
                Pos2::new(bar.max.x - metrics.window_button_width, bar.min.y),
                Pos2::new(bar.max.x, bar.max.y),
            );
            let maxi = close.translate(Vec2::new(-metrics.window_button_width, 0.0));
            let mini = maxi.translate(Vec2::new(-metrics.window_button_width, 0.0));
            window_buttons(ui, palette, colors, mini, maxi, close, model.app_title);

            // ---- left: fixed icon portal + tabs ----
            let left = Rect::from_min_max(bar.min, Pos2::new(mini.min.x, bar.max.y));
            let mut tabs_right = left.min.x;

            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(left), |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::BOTTOM), |ui| {
                    // Icon portal zone.
                    let (icon_rect, icon_resp) = ui.allocate_exact_size(
                        Vec2::new(metrics.icon_zone_width, metrics.height),
                        Sense::click(),
                    );
                    let icon_center =
                        Rect::from_center_size(icon_rect.center(), Vec2::splat(metrics.icon_size));
                    model.icon.paint(ui.painter(), icon_center, palette);

                    portal_anchor = icon_rect;
                    if icon_resp.clicked() {
                        if portal_state.open && portal_state.pinned {
                            portal_state = PortalMenuState::default();
                        } else {
                            portal_state.open = true;
                            portal_state.pinned = true;
                            portal_state.last_inside_time = now;
                        }
                    } else if icon_resp.hovered() {
                        portal_state.open = true;
                        portal_state.last_inside_time = now;
                    }

                    // Inline tab strip.
                    if let Some(action) = tabs::tab_strip(
                        ui,
                        palette,
                        &metrics,
                        model.tabs,
                        model.active_tab,
                        model.busy,
                    ) {
                        result.tab_action = Some(action);
                    }
                    tabs_right = ui.min_rect().max.x;
                });
            });

            // Hover affordance on the icon portal.
            if ctx
                .pointer_latest_pos()
                .is_some_and(|pointer| portal_anchor.contains(pointer))
            {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }

            // ---- caption drag (between tabs and window buttons) ----
            let caption = Rect::from_min_max(
                Pos2::new(tabs_right.max(left.min.x) + 4.0, bar.min.y),
                Pos2::new(mini.min.x, bar.max.y),
            );
            let drag = ui.interact(
                caption,
                egui::Id::new(("titlebar_drag", model.app_title)),
                Sense::click_and_drag(),
            );
            if drag.double_clicked() {
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
            } else if drag.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
        });

    if portal_state.open {
        let popup = show_portal_menu(
            ctx,
            palette,
            &metrics,
            &model,
            portal_anchor,
            &mut portal_state,
        );
        result.menu_clicked = popup.clicked;

        let pointer_inside = ctx.pointer_latest_pos().is_some_and(|pointer| {
            portal_anchor.contains(pointer)
                || popup.main_rect.contains(pointer)
                || popup.submenu_rect.contains(pointer)
        });
        if pointer_inside {
            portal_state.last_inside_time = now;
        }

        let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let outside_click = ctx.input(|input| input.pointer.any_click()) && !pointer_inside;
        let hover_expired = !portal_state.pinned
            && now - portal_state.last_inside_time > metrics.portal.close_delay as f64;
        if locked_preview.is_none()
            && (result.menu_clicked.is_some() || escape || outside_click || hover_expired)
        {
            portal_state = PortalMenuState::default();
        } else {
            ctx.request_repaint();
        }
    }
    ctx.data_mut(|data| data.insert_temp(portal_state_id, portal_state));

    resize_borders(ctx, model.app_title);
    result
}

fn window_buttons(
    ui: &mut egui::Ui,
    palette: &Palette,
    colors: TabChromeColors,
    mini: Rect,
    maxi: Rect,
    close: Rect,
    salt: &str,
) {
    let ctx = ui.ctx().clone();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

    let button = |ui: &mut egui::Ui, rect: Rect, id: &str| -> (egui::Response, bool) {
        let resp = ui.interact(rect, egui::Id::new((id, salt)), Sense::click());
        (resp.clone(), resp.hovered())
    };

    let (resp, hover) = button(ui, mini, "win_min");
    if hover {
        ui.painter().rect_filled(mini, 0.0, colors.inactive_hover);
    }
    let c = mini.center();
    ui.painter().line_segment(
        [Pos2::new(c.x - 5.0, c.y), Pos2::new(c.x + 5.0, c.y)],
        Stroke::new(1.2_f32, palette.ink),
    );
    if resp.clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }

    let (resp, hover) = button(ui, maxi, "win_max");
    if hover {
        ui.painter().rect_filled(maxi, 0.0, colors.inactive_hover);
    }
    let c = maxi.center();
    let s = Stroke::new(1.2_f32, palette.ink);
    if maximized {
        let r = Rect::from_center_size(c + Vec2::new(-1.0, 1.0), Vec2::splat(8.0));
        let back = r.translate(Vec2::new(2.5, -2.5));
        ui.painter()
            .line_segment([back.left_top(), back.right_top()], s);
        ui.painter()
            .line_segment([back.right_top(), back.right_bottom()], s);
        ui.painter().rect_stroke(r, 1.0, s, StrokeKind::Middle);
    } else {
        let r = Rect::from_center_size(c, Vec2::splat(9.0));
        ui.painter().rect_stroke(r, 1.0, s, StrokeKind::Middle);
    }
    if resp.clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }

    let (resp, hover) = button(ui, close, "win_close");
    let glyph = if hover {
        ui.painter()
            .rect_filled(close, 0.0, Color32::from_rgb(0xc4, 0x2b, 0x1c));
        Color32::WHITE
    } else {
        palette.ink
    };
    let c = close.center();
    let r = 5.0;
    let s = Stroke::new(1.2_f32, glyph);
    ui.painter()
        .line_segment([c + Vec2::new(-r, -r), c + Vec2::new(r, r)], s);
    ui.painter()
        .line_segment([c + Vec2::new(-r, r), c + Vec2::new(r, -r)], s);
    if resp.clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}

fn resize_borders(ctx: &egui::Context, salt: &str) {
    use egui::viewport::ResizeDirection as Dir;
    if ctx.input(|i| {
        i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false)
    }) {
        return;
    }
    let screen = ctx.screen_rect();
    let e = RESIZE_EDGE;
    let k = RESIZE_CORNER;

    let zones: [(Dir, Rect, CursorIcon); 8] = [
        (
            Dir::NorthWest,
            Rect::from_min_size(screen.min, Vec2::splat(k)),
            CursorIcon::ResizeNorthWest,
        ),
        (
            Dir::NorthEast,
            Rect::from_min_size(Pos2::new(screen.max.x - k, screen.min.y), Vec2::splat(k)),
            CursorIcon::ResizeNorthEast,
        ),
        (
            Dir::SouthWest,
            Rect::from_min_size(Pos2::new(screen.min.x, screen.max.y - k), Vec2::splat(k)),
            CursorIcon::ResizeSouthWest,
        ),
        (
            Dir::SouthEast,
            Rect::from_min_size(screen.max - Vec2::splat(k), Vec2::splat(k)),
            CursorIcon::ResizeSouthEast,
        ),
        (
            Dir::North,
            Rect::from_min_max(
                Pos2::new(screen.min.x + k, screen.min.y),
                Pos2::new(screen.max.x - k, screen.min.y + e),
            ),
            CursorIcon::ResizeNorth,
        ),
        (
            Dir::South,
            Rect::from_min_max(
                Pos2::new(screen.min.x + k, screen.max.y - e),
                Pos2::new(screen.max.x - k, screen.max.y),
            ),
            CursorIcon::ResizeSouth,
        ),
        (
            Dir::West,
            Rect::from_min_max(
                Pos2::new(screen.min.x, screen.min.y + k),
                Pos2::new(screen.min.x + e, screen.max.y - k),
            ),
            CursorIcon::ResizeWest,
        ),
        (
            Dir::East,
            Rect::from_min_max(
                Pos2::new(screen.max.x - e, screen.min.y + k),
                Pos2::new(screen.max.x, screen.max.y - k),
            ),
            CursorIcon::ResizeEast,
        ),
    ];

    egui::Area::new(egui::Id::new(("window_resize_borders", salt)))
        .fixed_pos(screen.min)
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            for (i, (dir, rect, cursor)) in zones.into_iter().enumerate() {
                let resp = ui.interact(
                    rect,
                    egui::Id::new(("window_resize_zone", salt, i)),
                    Sense::drag(),
                );
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(cursor);
                }
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    ui.ctx()
                        .send_viewport_cmd(ViewportCommand::BeginResize(dir));
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_button_rects_tile_the_right_edge() {
        let metrics = TopBarTokens::default();
        let bar = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, metrics.height));
        let close = Rect::from_min_max(
            Pos2::new(bar.max.x - metrics.window_button_width, bar.min.y),
            Pos2::new(bar.max.x, bar.max.y),
        );
        let maxi = close.translate(Vec2::new(-metrics.window_button_width, 0.0));
        let mini = maxi.translate(Vec2::new(-metrics.window_button_width, 0.0));
        assert_eq!(close.max.x, bar.max.x);
        assert_eq!(maxi.max.x, close.min.x);
        assert_eq!(mini.max.x, maxi.min.x);
        assert_eq!(mini.height(), metrics.height);
    }
}
