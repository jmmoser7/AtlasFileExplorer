//! Canvas-space text sizing — the one place both apps compute how big text
//! painted *into* the canvas should be.
//!
//! The rule is pattern **P0.9** in `docs/keymap/contracts/PATTERNS.md` (L0,
//! both apps): every canvas object — type, icons, badges, node-local tabs —
//! scales with the camera as a shape does. Text that belongs to a canvas
//! object renders as part of that object, so its size is fixed to the host's
//! size on screen and it tracks the host through every zoom. File Atlas' tag
//! labels are the reference — a tag's name, meta, and path are each a
//! fraction of the card's on-screen height, which is why type there reads as
//! being *in* the canvas rather than floating above it.
//!
//! Two kinds of text, because they answer to different masters:
//!
//! - [`authored_px`] — the user typed it and its size is a document property
//!   (`TextNode.size`, world units). Exactly `size × zoom`: no floor, no
//!   ceiling, no auto-fit. That ratio is the composition the user authored,
//!   and the artifact writer has to reproduce it or the export stops being a
//!   serialization of the same model (Constitution Art. IV).
//! - [`derived_px`] / [`derived_world_px`] — the app generated it *about* an
//!   object: tag names, frame titles, media badges, portal captions. Sized in
//!   canvas units either way — a fraction of the host's on-screen height, or a
//!   fixed world size times the zoom — with a legibility floor allowed,
//!   because nothing in the document depends on its exact ratio.
//!
//! What both derived forms have in common is the thing that matters: the size
//! is expressed in canvas units, so the type belongs to the scene. A **screen**
//! constant does not, and neither does a canvas size with a ceiling bolted on,
//! which becomes a screen constant the moment the ceiling bites.
//!
//! **Never a ceiling, in either case.** `(10.0 * z).clamp(8.0, 13.0)` tracks
//! the host only in the middle of the zoom range and then freezes while the
//! object keeps growing — the precise failure this module exists to prevent.
//! When text gets too small to read, drop it ([`legible`]) or drop whole lines
//! by zoom bucket; do not clamp it into place.
//!
//! # Computing the size right is only half of it
//!
//! egui cannot paint a continuously varying font size continuously. It
//! rasterizes each font at a **whole number of physical pixels**
//! (`scale_in_pixels.round() as u32`), rounds every glyph advance and row
//! height to the pixel grid during layout, and — with the default
//! `round_text_to_pixels` — snaps the galley's origin to a physical pixel
//! before tessellating. Feed that a size that grows smoothly with the camera
//! and the type climbs a staircase while the host it names glides: the glyphs
//! hold one size, jump a pixel, hold again, and the whole block slides against
//! the node by up to a pixel per frame. Correct arithmetic, juddering result.
//!
//! So painting goes through [`text`] / [`layout`] here, which lay the text out
//! at a **quantized** size from a coarse geometric ladder and then scale the
//! resulting galley to the exact size with a mesh transform. The rasterized
//! size steps; what reaches the screen does not. Two consequences worth
//! knowing:
//!
//! - Between ladder steps the glyph bitmap is magnified by at most half a
//!   step (~4%), which is not visible, and in exchange the size is exact.
//! - The galley cache and the font atlas see a handful of sizes instead of a
//!   fresh one every frame of a zoom, so the layout itself is now a cache hit
//!   during a zoom where before it was a fresh shaping pass every frame
//!   (Art. II — nothing per-frame that can be cached). Scaling copies the
//!   cached galley's vertices, which is an O(glyphs) memcpy of the same
//!   vertices the tessellator has to walk anyway.
//!
//! Callers must also run [`install`] once per [`egui::Context`] to turn off
//! the origin snapping; the ladder cannot fix a position the tessellator
//! rounds after the fact.
//!
//! # Type on a yawed or perspective host
//!
//! [`text`] / [`layout`] fix zoom judder for **axis-aligned** canvas type.
//! They do not put glyphs *in* a rotating card. Cover Flow album titles
//! (and any future label that lives on a projected face) must:
//!
//! 1. Lay the string out **once** at a **fixed** pixel size — never re-fit
//!    the font to the apparent width as the card yaws. That rescale is the
//!    judder.
//! 2. Cache each glyph's card-local rect and font-atlas UV.
//! 3. Paint **each glyph** as a short column-strip mesh through
//!    `project_point` (the same projection as the card). A full-face
//!    title texture through `paint_artwork` foreshortens, but affine UV
//!    across a whole card ripples high-contrast stems. A letter is a few
//!    millimetres of face, so the same affine error is invisible.
//!
//! Do **not** `painter.galley` at the projected midpoint. That sits on the
//! card's edge, ignores yaw, and re-layouts every frame. Do **not** blit
//! the whole title to a texture and project that — photos hide the affine
//! error; type does not. The Cover Flow title-face is the reference.
//!
//! # Line breaks are not a function of zoom
//!
//! egui rounds every glyph advance to a physical pixel, so text laid out at
//! its screen size can push a word across the line's capacity when the
//! camera moves and the raster size changes. Canvas text that wraps —
//! authored or derived, a text node or a chat transcript — must not do that:
//! a line break is a property of the text, the typeface, the world font
//! size, and the world wrap width.
//!
//! [`world_layout`] shapes once at a fixed 64-physical-pixel reference and
//! records every line and every glyph position in world units. Painting goes
//! through [`world_text`] (or [`zoom_galley`] when egui needs a screen-space
//! galley, as an inline editor does). Each rasterizes the lines at a ladder
//! rung and moves every glyph to its recorded position, so the block is the
//! same shape at every zoom and only the glyph bitmaps change. Neither is
//! keyed by zoom: a gesture builds one galley per rung it crosses and never
//! shapes. Glyph advances inside a line are therefore the reference ones
//! too; laying a line out at the rung size and keeping its own advances
//! would slide words a pixel at each rung.
//!
//! [`WorldSpec`] adds what a label needs: one row cut with `…`, tracking,
//! and a block as wide as its text. [`text`], [`layout`], [`layout_rows`],
//! and [`layout_no_wrap`] take screen sizes but shape the same way, in units
//! of the font size, so a label's glyphs sit in proportion to its size and a
//! screen wrap that scales with the font breaks in the same places.
//!
//! The HTML artifact does not share this cache. It writes the same world
//! font size and the same box — shape text keeps its 8px padding — as CSS
//! `white-space: pre-wrap; line-height: 1.3`, and the browser breaks the
//! lines. Those breaks can differ from egui's: another line breaker, and a
//! line-height of 1.3 against the font's own row height. Sticky shrink-to-fit
//! on the board is this egui block height in world units; the artifact's
//! `fitStickies` binary-searches the browser's `scrollHeight`. Scaling the
//! slide does not reflow either side.

/// Feel constants for canvas text (P0.6 — named, not inline magic numbers).
pub mod consts {
    /// Below this on-screen size text is a smear, so the honest response is to
    /// stop drawing it rather than clamp it to a size the host cannot justify.
    /// Chosen to match what the board already treated as its lower bound.
    pub const MIN_RENDER_PX: f32 = 4.0;

    /// Floor for derived labels that must stay readable when the camera pulls
    /// back — a tag's name, a frame's title. Legibility, not composition.
    pub const LABEL_FLOOR_PX: f32 = 8.0;

    /// Floor for secondary derived lines (metadata, captions) that sit under a
    /// primary label and may go smaller before the LOD bucket drops them.
    pub const META_FLOOR_PX: f32 = 6.0;
}

/// Authored text: exactly `world_size × zoom` (P0.9.a).
///
/// No clamping of any kind. Ask [`legible`] whether the result is worth
/// painting; if it is not, skip the text — that is a level-of-detail decision,
/// and it keeps the glyph-to-geometry ratio honest at every zoom that draws.
#[inline]
pub fn authored_px(world_size: f32, zoom: f32) -> f32 {
    crate::canvas_scale::px(world_size, zoom)
}

/// Derived label: `fraction` of the host's on-screen height, floored (P0.9.b).
///
/// `host_screen_h` is the host rect's height **in screen pixels**, so the
/// result already carries the zoom — a caller that also multiplies by zoom has
/// applied it twice.
#[inline]
pub fn derived_px(host_screen_h: f32, fraction: f32, floor: f32) -> f32 {
    (host_screen_h * fraction).max(floor)
}

/// Derived label with a natural size in **world** units — a corner badge, a
/// caption, a placeholder name — painted at `world_size × zoom` (P0.9.b).
///
/// Use this where the label has a designed size of its own rather than one
/// borrowed from the host rect. It still tracks the canvas, so the label is
/// part of the scene; the floor keeps it readable when the camera pulls back.
#[inline]
pub fn derived_world_px(world_size: f32, zoom: f32, floor: f32) -> f32 {
    (world_size * zoom).max(floor)
}

/// Is this size worth painting, or should the caller drop the text entirely?
#[inline]
pub fn legible(px: f32) -> bool {
    px >= consts::MIN_RENDER_PX
}

// ---------------------------------------------------------------------------
// Painting: quantized layout + exact scale
// ---------------------------------------------------------------------------

use eframe::egui::{
    self, emath::TSTransform, epaint::TextShape, Align2, Color32, FontId, Galley, Painter, Pos2,
    Rect, Shape, Vec2,
};
use std::sync::Arc;

/// Ladder step, as a fraction of an octave. Four steps per doubling is a 19%
/// gap between rasterized sizes, bridged by a mesh scale of at most ~9%, which
/// is not a softness anyone can see. Finer than this and the ladder stops
/// buying anything: what makes text judder is *changing* the rasterized size,
/// so the rungs want to be far apart, not close together.
const LADDER_STEPS_PER_OCTAVE: f32 = 4.0;

/// Never rasterize smaller than this, in physical pixels — scale down instead.
///
/// egui rounds every glyph advance to a whole pixel while laying out, so at
/// small sizes a one-rung change in the rasterized size moves a word's width
/// by up to a fifth of it (measured: 18% at 8px, 26% at 4px). Holding one
/// rasterization across the whole small end and scaling it removes that
/// entirely, at the cost of sampling the glyph down.
const MIN_RASTER_PX: f32 = 16.0;

/// …but never sample a glyph down by more than this. The font atlas has no
/// mipmaps, so heavy minification would trade the judder for a shimmer.
const MAX_MINIFY: f32 = 2.0;

/// Largest size we ask the font atlas to rasterize, in physical pixels.
/// Above this the mesh is simply magnified: a glyph that tall is a title
/// filling the screen, and rasterizing a whole alphabet at that size would
/// grow the atlas by tens of megabytes for text nobody can see all of.
const MAX_RASTER_PX: f32 = 384.0;

/// Turn off the tessellator's pixel-grid snapping of text origins.
///
/// Call once per [`egui::Context`], in both apps, before the first frame.
/// With snapping on, canvas text lands on whole physical pixels while the node
/// it belongs to does not, so the two slide against each other as the camera
/// moves — the text is crisper and in the wrong place. Chrome text pays a
/// hair of sharpness for it, equally in both apps (shared-chrome rule).
pub fn install(ctx: &egui::Context) {
    ctx.tessellation_options_mut(|o| o.round_text_to_pixels = false);
}

/// Size we actually rasterize at, in physical pixels, for a wanted size.
fn raster_px(wanted: f32) -> f32 {
    let target = wanted
        .max(MIN_RASTER_PX)
        .min(wanted * MAX_MINIFY)
        .clamp(1.0, MAX_RASTER_PX);
    // Up to the next rung, never down: a glyph sampled slightly down stays
    // crisp, where the same glyph blown up reads as blurry. Unless rounding up
    // would spend more than the minification budget, in which case take the
    // rung below and let the glyph grow those few percent instead.
    let steps = target.log2() * LADDER_STEPS_PER_OCTAVE;
    let up = (steps.ceil() / LADDER_STEPS_PER_OCTAVE).exp2();
    let raster = if up > wanted * MAX_MINIFY {
        (steps.floor() / LADDER_STEPS_PER_OCTAVE).exp2()
    } else {
        up
    };
    raster.clamp(1.0, MAX_RASTER_PX)
}

/// Text laid out at a ladder size, plus the scale that takes it to the size
/// the caller asked for. Ask it for [`Scaled::size`] before painting when the
/// caller needs to place a background or a neighbour around it.
pub struct Scaled {
    galley: Arc<Galley>,
    scale: f32,
    /// Color for vertices the galley left uncolored; the paint call's color
    /// applies when this is `None`.
    color: Option<Color32>,
}

impl Scaled {
    /// Use native text-edit layout with the same final canvas transform.
    pub fn from_galley(galley: Arc<Galley>, scale: f32) -> Self {
        Self {
            galley,
            scale,
            color: None,
        }
    }

    /// Paint in `color` whatever color the paint call names.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    pub fn galley(&self) -> Arc<Galley> {
        Arc::clone(&self.galley)
    }
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Rotate about the label center, preserving exact board-space scale.
    pub fn paint_rotated(self, painter: &Painter, center: Pos2, angle: f32, color: Color32) {
        let origin = center - egui::emath::Rot2::from_angle(angle) * (self.size() * 0.5);
        let mut text = TextShape::new(Pos2::ZERO, self.galley, color).with_angle(angle);
        text.override_text_color = Some(color);
        let mut shape = Shape::Text(text);
        shape.transform(TSTransform::new(origin.to_vec2(), self.scale));
        painter.add(shape);
    }

    /// On-screen size of the text as it will paint — already scaled.
    #[inline]
    pub fn size(&self) -> Vec2 {
        self.galley.size() * self.scale
    }

    /// Selectable canvas text uses egui's native selection on a transformed child
    /// layer. The cached galley is unchanged, so zoom never reflows the transcript.
    pub fn selectable(self, ui: &egui::Ui, id: egui::Id, pos: Pos2, color: Color32) {
        let layer = egui::LayerId::new(ui.layer_id().order, id);
        let transform = TSTransform::new(pos.to_vec2(), self.scale);
        ui.ctx().set_transform_layer(layer, transform);
        ui.ctx().set_sublayer(ui.layer_id(), layer);
        let local = Rect::from_min_size(Pos2::ZERO, self.galley.size());
        egui::Area::new(id)
            .order(ui.layer_id().order)
            .fixed_pos(Pos2::ZERO)
            .movable(false)
            .constrain(false)
            .fade_in(false)
            .show(ui.ctx(), |text_ui| {
                text_ui.set_clip_rect(transform.inverse() * ui.clip_rect());
                let (rect, response) =
                    text_ui.allocate_exact_size(local.size(), egui::Sense::click_and_drag());
                egui::text_selection::LabelSelectionState::label_text_selection(
                    text_ui,
                    &response,
                    rect.min,
                    self.galley,
                    self.color.unwrap_or(color),
                    egui::Stroke::NONE,
                );
            });
    }

    /// Paint with `pos` as the text's top-left corner.
    pub fn paint(self, painter: &Painter, pos: Pos2, fallback_color: Color32) -> Rect {
        self.paint_anchored(painter, pos, Align2::LEFT_TOP, fallback_color)
    }

    /// Paint with `pos` interpreted through `anchor`, like [`Painter::text`].
    pub fn paint_anchored(
        self,
        painter: &Painter,
        pos: Pos2,
        anchor: Align2,
        fallback_color: Color32,
    ) -> Rect {
        let fallback_color = self.color.unwrap_or(fallback_color);
        let rect = anchor.anchor_size(pos, self.size());
        if (self.scale - 1.0).abs() < 1e-4 {
            painter.galley(rect.min, self.galley, fallback_color);
            return rect;
        }
        // Lay the glyphs out around the origin so the transform's translation
        // *is* the anchored position: `Shape::transform` scales a text shape's
        // vertices about the galley origin but sends `pos` through the whole
        // transform.
        let mut shape = Shape::Text(TextShape::new(Pos2::ZERO, self.galley, fallback_color));
        shape.transform(TSTransform::new(rect.min.to_vec2(), self.scale));
        painter.add(shape);
        rect
    }
}

/// World size the screen-sized calls below shape at. Any constant works: the
/// layout is in units of the font size, so one string is shaped once for
/// every size it paints at.
const UNIT_SIZE: f32 = 16.0;

/// Screen-sized text shaped in units of its own font size. Glyph positions,
/// breaks, and the block are then proportional to `font.size`, so a label
/// whose size and wrap both carry the zoom keeps its shape at every zoom.
fn unit_layout(painter: &Painter, text: &str, font: FontId, wrap: f32, max_rows: usize) -> Scaled {
    let zoom = font.size.max(0.0) / UNIT_SIZE;
    let wrap = if wrap.is_finite() && zoom > 0.0 {
        wrap / zoom
    } else {
        f32::INFINITY
    };
    let spec = WorldSpec {
        max_rows,
        hug: true,
        ..WorldSpec::new(FontId::new(UNIT_SIZE, font.family), wrap)
    };
    let layout = world_layout_spec(painter.ctx(), text, &spec);
    world_text(painter.ctx(), &layout, zoom)
}

/// [`Painter::layout`] for canvas text — `wrap_width` is in final, on-screen
/// units. Shaped by [`unit_layout`], so its breaks follow the ratio of wrap
/// to font size and not the raster.
pub fn layout(
    painter: &Painter,
    text: String,
    font: FontId,
    color: Color32,
    wrap_width: f32,
) -> Scaled {
    layout_rows(painter, text, font, color, wrap_width, usize::MAX)
}

/// [`layout`] limited to `max_rows`, ending in an ellipsis when text remains.
pub fn layout_rows(
    painter: &Painter,
    text: String,
    font: FontId,
    color: Color32,
    wrap_width: f32,
    max_rows: usize,
) -> Scaled {
    unit_layout(painter, &text, font, wrap_width, max_rows).with_color(color)
}

/// [`Painter::layout_no_wrap`] for canvas text.
pub fn layout_no_wrap(painter: &Painter, text: String, font: FontId, color: Color32) -> Scaled {
    layout_rows(painter, text, font, color, f32::INFINITY, usize::MAX)
}

/// [`Painter::text`] for canvas text. A repaint of the same label reuses its
/// cached layout and rung galley and allocates nothing.
pub fn text(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: impl std::fmt::Display,
    font: FontId,
    color: Color32,
) -> Rect {
    use std::fmt::Write;
    thread_local! {
        static LABEL: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }
    let laid = LABEL.with(|label| {
        let mut label = label.borrow_mut();
        label.clear();
        let _ = write!(label, "{text}");
        unit_layout(painter, &label, font, f32::INFINITY, usize::MAX)
    });
    laid.paint_anchored(painter, pos, anchor, color)
}

fn ladder_font(pixels_per_point: f32, font: FontId) -> (FontId, f32) {
    let ppp = pixels_per_point.max(0.01);
    let wanted = (font.size * ppp).max(1.0);
    let raster = raster_px(wanted);
    (FontId::new(raster / ppp, font.family), wanted / raster)
}

mod lru;
mod raster;
mod world;
pub use raster::{rung_galley_builds, world_text, world_wrapped, zoom_galley, zoom_galley_builds};
pub use world::{
    clear_world_layout_cache, world_layout, world_layout_rows, world_layout_shapes,
    world_layout_spec, WorldLayout, WorldLine, WorldSpec,
};

#[cfg(test)]
mod label_tests;
#[cfg(test)]
mod tests;
