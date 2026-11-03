//! Public agent messages and tool activity. Providers never supply hidden reasoning.
use super::*;

#[path = "generator_activity/fold.rs"]
mod fold;
pub use fold::*;

pub const TEXT_UNITS: f32 = 14.0;
const PARTIAL_VIEW: f32 = 220.0;
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
    fn is_user(&self, _index: usize) -> bool {
        false
    }
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
    /// One-shot navigation to an existing displayed transcript row.
    pub focus_index: Option<usize>,
}

impl Default for ActivityState {
    fn default() -> Self {
        Self {
            fold: CardFold::Collapsed,
            scroll: FollowScroll::default(),
            content_height: 0.0,
            focus_index: None,
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
        .filter_map(|index| entries.get(index).map(|entry| (index, entry)))
        .map(|(index, entry)| {
            let text = canvas_text::world_layout(
                ctx,
                entry.text,
                canvas_scale::font(TEXT_UNITS, 1.0),
                (width - transcript_user_indent(entries.is_user(index))).max(1.0),
                egui::Align::LEFT,
            );
            text.height + 20.0
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

/// Role geometry shared by generator dialogue and a continuous chat window.
pub fn transcript_user_indent(user: bool) -> f32 {
    if user {
        28.0
    } else {
        0.0
    }
}
pub fn transcript_role_ink(user: bool, ink: Color32) -> Color32 {
    if user {
        ink
    } else {
        ink.gamma_multiply(0.72)
    }
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
    // Raster ink can extend beyond the reference line box at a ladder rung.
    // Reserve its actual vertical extent without changing line breaks or gaps.
    let bounds = Rect::from_min_size(Pos2::ZERO, laid.size())
        .union(laid.galley().mesh_bounds * laid.scale());
    let size = Vec2::new(ui.available_width(), bounds.height());
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
    let origin = row.min - Vec2::new(0.0, bounds.top());
    if interactive {
        laid.selectable(ui, id, origin, ink);
    } else {
        laid.paint(ui.painter(), origin, ink);
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
    let focus_index = view.state.focus_index.take();
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
            for (index, entry) in (0..view.entries.len())
                .filter_map(|index| view.entries.get(index).map(|entry| (index, entry)))
            {
                let user = view.entries.is_user(index);
                let indent = transcript_user_indent(user) * zoom;
                let row_rect = Rect::from_min_size(
                    ui.cursor().min + Vec2::new(indent, 0.0),
                    Vec2::new((viewport.width() - indent).max(1.0), 0.0),
                );
                let mut row_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt(("activity-row", index, entry.id))
                        .max_rect(row_rect),
                );
                let painted = transcript_row(
                    &mut row_ui,
                    ui.id().with(("activity-entry", index, entry.id)),
                    entry.text,
                    TEXT_UNITS,
                    zoom,
                    transcript_role_ink(user, theme.ink),
                    true,
                    false,
                );
                if focus_index == Some(index) {
                    ui.scroll_to_rect(painted, Some(egui::Align::Center));
                    view.state.scroll.follow = false;
                }
                ui.allocate_space(Vec2::new(viewport.width(), painted.height()));
                ui.add_space(20.0 * zoom);
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
    super::scroll::paint(
        ui,
        ui.id().with("activity-scroll"),
        viewport,
        scroll.content_size.y,
        scroll.state.offset.y,
        zoom,
        theme,
    );
    crate::menu_wheel::claim(ui.ctx(), viewport);
}

/// Chat's same one/two-level offers on the compact header datum.
pub(super) fn paint_fold_controls(
    ui: &egui::Ui,
    header: Rect,
    fold: CardFold,
    zoom: f32,
    theme: Palette,
) -> Option<CardFold> {
    let mut chosen = None;
    for (index, (dir, step)) in chevron_offers(fold).iter().copied().enumerate() {
        let hit = Rect::from_min_size(
            header.min + Vec2::new(0.0, 5.6 * index as f32 * zoom),
            Vec2::new(14.0 * zoom, 5.6 * zoom),
        );
        let center = hit.center();
        let response = ui.interact(hit, ui.id().with(("activity-fold", index)), Sense::click());
        if response.hovered() || response.has_focus() {
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
        }
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
