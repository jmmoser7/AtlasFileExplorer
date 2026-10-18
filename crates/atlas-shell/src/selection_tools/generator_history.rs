//! Circular output history shared by every attached generator.
use super::*;

/// A preserved output. Labels can be borrowed or formatted for a visible row.
#[derive(Clone)]
pub struct GeneratorHistoryItem<'a> {
    pub label: std::borrow::Cow<'a, str>,
    pub thumb: Option<egui::TextureId>,
}

/// Indexed history lets the shared layout see every output without materializing
/// an album on the frame loop. Only the visible indices are requested.
pub trait GeneratorHistory {
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<GeneratorHistoryItem<'_>>;
    /// Non-image histories retain ordinary selection and revision browsing.
    fn image_actions(&self) -> bool {
        false
    }
    fn is_selected(&self, _index: usize) -> bool {
        false
    }
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl GeneratorHistory for [GeneratorHistoryItem<'_>] {
    fn len(&self) -> usize {
        <[GeneratorHistoryItem<'_>]>::len(self)
    }
    fn get(&self, index: usize) -> Option<GeneratorHistoryItem<'_>> {
        self.get(index).map(|item| GeneratorHistoryItem {
            label: std::borrow::Cow::Borrowed(item.label.as_ref()),
            thumb: item.thumb,
        })
    }
}

impl GeneratorHistory for &[GeneratorHistoryItem<'_>] {
    fn len(&self) -> usize {
        <[GeneratorHistoryItem<'_>]>::len(self)
    }
    fn get(&self, index: usize) -> Option<GeneratorHistoryItem<'_>> {
        GeneratorHistory::get(*self, index)
    }
}

impl<const N: usize> GeneratorHistory for [GeneratorHistoryItem<'_>; N] {
    fn len(&self) -> usize {
        N
    }
    fn get(&self, index: usize) -> Option<GeneratorHistoryItem<'_>> {
        GeneratorHistory::get(self.as_slice(), index)
    }
}

impl GeneratorHistory for Vec<GeneratorHistoryItem<'_>> {
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn get(&self, index: usize) -> Option<GeneratorHistoryItem<'_>> {
        GeneratorHistory::get(self.as_slice(), index)
    }
}

pub(super) const HEIGHT: f32 = 26.0;
const GAP: f32 = 3.0;
const ARROW_WIDTH: f32 = 16.0;

fn occupied(count: usize) -> f32 {
    HEIGHT * count as f32 + GAP * count.saturating_sub(1) as f32
}

/// A datum lane must fit every chip, or one chip with both navigation targets.
pub(super) fn fits(width: f32, count: usize) -> bool {
    width >= occupied(count).min(HEIGHT + (ARROW_WIDTH + GAP) * 2.0)
}

struct HistoryLayout {
    start: usize,
    count: usize,
    diameter: f32,
    spacing: f32,
    left: f32,
    earlier: Option<Rect>,
    later: Option<Rect>,
}

impl HistoryLayout {
    fn new(row: Rect, center_y: f32, len: usize, selected: Option<usize>, zoom: f32) -> Self {
        let width = row.width() / zoom;
        let overflow = width < occupied(len);
        let scale = if overflow {
            (width / (HEIGHT + (ARROW_WIDTH + GAP) * 2.0)).min(1.0)
        } else {
            (width / HEIGHT).min(1.0)
        };
        let spacing = GAP * scale * zoom;
        let diameter = HEIGHT * scale * zoom;
        let arrow = ARROW_WIDTH * scale * zoom;
        let navigation = if overflow {
            (arrow + spacing) * 2.0
        } else {
            0.0
        };
        let count = (((row.width() - navigation + spacing) / (diameter + spacing))
            .floor()
            .max(1.0) as usize)
            .min(len);
        let chips_width = diameter * count as f32 + spacing * count.saturating_sub(1) as f32;
        let left = row.right() - chips_width - navigation;
        let earlier = overflow.then(|| {
            Rect::from_center_size(
                Pos2::new(left + arrow * 0.5, center_y),
                Vec2::new(arrow, diameter),
            )
        });
        let chip_left = left + if overflow { arrow + spacing } else { 0.0 };
        let later = overflow.then(|| {
            Rect::from_center_size(
                Pos2::new(chip_left + chips_width + spacing + arrow * 0.5, center_y),
                Vec2::new(arrow, diameter),
            )
        });
        Self {
            start: selected
                .unwrap_or(0)
                .min(len.saturating_sub(1))
                .saturating_sub(count / 2)
                .min(len.saturating_sub(count)),
            count,
            diameter,
            spacing,
            left: chip_left,
            earlier,
            later,
        }
    }

    fn chip(&self, index: usize, center_y: f32) -> Rect {
        Rect::from_min_size(
            Pos2::new(
                self.left + (index - self.start) as f32 * (self.diameter + self.spacing),
                center_y - self.diameter * 0.5,
            ),
            Vec2::splat(self.diameter),
        )
    }
}

pub(super) fn visible_range(
    row: Rect,
    count: usize,
    selected: Option<usize>,
) -> std::ops::Range<usize> {
    if count == 0 || row.width() <= 0.0 {
        return 0..0;
    }
    let layout = HistoryLayout::new(row, row.center().y, count, selected, 1.0);
    layout.start..layout.start + layout.count
}

#[allow(clippy::too_many_arguments)]
pub(super) fn history_row(
    ui: &mut egui::Ui,
    inner: Rect,
    center_y: f32,
    history: &dyn GeneratorHistory,
    selected: Option<usize>,
    zoom: f32,
    theme: Palette,
) -> GeneratorResponse {
    let mut out = GeneratorResponse::default();
    let width = inner.width().max(0.0);
    if history.is_empty() || width <= 0.0 {
        return out;
    }
    let layout = HistoryLayout::new(inner, center_y, history.len(), selected, zoom);
    for (earlier, rect, target) in [
        (true, layout.earlier, layout.start.checked_sub(1)),
        (
            false,
            layout.later,
            (layout.start + layout.count < history.len()).then_some(layout.start + layout.count),
        ),
    ] {
        if let Some(rect) = rect {
            if navigation_control(ui, rect, earlier, target.is_some(), zoom, theme) {
                out.clicked_history = target;
            }
        }
    }
    for index in layout.start..layout.start + layout.count {
        let Some(item) = history.get(index) else {
            continue;
        };
        let circle = layout.chip(index, center_y);
        let response = ui.interact(
            circle,
            ui.id().with(("image-agent-history", index)),
            Sense::click_and_drag(),
        );
        paint_thumbnail(
            ui.painter(),
            circle,
            item.thumb,
            index,
            selected == Some(index) || history.is_selected(index),
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
        if history.image_actions() {
            let interaction = crate::result_actions::thumbnail_interaction(&response, index);
            if interaction.clicked.is_some()
                || interaction.menu.is_some()
                || interaction.drag_started.is_some()
                || interaction.drag_position.is_some()
            {
                out.result_interaction = interaction;
            }
        }
        if response.drag_started() {
            out.drag_started_history = Some(index);
        }
        if response.dragged() {
            out.history_drag_position = response.interact_pointer_pos();
        }
        if response.on_hover_text(item.label.as_ref()).hovered() {
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
                        ui.add(
                            egui::Image::new((texture, preview.size())).corner_radius(
                                egui::CornerRadius::same((5.0 * zoom).round() as u8),
                            ),
                        );
                    });
            }
        }
    }
    out
}

fn navigation_control(
    ui: &egui::Ui,
    rect: Rect,
    earlier: bool,
    enabled: bool,
    zoom: f32,
    theme: Palette,
) -> bool {
    let response = ui.interact(
        rect,
        ui.id().with(("generator-history-page", earlier)),
        // Even an exhausted endpoint owns its exact hit region. Otherwise
        // egui's fuzzy picking can send a small-zoom press to the next chip.
        Sense::click(),
    );
    let ink = if !enabled {
        theme.sub.gamma_multiply(0.4)
    } else if response.hovered() {
        theme.ink
    } else {
        theme.sub
    };
    icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(8.0 * zoom)),
        if earlier {
            Icon::ChevronLeft
        } else {
            Icon::ChevronRight
        },
        ink,
    );
    let _ = object_hover_text(
        response.clone(),
        match (earlier, enabled) {
            (true, true) => "Earlier outputs",
            (false, true) => "Later outputs",
            (true, false) => "No earlier outputs",
            (false, false) => "No later outputs",
        },
    );
    enabled && response.clicked()
}

#[allow(clippy::too_many_arguments)]
fn paint_thumbnail(
    painter: &egui::Painter,
    rect: Rect,
    texture: Option<egui::TextureId>,
    index: usize,
    selected: bool,
    hovered: bool,
    zoom: f32,
    theme: Palette,
) {
    let radio = FilterRadio {
        label: "",
        fill: [72, 82, 92],
        fill_b: None,
        thumb: texture,
    };
    paint_filter_radio(
        painter,
        rect.center(),
        rect.width() * 0.5,
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

#[cfg(test)]
#[path = "generator_history/tests.rs"]
mod tests;
