//! Drag-gesture behavior and the board camera transform.

use super::*;

impl BoardDrag {
    /// Drags that edit scene nodes in place before release. Each carries
    /// its press-time nodes, so Esc can put them back (P0.1).
    pub(crate) fn edits_nodes_live(&self) -> bool {
        matches!(
            self,
            BoardDrag::Move { .. }
                | BoardDrag::Resize { .. }
                | BoardDrag::Rotate { .. }
                | BoardDrag::CropEdge { .. }
                | BoardDrag::CropPan { .. }
                | BoardDrag::GroupResize { .. }
                | BoardDrag::GroupRotate { .. }
                | BoardDrag::LineGrip { .. }
                | BoardDrag::FilletRadius { .. }
                | BoardDrag::Direct(
                    super::super::board_direct::DirectDrag::Anchors { .. }
                        | super::super::board_direct::DirectDrag::Segment { .. }
                        | super::super::board_direct::DirectDrag::Handle { .. }
                        | super::super::board_direct::DirectDrag::Arc { .. }
                )
        )
    }

    /// The press-time nodes, and whether they are unjournaled Alt copies.
    /// `None` exactly when [`Self::edits_nodes_live`] is false.
    pub(super) fn into_press_nodes(self) -> Option<(Vec<Node>, bool)> {
        Some(match self {
            BoardDrag::Move { before, dup, .. } | BoardDrag::GroupResize { before, dup, .. } => {
                (before, dup)
            }
            BoardDrag::Resize { before, dup, .. } => (vec![before], dup),
            BoardDrag::GroupRotate { before, .. } => (before, false),
            BoardDrag::CropEdge { before, peers, .. }
            | BoardDrag::FilletRadius { before, peers, .. } => {
                (std::iter::once(before).chain(peers).collect(), false)
            }
            BoardDrag::Rotate { before, .. }
            | BoardDrag::CropPan { before, .. }
            | BoardDrag::LineGrip { before, .. }
            | BoardDrag::Direct(
                super::super::board_direct::DirectDrag::Anchors { before, .. }
                | super::super::board_direct::DirectDrag::Segment { before, .. }
                | super::super::board_direct::DirectDrag::Handle { before, .. }
                | super::super::board_direct::DirectDrag::Arc { before, .. },
            ) => (vec![before], false),
            _ => return None,
        })
    }
}

/// World→screen transform. The board uses the tab camera; presentation mode
/// builds its own transform per slide — both feed the same painters.
#[derive(Clone, Copy)]
pub struct BoardXf {
    pub center: Pos2,
    pub offset: Vec2,
    pub z: f32,
}

impl BoardXf {
    pub fn w2s(&self, w: Pos2) -> Pos2 {
        self.center + (w.to_vec2() - self.offset) * self.z
    }

    pub fn s2w(&self, s: Pos2) -> Pos2 {
        (((s - self.center) / self.z) + self.offset).to_pos2()
    }

    pub fn rect_w2s(&self, r: WorldRect) -> Rect {
        Rect::from_min_max(
            self.w2s(Pos2::new(r.x, r.y)),
            self.w2s(Pos2::new(r.x + r.w, r.y + r.h)),
        )
    }
}

pub fn wr(r: Rect) -> WorldRect {
    WorldRect::new(r.min.x, r.min.y, r.width(), r.height())
}

pub(crate) fn rgba32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.0[0], c.0[1], c.0[2], c.0[3])
}

pub fn to_rgba(c: Color32) -> Rgba {
    Rgba([c.r(), c.g(), c.b(), c.a()])
}

pub(super) fn typeface_font(face: Typeface, size: f32) -> FontId {
    match face {
        slate_doc::scene::Typeface::Sans => FontId::proportional(size),
        slate_doc::scene::Typeface::Mono => FontId::monospace(size),
        slate_doc::scene::Typeface::Serif => {
            FontId::new(size, egui::FontFamily::Name("slate-serif".into()))
        }
        other => FontId::new(
            size,
            egui::FontFamily::Name(other.egui_family().unwrap_or("slate-serif").into()),
        ),
    }
}

pub(super) fn egui_align(align: TextAlign) -> egui::Align {
    match align {
        TextAlign::Left => egui::Align::LEFT,
        TextAlign::Center => egui::Align::Center,
        TextAlign::Right => egui::Align::RIGHT,
    }
}

/// Screen galley for authored text. `font.size` and `world_wrap` are world
/// units; line breaks come from [`canvas_text::world_layout`] and do not
/// depend on `zoom`. The caret hit-tests this same galley.
pub(super) fn zoom_text_galley(
    ctx: &egui::Context,
    text: &str,
    font: FontId,
    world_wrap: f32,
    align: TextAlign,
    color: Color32,
    zoom: f32,
) -> std::sync::Arc<egui::Galley> {
    let layout = canvas_text::world_layout(ctx, text, font, world_wrap, egui_align(align));
    canvas_text::zoom_galley(ctx, &layout, color, zoom)
}

/// Excerpt type size on a text document card, in world units.
pub(super) const TEXT_CARD_BODY_PX: f32 = 9.0;

/// Where a text document card's words sit inside its screen rect.
pub(super) fn text_card_inner(srect: Rect, corner: slate_doc::scene::Corner, z: f32) -> Rect {
    let (_, radius) = corner.effective(srect.width() / z, srect.height() / z);
    let pad = canvas_scale::px(8.0, z).max(canvas_scale::px(radius, z));
    srect.shrink(pad)
}

/// Largest screen extent an inline text editor may hand to egui.
pub(super) const EDITOR_RECT_MAX: f32 = 1.0e7;

/// Gate for every screen rect an inline text editor registers with egui.
/// egui's hit test panics on a non-finite widget rect, so callers skip
/// registration for the frame instead.
pub(super) fn editor_rect_ok(r: Rect) -> bool {
    let ok =
        r.is_finite() && r.width().abs() <= EDITOR_RECT_MAX && r.height().abs() <= EDITOR_RECT_MAX;
    debug_assert!(ok, "inline text editor rect must be finite: {r:?}");
    ok
}

pub(super) fn offset_text_row(row: &mut egui::epaint::text::Row, dx: f32, dy: f32) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    let delta = egui::vec2(dx, dy);
    row.rect = row.rect.translate(delta);
    for glyph in &mut row.glyphs {
        glyph.pos += delta;
    }
    for vertex in &mut row.visuals.mesh.vertices {
        vertex.pos += delta;
    }
    row.visuals.mesh_bounds = row.visuals.mesh_bounds.translate(delta);
}

/// Move the text block to the vertical middle of `box_h` without moving the
/// galley origin, so caret hit-testing stays aligned with the glyphs.
///
/// The fit is a world-space size. The camera is not an input: a zoom must
/// not pick a different size, or the note would reflow as it scales.
pub(super) fn measure_sticky_font(
    ctx: &egui::Context,
    text: &str,
    family: Typeface,
    max_size: f32,
    box_w: f32,
    box_h: f32,
    align: TextAlign,
) -> f32 {
    if text.trim().is_empty() {
        return max_size.max(slate_doc::scene::STICKY_FIT_MIN);
    }
    let wrap = box_w.max(0.0);
    fit_sticky_font(max_size, |size| {
        let layout = canvas_text::world_layout(
            ctx,
            text,
            typeface_font(family, size),
            wrap,
            egui_align(align),
        );
        layout.height <= box_h + 0.5
    })
}

/// Largest size in `[min, max]` for which `fits` is true. `max` wins when it fits.
pub(crate) fn fit_sticky_font(max_size: f32, mut fits: impl FnMut(f32) -> bool) -> f32 {
    let min_size = slate_doc::scene::STICKY_FIT_MIN;
    let max_size = max_size.max(min_size);
    if fits(max_size) {
        return max_size;
    }
    let mut lo = min_size;
    let mut hi = max_size;
    for _ in 0..8 {
        let mid = (lo + hi) * 0.5;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Cached shrink-to-fit for one sticky. Not part of the document.
#[derive(Clone)]
pub(crate) struct StickyFit {
    pub(super) text: String,
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) max: f32,
    pub(super) family: Typeface,
    pub(super) align: TextAlign,
    pub(super) fitted: f32,
}

pub(super) fn center_galley_vertically(galley: &mut egui::Galley, box_h: f32) {
    let dy = (box_h - galley.rect.height()) * 0.5;
    if dy <= 0.0 {
        return;
    }
    for row in &mut galley.rows {
        offset_text_row(row, 0.0, dy);
    }
    galley.rect.max.y = box_h;
    galley.mesh_bounds = galley.mesh_bounds.translate(egui::vec2(0.0, dy));
}
