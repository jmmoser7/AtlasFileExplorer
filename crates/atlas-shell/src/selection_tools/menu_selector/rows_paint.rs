//! One row painter and its split provider hit ownership; popup navigation stays in the parent.
use super::presentation::studio_layout;
use super::*;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RowIntent {
    #[default]
    None,
    Primary,
    Secondary,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn paint(
    ui: &mut egui::Ui,
    id: Id,
    i: usize,
    choice: SelectorRow<'_>,
    hint: Option<&str>,
    action_icon: Option<crate::menu::MenuIcon>,
    row_h: f32,
    zoom: f32,
    theme: Palette,
    presentation: Presentation,
    rich: bool,
) -> RowIntent {
    let agent_datums = presentation == Presentation::ModelPicker;
    let studio = presentation.studio();
    let leading_check = matches!(
        presentation,
        Presentation::ImageOptions | Presentation::ElementOptions
    );
    let mut theme = theme;
    if leading_check && !choice.interactive {
        theme.ink = theme.ink.gamma_multiply(0.4);
        theme.sub = theme.sub.gamma_multiply(0.4);
        theme.card_hover = theme.card_hover.gamma_multiply(0.4);
        theme.danger = theme.danger.gamma_multiply(0.4);
    }
    let mut intent = RowIntent::None;
    let option = choice.label;
    let (_, row) = ui.allocate_space(Vec2::new(ui.available_width(), row_h));
    let select_row = if agent_datums && choice.secondary {
        Rect::from_min_max(row.min, Pos2::new(row.right() - 34.0 * zoom, row.bottom()))
    } else {
        row
    };
    let response = ui.interact(
        select_row,
        super::keyboard::primary(id, i),
        if choice.interactive {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if agent_datums && keyboard::scroll_requested(ui.ctx(), id, response.id) {
        response.scroll_to_me(None);
    }
    let key_response = (choice.secondary && choice.interactive).then(|| {
        let key_hit = Rect::from_min_max(
            Pos2::new(
                row.right() - if agent_datums { 34.0 } else { 30.0 } * zoom,
                row.top(),
            ),
            row.max,
        );
        ui.interact(key_hit, id.with(("secondary", i)), Sense::click())
    });
    if let Some(key) = key_response
        .as_ref()
        .filter(|key| agent_datums && keyboard::scroll_requested(ui.ctx(), id, key.id))
    {
        key.scroll_to_me(None);
    }
    let on = choice.selected;
    if (on && (!studio || presentation == Presentation::ImageOptions))
        || (choice.interactive && (response.hovered() || response.has_focus()))
    {
        ui.painter().rect_filled(
            select_row,
            if studio { 4.5 } else { 6.0 } * zoom,
            if on && !studio {
                theme.select_fill
            } else {
                theme.card_hover
            },
        );
    }
    if let Some(key) = &key_response {
        if agent_datums && (key.hovered() || key.has_focus()) {
            ui.painter()
                .rect_filled(key.rect, 4.5 * zoom, theme.card_hover);
        }
    }
    let back = choice.submenu == Some(Icon::ChevronLeft);
    let label_left = row.left()
        + if presentation == Presentation::PlainActions {
            8.0
        } else if presentation == Presentation::ElementOptions {
            31.5
        } else if studio {
            33.0
        } else if choice.icon.is_some() {
            30.0
        } else if back {
            22.0
        } else {
            8.0
        } * zoom;
    if presentation == Presentation::ElementOptions && choice.checked == Some(true) {
        canvas_text::text(
            ui.painter(),
            Pos2::new(row.left() + 15.5 * zoom, row.center().y),
            Align2::CENTER_CENTER,
            "✓",
            canvas_scale::font(13.0, zoom),
            theme.ink,
        );
    } else if leading_check && choice.checked == Some(true) {
        icons::paint(
            ui.painter(),
            Rect::from_center_size(
                Pos2::new(
                    row.left()
                        + if presentation == Presentation::ElementOptions {
                            15.5
                        } else {
                            16.25
                        } * zoom,
                    row.center().y,
                ),
                Vec2::splat(16.5 * zoom),
            ),
            Icon::MenuCheck,
            theme.ink,
        );
    } else if let Some(icon) = choice.icon {
        icons::paint(
            ui.painter(),
            Rect::from_center_size(
                Pos2::new(
                    row.left() + if studio { 16.25 } else { 16.0 } * zoom,
                    row.center().y,
                ),
                Vec2::splat(16.5 * zoom),
            ),
            icon,
            if studio { theme.ink } else { theme.sub },
        );
    } else if let Some(icon) = action_icon {
        crate::menu::paint_icon(
            ui.painter(),
            icon,
            Pos2::new(row.left() + 16.25 * zoom, row.center().y),
            16.5 * zoom,
            0.85 * zoom,
            theme.ink,
        );
    }
    let detail_right = select_row.right()
        - if presentation == Presentation::ImageOptions {
            if choice.submenu.is_some() {
                27.5
            } else {
                8.0
            }
        } else if presentation == Presentation::ElementOptions {
            8.0
        } else if agent_datums {
            26.5
        } else if (choice.submenu.is_some() && !back) || choice.checked.is_some() {
            22.0
        } else {
            8.0
        } * zoom;
    let detail_width = choice.detail.map_or(0.0, |detail| {
        if studio {
            studio_layout(
                ui.painter(),
                detail,
                if presentation == Presentation::ImageOptions {
                    11.0
                } else {
                    12.0
                },
                f32::INFINITY,
                theme.sub,
                presentation,
            )
            .size()
            .x
        } else {
            canvas_text::layout_no_wrap(
                ui.painter(),
                detail.to_owned(),
                canvas_scale::font(12.0, zoom),
                theme.sub,
            )
            .size()
            .x
        }
        .min(
            if presentation == Presentation::ImageOptions {
                115.0
            } else {
                112.0
            } * zoom,
        )
    });
    let label_right = if studio && choice.detail.is_some() {
        detail_right - detail_width - 8.5 * zoom
    } else if choice.detail.is_some() {
        row.left() + row.width() * 0.48
    } else {
        detail_right
    };
    let label_rect = Rect::from_min_max(
        Pos2::new(label_left, row.top()),
        Pos2::new(label_right, row.bottom()),
    );
    let label_color = if choice.danger {
        theme.danger
    } else if !choice.interactive {
        if studio {
            if agent_datums {
                theme.ink.gamma_multiply(0.65)
            } else {
                theme.ink
            }
        } else {
            theme.sub
        }
    } else if studio {
        theme.ink
    } else if on {
        theme.select_stroke
    } else if needs_key_label(option) {
        theme.sub
    } else {
        theme.ink
    };
    if studio {
        let galley = studio_layout(
            ui.painter(),
            option,
            13.0,
            (label_rect.width() - 4.0 * zoom).max(0.0),
            label_color,
            presentation,
        );
        super::presentation::ink_painter(ui, label_rect, zoom).galley(
            Align2::LEFT_CENTER
                .anchor_rect(Rect::from_min_size(label_rect.left_center(), galley.size()))
                .min,
            galley,
            label_color,
        );
    } else {
        canvas_text::layout_rows(
            &super::presentation::ink_painter(ui, label_rect, zoom),
            option.to_owned(),
            canvas_scale::font(if rich { 13.0 } else { 11.0 }, zoom),
            if choice.danger {
                theme.danger
            } else if !choice.interactive {
                theme.sub
            } else if on {
                theme.select_stroke
            } else if needs_key_label(option) {
                theme.sub
            } else {
                theme.ink
            },
            (label_rect.width() - 4.0 * zoom).max(0.0),
            1,
        )
        .paint_anchored(
            &super::presentation::ink_painter(ui, label_rect, zoom),
            label_rect.left_center(),
            Align2::LEFT_CENTER,
            label_color,
        );
    }
    if let Some(detail) = choice.detail {
        let value_color = if agent_datums && !choice.interactive {
            theme.sub.gamma_multiply(0.65 * 0.85)
        } else {
            theme.sub
        };
        let detail_rect = Rect::from_min_max(
            Pos2::new(
                label_right + if studio { 8.5 } else { 6.0 } * zoom,
                row.top(),
            ),
            Pos2::new(detail_right, row.bottom()),
        );
        if studio {
            let galley = studio_layout(
                ui.painter(),
                detail,
                if presentation == Presentation::ImageOptions {
                    11.0
                } else {
                    12.0
                },
                (detail_rect.width() - 4.0 * zoom).max(0.0),
                value_color,
                presentation,
            );
            super::presentation::ink_painter(ui, detail_rect, zoom).galley(
                Align2::RIGHT_CENTER
                    .anchor_rect(Rect::from_min_size(
                        detail_rect.right_center(),
                        galley.size(),
                    ))
                    .min,
                galley,
                value_color,
            );
        } else {
            canvas_text::layout_rows(
                &super::presentation::ink_painter(ui, detail_rect, zoom),
                detail.to_owned(),
                canvas_scale::font(if rich { 12.0 } else { 11.0 }, zoom),
                theme.sub,
                (detail_rect.width() - 4.0 * zoom).max(0.0),
                1,
            )
            .paint_anchored(
                &super::presentation::ink_painter(ui, detail_rect, zoom),
                detail_rect.right_center(),
                Align2::RIGHT_CENTER,
                theme.sub,
            );
        }
    }
    if choice.checked == Some(true) && !leading_check {
        crate::menu::paint_icon(
            ui.painter(),
            crate::menu::MenuIcon::Check,
            Pos2::new(
                select_row.right() - if agent_datums { 13.0 } else { 11.0 } * zoom,
                row.center().y,
            ),
            12.0 * zoom,
            1.4 * zoom,
            theme.ink,
        );
    }
    if let Some(icon) = choice.submenu {
        let icon_rect = Rect::from_center_size(
            Pos2::new(
                if back {
                    row.left() + 11.0 * zoom
                } else if presentation == Presentation::ImageOptions {
                    row.right() - 13.5 * zoom
                } else if agent_datums && choice.secondary {
                    row.right() - 17.0 * zoom
                } else {
                    row.right() - if agent_datums { 13.0 } else { 11.0 } * zoom
                },
                row.center().y,
            ),
            Vec2::splat(
                if presentation == Presentation::ImageOptions {
                    11.0
                } else {
                    7.0
                } * zoom,
            ),
        );
        if choice.expanded {
            generator::activity::paint_chevron_glyph(
                ui.painter(),
                icon_rect.center(),
                zoom,
                false,
                theme.sub,
            );
        } else {
            icons::paint(ui.painter(), icon_rect, icon, theme.sub);
        }
        if choice.secondary && choice.interactive {
            let key_hit = Rect::from_min_max(
                Pos2::new(
                    row.right() - if agent_datums { 34.0 } else { 30.0 } * zoom,
                    row.top(),
                ),
                row.max,
            );
            let key = key_response
                .as_ref()
                .expect("interactive secondary response");
            if agent_datums {
                ui.painter().line_segment(
                    [
                        Pos2::new(key_hit.left(), row.top()),
                        Pos2::new(key_hit.left(), row.bottom()),
                    ],
                    Stroke::new(0.65 * zoom, theme.border_strong),
                );
            }
            key.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!("{option} API key options"),
                )
            });
            if key.clicked() {
                intent = RowIntent::Secondary;
            }
            key.clone()
                .on_hover_text("Add or change the API key without changing provider");
        }
    }
    if choice.interactive
        && response.clicked()
        && (!choice.secondary
            || response
                .interact_pointer_pos()
                .is_none_or(|p| p.x < row.right() - if agent_datums { 34.0 } else { 30.0 } * zoom))
    {
        intent = RowIntent::Primary;
    }
    if studio {
        let identity = choice
            .detail
            .map_or_else(|| option.to_owned(), |detail| format!("{option}: {detail}"));
        response.widget_info(|| {
            if let Some(checked) = choice.checked {
                egui::WidgetInfo::selected(
                    if option == "Preserve model history" {
                        egui::WidgetType::Checkbox
                    } else {
                        egui::WidgetType::RadioButton
                    },
                    choice.interactive,
                    checked,
                    &identity,
                )
            } else {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, choice.interactive, &identity)
            }
        });
        response.on_hover_text(hint.unwrap_or(&identity));
    }

    intent
}
