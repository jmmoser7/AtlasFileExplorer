use super::*;

pub fn cover_flow_home(ui: &Ui, palette: &Palette, model: HomeModel<'_>) -> HomeResult {
    let rect = model
        .host
        .unwrap_or_else(|| ui.available_rect_before_wrap());
    let resp = ui.interact(
        rect,
        model.id.with("home_flow_hit"),
        if model.interactive {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    let painter = ui.painter_at(rect);
    let count = model.covers.len();

    // Background: subtle mesh gradient (soft color blobs over the theme bg).
    if model.backdrop {
        paint_mesh_gradient(&painter, model.id.with("backdrop"), rect, palette);
    }

    // Square covers (album-art aspect), sized and centered by the live tokens.
    let home_tokens = tokens::current().home.clone();
    let flow_center = Pos2::new(
        rect.center().x,
        rect.min.y + rect.height() * home_tokens.center_y_frac,
    );
    let raw_cover = rect.height() * home_tokens.cover_frac;
    let cover = if model.honor_cover_limits {
        raw_cover.clamp(home_tokens.cover_min, home_tokens.cover_max)
    } else {
        raw_cover
    };
    // Keep the reflecting plane and caption clear of the bottom CTA on short
    // windows too. The configured minimum is a preference, not an overflow.
    let cover = cover.min(
        rect.height() * (0.90 - home_tokens.center_y_frac).max(0.01)
            / (0.5 + home_tokens.reflection_height_frac + 0.09),
    );
    let (cover_w, cover_h) = (cover, cover);
    let tuning = CoverFlowTuning::from_tokens(&home_tokens, cover);

    let state_id = model.id.with("home_flow_state");
    let mut flow = ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_insert_with(state_id, CoverFlowState::default)
            .clone()
    });

    let (focus, keyboard_action, pointer_down) = advance_flow(
        ui,
        &resp,
        &mut flow,
        count,
        model.focus,
        FlowInput {
            drag: model.interactive,
            wheel: model.interactive,
            discrete_album_wheel: false,
            keyboard: true,
        },
        &tuning,
    );
    let mut action = keyboard_action;

    if count > 0 {
        // Full bleed: paint enough slots that the rack runs off both screen
        // edges (projected side spacing shrinks with depth).
        let projected_step =
            tuning.side_step * tuning.focal / (tuning.focal + tuning.depth_max).max(1.0);
        let max_slot =
            (((rect.width() * 0.5 + cover_w) / projected_step.max(8.0)).ceil() as i32).clamp(3, 64);
        let slots = visible_slots(flow.position, count, max_slot);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let mut hit: Option<(i32, usize)> = None;
        let mut removal = None;

        for &(slot, logical) in &slots {
            let off = slot_visual_offset(flow.position, slot, count);
            let quad = project_cover(flow_center, cover_w, cover_h, off, &tuning);
            let cover = &model.covers[logical];
            paint_cover(
                &painter,
                model.id.with(("cover_paint", slot)),
                palette,
                flow_center,
                cover_w,
                cover_h,
                off,
                &tuning,
                cover.texture,
                cover.placeholder,
                &cover.title,
                &model.title_font,
            );
            if let Some(p) = pointer {
                if point_in_quad(p, &quad) {
                    hit = Some((slot, logical));
                    if model.host.is_none() && model.interactive && !cover.placeholder {
                        let size = (cover_h * 0.055).max(12.0);
                        let close = Rect::from_center_size(
                            quad.br - Vec2::splat(size * 0.8),
                            Vec2::splat(size),
                        );
                        let response = ui
                            .interact(
                                close,
                                model.id.with(("remove-recent", slot)),
                                Sense::click(),
                            )
                            .on_hover_text("Remove from recent files");
                        painter.text(
                            close.center(),
                            Align2::CENTER_CENTER,
                            "×",
                            FontId::proportional(size),
                            palette.ink,
                        );
                        if response.clicked() {
                            removal = Some(logical);
                        }
                    }
                }
            }
        }

        // Subtle label under the focused projected cover.
        if let Some(cover) = model.covers.get(focus) {
            paint_caption(
                ui,
                &painter,
                model.id,
                Pos2::new(
                    rect.center().x,
                    flow_center.y + cover_h * (0.5 + tuning.reflection_height + 0.045),
                ),
                cover,
                cover_h * 0.033,
                palette.sub,
            );
        }

        // Click / tap — suppressed when the press only stopped momentum.
        if model.interactive && resp.clicked() && !flow.stop_gesture {
            if let Some((_slot, logical)) = hit {
                if logical == focus {
                    action = Some(HomeAction::Open(logical));
                } else {
                    let off = slot_visual_offset(flow.position, _slot, count);
                    flow.target = (flow.position + off).round();
                    flow.phase = InteractionPhase::Snapping;
                    ui.ctx().request_repaint();
                }
            }
        }
        if let Some(index) = removal {
            action = Some(HomeAction::Remove(index));
        }
    }

    if flow.stop_gesture && (!pointer_down || resp.clicked()) {
        flow.stop_gesture = false;
    }

    ui.ctx().data_mut(|d| d.insert_temp(state_id, flow));

    // Home actions share the same pill styling. Opening is the album flow.
    if model.interactive && !model.new_label.is_empty() {
        let type_px = rect.height() * (14.0 / 780.0);
        let cta = match model.cta {
            HomeCta::Bottom => Pos2::new(rect.center().x, rect.max.y - type_px * 3.7),
            HomeCta::Center => rect.center(),
        };
        if let Some(index) = pill_buttons(
            ui,
            &painter,
            cta,
            [model.new_label, model.secondary_label.unwrap_or("")],
            palette.accent,
            Color32::WHITE,
            type_px,
        ) {
            action = Some(if index == 0 {
                HomeAction::New
            } else {
                HomeAction::Secondary
            });
        }
    }

    HomeResult { action, focus }
}

#[derive(Clone)]
struct Caption {
    text: String,
    previous: String,
    changed: f64,
}

fn paint_caption(
    ui: &Ui,
    painter: &egui::Painter,
    id: Id,
    position: Pos2,
    cover: &HomeCover,
    size: f32,
    color: Color32,
) {
    const FADE: f64 = 0.12;
    let date = cover
        .modified_at
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds as i64, 0))
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%y-%m-%d")
                .to_string()
        });
    let text = if let Some(date) = date {
        format!("{}  ·  {date}", cover.title)
    } else {
        cover.title.clone()
    };
    let now = ui.input(|input| input.time);
    let id = id.with("home-caption");
    let mut caption = ui
        .ctx()
        .data(|data| data.get_temp::<Caption>(id))
        .unwrap_or(Caption {
            text: text.clone(),
            previous: String::new(),
            changed: now - FADE,
        });
    if caption.text != text {
        caption.previous = std::mem::replace(&mut caption.text, text);
        caption.changed = now;
    }
    let alpha = ((now - caption.changed) / FADE).clamp(0.0, 1.0) as f32;
    if alpha < 1.0 {
        painter.text(
            position,
            Align2::CENTER_TOP,
            &caption.previous,
            FontId::proportional(size),
            color.gamma_multiply(1.0 - alpha),
        );
        ui.ctx().request_repaint();
    }
    painter.text(
        position,
        Align2::CENTER_TOP,
        &caption.text,
        FontId::proportional(size),
        color.gamma_multiply(alpha),
    );
    ui.ctx().data_mut(|data| data.insert_temp(id, caption));
}

fn pill_buttons(
    ui: &Ui,
    painter: &egui::Painter,
    center: Pos2,
    labels: [&str; 2],
    fill: Color32,
    text: Color32,
    type_px: f32,
) -> Option<usize> {
    if type_px < crate::canvas_text::consts::MIN_RENDER_PX {
        return None;
    }
    let pad = Vec2::new(type_px * 1.28, type_px * 0.57);
    let gap = type_px * 0.8;
    let galleys = labels.map(|label| {
        (!label.is_empty())
            .then(|| painter.layout_no_wrap(label.to_owned(), FontId::proportional(type_px), text))
    });
    let width = galleys
        .iter()
        .flatten()
        .map(|galley| galley.size().x + pad.x * 2.0)
        .sum::<f32>()
        + gap * galleys.iter().flatten().count().saturating_sub(1) as f32;
    let mut left = center.x - width * 0.5;
    let mut clicked = None;
    for (index, galley) in galleys.into_iter().enumerate() {
        let Some(galley) = galley else {
            continue;
        };
        let size = galley.size() + pad * 2.0;
        let rect = Rect::from_center_size(Pos2::new(left + size.x * 0.5, center.y), size);
        let id = ui.id().with(("home_pill", galley.text()));
        let resp = ui.interact(rect, id, Sense::click());
        let fill = if resp.hovered() {
            fill.gamma_multiply(1.08)
        } else {
            fill
        };
        let radius = (type_px * 1.14).round().clamp(4.0, 32.0) as u8;
        painter.rect_filled(rect, CornerRadius::same(radius), fill);
        painter.galley(rect.center() - galley.size() * 0.5, galley, text);
        if resp.clicked() {
            clicked = Some(index);
        }
        left += size.x + gap;
    }
    clicked
}
