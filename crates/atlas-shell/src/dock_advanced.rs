//! Advanced dock catalog: a tinted infinite canvas of tool cards.
//!
//! This is window chrome that reuses the slate canvas paradigm (camera,
//! objects, selection) so the Advanced overlay is not a stacked list.
//! It is **not** a workbook scene — nothing here is journaled
//! (Constitution Art. V / VI). Cards scale with *this* camera (P0.9).

use crate::canvas_scale;
use crate::canvas_text;
use crate::commands::TurboPanState;
use crate::dock::{
    current_palette, hidden_from_ctx, paint_dock_icon, set_tool_on_strip, tool_hidden_in,
    FlyoutItem, FlyoutRole,
};
use crate::menu::{self, MenuIcon};
use crate::tokens::{DockAdvancedTheme, DockAdvancedTokens};
use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

const VIEW_ID: &str = "atlas_dock_adv_view";
const ESCAPE_ID: &str = "atlas_dock_adv_esc";
const CLOSE_ID: &str = "atlas_dock_adv_close";
const DUP_ID: &str = "atlas_dock_adv_dup";
const USE_ID: &str = "atlas_dock_adv_use";
const PAN_MENU_PX: f32 = 4.0;
const CLOSE_S: f32 = 28.0;

#[derive(Clone, Default)]
struct AdvView {
    pan: Vec2,
    z: f32,
    sel: HashSet<String>,
    fitted_sig: u64,
    drag: Option<AdvDrag>,
    linger_id: Option<String>,
    linger_since: f64,
    linger_menu_rect: Option<Rect>,
    turbo: TurboPanState,
    ctx_menu_open: bool,
    secondary_travel: f32,
}

#[derive(Clone)]
enum AdvDrag {
    Pan { last: Pos2 },
    Marquee { start_world: Pos2 },
}

/// One tool card in world units (zoom = 1).
#[derive(Clone, Debug)]
pub struct CardPlace {
    pub id: &'static str,
    pub world: Rect,
}

/// Group frame (portal) in world units.
#[derive(Clone, Debug)]
pub struct PortalPlace {
    pub label: Option<String>,
    pub world: Rect,
}

#[derive(Clone, Debug)]
pub struct CatalogLayout {
    pub cards: Vec<CardPlace>,
    pub portals: Vec<PortalPlace>,
    pub bounds: Rect,
}

pub struct CatalogSpec {
    pub id: &'static str,
    pub group: Option<String>,
}

/// Place group portals and cards on an infinite plane. Pure; no renderer.
pub fn layout_catalog(items: &[CatalogSpec], t: &DockAdvancedTokens) -> CatalogLayout {
    if items.is_empty() {
        return CatalogLayout {
            cards: Vec::new(),
            portals: Vec::new(),
            bounds: Rect::from_min_size(Pos2::ZERO, Vec2::splat(1.0)),
        };
    }
    let cols = (t.cols.round() as usize).clamp(2, 8);
    let mut groups: Vec<(Option<&str>, Vec<&CatalogSpec>)> = Vec::new();
    for item in items {
        match groups.last_mut() {
            Some((g, list)) if *g == item.group.as_deref() => list.push(item),
            _ => groups.push((item.group.as_deref(), vec![item])),
        }
    }

    let mut cards = Vec::new();
    let mut portals = Vec::new();
    let mut cursor = Pos2::ZERO;
    let mut bounds = Rect::NOTHING;

    for (label, group) in groups {
        let n = group.len();
        let c = cols.min(n).max(1);
        let rows = n.div_ceil(c);
        let inner_w = c as f32 * t.card_w + (c.saturating_sub(1) as f32) * t.card_gap;
        let inner_h = rows as f32 * t.card_h + (rows.saturating_sub(1) as f32) * t.card_gap;
        let title = if label.is_some() { t.portal_title } else { 0.0 };
        let portal = Rect::from_min_size(
            cursor,
            Vec2::new(
                inner_w + t.portal_pad * 2.0,
                inner_h + t.portal_pad * 2.0 + title,
            ),
        );
        let origin = portal.min + Vec2::new(t.portal_pad, t.portal_pad + title);
        for (i, item) in group.iter().enumerate() {
            let col = i % c;
            let row = i / c;
            let min = origin
                + Vec2::new(
                    col as f32 * (t.card_w + t.card_gap),
                    row as f32 * (t.card_h + t.card_gap),
                );
            cards.push(CardPlace {
                id: item.id,
                world: Rect::from_min_size(min, Vec2::new(t.card_w, t.card_h)),
            });
        }
        portals.push(PortalPlace {
            label: label.map(str::to_owned),
            world: portal,
        });
        if bounds == Rect::NOTHING {
            bounds = portal;
        } else {
            bounds = bounds.union(portal);
        }
        cursor.y = portal.bottom() + t.portal_gap;
    }

    CatalogLayout {
        cards,
        portals,
        bounds,
    }
}

fn fit_camera(bounds: Rect, viewport: Rect, t: &DockAdvancedTokens) -> (Vec2, f32) {
    let pad = t.portal_gap * 0.5;
    let bw = (bounds.width() + pad * 2.0).max(1.0);
    let bh = (bounds.height() + pad * 2.0).max(1.0);
    let z = (viewport.width() / bw)
        .min(viewport.height() / bh)
        .clamp(t.zoom_min, t.zoom_max);
    let pan = Vec2::new(
        bounds.center().x - viewport.width() * 0.5 / z,
        bounds.center().y - viewport.height() * 0.5 / z,
    );
    (pan, z)
}

/// Paint the catalog into `viewport` and return a tool id to activate.
pub(crate) fn show(ui: &mut Ui, items: &[FlyoutItem<'_>]) -> Option<&'static str> {
    let mut dock = crate::tokens::current().dock;
    dock.normalize();
    let linger_delay = dock.dashboard_describe_delay;
    let tokens = dock.advanced;
    let dark = ui.visuals().dark_mode;
    let theme = tokens.theme(dark);

    let size = Vec2::new(
        ui.available_width().max(1.0),
        ui.available_height().max(1.0),
    );
    let (viewport, _) = ui.allocate_exact_size(size, Sense::hover());
    if !viewport.is_positive() {
        return None;
    }
    let resp = ui.interact(viewport, ui.id().with("adv_vp"), Sense::click_and_drag());

    let specs: Vec<CatalogSpec> = items
        .iter()
        .map(|item| CatalogSpec {
            id: item.id,
            group: item.group.map(str::to_owned),
        })
        .collect();
    let layout = layout_catalog(&specs, &tokens);
    let sig = catalog_sig(&specs);
    let palette = current_palette(ui.ctx()).unwrap_or("");
    let view_id = Id::new((VIEW_ID, palette));
    let mut view = ui
        .ctx()
        .data_mut(|d| d.get_temp::<AdvView>(view_id).unwrap_or_default());
    if view.z <= 0.0 || view.fitted_sig != sig {
        let (pan, z) = fit_camera(layout.bounds, viewport, &tokens);
        view.pan = pan;
        view.z = z;
        view.fitted_sig = sig;
        view.drag = None;
    }

    let pointer = ui.input(|i| i.pointer.hover_pos());
    let close_rect = close_hit_rect(viewport);
    let over_close = pointer.is_some_and(|p| close_rect.contains(p));
    let over = pointer.is_some_and(|p| viewport.contains(p));
    let panning = interact_camera(
        ui,
        &mut view,
        &tokens,
        viewport,
        &layout,
        &resp,
        over && !over_close,
    );
    let activate = if panning || over_close {
        None
    } else {
        interact_cards(ui, items, &layout, &mut view, viewport, &resp, over)
    };

    let painter = ui.painter_at(viewport);
    paint_paper(&painter, viewport, &tokens, &theme, &view);
    paint_portals(&painter, &layout, &tokens, &theme, &view, viewport);
    paint_cards(
        ui, &painter, items, &layout, &tokens, &theme, &view, viewport, palette,
    );
    if let Some(AdvDrag::Marquee { start_world }) = view.drag {
        if let Some(p) = pointer.filter(|p| viewport.contains(*p)) {
            let a = world_to_screen(start_world, viewport, &view);
            let r = Rect::from_two_pos(a, p);
            painter.rect(
                r,
                0.0,
                theme.select_color().gamma_multiply(0.12),
                Stroke::new(1.0_f32, theme.select_color()),
                StrokeKind::Inside,
            );
        }
    }

    let allow_menu = !panning
        && view.secondary_travel < PAN_MENU_PX
        && !view.turbo.should_suppress_context_menu();
    view.turbo.acknowledge_context_menu();
    if allow_menu {
        context_menu(&resp, items, &mut view, palette, dark);
    } else {
        view.ctx_menu_open = false;
    }
    linger_menu(
        ui,
        items,
        &layout,
        &mut view,
        viewport,
        palette,
        dark,
        linger_delay,
    );
    paint_chrome(
        ui,
        &painter,
        viewport,
        items,
        palette,
        &theme,
        dark,
        close_rect,
    );

    ui.ctx().data_mut(|d| d.insert_temp(view_id, view));
    activate
}

fn catalog_sig(items: &[CatalogSpec]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for item in items {
        item.id.hash(&mut h);
        item.group.hash(&mut h);
    }
    items.len().hash(&mut h);
    h.finish()
}

fn world_to_screen(world: Pos2, viewport: Rect, view: &AdvView) -> Pos2 {
    Pos2::new(
        viewport.min.x + (world.x - view.pan.x) * view.z,
        viewport.min.y + (world.y - view.pan.y) * view.z,
    )
}

fn screen_to_world(screen: Pos2, viewport: Rect, view: &AdvView) -> Pos2 {
    Pos2::new(
        view.pan.x + (screen.x - viewport.min.x) / view.z,
        view.pan.y + (screen.y - viewport.min.y) / view.z,
    )
}

fn world_rect_to_screen(world: Rect, viewport: Rect, view: &AdvView) -> Rect {
    Rect::from_min_max(
        world_to_screen(world.min, viewport, view),
        world_to_screen(world.max, viewport, view),
    )
}

fn zoom_at(view: &mut AdvView, viewport: Rect, screen: Pos2, factor: f32, tokens: &DockAdvancedTokens) {
    let world = screen_to_world(screen, viewport, view);
    let z = (view.z * factor).clamp(tokens.zoom_min, tokens.zoom_max);
    view.pan = Vec2::new(
        world.x - (screen.x - viewport.min.x) / z,
        world.y - (screen.y - viewport.min.y) / z,
    );
    view.z = z;
}

fn interact_camera(
    ui: &mut Ui,
    view: &mut AdvView,
    tokens: &DockAdvancedTokens,
    viewport: Rect,
    layout: &CatalogLayout,
    host: &egui::Response,
    over: bool,
) -> bool {
    let continuing_pan = matches!(view.drag, Some(AdvDrag::Pan { .. }));
    if !over && !continuing_pan {
        return false;
    }

    let (zoom_delta, scroll, raw_scroll, pointer, space, middle, primary, secondary, shift, dt) =
        ui.input(|i| {
            (
                i.zoom_delta(),
                i.smooth_scroll_delta,
                i.raw_scroll_delta,
                i.pointer.hover_pos(),
                i.key_down(egui::Key::Space),
                i.pointer.middle_down(),
                i.pointer.button_down(egui::PointerButton::Primary),
                i.pointer.button_down(egui::PointerButton::Secondary),
                i.modifiers.shift,
                i.stable_dt,
            )
        });
    let wants_kb = ui.ctx().wants_keyboard_input();

    if over {
        let wheel = raw_scroll.y + scroll.y;
        if let Some(p) = pointer.filter(|p| viewport.contains(*p)) {
            if shift && (wheel.abs() > 0.0 || raw_scroll.x.abs() > 0.0 || scroll.x.abs() > 0.0) {
                view.pan.x -= (wheel + raw_scroll.x + scroll.x) / view.z;
                ui.ctx().input_mut(|i| {
                    i.smooth_scroll_delta = Vec2::ZERO;
                    i.raw_scroll_delta = Vec2::ZERO;
                });
                ui.ctx().request_repaint();
            } else if wheel.abs() > 0.0 {
                zoom_at(
                    view,
                    viewport,
                    p,
                    atlas_core::display::SLATE_CANVAS.wheel_factor(wheel),
                    tokens,
                );
                ui.ctx().input_mut(|i| {
                    i.smooth_scroll_delta = Vec2::ZERO;
                    i.raw_scroll_delta = Vec2::ZERO;
                });
                ui.ctx().request_repaint();
            }
            if (zoom_delta - 1.0).abs() > 0.001 {
                zoom_at(view, viewport, p, zoom_delta, tokens);
                ui.ctx().request_repaint();
            }
        }
    }

    if !wants_kb && over {
        let (plus, minus, fit) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
                i.key_pressed(egui::Key::Minus),
                i.key_pressed(egui::Key::F) && !i.modifiers.ctrl,
            )
        });
        let focus = pointer
            .filter(|p| viewport.contains(*p))
            .unwrap_or(viewport.center());
        if plus {
            zoom_at(view, viewport, focus, 1.2, tokens);
        }
        if minus {
            zoom_at(view, viewport, focus, 1.0 / 1.2, tokens);
        }
        if fit {
            let (pan, z) = fit_camera(layout.bounds, viewport, tokens);
            view.pan = pan;
            view.z = z;
        }
        let (l, r, u, d, shift_held) = ui.input(|i| {
            (
                i.key_down(egui::Key::ArrowLeft),
                i.key_down(egui::Key::ArrowRight),
                i.key_down(egui::Key::ArrowUp),
                i.key_down(egui::Key::ArrowDown),
                i.modifiers.shift,
            )
        });
        if l || r || u || d {
            let step = 900.0 * dt.clamp(0.0, 0.05) * if shift_held { 4.0 } else { 1.0 };
            if l {
                view.pan.x -= step / view.z;
            }
            if r {
                view.pan.x += step / view.z;
            }
            if u {
                view.pan.y -= step / view.z;
            }
            if d {
                view.pan.y += step / view.z;
            }
            ui.ctx().request_repaint();
        }
    }

    let mut screen_pan = Vec2::ZERO;
    let turbo = view
        .turbo
        .step(ui.ctx(), viewport, pointer, &mut screen_pan);
    if turbo {
        view.pan -= screen_pan / view.z.max(0.001);
        view.drag = None;
        view.linger_id = None;
        view.linger_menu_rect = None;
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        return true;
    }

    if host.dragged_by(egui::PointerButton::Secondary) {
        view.secondary_travel += host.drag_delta().length();
    } else if !secondary && !view.ctx_menu_open {
        view.secondary_travel = 0.0;
    }

    let secondary_pan = host.dragged_by(egui::PointerButton::Secondary)
        && view.secondary_travel >= PAN_MENU_PX;
    let pan_buttons = secondary_pan
        || host.dragged_by(egui::PointerButton::Middle)
        || (space && host.dragged_by(egui::PointerButton::Primary));

    if let Some(AdvDrag::Pan { last }) = view.drag {
        if let Some(p) = pointer {
            view.pan -= (p - last) / view.z.max(0.001);
            view.drag = Some(AdvDrag::Pan { last: p });
            ui.ctx().request_repaint();
        }
        if !primary && !middle && !secondary {
            view.drag = None;
        } else {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            return true;
        }
    }

    if over && pan_buttons {
        if let Some(p) = pointer {
            view.drag = Some(AdvDrag::Pan { last: p });
            view.linger_id = None;
            view.linger_menu_rect = None;
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            return true;
        }
    }

    let _ = shift;
    matches!(view.drag, Some(AdvDrag::Pan { .. }))
}

fn interact_cards(
    ui: &mut Ui,
    items: &[FlyoutItem<'_>],
    layout: &CatalogLayout,
    view: &mut AdvView,
    viewport: Rect,
    host: &egui::Response,
    over: bool,
) -> Option<&'static str> {
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !view.sel.is_empty() {
        view.sel.clear();
        ui.ctx()
            .data_mut(|d| d.insert_temp(Id::new(ESCAPE_ID), true));
        view.drag = None;
        return None;
    }

    if !over {
        if let Some(AdvDrag::Marquee { start_world }) = view.drag.clone() {
            if ui.input(|i| i.pointer.primary_released()) {
                view.drag = None;
                let _ = start_world;
            }
        }
        return None;
    }

    let pointer = ui.input(|i| i.pointer.hover_pos());
    let Some(screen) = pointer else {
        return None;
    };
    let world = screen_to_world(screen, viewport, view);
    let hit = layout
        .cards
        .iter()
        .rev()
        .find(|c| c.world.contains(world))
        .map(|c| c.id);
    let portal_hit = hit.is_none().then(|| {
        layout
            .portals
            .iter()
            .rev()
            .find(|p| p.world.contains(world))
    });

    if matches!(view.drag, Some(AdvDrag::Pan { .. })) {
        return None;
    }

    if let Some(AdvDrag::Marquee { start_world }) = view.drag.clone() {
        if ui.input(|i| i.pointer.primary_released()) {
            apply_marquee(layout, view, start_world, world);
            view.drag = None;
        }
        return None;
    }

    let shift = ui.input(|i| i.modifiers.shift);
    if host.double_clicked() {
        if let Some(id) = hit {
            view.sel.clear();
            view.sel.insert(id.to_owned());
            return Some(id);
        }
        let (pan, z) = {
            let mut dock = crate::tokens::current().dock;
            dock.normalize();
            fit_camera(layout.bounds, viewport, &dock.advanced)
        };
        view.pan = pan;
        view.z = z;
        view.sel.clear();
        return None;
    }

    if host.secondary_clicked() {
        if let Some(id) = hit {
            if !view.sel.contains(id) {
                view.sel.clear();
                view.sel.insert(id.to_owned());
            }
        } else if view.sel.is_empty() {
            if let Some(portal) = layout
                .portals
                .iter()
                .rev()
                .find(|p| p.world.contains(world))
            {
                for card in layout
                    .cards
                    .iter()
                    .filter(|c| portal.world.contains(c.world.center()))
                {
                    view.sel.insert(card.id.to_owned());
                }
            }
        }
        return None;
    }

    if host.clicked() {
        if let Some(id) = hit {
            if shift {
                if !view.sel.remove(id) {
                    view.sel.insert(id.to_owned());
                }
            } else {
                view.sel.clear();
                view.sel.insert(id.to_owned());
            }
        } else if let Some(Some(portal)) = portal_hit {
            let ids: Vec<&'static str> = layout
                .cards
                .iter()
                .filter(|c| portal.world.contains(c.world.center()))
                .map(|c| c.id)
                .collect();
            if !shift {
                view.sel.clear();
            }
            for id in ids {
                view.sel.insert(id.to_owned());
            }
        } else if !shift {
            view.sel.clear();
        }
        return None;
    }

    if host.drag_started_by(egui::PointerButton::Primary)
        && ui.input(|i| !i.key_down(egui::Key::Space))
        && (hit.is_none() || shift)
    {
        view.drag = Some(AdvDrag::Marquee { start_world: world });
    }

    let _ = items;
    None
}

fn apply_marquee(layout: &CatalogLayout, view: &mut AdvView, a: Pos2, b: Pos2) {
    let r = Rect::from_two_pos(a, b);
    if r.width() < 4.0 && r.height() < 4.0 {
        return;
    }
    view.sel.clear();
    for card in &layout.cards {
        if card.world.intersects(r) {
            view.sel.insert(card.id.to_owned());
        }
    }
}

fn paint_paper(
    painter: &egui::Painter,
    viewport: Rect,
    tokens: &DockAdvancedTokens,
    theme: &DockAdvancedTheme,
    view: &AdvView,
) {
    painter.rect_filled(viewport, 0.0, theme.canvas_color());
    painter.rect_filled(viewport, 0.0, theme.cast_color());
    let step = tokens.grid_step;
    if canvas_scale::px(step, view.z) < 6.0 {
        return;
    }
    let w0 = screen_to_world(viewport.min, viewport, view);
    let w1 = screen_to_world(viewport.max, viewport, view);
    let x0 = (w0.x / step).floor() * step;
    let y0 = (w0.y / step).floor() * step;
    let stroke = Stroke::new(1.0_f32, theme.grid_color());
    let mut x = x0;
    while x <= w1.x + step * 0.5 {
        let a = world_to_screen(Pos2::new(x, w0.y), viewport, view);
        let b = world_to_screen(Pos2::new(x, w1.y), viewport, view);
        painter.line_segment(
            [
                Pos2::new(a.x, viewport.min.y),
                Pos2::new(b.x, viewport.max.y),
            ],
            stroke,
        );
        x += step;
    }
    let mut y = y0;
    while y <= w1.y + step * 0.5 {
        let a = world_to_screen(Pos2::new(w0.x, y), viewport, view);
        let b = world_to_screen(Pos2::new(w1.x, y), viewport, view);
        painter.line_segment(
            [
                Pos2::new(viewport.min.x, a.y),
                Pos2::new(viewport.max.x, b.y),
            ],
            stroke,
        );
        y += step;
    }
}

fn paint_portals(
    painter: &egui::Painter,
    layout: &CatalogLayout,
    tokens: &DockAdvancedTokens,
    theme: &DockAdvancedTheme,
    view: &AdvView,
    viewport: Rect,
) {
    for portal in &layout.portals {
        let rect = world_rect_to_screen(portal.world, viewport, view);
        if !rect.intersects(viewport) {
            continue;
        }
        let radius = canvas_scale::px(tokens.portal_radius, view.z);
        let stroke_w = canvas_scale::px(1.5, view.z);
        painter.rect(
            rect,
            radius,
            theme.portal_fill_color(),
            Stroke::new(stroke_w, theme.portal_border_color()),
            StrokeKind::Inside,
        );
        if let Some(label) = &portal.label {
            let size = canvas_scale::px(13.0, view.z);
            if canvas_text::legible(size) {
                let pos = Pos2::new(
                    rect.min.x + canvas_scale::px(12.0, view.z),
                    rect.min.y + canvas_scale::px(6.0, view.z),
                );
                canvas_text::text(
                    painter,
                    pos,
                    Align2::LEFT_TOP,
                    label,
                    FontId::proportional(size),
                    theme.muted_color(),
                );
            }
        }
    }
}

fn paint_cards(
    ui: &Ui,
    painter: &egui::Painter,
    items: &[FlyoutItem<'_>],
    layout: &CatalogLayout,
    tokens: &DockAdvancedTokens,
    theme: &DockAdvancedTheme,
    view: &AdvView,
    viewport: Rect,
    palette: &str,
) {
    let hidden = hidden_from_ctx(ui.ctx());
    for card in &layout.cards {
        let rect = world_rect_to_screen(card.world, viewport, view);
        if !rect.intersects(viewport) {
            continue;
        }
        let Some(item) = items.iter().find(|i| i.id == card.id) else {
            continue;
        };
        let selected = view.sel.contains(card.id);
        let hovered = ui
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|p| rect.contains(p));
        let radius = canvas_scale::px(tokens.card_radius, view.z);
        let fill = if hovered || selected {
            theme.card_fill_color()
        } else {
            theme.card_fill_color().gamma_multiply(0.92)
        };
        painter.rect_filled(rect, radius, fill);

        let pad = canvas_scale::px(10.0, view.z);
        let icon_s = canvas_scale::px(36.0, view.z);
        let icon_rect = Rect::from_center_size(
            Pos2::new(rect.center().x, rect.min.y + pad + icon_s * 0.5),
            Vec2::splat(icon_s),
        );
        if icon_s >= 6.0 {
            paint_dock_icon(painter, icon_rect, item.icon, theme.text_color());
        }

        let label_size = canvas_scale::px(13.0, view.z);
        if canvas_text::legible(label_size) {
            canvas_text::text(
                painter,
                Pos2::new(
                    rect.center().x,
                    icon_rect.bottom() + canvas_scale::px(6.0, view.z),
                ),
                Align2::CENTER_TOP,
                item.label,
                FontId::proportional(label_size),
                theme.text_color(),
            );
        }
        if let Some(hk) = item.hotkey {
            let hk_size = canvas_scale::px(10.0, view.z);
            if canvas_text::legible(hk_size) {
                canvas_text::text(
                    painter,
                    Pos2::new(rect.center().x, rect.max.y - canvas_scale::px(8.0, view.z)),
                    Align2::CENTER_BOTTOM,
                    hk,
                    FontId::proportional(hk_size),
                    theme.muted_color(),
                );
            }
        }

        let on_strip = !tool_hidden_in(&hidden, palette, item.id);
        if on_strip && rect.height() >= 36.0 {
            let badge = Rect::from_min_size(
                Pos2::new(
                    rect.max.x - canvas_scale::px(22.0, view.z),
                    rect.min.y + canvas_scale::px(6.0, view.z),
                ),
                Vec2::splat(canvas_scale::px(10.0, view.z)),
            );
            painter.circle_filled(badge.center(), badge.width() * 0.5, theme.badge_color());
        }

        if item.role == FlyoutRole::Toggle {
            let track = Rect::from_center_size(
                Pos2::new(rect.center().x, rect.max.y - canvas_scale::px(14.0, view.z)),
                Vec2::new(
                    canvas_scale::px(22.0, view.z),
                    canvas_scale::px(10.0, view.z),
                ),
            );
            if track.width() >= 8.0 {
                painter.rect_filled(
                    track,
                    track.height() * 0.5,
                    if item.active {
                        theme.badge_color()
                    } else {
                        theme.muted_color().gamma_multiply(0.45)
                    },
                );
            }
        }
    }
}

fn selected_items<'a>(items: &'a [FlyoutItem<'a>], view: &AdvView) -> Vec<&'a FlyoutItem<'a>> {
    items
        .iter()
        .filter(|item| view.sel.contains(item.id))
        .collect()
}

fn catalog_actions(ui: &mut Ui, selected: &[&FlyoutItem<'_>], palette: &str, dark: bool) -> bool {
    if selected.is_empty() {
        return false;
    }
    let mut close = false;
    if menu::item(ui, MenuIcon::Front, "Add to toolbar", dark).clicked() {
        set_selected_on_strip(ui.ctx(), palette, selected, true);
        close = true;
    }
    if menu::item(ui, MenuIcon::Hide, "Remove from toolbar", dark).clicked() {
        set_selected_on_strip(ui.ctx(), palette, selected, false);
        close = true;
    }
    menu::separator(ui, dark);
    if menu::item(ui, MenuIcon::Copy, "Copy to clipboard", dark).clicked() {
        copy_selected(ui.ctx(), selected);
        close = true;
    }
    if menu::item(ui, MenuIcon::Duplicate, "Duplicate", dark).clicked() {
        queue_duplicate(ui.ctx(), selected);
        close = true;
    }
    if selected.len() == 1 && menu::item(ui, MenuIcon::Cursor, "Use tool", dark).clicked() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(Id::new(USE_ID), selected[0].id));
        close = true;
    }
    if close {
        ui.close_menu();
    }
    close
}

fn context_menu(
    host: &egui::Response,
    items: &[FlyoutItem<'_>],
    view: &mut AdvView,
    palette: &str,
    dark: bool,
) {
    if view.sel.is_empty() {
        view.ctx_menu_open = false;
        return;
    }
    let mut open = false;
    host.context_menu(|ui| {
        open = true;
        let selected = selected_items(items, view);
        let _ = catalog_actions(ui, &selected, palette, dark);
    });
    view.ctx_menu_open = open;
    if open {
        view.linger_id = None;
        view.linger_menu_rect = None;
    }
}

fn linger_menu(
    ui: &mut Ui,
    items: &[FlyoutItem<'_>],
    layout: &CatalogLayout,
    view: &mut AdvView,
    viewport: Rect,
    palette: &str,
    dark: bool,
    delay: f32,
) {
    if view.ctx_menu_open || matches!(view.drag, Some(_)) {
        view.linger_id = None;
        view.linger_menu_rect = None;
        return;
    }
    if ui.input(|i| {
        i.pointer.button_down(egui::PointerButton::Secondary)
            || i.pointer.button_released(egui::PointerButton::Secondary)
    }) {
        view.linger_id = None;
        view.linger_menu_rect = None;
        return;
    }
    let pointer = ui.input(|i| i.pointer.hover_pos());
    if pointer.is_some_and(|p| close_hit_rect(viewport).contains(p)) {
        view.linger_id = None;
        view.linger_menu_rect = None;
        return;
    }
    let now = ui.input(|i| i.time);
    let over_menu = pointer.is_some_and(|p| {
        view.linger_menu_rect
            .is_some_and(|r| r.expand(6.0).contains(p))
    });
    let hover_id = pointer.and_then(|p| {
        let world = screen_to_world(p, viewport, view);
        layout
            .cards
            .iter()
            .rev()
            .find(|c| c.world.contains(world))
            .map(|c| c.id)
    });
    if !over_menu {
        match hover_id {
            Some(id) if view.linger_id.as_deref() == Some(id) => {}
            Some(id) => {
                view.linger_id = Some(id.to_owned());
                view.linger_since = now;
                view.linger_menu_rect = None;
            }
            None => {
                view.linger_id = None;
                view.linger_menu_rect = None;
                return;
            }
        }
        if now - view.linger_since < delay as f64 {
            ui.ctx().request_repaint();
            return;
        }
    }
    let Some(id) = view.linger_id.clone() else {
        return;
    };
    if !view.sel.contains(id.as_str()) {
        view.sel.clear();
        view.sel.insert(id.clone());
    }
    let Some(card) = layout.cards.iter().find(|c| c.id == id) else {
        return;
    };
    let rect = world_rect_to_screen(card.world, viewport, view);
    let selected = selected_items(items, view);
    if selected.is_empty() {
        return;
    }
    let pos = Pos2::new(rect.center().x, rect.min.y - 8.0);
    let mut dismissed = false;
    let response = egui::Area::new(ui.id().with("adv_linger"))
        .order(egui::Order::Tooltip)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(pos)
        .constrain(false)
        .show(ui.ctx(), |ui| {
            menu::frame(dark).show(ui, |ui| {
                ui.set_min_width(188.0);
                if catalog_actions(ui, &selected, palette, dark) {
                    dismissed = true;
                }
            });
        });
    view.linger_menu_rect = Some(response.response.rect);
    if dismissed {
        view.linger_id = None;
        view.linger_menu_rect = None;
    }
}

fn queue_duplicate(ctx: &egui::Context, selected: &[&FlyoutItem<'_>]) {
    let ids: Vec<String> = selected.iter().map(|item| item.id.to_string()).collect();
    if !ids.is_empty() {
        ctx.data_mut(|d| d.insert_temp(Id::new(DUP_ID), ids));
    }
}

fn copy_selected(ctx: &egui::Context, selected: &[&FlyoutItem<'_>]) {
    let text = selected
        .iter()
        .map(|item| format!("{}\t{}", item.id, item.label))
        .collect::<Vec<_>>()
        .join("\n");
    if !text.is_empty() {
        ctx.copy_text(text);
    }
}

fn set_selected_on_strip(
    ctx: &egui::Context,
    palette: &str,
    selected: &[&FlyoutItem<'_>],
    on: bool,
) {
    for item in selected {
        set_tool_on_strip(ctx, palette, item.id, on);
    }
}

fn close_hit_rect(viewport: Rect) -> Rect {
    Rect::from_min_size(
        Pos2::new(viewport.max.x - CLOSE_S - 14.0, viewport.min.y + 12.0),
        Vec2::splat(CLOSE_S),
    )
}

fn paint_chrome(
    ui: &mut Ui,
    painter: &egui::Painter,
    viewport: Rect,
    items: &[FlyoutItem<'_>],
    _palette: &str,
    theme: &DockAdvancedTheme,
    _dark: bool,
    close_rect: Rect,
) {
    let close = ui.interact(close_rect, ui.id().with("adv_close"), Sense::click());
    if close.clicked() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(Id::new(CLOSE_ID), true));
    }
    let over = close.hovered();
    painter.rect_filled(
        close_rect,
        6.0,
        if over {
            theme.card_fill_color()
        } else {
            Color32::TRANSPARENT
        },
    );
    painter.text(
        close_rect.center(),
        Align2::CENTER_CENTER,
        "×",
        FontId::proportional(20.0),
        if over {
            theme.text_color()
        } else {
            theme.muted_color()
        },
    );
    close.on_hover_text("Close Advanced");

    if items.is_empty() {
        return;
    }
    let legend = Pos2::new(viewport.min.x + 20.0, viewport.max.y - 22.0);
    painter.circle_filled(legend, 5.0, theme.badge_color());
    painter.text(
        Pos2::new(legend.x + 12.0, legend.y),
        Align2::LEFT_CENTER,
        "On the toolbar",
        FontId::proportional(12.0),
        theme.muted_color(),
    );
}

/// True when the catalog consumed Escape (cleared a selection).
pub(crate) fn escape_consumed(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(Id::new(ESCAPE_ID)))
        .unwrap_or(false)
}

/// Close control in the catalog chrome, or an explicit dismiss.
pub(crate) fn close_requested(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(Id::new(CLOSE_ID)))
        .unwrap_or(false)
}

/// Context-menu "Use tool" landing, if any.
pub(crate) fn take_use(ctx: &egui::Context) -> Option<&'static str> {
    ctx.data_mut(|d| d.remove_temp::<&'static str>(Id::new(USE_ID)))
}

/// Flyout ids queued by Duplicate on the catalog.
pub(crate) fn take_duplicate(ctx: &egui::Context) -> Option<Vec<String>> {
    ctx.data_mut(|d| d.remove_temp::<Vec<String>>(Id::new(DUP_ID)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens() -> DockAdvancedTokens {
        let mut t = DockAdvancedTokens::default();
        t.normalize();
        t
    }

    #[test]
    fn empty_catalog_has_unit_bounds() {
        let layout = layout_catalog(&[], &tokens());
        assert!(layout.cards.is_empty());
        assert!(layout.portals.is_empty());
        assert!(layout.bounds.width() > 0.0);
    }

    #[test]
    fn cards_sit_inside_their_portal() {
        let items = [
            CatalogSpec {
                id: "a",
                group: Some("shapes".into()),
            },
            CatalogSpec {
                id: "b",
                group: Some("shapes".into()),
            },
            CatalogSpec {
                id: "c",
                group: Some("ink".into()),
            },
        ];
        let layout = layout_catalog(&items, &tokens());
        assert_eq!(layout.cards.len(), 3);
        assert_eq!(layout.portals.len(), 2);
        for card in &layout.cards {
            let portal = layout
                .portals
                .iter()
                .find(|p| p.world.contains(card.world.center()))
                .expect("card outside every portal");
            assert!(
                portal.world.contains(card.world.min) && portal.world.contains(card.world.max),
                "card {} not inside portal",
                card.id
            );
        }
    }

    #[test]
    fn portals_do_not_overlap() {
        let items = [
            CatalogSpec {
                id: "a",
                group: Some("one".into()),
            },
            CatalogSpec {
                id: "b",
                group: Some("one".into()),
            },
            CatalogSpec {
                id: "c",
                group: Some("two".into()),
            },
            CatalogSpec {
                id: "d",
                group: Some("two".into()),
            },
        ];
        let layout = layout_catalog(&items, &tokens());
        for (i, a) in layout.portals.iter().enumerate() {
            for b in layout.portals.iter().skip(i + 1) {
                assert!(
                    !a.world.intersects(b.world),
                    "portals overlap: {:?} vs {:?}",
                    a.world,
                    b.world
                );
            }
        }
    }

    #[test]
    fn fit_camera_keeps_bounds_in_view() {
        let t = tokens();
        let items = [
            CatalogSpec {
                id: "a",
                group: Some("g".into()),
            },
            CatalogSpec {
                id: "b",
                group: Some("g".into()),
            },
        ];
        let layout = layout_catalog(&items, &t);
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 500.0));
        let (pan, z) = fit_camera(layout.bounds, viewport, &t);
        let view = AdvView {
            pan,
            z,
            ..AdvView::default()
        };
        let s = world_rect_to_screen(layout.bounds, viewport, &view);
        assert!(
            viewport.expand(2.0).contains(s.min) && viewport.expand(2.0).contains(s.max),
            "fitted bounds {s:?} escape viewport {viewport:?}"
        );
        assert!(z >= t.zoom_min && z <= t.zoom_max);
    }
}
