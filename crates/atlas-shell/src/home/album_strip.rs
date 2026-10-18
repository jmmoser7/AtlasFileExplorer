//! Album index geometry and painting.
use super::*;

/// Designed size of one album index square and the gap between them.
const STRIP_SQUARE: f32 = 22.0;
const STRIP_GAP: f32 = 5.0;
/// A wider gap separates pictures from different runs.
const STRIP_RUN_GAP: f32 = 12.0;
const STRIP_OFFSET: f32 = 10.0;
/// At most this many squares show, centered on the shown picture.
const STRIP_MAX: usize = 24;

/// Where the index strip sits under an album, in the album's board units.
pub fn album_strip_rect(album: Rect, count: usize, zoom: f32) -> Rect {
    let n = count.min(STRIP_MAX) as f32;
    let width = n * STRIP_SQUARE + (n - 1.0).max(0.0) * STRIP_RUN_GAP;
    Rect::from_center_size(
        Pos2::new(
            album.center().x,
            album.bottom() + (STRIP_OFFSET + STRIP_SQUARE * 0.5) * zoom,
        ),
        Vec2::new(width, STRIP_SQUARE) * zoom,
    )
}

/// Exact visible hit rectangles; painting, early input capture and tests share
/// the same run gaps and history window.
pub fn album_index_rects(
    album: Rect,
    runs: &[&str],
    count: usize,
    focus: usize,
    zoom: f32,
) -> Vec<(usize, Rect)> {
    if count == 0 {
        return Vec::new();
    }
    let start = focus
        .saturating_sub(STRIP_MAX / 2)
        .min(count.saturating_sub(STRIP_MAX));
    let end = (start + STRIP_MAX).min(count);
    let gaps: Vec<f32> = (start..end)
        .map(|i| {
            if i == start {
                0.0
            } else if runs.get(i) != runs.get(i - 1) {
                STRIP_RUN_GAP * zoom
            } else {
                STRIP_GAP * zoom
            }
        })
        .collect();
    let side = STRIP_SQUARE * zoom;
    let width = side * (end - start) as f32 + gaps.iter().sum::<f32>();
    let top = album.bottom() + STRIP_OFFSET * zoom;
    let mut x = album.center().x - width * 0.5;
    (start..end)
        .enumerate()
        .map(|(offset, index)| {
            x += gaps[offset];
            let square = Rect::from_min_size(Pos2::new(x, top), Vec2::splat(side));
            x += side;
            (index, square)
        })
        .collect()
}

/// Square thumbnails under an album while it is browsed (or `shown`), marking
/// the picture on display among every picture the album holds. `runs[i]` names
/// the run that made picture `i`; a wider gap separates runs. Returns a square
/// the pointer clicked.
#[allow(clippy::too_many_arguments)]
pub fn album_index_strip(
    ui: &egui::Ui,
    id: Id,
    album: Rect,
    images: &[AlbumImage],
    runs: &[&str],
    focus: usize,
    shown: bool,
    zoom: f32,
    theme: crate::theme::Palette,
    min_images: usize,
    disabled_hover: Option<&str>,
) -> Option<usize> {
    album_result_strip(
        ui,
        id,
        album,
        images,
        runs,
        focus,
        shown,
        zoom,
        theme,
        min_images,
        disabled_hover,
        &[],
    )
    .clicked
}

/// Current visible result squares and opacity, shared by paint and early input.
#[allow(clippy::too_many_arguments)]
pub fn album_result_layout(
    ctx: &egui::Context,
    id: Id,
    album: Rect,
    runs: &[&str],
    count: usize,
    focus: usize,
    shown: bool,
    zoom: f32,
    min_images: usize,
) -> Option<(f32, Vec<(usize, Rect)>)> {
    if count < min_images.max(1) {
        return None;
    }
    let fade = album_browsing(ctx, id).max(ctx.animate_bool_with_time(
        id.with("album-strip"),
        shown,
        ALBUM_SETTLE_SECONDS,
    ));
    if fade <= 0.01 {
        return None;
    }
    let side = STRIP_SQUARE * zoom;
    if side < 2.0 {
        return None;
    }
    Some((fade, album_index_rects(album, runs, count, focus, zoom)))
}

/// Interactive generated-result variant; one response feeds the same controller
/// as the circular history inside the generator editor.
#[allow(clippy::too_many_arguments)]
pub fn album_result_strip(
    ui: &egui::Ui,
    id: Id,
    album: Rect,
    images: &[AlbumImage],
    runs: &[&str],
    focus: usize,
    shown: bool,
    zoom: f32,
    theme: crate::theme::Palette,
    min_images: usize,
    disabled_hover: Option<&str>,
    selected: &[usize],
) -> crate::result_actions::ResultInteraction {
    let mut out = crate::result_actions::ResultInteraction::default();
    let Some((fade, squares)) = album_result_layout(
        ui.ctx(),
        id,
        album,
        runs,
        images.len(),
        focus,
        shown,
        zoom,
        min_images,
    ) else {
        return out;
    };
    let mut painter = ui.painter().clone();
    painter.multiply_opacity(fade);
    let mut painted_bounds = Rect::NOTHING;
    for (i, square) in squares {
        painted_bounds = painted_bounds.union(square);
        let enabled = images[i].enabled;
        let mut response = ui.interact(
            square,
            id.with(("album-index", i)),
            if enabled {
                egui::Sense::click_and_drag()
            } else {
                egui::Sense::hover()
            },
        );
        if !enabled {
            if let Some(tip) = disabled_hover {
                response = response.on_hover_text(tip);
            }
        }
        if enabled {
            let interaction = crate::result_actions::thumbnail_interaction(&response, i);
            if interaction.clicked.is_some()
                || interaction.menu.is_some()
                || interaction.drag_started.is_some()
                || interaction.drag_position.is_some()
            {
                out = interaction;
            }
        }
        let radius = 3.0 * zoom;
        let image = images[i];
        match image.texture {
            Some(texture) => painter.add(
                egui::epaint::RectShape::filled(square, radius, Color32::WHITE)
                    .with_texture(texture, album_uv(image.size, square.size())),
            ),
            None => painter.add(egui::epaint::RectShape::filled(square, radius, theme.card)),
        };
        let active = i == focus || selected.contains(&i);
        painter.rect_stroke(
            square,
            radius,
            egui::Stroke::new(
                (if active { 1.6 } else { 0.8 }) * zoom,
                if active {
                    theme.select
                } else if response.hovered() {
                    theme.ink
                } else {
                    theme.border_strong
                },
            ),
            egui::StrokeKind::Outside,
        );
    }
    out.painted_bounds = painted_bounds.is_positive().then_some(painted_bounds);
    out
}
