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
    paint_result_strip(
        StripPolicy::Album,
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
        |_, _, _| {},
    )
    .clicked
}

#[derive(Clone, Copy)]
enum StripPolicy {
    Album,
    Generator,
}

/// Generator-only eight-column running bond; ordinary album navigation is unchanged.
pub fn generator_result_rects(
    album: Rect,
    count: usize,
    focus: usize,
    zoom: f32,
) -> Vec<(usize, Rect)> {
    let start = focus.saturating_sub(12).min(count.saturating_sub(24));
    let end = (start + 24).min(count);
    let columns = (end - start).min(8);
    let width = columns as f32 * 24.0 + columns.saturating_sub(1) as f32 * 4.0;
    (start..end)
        .enumerate()
        .map(|(offset, index)| {
            let row = offset / 8;
            let column = offset % 8;
            let stagger = if row % 2 == 1 { 14.0 } else { 0.0 };
            let position = Pos2::new(
                album.center().x + (column as f32 * 28.0 + stagger - width * 0.5) * zoom,
                album.bottom() + (10.0 + row as f32 * 28.0) * zoom,
            );
            (
                index,
                Rect::from_min_size(position, Vec2::splat(24.0 * zoom)),
            )
        })
        .collect()
}

/// Current visible result squares and opacity, shared by paint and early input.
#[allow(clippy::too_many_arguments)]
pub fn album_result_layout(
    ctx: &egui::Context,
    id: Id,
    album: Rect,
    _runs: &[&str],
    count: usize,
    focus: usize,
    shown: bool,
    zoom: f32,
    min_images: usize,
) -> Option<(f32, Vec<(usize, Rect)>)> {
    strip_layout(
        StripPolicy::Generator,
        ctx,
        id,
        album,
        _runs,
        count,
        focus,
        shown,
        zoom,
        min_images,
    )
}

#[allow(clippy::too_many_arguments)]
fn strip_layout(
    policy: StripPolicy,
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
    let side = match policy {
        StripPolicy::Album => STRIP_SQUARE,
        StripPolicy::Generator => 24.0,
    } * zoom;
    if side < 2.0 {
        return None;
    }
    Some((
        fade,
        match policy {
            StripPolicy::Album => album_index_rects(album, runs, count, focus, zoom),
            StripPolicy::Generator => generator_result_rects(album, count, focus, zoom),
        },
    ))
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
    album_result_strip_content(
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
        selected,
        |_, _, _| {},
    )
}

#[allow(clippy::too_many_arguments)]
pub fn album_result_strip_content(
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
    paint_content: impl FnMut(usize, &egui::Painter, Rect),
) -> crate::result_actions::ResultInteraction {
    paint_result_strip(
        StripPolicy::Generator,
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
        selected,
        paint_content,
    )
}

#[allow(clippy::too_many_arguments)]
fn paint_result_strip(
    policy: StripPolicy,
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
    mut paint_content: impl FnMut(usize, &egui::Painter, Rect),
) -> crate::result_actions::ResultInteraction {
    let mut out = crate::result_actions::ResultInteraction::default();
    let Some((fade, squares)) = strip_layout(
        policy,
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
        let radius = match policy {
            StripPolicy::Album => 3.0,
            StripPolicy::Generator => 4.0,
        } * zoom;
        let image = images[i];
        match image.texture {
            Some(texture) => painter.add(
                egui::epaint::RectShape::filled(square, radius, Color32::WHITE)
                    .with_texture(texture, album_uv(image.size, square.size())),
            ),
            None => painter.add(egui::epaint::RectShape::filled(square, radius, theme.card)),
        };
        paint_content(i, &painter, square);
        if matches!(policy, StripPolicy::Album) {
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
    }
    out.painted_bounds = painted_bounds.is_positive().then_some(painted_bounds);
    out
}

#[cfg(test)]
mod generator_geometry_tests {
    use super::*;
    #[test]
    fn generator_running_bond_uses_reviewed_geometry_without_changing_albums() {
        let host = Rect::from_min_size(Pos2::ZERO, Vec2::new(360.0, 300.0));
        let squares = generator_result_rects(host, 17, 0, 1.0);
        assert_eq!(squares.len(), 17);
        assert_eq!(squares[0].1.size(), Vec2::splat(24.0));
        assert_eq!(squares[1].1.left() - squares[0].1.right(), 4.0);
        assert_eq!(squares[8].1.left() - squares[0].1.left(), 14.0);
        assert_eq!(squares[8].1.top() - squares[0].1.top(), 28.0);
        assert_eq!(squares[16].1.left(), squares[0].1.left());
        assert_eq!(squares[0].1.top(), 310.0);
        assert_eq!(
            album_index_rects(host, &[], 1, 0, 1.0)[0].1.size(),
            Vec2::splat(22.0)
        );
    }
    #[test]
    fn public_saved_view_and_generator_entry_points_keep_distinct_geometry_and_selection_style() {
        fn paint(generated: bool) -> Vec<(Vec2, f32)> {
            let ctx = egui::Context::default();
            let host = Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(360.0, 100.0));
            let images = vec![
                AlbumImage {
                    texture: None,
                    size: Vec2::splat(32.0),
                    enabled: true
                };
                10
            ];
            let mut shapes = Vec::new();
            for time in [0.0, 1.0] {
                let output = ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 480.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            if generated {
                                album_result_strip(
                                    ui,
                                    Id::new("public-strip"),
                                    host,
                                    &images,
                                    &[],
                                    0,
                                    true,
                                    1.0,
                                    Palette::dark(),
                                    1,
                                    None,
                                    &[],
                                );
                            } else {
                                album_index_strip(
                                    ui,
                                    Id::new("public-strip"),
                                    host,
                                    &images,
                                    &[],
                                    0,
                                    true,
                                    1.0,
                                    Palette::dark(),
                                    1,
                                    None,
                                );
                            }
                        });
                    },
                );
                shapes = output
                    .shapes
                    .into_iter()
                    .filter_map(|shape| match shape.shape {
                        egui::Shape::Rect(rect)
                            if (rect.rect.width() - 22.0).abs() < 0.01
                                || (rect.rect.width() - 24.0).abs() < 0.01 =>
                        {
                            Some((rect.rect.size(), rect.stroke.width))
                        }
                        _ => None,
                    })
                    .collect();
            }
            shapes
        }
        let ordinary = paint(false);
        assert!(ordinary
            .iter()
            .any(|(size, stroke)| *size == Vec2::splat(22.0) && *stroke > 1.0));
        assert!(ordinary.iter().all(|(size, _)| *size == Vec2::splat(22.0)));
        let generated = paint(true);
        assert!(!generated.is_empty());
        assert!(generated
            .iter()
            .all(|(size, stroke)| *size == Vec2::splat(24.0) && *stroke == 0.0));
    }
}
