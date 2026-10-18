//! Public agent messages and tool activity. Providers never supply hidden reasoning.
use super::*;

#[path = "generator_activity/fold.rs"]
mod fold;
pub use fold::*;

pub const TEXT_UNITS: f32 = 14.0;
const PARTIAL_VIEW: f32 = 96.0;
const FULL_VIEW_LIMIT: f32 = 480.0;

/// A public, request-scoped event or transcript message supplied by the app.
#[derive(Clone, Copy)]
pub struct ActivityEntry<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub text: &'a str,
}

/// Read-only public events borrowed directly from a provider/session adapter.
pub trait ActivityEntries {
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<ActivityEntry<'_>>;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ActivityEntries for [ActivityEntry<'_>] {
    fn len(&self) -> usize {
        <[ActivityEntry<'_>]>::len(self)
    }
    fn get(&self, index: usize) -> Option<ActivityEntry<'_>> {
        <[ActivityEntry<'_>]>::get(self, index).copied()
    }
}
impl<const N: usize> ActivityEntries for [ActivityEntry<'_>; N] {
    fn len(&self) -> usize {
        N
    }
    fn get(&self, index: usize) -> Option<ActivityEntry<'_>> {
        self.as_slice().get(index).copied()
    }
}
impl ActivityEntries for Vec<ActivityEntry<'_>> {
    fn len(&self) -> usize {
        self.as_slice().len()
    }
    fn get(&self, index: usize) -> Option<ActivityEntry<'_>> {
        self.as_slice().get(index).copied()
    }
}

/// Ephemeral presentation, suitable for the existing selection-memory owner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivityState {
    pub fold: CardFold,
    pub scroll: FollowScroll,
    pub content_height: f32,
}

impl Default for ActivityState {
    fn default() -> Self {
        Self {
            fold: CardFold::Collapsed,
            scroll: FollowScroll::default(),
            content_height: 0.0,
        }
    }
}

pub struct ActivityView<'a> {
    pub entries: &'a dyn ActivityEntries,
    pub state: &'a mut ActivityState,
    pub running: bool,
}

fn content_height(ctx: &egui::Context, entries: &dyn ActivityEntries, width: f32) -> f32 {
    (0..entries.len())
        .filter_map(|index| entries.get(index))
        .map(|entry| {
            let text = canvas_text::world_layout(
                ctx,
                entry.text,
                canvas_scale::font(TEXT_UNITS, 1.0),
                width.max(1.0),
                egui::Align::LEFT,
            );
            text.height + if entry.label.is_empty() { 0.0 } else { 16.0 } + 12.0
        })
        .sum()
}

/// Empty and collapsed activity never reserve a row or gap.
pub fn height(
    ctx: &egui::Context,
    entries: &dyn ActivityEntries,
    width: f32,
    fold: CardFold,
) -> f32 {
    if entries.is_empty() || fold == CardFold::Collapsed {
        return 0.0;
    }
    let content = content_height(ctx, entries, width);
    content.min(if fold == CardFold::Partial {
        PARTIAL_VIEW
    } else {
        FULL_VIEW_LIMIT
    })
}

/// Shared chat/generator message row, shaped at a world width and selectable.
#[allow(clippy::too_many_arguments)]
pub fn transcript_row(
    ui: &mut egui::Ui,
    id: Id,
    text: &str,
    font_units: f32,
    zoom: f32,
    ink: Color32,
    interactive: bool,
    streaming: bool,
) -> Rect {
    let display = if streaming {
        std::borrow::Cow::Owned(format!("{text} |"))
    } else {
        std::borrow::Cow::Borrowed(text)
    };
    let layout = canvas_text::world_layout(
        ui.ctx(),
        &display,
        canvas_scale::font(font_units, 1.0),
        ui.available_width() / zoom,
        egui::Align::LEFT,
    );
    let laid = canvas_text::world_text(ui.ctx(), &layout, zoom);
    let size = Vec2::new(ui.available_width(), layout.height * zoom);
    let (allocated, _) = ui.allocate_exact_size(
        size,
        if interactive {
            Sense::hover()
        } else {
            Sense::empty()
        },
    );
    // egui's Layout::align_size_within_rect rounds both edges to 1/32 point.
    // Keep its cursor reservation, but paint/return the exact world-sized row.
    let row = Rect::from_min_size(allocated.min, size);
    if interactive {
        laid.selectable(ui, id, row.min, ink);
    } else {
        laid.paint(ui.painter(), row.min, ink);
    }
    row
}

pub fn paint(ui: &mut egui::Ui, rect: Rect, view: ActivityView<'_>, zoom: f32, theme: Palette) {
    if view.entries.is_empty() || view.state.fold == CardFold::Collapsed {
        return;
    }
    let viewport = rect;
    let requested = requested_offset(
        &view.state.scroll,
        view.state.content_height,
        viewport.height() / zoom,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("activity")
            .max_rect(viewport),
    );
    child.set_clip_rect(viewport.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing.y = 0.0;
    let scroll = egui::ScrollArea::vertical()
        .id_salt("activity-scroll")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .animated(false)
        .min_scrolled_height(0.0)
        .max_height(viewport.height())
        .auto_shrink([false, false])
        .vertical_scroll_offset(requested * zoom)
        .stick_to_bottom(view.running && view.state.scroll.follow)
        .show(&mut child, |ui| {
            ui.set_width(viewport.width());
            for entry in (0..view.entries.len()).filter_map(|index| view.entries.get(index)) {
                if !entry.label.is_empty() {
                    let (row, _) = ui.allocate_exact_size(
                        Vec2::new(viewport.width(), 16.0 * zoom),
                        Sense::hover(),
                    );
                    canvas_text::text(
                        ui.painter(),
                        row.min,
                        Align2::LEFT_TOP,
                        entry.label,
                        canvas_scale::font(11.0, zoom),
                        theme.sub,
                    );
                }
                transcript_row(
                    ui,
                    ui.id().with(("activity-entry", entry.id)),
                    entry.text,
                    TEXT_UNITS,
                    zoom,
                    theme.ink,
                    true,
                    false,
                );
                ui.add_space(12.0 * zoom);
            }
        });
    view.state.content_height = scroll.content_size.y / zoom;
    settle_follow(
        &mut view.state.scroll,
        requested,
        scroll.state.offset.y / zoom,
        view.state.content_height,
        viewport.height() / zoom,
    );
    crate::menu_wheel::claim(ui.ctx(), viewport);
}

/// Chat's same one/two-level offers on the generator's single summary row.
pub(super) fn paint_fold_controls(
    ui: &egui::Ui,
    header: Rect,
    fold: CardFold,
    zoom: f32,
    theme: Palette,
) -> Option<CardFold> {
    let mut chosen = None;
    for (index, (dir, step)) in chevron_offers(fold).iter().copied().enumerate() {
        let center = header.right_center() - Vec2::new((12.0 + 24.0 * index as f32) * zoom, 0.0);
        let hit = Rect::from_center_size(center, Vec2::splat(24.0 * zoom));
        let response = ui.interact(hit, ui.id().with(("activity-fold", index)), Sense::click());
        paint_fold_glyph(
            ui.painter(),
            center,
            zoom,
            dir,
            step,
            if response.hovered() {
                theme.ink
            } else {
                theme.sub
            },
        );
        let _ = object_hover_text(
            response.clone(),
            if dir == FoldDir::Collapse {
                "Fold activity"
            } else {
                "Expand activity"
            },
        );
        if response.clicked() {
            chosen = Some(apply_fold(fold, dir, step));
        }
    }
    chosen
}

#[cfg(test)]
#[path = "generator_activity/tests.rs"]
mod tests;
