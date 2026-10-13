//! Bottom-docked families: one capsule, one datum, one shared icon pitch.
use super::*;

const ACTIVE: &str = "dock-inline-body";

/// Tool palettes use capsules; Dashboard menus retain their form host on either edge.
pub(super) fn hosts(side: DockSide, items: &[&DockItem<'_>]) -> bool {
    side == DockSide::BottomCenter && items.iter().any(|item| item.kind == DockItemKind::Tool)
}

pub(super) fn active(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(egui::Id::new(ACTIVE)))
        .unwrap_or(false)
}

fn set_active(ctx: &egui::Context, value: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(ACTIVE), value));
}

/// Reuse the strip interpreter, catalog ordering, toggles and hover chips;
/// only its geometry changes while it is hosted inside a primary capsule.
pub(super) fn layout(shown: &[&FlyoutItem<'_>], tokens: &DockTokens) -> IconStripLayout {
    let icon = flyout_icon_size(tokens);
    let gap = icon_gap(tokens);
    let width = if shown.is_empty() {
        0.0
    } else {
        icon_line_along(shown.len(), icon, gap)
    };
    let size = Vec2::new(width, icon);
    IconStripLayout {
        size,
        groups: vec![IconStripGroup {
            label: None,
            origin: Vec2::ZERO,
            frame: size,
            slots: shown
                .iter()
                .enumerate()
                .map(|(i, item)| IconStripSlot {
                    id: item.id,
                    off: Vec2::new(i as f32 * (icon + gap), 0.0),
                    size: Vec2::splat(icon),
                })
                .collect(),
        }],
    }
}

fn icon_gap(tokens: &DockTokens) -> f32 {
    (tokens.icon_gap * tokens.flyout_icon_scale).max(4.0)
}

#[derive(Clone, Copy, Default)]
struct Motion {
    from: f32,
    to: f32,
    started: f64,
}

impl Motion {
    fn value(self, now: f64, duration: f32) -> f32 {
        let t = ((now - self.started) as f32 / duration).clamp(0.0, 1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        self.from + (self.to - self.from) * eased
    }

    fn step(&mut self, open: bool, now: f64, duration: f32) -> f32 {
        let current = self.value(now, duration);
        let target = if open { 1.0 } else { 0.0 };
        if self.to != target {
            *self = Self {
                from: current,
                to: target,
                started: now,
            };
        }
        self.value(now, duration)
    }
}

#[derive(Clone, Default)]
struct Memory {
    widths: HashMap<&'static str, f32>,
    motion: HashMap<&'static str, Motion>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn show(
    ctx: &egui::Context,
    state_id: egui::Id,
    state: &mut DockState,
    visible: &[&DockItem<'_>],
    tokens: &DockTokens,
    th: &DockThemeTokens,
    canvas: Rect,
    now: f64,
    clicked: &mut Option<&'static str>,
    drop_to_canvas: &mut Option<&'static str>,
    panel_body: &mut impl FnMut(&mut egui::Ui, &'static str),
) -> super::primary::PrimaryBar {
    let memory_id = state_id.with("inline-memory");
    let mut memory = ctx.data_mut(|d| d.get_temp::<Memory>(memory_id).unwrap_or_default());
    let mut result = super::primary::PrimaryBar {
        bar_rect: Rect::NOTHING,
        icon_rects: state.last_icon_rects.clone(),
        hovered_icon: None,
        label_hover_candidate: None,
        label_blocked_by_open: false,
        panel_owns: panel_owns_pointer(state.last_union_panels, ctx.pointer_latest_pos()),
        palette_hover: None,
        overlay_rect: None,
    };
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(DOCK_SIDE_ID), DockSide::BottomCenter));
    if state.bar_collapsed {
        // Pause hidden transitions. Restore the full family state at the blister.
        if result.icon_rects.is_empty() {
            result.icon_rects =
                estimated_icon_rects(visible, tokens, canvas, DockSide::BottomCenter);
        }
        return result;
    }
    let icon = flyout_icon_size(tokens);
    let gap = icon_gap(tokens);
    let pad = tokens.palette.group_pad;
    let height = icon + pad * 2.0;
    let mut widths = Vec::with_capacity(visible.len());
    for item in visible {
        let open = state.pinned.contains(&item.id) || state.body_preview == Some(item.id);
        let t =
            memory
                .motion
                .entry(item.id)
                .or_default()
                .step(open, now, tokens.inline.open_duration);
        let body = memory.widths.get(item.id).copied().unwrap_or(0.0);
        let extra = if item.kind == DockItemKind::Tool && body > 0.0 {
            gap + body
        } else {
            0.0
        };
        widths.push((icon + pad * 2.0 + extra * t, t));
        if (t - if open { 1.0 } else { 0.0 }).abs() > 0.001 {
            ctx.request_repaint();
        }
    }
    let available = (canvas.width() - tokens.left_margin * 2.0).max(height);
    let content_width = widths.iter().map(|(width, _)| width).sum::<f32>()
        + tokens.palette.group_gap * visible.len().saturating_sub(1) as f32;
    let row_size = Vec2::new(content_width.min(available), height);
    // Area pivots and ScrollArea otherwise use last frame's measured size.
    // Place and size the viewport from this frame's animated capsule widths.
    let row_min = Pos2::new(
        canvas.center().x - row_size.x * 0.5,
        canvas.bottom() - tokens.bottom_margin - height,
    );
    let area = egui::Area::new(state_id.with("bar"))
        .order(egui::Order::Foreground)
        .pivot(Align2::LEFT_TOP)
        .fixed_pos(row_min)
        .default_size(row_size)
        .movable(false)
        .fade_in(false)
        .constrain(false);
    let response = area.show(ctx, |ui| {
        ui.set_min_size(row_size);
        ui.set_max_size(row_size);
        // Vertical wheel still belongs to the canvas. Only genuine overflow
        // needs scrolling; a floating scrollbar must not cover the circles.
        ui.style_mut().always_scroll_the_only_direction = false;
        let mut paint_row = |ui: &mut egui::Ui| {
            ui.spacing_mut().item_spacing.x = tokens.palette.group_gap;
            ui.horizontal(|ui| {
                for (item, &(width, t)) in visible.iter().zip(&widths) {
                    let (capsule, _) =
                        ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                    paint_fieldset_frame(ui, capsule, None, th, &tokens.palette, 1.0, 0.0);
                    let primary =
                        Rect::from_min_size(capsule.min + Vec2::splat(pad), Vec2::splat(icon));
                    let resp = ui
                        .interact(primary, state_id.with(("primary", item.id)), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    result.icon_rects.insert(item.id, primary);
                    let open =
                        state.pinned.contains(&item.id) || state.body_preview == Some(item.id);
                    if resp.hovered() && !result.panel_owns {
                        result.hovered_icon = Some(item.id);
                        state.last_inside_time = now;
                        if open {
                            result.label_blocked_by_open = true;
                        } else {
                            result.label_hover_candidate = Some(item.id);
                        }
                    }
                    paint_primary(ui, primary, item, th, tokens);
                    if resp.clicked() {
                        *clicked = Some(item.id);
                        if item.kind.opens_body() {
                            toggle(state, item.id, open);
                        }
                        ctx.request_repaint();
                    }
                    if item.kind == DockItemKind::Tool {
                        resp.context_menu(|ui| {
                            if ui
                                .button(if open { "Collapse" } else { "Expand" })
                                .clicked()
                            {
                                toggle(state, item.id, open);
                                ui.close_menu();
                            }
                            if ui.button("Drop to canvas").clicked() {
                                *drop_to_canvas = Some(item.id);
                                ui.close_menu();
                            }
                        });
                        if open || t > 0.0 {
                            let start = primary.right_top() + Vec2::new(gap, 0.0);
                            let full = Rect::from_min_size(start, Vec2::new(16_384.0, icon));
                            // Clip the horizontal reveal, while retaining the
                            // capsule's vertical padding for antialiased outlines.
                            let clip = Rect::from_min_max(
                                Pos2::new(start.x - gap * 0.5, capsule.top()),
                                capsule.max,
                            );
                            let mut child = ui.new_child(
                                egui::UiBuilder::new()
                                    .id_salt(("inline-body", item.id))
                                    .max_rect(full)
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            );
                            child.set_clip_rect(ui.clip_rect().intersect(clip));
                            if clip.width() <= 0.0 {
                                child.set_invisible();
                            }
                            // A retiring family paints while closing, but cannot fire hidden tools.
                            if !open {
                                child.disable();
                            }
                            set_current_palette(ctx, item.id, item.label);
                            set_body_layout(ctx, DockBodyLayout::Icons);
                            set_active(ctx, true);
                            panel_body(&mut child, item.id);
                            set_active(ctx, false);
                            let measured = child.min_rect().width();
                            if memory
                                .widths
                                .get(item.id)
                                .is_none_or(|w| (w - measured).abs() > 0.5)
                            {
                                memory.widths.insert(item.id, measured);
                                ctx.request_repaint();
                            }
                        }
                    }
                }
            });
        };
        if content_width > available {
            egui::ScrollArea::horizontal()
                .id_salt(state_id.with("inline-scroll"))
                .max_width(row_size.x)
                .min_scrolled_width(row_size.x)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .show(ui, paint_row);
        } else {
            paint_row(ui);
        }
    });
    result.bar_rect = response.response.rect;
    state.last_icon_rects = result.icon_rects.clone();
    remember_bar_rect(ctx, result.bar_rect);
    ctx.data_mut(|d| d.insert_temp(memory_id, memory));
    // The existing full-screen catalog remains available to the live tuner.
    if let Some(id) = state.advanced {
        if let Some(item) = visible.iter().find(|item| item.id == id) {
            let adv = show_advanced_overlay(
                ctx,
                state_id.with(("advanced", id)),
                tokens,
                canvas,
                item.label,
                1.0,
                |ui| {
                    set_body_layout(ctx, DockBodyLayout::Advanced);
                    set_current_palette(ctx, id, item.label);
                    panel_body(ui, id);
                },
            );
            if adv.minimize {
                state.advanced = None;
            }
            result.overlay_rect = Some(adv.rect);
            nav_push_rect(ctx, adv.rect);
        }
    }
    result
}

fn toggle(state: &mut DockState, id: &'static str, open: bool) {
    state.pinned.retain(|p| *p != id);
    if !open {
        state.pinned.push(id);
    }
    state.body_preview = None;
    state.label_hover = None;
    state.label_suppress = true;
}

fn paint_primary(
    ui: &egui::Ui,
    rect: Rect,
    item: &DockItem<'_>,
    th: &DockThemeTokens,
    tokens: &DockTokens,
) {
    let mut p = tokens.palette.clone();
    let (fill, ink) = tokens.inline.primary_colors(
        ui.visuals().dark_mode,
        matches!(item.icon, DockIcon::Feedback),
    );
    let mut primary_theme = th.clone();
    primary_theme.text = ink.to_array();
    primary_theme.muted_text = fill.to_array();
    primary_theme.icon_active = Color32::TRANSPARENT.to_array();
    ui.painter()
        .circle_filled(rect.center(), (rect.width() * 0.5 - 0.5).max(1.0), fill);
    let primary = FlyoutItem::new(
        item.id,
        item.label,
        item.description,
        None,
        item.icon,
        item.active,
    );
    p.well_fill = 0.0;
    paint_secondary_icon(ui.painter(), rect, &primary, false, &primary_theme, &p, 1.0);
}

#[cfg(test)]
mod tests;
