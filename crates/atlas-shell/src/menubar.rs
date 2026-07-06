//! Custom window title bar — the topmost chrome row.
//!
//! The apps run without OS decorations (`ViewportBuilder::with_decorations(false)`);
//! this bar replaces them with one shared row: app icon (no app-name text),
//! the File / View menus inline, a draggable caption area, and the standard
//! minimize / maximize / close buttons on the right — all at the same level.
//!
//! Data-driven so every app in the ecosystem paints an identical bar: the app
//! supplies [`MenuSpec`]s plus its [`AppIcon`] and reacts to the returned item
//! id. All geometry and colors live here; apps must not paint their own menus
//! or window buttons.
//!
//! Item ids are plain `&'static str` (e.g. `"file.open"`) so each app can
//! match on them without a shared action enum leaking app concepts into the
//! chrome crate.

use crate::tabs::TabChromeColors;
use crate::theme::Palette;
use eframe::egui::{
    self, Color32, CursorIcon, Margin, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
    ViewportCommand,
};

/// Height of the title bar (the OS caption replacement).
pub const TITLE_BAR_H: f32 = 32.0;
/// Width of each window button (minimize / maximize / close).
const WIN_BTN_W: f32 = 44.0;
/// Thickness of the invisible resize border on an undecorated window.
const RESIZE_EDGE: f32 = 5.0;
/// Corner zones get a larger diagonal grab area.
const RESIZE_CORNER: f32 = 12.0;

/// One clickable row inside a menu.
pub struct MenuItem {
    /// Returned from [`menu_bar`] when the item is clicked.
    pub id: &'static str,
    pub label: String,
    /// Right-aligned shortcut hint (empty = none). Purely informational; the
    /// binding itself lives in the app's `hotkeys` + `commands::ENTRIES`.
    pub shortcut: &'static str,
    pub enabled: bool,
    /// `Some(state)` renders a checkmark column (toggle / radio items).
    pub checked: Option<bool>,
    /// Paint a separator line above this item.
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

/// Which app's mark to paint at the left edge of the title bar. The bar shows
/// only the icon — never the app name (the window title still carries it for
/// the taskbar / alt-tab).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppIcon {
    /// File Atlas: a root node fanning out to three leaves (the tree canvas).
    Atlas,
    /// Slate: a workbook page with a tag diamond.
    Slate,
}

impl AppIcon {
    /// Paint the mark centered in `rect` (vector, palette-driven, so it
    /// follows the theme like the rest of the chrome).
    pub fn paint(self, painter: &egui::Painter, rect: Rect, palette: &Palette) {
        let c = rect.center();
        let s = rect.height().min(rect.width()) * 0.5; // half-extent
        match self {
            AppIcon::Atlas => {
                // Root dot on the left, three leaves fanning right.
                let root = Pos2::new(c.x - s * 0.55, c.y);
                let leaves = [
                    Pos2::new(c.x + s * 0.55, c.y - s * 0.6),
                    Pos2::new(c.x + s * 0.65, c.y),
                    Pos2::new(c.x + s * 0.55, c.y + s * 0.6),
                ];
                let line = Stroke::new(1.3, palette.sub);
                for leaf in leaves {
                    painter.line_segment([root, leaf], line);
                }
                painter.circle_filled(root, s * 0.28, palette.accent);
                for leaf in leaves {
                    painter.circle_filled(leaf, s * 0.18, palette.ink);
                }
            }
            AppIcon::Slate => {
                // Workbook page with a tag diamond in the lower-right corner.
                let page = Rect::from_center_size(
                    Pos2::new(c.x - s * 0.1, c.y),
                    Vec2::new(s * 1.3, s * 1.6),
                );
                painter.rect_stroke(page, 2.0, Stroke::new(1.4, palette.ink), StrokeKind::Inside);
                for i in 0..2 {
                    let y = page.min.y + page.height() * (0.3 + 0.22 * i as f32);
                    painter.line_segment(
                        [
                            Pos2::new(page.min.x + s * 0.25, y),
                            Pos2::new(page.max.x - s * 0.25, y),
                        ],
                        Stroke::new(1.1, palette.sub),
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

/// State the app feeds into [`menu_bar`] each frame.
pub struct MenuBarModel<'a> {
    /// Salts the panel id — in linked sessions two apps share one egui
    /// Context (two viewports), and panel state must not collide. Not
    /// rendered: the bar shows the [`AppIcon`] only.
    pub app_title: &'a str,
    /// App mark painted at the left edge of the bar.
    pub icon: AppIcon,
    pub menus: &'a [MenuSpec],
}

/// Renders the custom title bar (icon, menus, caption drag area, window
/// buttons) as the topmost panel; returns the id of the clicked menu item, if
/// any. Identical chrome for every Atlas app. Also installs the invisible
/// resize borders that an undecorated window needs.
pub fn menu_bar(
    ctx: &egui::Context,
    palette: &Palette,
    model: MenuBarModel<'_>,
) -> Option<&'static str> {
    let colors = TabChromeColors::from_palette(palette);
    let mut clicked: Option<&'static str> = None;

    egui::TopBottomPanel::top(egui::Id::new(("menubar", model.app_title)))
        .exact_height(TITLE_BAR_H)
        .frame(
            egui::Frame::new()
                .fill(colors.bar)
                .inner_margin(Margin::ZERO),
        )
        .show(ctx, |ui| {
            let bar = ui.max_rect();

            // ---- window buttons (right, full bar height) ----
            let close = Rect::from_min_max(
                Pos2::new(bar.max.x - WIN_BTN_W, bar.min.y),
                Pos2::new(bar.max.x, bar.max.y),
            );
            let maxi = close.translate(Vec2::new(-WIN_BTN_W, 0.0));
            let mini = maxi.translate(Vec2::new(-WIN_BTN_W, 0.0));
            window_buttons(ui, palette, colors, mini, maxi, close, model.app_title);

            // ---- icon + menus (left, vertically centered) ----
            let left = Rect::from_min_max(bar.min, Pos2::new(mini.min.x, bar.max.y));
            let mut menus_right = left.min.x;
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(left), |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::hover());
                    model.icon.paint(ui.painter(), icon_rect, palette);
                    ui.add_space(6.0);
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for menu in model.menus {
                        let title = RichText::new(menu.title).size(12.5).color(palette.ink);
                        let resp = ui.menu_button(title, |ui| {
                            ui.set_min_width(220.0);
                            if let Some(id) = menu_body(ui, palette, &menu.items) {
                                clicked = Some(id);
                                ui.close_menu();
                            }
                        });
                        menus_right = menus_right.max(resp.response.rect.max.x);
                    }
                });
            });

            // ---- caption drag area (everything between menus and buttons) ----
            let caption = Rect::from_min_max(
                Pos2::new(menus_right + 4.0, bar.min.y),
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

    resize_borders(ctx, model.app_title);

    clicked
}

/// Minimize / maximize-restore / close, painted Windows-caption style.
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

    // Minimize.
    let (resp, hover) = button(ui, mini, "win_min");
    if hover {
        ui.painter().rect_filled(mini, 0.0, colors.inactive_hover);
    }
    let c = mini.center();
    ui.painter().line_segment(
        [Pos2::new(c.x - 5.0, c.y), Pos2::new(c.x + 5.0, c.y)],
        Stroke::new(1.2, palette.ink),
    );
    if resp.clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }

    // Maximize / restore.
    let (resp, hover) = button(ui, maxi, "win_max");
    if hover {
        ui.painter().rect_filled(maxi, 0.0, colors.inactive_hover);
    }
    let c = maxi.center();
    let s = Stroke::new(1.2, palette.ink);
    if maximized {
        // Restore: two offset squares.
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

    // Close (Windows caption red on hover).
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
    let s = Stroke::new(1.2, glyph);
    ui.painter()
        .line_segment([c + Vec2::new(-r, -r), c + Vec2::new(r, r)], s);
    ui.painter()
        .line_segment([c + Vec2::new(-r, r), c + Vec2::new(r, -r)], s);
    if resp.clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}

fn menu_body(ui: &mut egui::Ui, palette: &Palette, items: &[MenuItem]) -> Option<&'static str> {
    let mut clicked = None;
    for item in items {
        if item.separator_before {
            ui.separator();
        }
        let label = match item.checked {
            Some(true) => format!("✔ {}", item.label),
            Some(false) => format!("    {}", item.label),
            None => item.label.clone(),
        };
        let mut button = egui::Button::new(RichText::new(label).size(12.5));
        if !item.shortcut.is_empty() {
            button =
                button.shortcut_text(RichText::new(item.shortcut).size(11.0).color(palette.sub));
        }
        if ui.add_enabled(item.enabled, button).clicked() {
            clicked = Some(item.id);
        }
    }
    clicked
}

/// Invisible drag zones along the window edges so an undecorated window can
/// still be resized. Skipped while maximized.
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
        let bar = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, TITLE_BAR_H));
        let close = Rect::from_min_max(
            Pos2::new(bar.max.x - WIN_BTN_W, bar.min.y),
            Pos2::new(bar.max.x, bar.max.y),
        );
        let maxi = close.translate(Vec2::new(-WIN_BTN_W, 0.0));
        let mini = maxi.translate(Vec2::new(-WIN_BTN_W, 0.0));
        assert_eq!(close.max.x, bar.max.x);
        assert_eq!(maxi.max.x, close.min.x);
        assert_eq!(mini.max.x, maxi.min.x);
        assert_eq!(mini.height(), TITLE_BAR_H);
    }
}
