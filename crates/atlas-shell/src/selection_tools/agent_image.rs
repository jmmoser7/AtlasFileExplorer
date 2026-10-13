//! Shared image-agent editor embedded in the image selection blister.

use super::*;
#[path = "agent_image_layout.rs"]
mod layout;
use layout::{image_agent_datum_layout, ImageAgentDatumLayout};

#[cfg(test)]
#[path = "agent_image_tests.rs"]
mod tests;

/// A prior image result shown as a circular, draggable thumbnail.
#[derive(Clone, Copy)]
pub struct ImageAgentHistoryItem<'a> {
    pub label: &'a str,
    pub thumb: Option<egui::TextureId>,
}

/// State and data for the image-specific agent presentation. The app keeps
/// ownership of prompts, models, source identity, and generation lifecycle.
pub struct ImageAgentOptions<'a> {
    pub history: &'a [ImageAgentHistoryItem<'a>],
    pub selected_history: Option<usize>,
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
    pub popups: Vec<Rect>,
}

/// Shared image-agent height in board units. Prompt lines are measured by the
/// app's prompt source. Wired inputs remain on the host's input port.
pub const IMAGE_AGENT_NARROW_WIDTH: f32 = 360.0;
pub const IMAGE_PROMPT_MIN_HEIGHT: f32 = 40.0;

pub fn image_agent_height(prompt_lines: usize, has_status: bool, width_board_units: f32) -> f32 {
    let prompt_height =
        (16.0 + prompt_lines.clamp(1, 8) as f32 * 16.0).max(IMAGE_PROMPT_MIN_HEIGHT);
    let rows = has_status as usize;
    let datum_rows = if width_board_units < IMAGE_AGENT_NARROW_WIDTH + 24.0 {
        3.0
    } else {
        1.0
    };
    24.0 + 24.0 * datum_rows
        + prompt_height
        + if has_status { 18.0 } else { 0.0 }
        + 6.0 * (rows as f32 + datum_rows)
}

/// Image-specific agent editor for the image blister. It keeps the common
/// prompt and menu semantics while replacing the generic capsule rows with
/// compact image controls and circular result-history chips.
pub fn image_agent_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    edit: AgentEdit<'_>,
    options: ImageAgentOptions<'_>,
    zoom: f32,
    theme: Palette,
) -> ImageAgentEditResponse {
    let mut out = ImageAgentEditResponse::default();
    let inner = rect.shrink(12.0 * zoom);
    let gap = 6.0 * zoom;
    let narrow = inner.width() / zoom.max(f32::EPSILON) < IMAGE_AGENT_NARROW_WIDTH;
    let datum = image_agent_datum_layout(inner, zoom);
    let ImageAgentDatumLayout {
        model: model_rect,
        aspect: aspect_rect,
        mode: mode_rect,
        settings: settings_rect,
        count: count_rect,
        play: play_rect,
        history_y,
        controls_bottom,
    } = datum;
    let mode_id = ui.id().with("image-agent-live-mode");
    let mut mode_open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(mode_id))
        .unwrap_or(false);
    let model_id = ui.id().with("image-agent-model");
    let model = bare_menu(
        ui,
        BareMenu {
            id: model_id,
            rect: model_rect,
            label: edit.models.get(edit.model).copied().unwrap_or("Auto"),
            options: edit.models,
            selected: edit.model,
            open: edit.model_open,
            zoom,
            theme,
        },
    );
    out.model = model.picked;
    out.popups.extend(model.popup);
    if model.opened {
        mode_open = false;
        *edit.aspect_open = false;
        *options.settings_open = false;
    }
    let aspect_id = ui.id().with("image-agent-aspect");
    let aspect = bare_menu(
        ui,
        BareMenu {
            id: aspect_id,
            rect: aspect_rect,
            label: edit.aspects.get(edit.aspect).copied().unwrap_or("Source"),
            options: edit.aspects,
            selected: edit.aspect,
            open: edit.aspect_open,
            zoom,
            theme,
        },
    );
    out.aspect = aspect.picked;
    out.popups.extend(aspect.popup);
    if aspect.opened {
        mode_open = false;
        *edit.model_open = false;
        *options.settings_open = false;
    }
    let modes = ["Single", "Live"];
    let mode = bare_menu(
        ui,
        BareMenu {
            id: mode_id,
            rect: mode_rect,
            label: modes[usize::from(edit.live)],
            options: &modes,
            selected: usize::from(edit.live),
            open: &mut mode_open,
            zoom,
            theme,
        },
    );
    out.live = mode.picked.map(|picked| picked == 1);
    out.popups.extend(mode.popup);
    if mode.opened {
        *edit.model_open = false;
        *edit.aspect_open = false;
        *options.settings_open = false;
    }
    let settings_options = ["Seed unlocked", "Seed locked"];
    let settings_id = ui.id().with("image-agent-settings");
    let settings = bare_menu(
        ui,
        BareMenu {
            id: settings_id,
            rect: settings_rect,
            label: "Settings",
            options: &settings_options,
            selected: usize::from(edit.seed_locked),
            open: options.settings_open,
            zoom,
            theme,
        },
    );
    out.seed_locked = settings.picked.map(|picked| picked == 1);
    out.popups.extend(settings.popup);
    if settings.opened {
        mode_open = false;
        *edit.model_open = false;
        *edit.aspect_open = false;
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(mode_id, mode_open));

    let history_left = if narrow {
        inner.left()
    } else {
        settings_rect.right() + gap
    };
    let history_right = count_rect.left() - gap;
    let history_width = (history_right - history_left).max(0.0);
    if !options.history.is_empty() && history_width > 0.0 {
        let spacing = 3.0 * zoom;
        let max_diameter = 26.0 * zoom;
        let visible_count = ((history_width + spacing) / (max_diameter + spacing))
            .floor()
            .max(1.0) as usize;
        let visible_count = visible_count.min(options.history.len());
        let diameter = if history_width < max_diameter && visible_count == 1 {
            history_width
        } else {
            max_diameter
        };
        let start = options
            .selected_history
            .unwrap_or(0)
            .saturating_sub(visible_count / 2)
            .min(options.history.len().saturating_sub(visible_count));
        let occupied =
            diameter * visible_count as f32 + spacing * visible_count.saturating_sub(1) as f32;
        let group_left = (history_right - occupied).max(history_left);
        for index in start..start + visible_count {
            let item = options.history[index];
            let circle = Rect::from_min_size(
                Pos2::new(
                    group_left + (index - start) as f32 * (diameter + spacing),
                    history_y - diameter * 0.5,
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
                AgentThumbnail {
                    texture: item.thumb,
                    index,
                    selected: options.selected_history == Some(index),
                    hovered: response.hovered(),
                    zoom,
                    theme,
                },
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

    let mut y = controls_bottom + gap;
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
    let face_top = (y + gap).min(prompt_bottom - IMAGE_PROMPT_MIN_HEIGHT * zoom);
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
    out.prompt = image_prompt_field(
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
    // Register both targets every frame. Conditional registration based on a
    // parent hover response loses hover as soon as a child owns the pointer.
    let show_count = ui.rect_contains_pointer(count_rect.union(play_rect));
    {
        let minus =
            Rect::from_min_size(count_rect.min, Vec2::new(13.0 * zoom, count_rect.height()));
        let plus = Rect::from_min_size(
            Pos2::new(count_rect.right() - 13.0 * zoom, count_rect.top()),
            Vec2::new(13.0 * zoom, count_rect.height()),
        );
        for (rect, icon, name, delta) in [
            (minus, Icon::ChevronLeft, "Fewer images", -1i16),
            (plus, Icon::ChevronRight, "More images", 1i16),
        ] {
            let response = ui
                .interact(rect, ui.id().with(name), Sense::click())
                .on_hover_text(name);
            let glyph = Rect::from_center_size(rect.center(), Vec2::splat(6.0 * zoom));
            if show_count {
                icons::paint(
                    ui.painter(),
                    glyph,
                    icon,
                    if response.hovered() {
                        theme.ink
                    } else {
                        theme.sub
                    },
                );
            }
            if response.clicked() {
                out.count = Some((i16::from(edit.count.max(1)) + delta).clamp(1, 8) as u8);
            }
        }
    }
    if show_count {
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
    } else if options.live_active {
        theme.accent
    } else if options.running {
        theme.sub
    } else {
        theme.accent
    };
    if options.running && !options.live_active {
        icons::paint(ui.painter(), play_rect, Icon::Stop, ink);
    } else {
        let r = play_rect.shrink(3.0 * zoom);
        ui.painter().add(egui::Shape::convex_polygon(
            vec![r.left_top(), r.left_bottom(), r.right_center()],
            ink,
            Stroke::NONE,
        ));
    }
    let play_response = play_response.on_hover_text(if options.live_active {
        "Keep this result"
    } else if options.running {
        "Stop generation"
    } else {
        "Generate"
    });
    out.submit = play_response.clicked() || out.prompt.submit;
    out
}
struct AgentThumbnail {
    texture: Option<egui::TextureId>,
    index: usize,
    selected: bool,
    hovered: bool,
    zoom: f32,
    theme: Palette,
}

fn paint_agent_thumbnail(painter: &egui::Painter, rect: Rect, thumbnail: AgentThumbnail) {
    let AgentThumbnail {
        texture,
        index,
        selected,
        hovered,
        zoom,
        theme,
    } = thumbnail;
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
