//! Shared image-agent editor embedded in the image selection blister.

use super::*;

/// A prior image result shown as a circular, draggable thumbnail.
#[derive(Clone, Copy)]
pub struct ImageAgentHistoryItem<'a> {
    pub label: &'a str,
    pub thumb: Option<egui::TextureId>,
}

/// A source image already wired into the image agent's input set.
#[derive(Clone, Copy)]
pub struct ImageAgentInput<'a> {
    pub label: &'a str,
    pub thumb: Option<egui::TextureId>,
}

/// State and data for the image-specific agent presentation. The app keeps
/// ownership of prompts, models, source identity, and generation lifecycle.
pub struct ImageAgentOptions<'a> {
    pub history: &'a [ImageAgentHistoryItem<'a>],
    pub selected_history: Option<usize>,
    pub inputs: &'a [ImageAgentInput<'a>],
    pub running: bool,
    pub live_active: bool,
    pub status: Option<&'a str>,
    pub settings_open: &'a mut bool,
}

#[derive(Default)]
pub struct ImageAgentEditResponse {
    pub mode: Option<usize>,
    pub model: Option<usize>,
    pub count: Option<u8>,
    pub aspect: Option<usize>,
    pub seed_locked: Option<bool>,
    pub live: Option<bool>,
    pub submit: bool,
    pub prompt: PromptField,
    pub hovered_history: Option<usize>,
    pub clicked_history: Option<usize>,
    pub drag_started_history: Option<usize>,
    pub history_drag_position: Option<Pos2>,
    pub hovered_input: Option<usize>,
    pub clicked_input: Option<usize>,
    pub popups: Vec<Rect>,
}

/// Shared image-agent height in board units. Prompt lines are measured by the
/// app's prompt source; input descriptors add one compact row when present.
pub fn image_agent_height(prompt_lines: usize, input_count: usize, has_status: bool) -> f32 {
    let prompt_height = (22.0 + prompt_lines.clamp(1, 8) as f32 * 18.0).max(AGENT_PROMPT_HEIGHT);
    let rows = (input_count > 0) as usize + has_status as usize;
    24.0 + 24.0
        + prompt_height
        + if input_count > 0 { 28.0 } else { 0.0 }
        + if has_status { 18.0 } else { 0.0 }
        + 6.0 * (rows as f32 + 1.0)
}

/// Image-specific agent editor for the image blister. It keeps the common
/// prompt and menu semantics while replacing the generic capsule rows with
/// compact image controls and circular history/input chips.
pub fn image_agent_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    edit: AgentEdit<'_>,
    mut options: ImageAgentOptions<'_>,
    zoom: f32,
    theme: Palette,
) -> ImageAgentEditResponse {
    let mut out = ImageAgentEditResponse::default();
    let inner = rect.shrink(12.0 * zoom);
    let row_h = 24.0 * zoom;
    let gap = 6.0 * zoom;
    let top = Rect::from_min_size(inner.min, Vec2::new(inner.width(), row_h));
    let mut x = top.left();
    let model_width = (inner.width() * 0.27).min(126.0 * zoom);
    let model_rect = Rect::from_min_size(Pos2::new(x, top.top()), Vec2::new(model_width, row_h));
    let model = bare_menu(
        ui,
        ui.id().with("image-agent-model"),
        model_rect,
        edit.models.get(edit.model).copied().unwrap_or("Auto"),
        edit.models,
        edit.model,
        edit.model_open,
        zoom,
        theme,
    );
    out.model = model.picked;
    out.popups.extend(model.popup);
    if model.opened {
        *edit.aspect_open = false;
        *options.settings_open = false;
    }
    x = model_rect.right() + gap;
    let ratio_width = (inner.width() * 0.18).min(82.0 * zoom);
    let aspect_rect = Rect::from_min_size(Pos2::new(x, top.top()), Vec2::new(ratio_width, row_h));
    let aspect = bare_menu(
        ui,
        ui.id().with("image-agent-aspect"),
        aspect_rect,
        edit.aspects.get(edit.aspect).copied().unwrap_or("Source"),
        edit.aspects,
        edit.aspect,
        edit.aspect_open,
        zoom,
        theme,
    );
    out.aspect = aspect.picked;
    out.popups.extend(aspect.popup);
    if aspect.opened {
        *edit.model_open = false;
        *options.settings_open = false;
    }
    x = aspect_rect.right() + gap;
    let mode_rect = Rect::from_min_size(Pos2::new(x, top.top()), Vec2::new(50.0 * zoom, row_h));
    let mode_response = ui.interact(
        mode_rect,
        ui.id().with("image-agent-live-mode"),
        Sense::click(),
    );
    canvas_text::text(
        ui.painter(),
        mode_rect.center(),
        Align2::CENTER_CENTER,
        if options.live_active {
            "Live⌄"
        } else {
            "Single⌄"
        },
        canvas_scale::font(8.5, zoom),
        if mode_response.hovered() {
            theme.ink
        } else {
            theme.sub
        },
    );
    if mode_response.clicked() {
        out.live = Some(!options.live_active);
    }
    x = mode_rect.right() + gap;
    let play_width = 20.0 * zoom;
    let count_width = 32.0 * zoom;
    let play_rect = Rect::from_min_size(
        Pos2::new(top.right() - play_width, top.top()),
        Vec2::splat(play_width),
    );
    let count_rect = Rect::from_min_size(
        Pos2::new(play_rect.left() - count_width, top.top()),
        Vec2::new(count_width, row_h),
    );
    let settings_width = (76.0 * zoom).min((count_rect.left() - gap - x).max(0.0));
    let settings_rect =
        Rect::from_min_size(Pos2::new(x, top.top()), Vec2::new(settings_width, row_h));
    let settings_options = ["Seed unlocked", "Seed locked"];
    let settings = bare_menu(
        ui,
        ui.id().with("image-agent-settings"),
        settings_rect,
        "Settings",
        &settings_options,
        usize::from(edit.seed_locked),
        options.settings_open,
        zoom,
        theme,
    );
    out.seed_locked = settings.picked.map(|picked| picked == 1);
    out.popups.extend(settings.popup);
    if settings.opened {
        *edit.model_open = false;
        *edit.aspect_open = false;
    }

    let history_left = settings_rect.right() + gap;
    let history_right = count_rect.left() - gap;
    let history_width = (history_right - history_left).max(0.0);
    if !options.history.is_empty() && history_width > 0.0 {
        let spacing = 3.0 * zoom;
        let max_diameter = 26.0 * zoom;
        let min_diameter = 10.0 * zoom;
        let visible_count = ((history_width + spacing) / (min_diameter + spacing))
            .floor()
            .max(1.0) as usize;
        let visible_count = visible_count.min(options.history.len());
        let diameter = ((history_width - spacing * visible_count.saturating_sub(1) as f32)
            / visible_count as f32)
            .clamp(min_diameter, max_diameter);
        let start = options
            .selected_history
            .unwrap_or(0)
            .saturating_sub(visible_count / 2)
            .min(options.history.len().saturating_sub(visible_count));
        for index in start..start + visible_count {
            let item = options.history[index];
            let circle = Rect::from_min_size(
                Pos2::new(
                    history_left + (index - start) as f32 * (diameter + spacing),
                    top.center().y - diameter * 0.5,
                ),
                Vec2::splat(diameter),
            );
            let response = ui.interact(
                circle,
                ui.id().with(("image-agent-history", index)),
                Sense::click_and_drag(),
            );
            paint_agent_thumbnail(
                ui.painter(),
                circle,
                item.thumb,
                index,
                options.selected_history == Some(index),
                response.hovered(),
                zoom,
                theme,
            );
            if response.hovered() {
                out.hovered_history = Some(index);
            }
            if response.clicked() {
                out.clicked_history = Some(index);
            }
            if response.drag_started() {
                out.drag_started_history = Some(index);
            }
            if response.dragged() {
                out.history_drag_position = response.interact_pointer_pos();
            }
            let response = response.on_hover_text(item.label);
            if response.hovered() {
                if let Some(texture) = item.thumb {
                    let preview = Rect::from_center_size(
                        circle.center() - Vec2::new(0.0, 54.0 * zoom),
                        Vec2::splat(92.0 * zoom),
                    );
                    egui::Area::new(ui.id().with(("image-agent-preview", index)))
                        .order(egui::Order::Tooltip)
                        .fixed_pos(preview.min)
                        .interactable(false)
                        .show(ui.ctx(), |ui| {
                            ui.add(egui::Image::new((texture, preview.size())).corner_radius(
                                egui::CornerRadius::same((5.0 * zoom).round() as u8),
                            ));
                        });
                }
            }
        }
    }

    let mut y = top.bottom() + gap;
    if !options.inputs.is_empty() {
        let diameter = 22.0 * zoom;
        let mut x = inner.left();
        for (index, input) in options.inputs.iter().enumerate() {
            let circle = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(diameter));
            let response = ui.interact(
                circle,
                ui.id().with(("image-agent-input", index)),
                Sense::click(),
            );
            paint_agent_thumbnail(
                ui.painter(),
                circle,
                input.thumb,
                index,
                false,
                response.hovered(),
                zoom,
                theme,
            );
            if response.hovered() {
                out.hovered_input = Some(index);
            }
            if response.clicked() {
                out.clicked_input = Some(index);
            }
            response.on_hover_text(input.label);
            x += diameter + 5.0 * zoom;
        }
        y += 28.0 * zoom;
    }

    if let Some(status) = options.status {
        let status_rect = Rect::from_min_size(
            Pos2::new(inner.left(), y),
            Vec2::new(inner.width(), 14.0 * zoom),
        );
        canvas_text::text(
            ui.painter(),
            status_rect.left_center(),
            Align2::LEFT_CENTER,
            status,
            canvas_scale::font(8.0, zoom),
            theme.sub,
        );
        y += 18.0 * zoom;
    }
    let prompt_bottom = inner.bottom();
    let face_top = (y + gap).min(prompt_bottom - AGENT_PROMPT_HEIGHT * zoom);
    let face = Rect::from_min_max(
        Pos2::new(inner.left(), face_top),
        Pos2::new(inner.right(), prompt_bottom),
    );
    ui.painter().rect(
        face,
        5.0 * zoom,
        theme.bg,
        Stroke::new(0.7 * zoom, theme.border),
        egui::StrokeKind::Inside,
    );
    out.prompt = prompt_field(
        ui,
        ui.id().with("image-agent-prompt"),
        face.shrink(8.0 * zoom),
        zoom,
        edit.prompt,
        edit.hint,
        true,
        edit.take_focus,
        theme,
    );

    let play_response = ui.interact(play_rect, ui.id().with("image-agent-play"), Sense::click());
    let count_hover = ui.interact(
        count_rect,
        ui.id().with("image-agent-count"),
        Sense::hover(),
    );
    if count_hover.hovered() || play_response.hovered() {
        let minus =
            Rect::from_min_size(count_rect.min, Vec2::new(13.0 * zoom, count_rect.height()));
        let plus = Rect::from_min_size(
            Pos2::new(count_rect.right() - 13.0 * zoom, count_rect.top()),
            Vec2::new(13.0 * zoom, count_rect.height()),
        );
        if chip(
            ui,
            minus,
            ui.id().with("image-agent-count-minus"),
            "−",
            false,
            zoom,
            theme,
        ) && edit.count > 1
        {
            out.count = Some(edit.count - 1);
        }
        if chip(
            ui,
            plus,
            ui.id().with("image-agent-count-plus"),
            "+",
            false,
            zoom,
            theme,
        ) && edit.count < 8
        {
            out.count = Some(edit.count.max(1) + 1);
        }
    }
    if count_hover.hovered() || play_response.hovered() {
        canvas_text::text(
            ui.painter(),
            count_rect.center(),
            Align2::CENTER_CENTER,
            edit.count.max(1).to_string(),
            canvas_scale::font(8.0, zoom),
            theme.sub,
        );
    }
    let ink = if play_response.hovered() {
        theme.ink
    } else {
        theme.sub
    };
    if options.running {
        icons::paint(ui.painter(), play_rect, Icon::Stop, ink);
    } else {
        let r = play_rect.shrink(5.0 * zoom);
        ui.painter().add(egui::Shape::convex_polygon(
            vec![r.left_top(), r.left_bottom(), r.right_center()],
            ink,
            Stroke::NONE,
        ));
    }
    let play_response = play_response.on_hover_text(if options.running {
        "Stop generation"
    } else if options.live_active {
        "Keep this result"
    } else {
        "Generate"
    });
    out.submit = play_response.clicked() || out.prompt.submit;
    out
}
fn paint_agent_thumbnail(
    painter: &egui::Painter,
    rect: Rect,
    texture: Option<egui::TextureId>,
    index: usize,
    selected: bool,
    hovered: bool,
    zoom: f32,
    theme: Palette,
) {
    let radius = rect.width() * 0.5;
    let radio = FilterRadio {
        label: "",
        fill: [72, 82, 92],
        fill_b: None,
        thumb: texture,
    };
    paint_filter_radio(
        painter,
        rect.center(),
        radius,
        &radio,
        selected,
        hovered,
        zoom,
        theme,
    );
    if texture.is_none() {
        canvas_text::text(
            painter,
            rect.center(),
            Align2::CENTER_CENTER,
            (index + 1).to_string(),
            canvas_scale::font(8.0, zoom),
            theme.ink,
        );
    }
}
