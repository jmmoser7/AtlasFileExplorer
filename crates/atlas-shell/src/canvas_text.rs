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
}

impl Scaled {
    /// On-screen size of the text as it will paint — already scaled.
    #[inline]
    pub fn size(&self) -> Vec2 {
        self.galley.size() * self.scale
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

/// [`Painter::layout`] on the ladder — `wrap_width` is in final, on-screen units.
pub fn layout(
    painter: &Painter,
    text: String,
    font: FontId,
    color: Color32,
    wrap_width: f32,
) -> Scaled {
    let (font, scale) = on_ladder(painter, font);
    let galley = painter.layout(text, font, color, wrap_width / scale);
    Scaled { galley, scale }
}

/// [`Painter::layout_no_wrap`] on the ladder.
pub fn layout_no_wrap(painter: &Painter, text: String, font: FontId, color: Color32) -> Scaled {
    let (font, scale) = on_ladder(painter, font);
    let galley = painter.layout_no_wrap(text, font, color);
    Scaled { galley, scale }
}

/// [`Painter::text`] on the ladder — the drop-in for canvas-space text.
pub fn text(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: impl ToString,
    font: FontId,
    color: Color32,
) -> Rect {
    layout_no_wrap(painter, text.to_string(), font, color)
        .paint_anchored(painter, pos, anchor, color)
}

/// Split a wanted font size into the nearest ladder size and the residual scale.
fn on_ladder(painter: &Painter, font: FontId) -> (FontId, f32) {
    let ppp = painter.ctx().pixels_per_point().max(0.01);
    let wanted = (font.size * ppp).max(1.0);
    let raster = raster_px(wanted);
    (FontId::new(raster / ppp, font.family), wanted / raster)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole pattern is about: double the host, double the
    /// type. Anything that clamps breaks this at one end or the other.
    #[test]
    fn text_scales_one_for_one_with_its_host() {
        for zoom in [0.05_f32, 0.5, 1.0, 4.0, 40.0] {
            let single = authored_px(24.0, zoom);
            let double = authored_px(24.0, zoom * 2.0);
            assert!(
                (double - single * 2.0).abs() < 1e-3,
                "authored text stopped tracking the host at z={zoom}: {single} → {double}"
            );
        }

        // Above the floor, where the fraction is what decides the size — the
        // floor region below it is the one place clamping is correct.
        for host_h in [50.0_f32, 100.0, 1000.0, 10_000.0] {
            let single = derived_px(host_h, 0.36, consts::LABEL_FLOOR_PX);
            let double = derived_px(host_h * 2.0, 0.36, consts::LABEL_FLOOR_PX);
            assert!(
                (double - single * 2.0).abs() < 1e-3,
                "derived label stopped tracking the host at h={host_h}: {single} → {double}"
            );
        }
    }

    /// A floor is a legibility aid at the small end; there is no ceiling at
    /// the large end, which is the half people get wrong.
    #[test]
    fn the_floor_is_the_only_clamp() {
        assert_eq!(
            derived_px(1.0, 0.36, consts::LABEL_FLOOR_PX),
            consts::LABEL_FLOOR_PX,
            "a tiny host should still yield a readable label"
        );
        assert!(
            derived_px(100_000.0, 0.36, consts::LABEL_FLOOR_PX) > 30_000.0,
            "a huge host must keep growing its label — no ceiling"
        );
        assert!(
            authored_px(24.0, 500.0) > 10_000.0,
            "authored text must keep growing with the camera"
        );
    }

    #[test]
    fn text_too_small_to_read_is_dropped_rather_than_clamped() {
        assert!(!legible(authored_px(24.0, 0.01)));
        assert!(legible(authored_px(24.0, 1.0)));
    }

    /// Width of a word over a zoom sweep, divided by the zoom: how much the
    /// text drifts against the host it belongs to overall (`spread`), and the
    /// worst single frame-to-frame lurch (`jump`), which is what reads as
    /// judder. A rigid canvas object gives zero for both.
    fn sweep(paint: impl Fn(&Painter, f32) -> Vec2) -> (f32, f32) {
        let ctx = egui::Context::default();
        let (mut spread, mut jump) = (0.0_f32, 0.0_f32);
        let _ = ctx.run(Default::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::debug());
            let (mut lo, mut hi) = (f32::MAX, 0.0_f32);
            let mut prev: Option<f32> = None;
            // A fine sweep, as a scroll-wheel zoom would produce.
            for i in 0..=400 {
                let z = 0.5 + i as f32 * 0.00875;
                let per_zoom = paint(&painter, z).x / z;
                lo = lo.min(per_zoom);
                hi = hi.max(per_zoom);
                if let Some(p) = prev {
                    jump = jump.max((per_zoom - p).abs() / p);
                }
                prev = Some(per_zoom);
            }
            spread = (hi - lo) / lo;
        });
        (spread, jump)
    }

    /// The bug the painting half of this module exists to fix: a size that
    /// varies smoothly does not paint smoothly, because egui rasterizes at
    /// whole physical pixels. Text laid out directly visibly steps against its
    /// host; the same text through [`layout_no_wrap`] holds its proportion.
    #[test]
    fn zooming_does_not_make_text_climb_a_staircase() {
        let word = "Climate";
        for base in [8.0_f32, 11.0, 14.0, 24.0] {
            let (raw_spread, raw_jump) = sweep(|painter, z| {
                painter
                    .layout_no_wrap(
                        word.to_owned(),
                        FontId::proportional(base * z),
                        Color32::WHITE,
                    )
                    .size()
            });
            let (spread, jump) = sweep(|painter, z| {
                layout_no_wrap(
                    painter,
                    word.to_owned(),
                    FontId::proportional(base * z),
                    Color32::WHITE,
                )
                .size()
            });

            assert!(
                raw_jump > 0.05,
                "egui stopped quantizing text at {base}px ({:.1}% worst jump) \
                 — if that is genuinely fixed upstream, the ladder can go",
                raw_jump * 100.0
            );
            assert!(
                jump < 0.04 && spread < 0.07,
                "canvas text at {base}px lurched {:.1}% between frames and \
                 drifted {:.1}% across the sweep (raw egui: {:.1}% / {:.1}%)",
                jump * 100.0,
                spread * 100.0,
                raw_jump * 100.0,
                raw_spread * 100.0
            );
        }
    }

    /// The ladder must be coarse enough to be worth having, never blow a
    /// glyph up, and never sample one so far down that it shimmers.
    #[test]
    fn the_ladder_is_coarse_but_never_visibly_soft() {
        let mut sizes = std::collections::BTreeSet::new();
        for i in 0..4000 {
            let px = 4.0 + i as f32 * 0.1;
            let scale = px / raster_px(px);
            if px <= MAX_RASTER_PX {
                assert!(
                    scale <= 1.001,
                    "{px}px would be blown up from {}px — blurry",
                    raster_px(px)
                );
                assert!(
                    scale >= 1.0 / MAX_MINIFY - 0.001,
                    "{px}px would be sampled down {:.1}x from {}px — shimmery",
                    1.0 / scale,
                    raster_px(px)
                );
            }
            sizes.insert(raster_px(px).to_bits());
        }
        assert!(
            sizes.len() < 40,
            "{} rasterized sizes over the zoom range is atlas churn",
            sizes.len()
        );
    }
}
