//! Primary-bar placement; left-edge chrome retains its existing behavior.
use super::*;

pub(super) struct PrimaryBar {
    pub bar_rect: Rect,
    pub icon_rects: HashMap<&'static str, Rect>,
    pub hovered_icon: Option<&'static str>,
    pub label_hover_candidate: Option<&'static str>,
    pub label_blocked_by_open: bool,
    pub panel_owns: bool,
    pub palette_hover: Option<&'static str>,
    pub overlay_rect: Option<Rect>,
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
    side: DockSide,
    now: f64,
    clicked: &mut Option<&'static str>,
    drop_to_canvas: &mut Option<&'static str>,
    panel_body: &mut impl FnMut(&mut egui::Ui, &'static str),
) -> PrimaryBar {
    if super::inline::hosts(side, visible) {
        return super::inline::show(
            ctx,
            state_id,
            state,
            visible,
            tokens,
            th,
            canvas,
            now,
            clicked,
            drop_to_canvas,
            panel_body,
        );
    }
    // ---- icon strip ----
    let bar_area = match side {
        DockSide::LeftCenter => egui::Area::new(state_id.with("bar"))
            .order(egui::Order::Foreground)
            .pivot(Align2::LEFT_CENTER)
            .fixed_pos(Pos2::new(
                canvas.left() + tokens.left_margin,
                canvas.center().y,
            ))
            .constrain(false),
        DockSide::BottomCenter => egui::Area::new(state_id.with("bar"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(Pos2::new(
                canvas.center().x,
                canvas.bottom() - tokens.bottom_margin,
            ))
            .constrain(false),
    };

    let pointer = ctx.pointer_latest_pos();
    let panel_owns = panel_owns_pointer(state.last_union_panels, pointer);
    let palette_hover: Option<&'static str> = pointer.and_then(|p| {
        state
            .last_panel_rects
            .iter()
            .find(|(_, r)| r.expand(4.0).contains(p))
            .map(|(id, _)| *id)
    });
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(DOCK_SIDE_ID), side));

    let mut icon_rects: HashMap<&'static str, Rect> = HashMap::new();
    let mut label_hover_candidate: Option<&'static str> = None;
    // Pointer is over a pinned / volatile-open icon — previous chips must die
    // immediately (the bar still counts as "inside", so close-delay alone
    // would leave the prior name stuck).
    let mut label_blocked_by_open = false;
    let mut hovered_icon: Option<&'static str> = None;
    let bar_response = (!state.bar_collapsed).then(|| {
        bar_area.show(ctx, |ui| {
            let mut draw_items = |ui: &mut egui::Ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(tokens.icon_gap);
                for item in visible {
                    if item.gap_before {
                        ui.add_space(tokens.icon_gap * 1.5);
                    }
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::splat(tokens.icon_size), Sense::click());
                    icon_rects.insert(item.id, rect);
                    let hovered = chip_from_icon_hover(resp.hovered(), panel_owns);
                    let is_pinned = state.pinned.contains(&item.id);
                    let is_preview = state.body_preview == Some(item.id);
                    let associated = palette_hover == Some(item.id)
                        || (resp.hovered() && (is_pinned || is_preview));
                    let hover_t = ui.ctx().animate_bool_with_time(
                        state_id.with(("icon_hover", item.id)),
                        resp.hovered(),
                        tokens.palette.hover_fade,
                    );
                    let assoc_t = ui.ctx().animate_bool_with_time(
                        state_id.with(("icon_assoc", item.id)),
                        associated,
                        tokens.palette.hover_fade,
                    );
                    let dark = ui.visuals().dark_mode;
                    let base = primary_plate_fill(th, &tokens.palette, dark);
                    let fill = if assoc_t > 0.001 {
                        let shaded = associate_shade(
                            base,
                            dark,
                            tokens
                                .palette
                                .associate_tint
                                .max(tokens.palette.icon_hover_tint),
                        );
                        mix_icon_fill(base, shaded, assoc_t)
                    } else if item.active || is_pinned || is_preview {
                        mix_icon_fill(base, th.icon_active_color(), ICON_ACTIVE_MIX)
                    } else {
                        let hover_c = th.icon_hover_color();
                        let hover_c = Color32::from_rgba_unmultiplied(
                            hover_c.r(),
                            hover_c.g(),
                            hover_c.b(),
                            (hover_c.a() as f32 * tokens.palette.icon_hover_opacity).round() as u8,
                        );
                        let hovered_fill =
                            mix_icon_fill(base, hover_c, tokens.palette.icon_hover_fill);
                        let shaded =
                            associate_shade(hovered_fill, dark, tokens.palette.icon_hover_tint);
                        mix_icon_fill(base, shaded, hover_t)
                    };
                    let (outline_w, outline_tint) =
                        icon_outline(associated, is_pinned, &tokens.palette);
                    let outline = if outline_tint > 0.0 {
                        associate_shade(th.border_color(), dark, outline_tint)
                    } else {
                        th.border_color()
                    };
                    paint_squircle(
                        ui.painter(),
                        rect.shrink(0.5),
                        fill,
                        Stroke::new(outline_w, outline),
                        tokens.squircle_exponent,
                    );
                    paint_builtin_icon(ui.painter(), rect.shrink(7.0), item.icon, th.text_color());

                    if resp.hovered() {
                        hovered_icon = Some(item.id);
                    }
                    if hovered {
                        state.last_inside_time = now;
                        // Title chip only when the panel isn't already open —
                        // pinned / volatile bodies carry their own caption.
                        if is_pinned || is_preview {
                            label_blocked_by_open = true;
                        } else {
                            label_hover_candidate = Some(item.id);
                        }
                    }
                    if item.kind.opens_body() && resp.double_clicked() {
                        *clicked = Some(item.id);
                        state.last_inside_time = now;
                        if let Some(idx) = state.pinned.iter().position(|p| *p == item.id) {
                            state.pinned.remove(idx);
                            state.panel_open.remove(&item.id);
                        } else {
                            state.pinned.push(item.id);
                            state.panel_open.insert(item.id, 0.0);
                        }
                        if state.body_preview == Some(item.id) {
                            state.body_preview = None;
                        }
                        // Pin/unpin under a still-hovering cursor must not flash
                        // the name chip back on top of the icon.
                        state.label_hover = None;
                        state.label_hover_since = 0.0;
                        state.describe_blend = 0.0;
                        state.label_suppress = true;
                    } else if resp.clicked() {
                        *clicked = Some(item.id);
                        state.last_inside_time = now;
                        if item.kind.opens_body() {
                            if state.pinned.contains(&item.id) {
                                if let Some(idx) = state.pinned.iter().position(|p| *p == item.id) {
                                    state.pinned.remove(idx);
                                }
                                state.panel_open.remove(&item.id);
                                if state.advanced == Some(item.id) {
                                    state.advanced = None;
                                }
                            } else if state.body_preview == Some(item.id) {
                                state.body_preview = None;
                                state.panel_open.remove(&item.id);
                            } else {
                                // Volatile menu.
                                state.body_preview = Some(item.id);
                                state.panel_open.insert(item.id, 0.0);
                            }
                            state.label_hover = None;
                            state.label_hover_since = 0.0;
                            state.describe_blend = 0.0;
                            state.label_suppress = true;
                        }
                    }
                }
            };
            match side {
                DockSide::LeftCenter => {
                    ui.vertical(|ui| draw_items(ui));
                }
                DockSide::BottomCenter => {
                    ui.horizontal(|ui| draw_items(ui));
                }
            }
        })
    });
    let bar_rect = if let Some(resp) = bar_response {
        state.last_icon_rects = icon_rects.clone();
        remember_bar_rect(ctx, resp.response.rect);
        resp.response.rect
    } else {
        icon_rects = if !state.last_icon_rects.is_empty() {
            state.last_icon_rects.clone()
        } else {
            estimated_icon_rects(visible, tokens, canvas, side)
        };
        Rect::NOTHING
    };

    PrimaryBar {
        bar_rect,
        icon_rects,
        hovered_icon,
        label_hover_candidate,
        label_blocked_by_open,
        panel_owns,
        palette_hover,
        overlay_rect: None,
    }
}

pub(super) fn paint_builtin_icon(
    painter: &egui::Painter,
    rect: Rect,
    icon: DockIcon,
    color: Color32,
) {
    use crate::icons::{self, Icon};
    let icon = match icon {
        DockIcon::Custom(paint) => return paint(painter, rect, color),
        DockIcon::Filters => Icon::Filters,
        DockIcon::Display => Icon::Display,
        DockIcon::Mode => Icon::Mode,
        DockIcon::Workflow => Icon::Workflow,
        DockIcon::Ai => Icon::Ai,
        DockIcon::Tags => Icon::Tags,
        DockIcon::Selection => Icon::Selection,
        DockIcon::View => Icon::View,
        DockIcon::Lens => Icon::Lens,
        DockIcon::Grid => Icon::Grid,
        DockIcon::SnapGrid => Icon::SnapGrid,
        DockIcon::Osnap => Icon::Snap,
        DockIcon::SnapEnd => Icon::SnapEnd,
        DockIcon::SnapMid => Icon::SnapMid,
        DockIcon::SnapCenter => Icon::SnapCenter,
        DockIcon::SnapNear => Icon::SnapNear,
        DockIcon::SnapInt => Icon::SnapInt,
        DockIcon::SnapQuad => Icon::SnapQuad,
        DockIcon::SnapPerp => Icon::SnapPerp,
        DockIcon::SnapTan => Icon::SnapTan,
        DockIcon::Swap => Icon::Swap,
        DockIcon::Reset => Icon::Reset,
        DockIcon::Fit => Icon::Fit,
        DockIcon::Dark => Icon::Dark,
        DockIcon::Ghost => Icon::Ghost,
        DockIcon::Hide => Icon::Hide,
        DockIcon::Duplicates => Icon::Colors,
        DockIcon::Feedback => Icon::Feedback,
        DockIcon::Actions => Icon::Actions,
        DockIcon::ObjectProperties => Icon::ObjectProperties,
        DockIcon::DocumentSettings => Icon::DocumentSettings,
        DockIcon::ModeEdit => Icon::ModeEdit,
    };
    icons::paint(painter, rect, icon, color);
}
