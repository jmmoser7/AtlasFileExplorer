//! Named tag controls with top Show / bottom Hide gestures.
use super::*;
use std::{borrow::Cow, collections::VecDeque, sync::Arc};

const LABEL_SIZE: f32 = 15.0;
const LAYOUT_CACHE_CAP: usize = 32;

pub struct TagPaletteItem<'a> {
    pub label: &'a str,
    pub color: [u8; 3],
    pub selected: bool,
    pub visible: bool,
}

#[derive(Default)]
pub struct TagPaletteResponse {
    pub selection: Option<usize>,
    pub visibility: Option<(usize, bool)>,
}

struct TagSlot {
    rect: Rect,
    source_label: Arc<str>,
    label: Arc<canvas_text::WorldLayout>,
}

/// One measured geometry shared by dock layout, canvas paint, and canvas hits.
pub struct TagPaletteLayout {
    pub size: Vec2,
    diameter: f32,
    pixels_per_point: f32,
    slots: Vec<TagSlot>,
}

pub struct TagPalettePlacement {
    pub card: Rect,
    pub origin: Pos2,
    pub scale: f32,
}

impl TagPaletteLayout {
    /// The full disc-and-name control, without the gap to its neighbour.
    pub fn slot(&self, origin: Pos2, index: usize, scale: f32) -> Rect {
        let rect = self.slots[index].rect;
        Rect::from_min_size(origin + rect.min.to_vec2() * scale, rect.size() * scale)
    }

    pub fn circle(&self, origin: Pos2, index: usize, scale: f32) -> Rect {
        Rect::from_min_size(
            self.slot(origin, index, scale).min,
            Vec2::splat(self.diameter * scale),
        )
    }

    pub fn natural_size(&self, tokens: &crate::tokens::DockTokens) -> Vec2 {
        self.size + Vec2::splat(tokens.palette.group_pad * 2.0)
    }

    /// Canvas copies contain-scale the same intrinsic geometry, including padding.
    pub fn fit(&self, dest: Rect, tokens: &crate::tokens::DockTokens) -> TagPalettePlacement {
        let pad = tokens.palette.group_pad;
        let natural = self.natural_size(tokens);
        let scale = dock::contain_scale(dest.size(), natural);
        let card = Rect::from_center_size(dest.center(), natural * scale);
        TagPalettePlacement {
            card,
            origin: card.min + Vec2::splat(pad * scale),
            scale,
        }
    }
}

#[derive(Clone, Default)]
struct TagPaletteCache(VecDeque<Arc<TagPaletteLayout>>);

/// Names shape on change; visibility, selection and camera movement reuse layout.
pub fn measure_tag_palette(
    ctx: &egui::Context,
    items: &[TagPaletteItem<'_>],
    diameter: f32,
) -> Arc<TagPaletteLayout> {
    let id = Id::new("named-tag-palette-layouts");
    let ppp = ctx.pixels_per_point();
    let cached = ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<TagPaletteCache>(id);
        let index = cache.0.iter().position(|layout| {
            layout.diameter == diameter
                && layout.pixels_per_point == ppp
                && layout.slots.len() == items.len()
                && layout
                    .slots
                    .iter()
                    .zip(items)
                    .all(|(slot, item)| &*slot.source_label == item.label)
        })?;
        let hit = cache.0.remove(index)?;
        cache.0.push_back(Arc::clone(&hit));
        Some(hit)
    });
    if let Some(layout) = cached {
        return layout;
    }
    let label_gap = diameter * 0.27;
    let tag_gap = diameter * 0.5;
    let mut x = 0.0;
    let slots: Vec<_> = items
        .iter()
        .map(|item| {
            // Imported names can contain paragraphs; this control has one line.
            // Keep the original name as the key and flatten only its presentation.
            let display = if item.label.contains(['\r', '\n']) {
                Cow::Owned(item.label.replace("\r\n", " ").replace(['\r', '\n'], " "))
            } else {
                Cow::Borrowed(item.label)
            };
            let label = canvas_text::world_layout(
                ctx,
                &display,
                canvas_scale::font(LABEL_SIZE, 1.0),
                f32::INFINITY,
                egui::Align::LEFT,
            );
            let source_label = if display.as_ref() == item.label {
                Arc::clone(&label.text)
            } else {
                Arc::from(item.label)
            };
            let width = diameter + label_gap + label.width;
            let rect = Rect::from_min_size(Pos2::new(x, 0.0), Vec2::new(width, diameter));
            x += width + tag_gap;
            TagSlot {
                rect,
                source_label,
                label,
            }
        })
        .collect();
    let layout = Arc::new(TagPaletteLayout {
        size: Vec2::new(if slots.is_empty() { 0.0 } else { x - tag_gap }, diameter),
        diameter,
        pixels_per_point: ppp,
        slots,
    });
    ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<TagPaletteCache>(id);
        if cache.0.len() == LAYOUT_CACHE_CAP {
            cache.0.pop_front();
        }
        cache.0.push_back(Arc::clone(&layout));
    });
    layout
}

fn paint_tag(
    ui: &egui::Ui,
    placement: &TagPalettePlacement,
    layout: &TagPaletteLayout,
    index: usize,
    item: &TagPaletteItem<'_>,
    theme: Palette,
    hovered: bool,
) {
    let origin = placement.origin;
    let scale = placement.scale;
    let circle = layout.circle(origin, index, scale);
    let diameter = circle.width();
    let color = Color32::from_rgb(item.color[0], item.color[1], item.color[2]);
    let fill = if item.visible {
        color
    } else {
        color.gamma_multiply(0.3)
    };
    let stroke = Stroke::new(
        if item.selected { 2.0 } else { 0.7 } * scale,
        if item.selected {
            theme.accent
        } else {
            theme.border
        },
    );
    ui.painter().circle(
        circle.center(),
        (diameter * 0.5 - (scale * 0.5).max(stroke.width * 0.5)).max(0.0),
        fill,
        stroke,
    );
    let slot = layout.slot(origin, index, scale);
    let text = canvas_text::world_text(ui.ctx(), &layout.slots[index].label, scale);
    text.paint_anchored(
        ui.painter(),
        Pos2::new(slot.right(), circle.center().y),
        Align2::RIGHT_CENTER,
        if item.visible { theme.ink } else { theme.sub },
    );
    if hovered {
        for (direction, icon) in [(-1.0, Icon::View), (1.0, Icon::Hide)] {
            let glyph = Rect::from_center_size(
                circle.center() + Vec2::new(0.0, direction * diameter * 0.22),
                Vec2::splat(diameter * 0.30),
            );
            ui.painter()
                .circle_filled(glyph.center(), glyph.width() * 0.58, theme.card);
            icons::paint(ui.painter(), glyph, icon, theme.ink);
        }
    }
}

pub fn tag_palette_card_rect(
    dest: Rect,
    layout: &TagPaletteLayout,
    tokens: &crate::tokens::DockTokens,
) -> Rect {
    layout.fit(dest, tokens).card
}

pub fn paint_tag_palette_card(
    ui: &egui::Ui,
    dest: Rect,
    items: &[TagPaletteItem<'_>],
    tokens: &crate::tokens::DockTokens,
    theme: Palette,
) {
    let layout = measure_tag_palette(ui.ctx(), items, dock::flyout_icon_size(tokens));
    let placement = layout.fit(dest, tokens);
    let th = if theme.dark_mode {
        &tokens.dark
    } else {
        &tokens.light
    };
    dock::paint_fieldset_frame(
        ui,
        placement.card,
        None,
        th,
        &tokens.palette,
        placement.scale,
        0.0,
    );
    for (index, item) in items.iter().enumerate() {
        let slot = layout.slot(placement.origin, index, placement.scale);
        let hovered = ui
            .ctx()
            .pointer_latest_pos()
            .is_some_and(|point| slot.contains(point));
        paint_tag(ui, &placement, &layout, index, item, theme, hovered);
    }
}

/// The parent already owns the horizontal row and centerline; add no nested row.
pub fn tag_palette(
    ui: &mut egui::Ui,
    items: &[TagPaletteItem<'_>],
    diameter: f32,
    theme: Palette,
) -> TagPaletteResponse {
    let mut result = TagPaletteResponse::default();
    let layout = measure_tag_palette(ui.ctx(), items, diameter);
    let (rect, _) = ui.allocate_exact_size(layout.size, Sense::hover());
    let placement = TagPalettePlacement {
        card: rect,
        origin: rect.min,
        scale: 1.0,
    };
    for (index, item) in items.iter().enumerate() {
        let slot = layout.slot(rect.min, index, 1.0);
        let response = ui
            .interact(slot, ui.id().with(("tag", index)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), item.label)
        });
        paint_tag(
            ui,
            &placement,
            &layout,
            index,
            item,
            theme,
            response.hovered(),
        );
        if response.clicked() {
            if ui.input(|input| input.modifiers.shift || input.modifiers.ctrl) {
                result.selection = Some(index);
            } else if let Some(point) = response.interact_pointer_pos() {
                result.visibility = Some((index, point.y <= slot.center().y));
            }
        }
        response.on_hover_ui(|ui| {
            ui.label(item.label);
            ui.label("Top: show · Bottom: hide");
            ui.label("Shift/Ctrl-click: select tags together");
        });
    }
    result
}

#[cfg(test)]
mod tests;
