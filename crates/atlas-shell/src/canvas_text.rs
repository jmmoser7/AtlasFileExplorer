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
//! [`layout`] wraps at the on-screen width. egui rounds every glyph advance
//! to a physical pixel, so the same word can cross the line's capacity when
//! the camera moves and the raster size changes. Authored text must not do
//! that: a line break is a property of the text, the typeface, the world
//! font size, and the world wrap width.
//!
//! [`world_layout`] shapes once at a fixed 64-physical-pixel reference and
//! caches the breaks. [`zoom_galley`] paints those lines through the same
//! ladder as [`layout_no_wrap`], each line at its world position times the
//! zoom, so only the rasterization changes with the camera.
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
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
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
    /// Use native text-edit layout with the same final canvas transform.
    pub fn from_galley(galley: Arc<Galley>, scale: f32) -> Self {
        Self { galley, scale }
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
                    color,
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

/// [`layout`] limited to `max_rows`, ending in an ellipsis when text remains.
pub fn layout_rows(
    painter: &Painter,
    text: String,
    font: FontId,
    color: Color32,
    wrap_width: f32,
    max_rows: usize,
) -> Scaled {
    let (font, scale) = on_ladder(painter, font);
    let mut job = egui::text::LayoutJob::simple(text, font, color, wrap_width / scale);
    job.wrap.max_rows = max_rows;
    job.wrap.overflow_character = Some('…');
    let galley = painter.ctx().fonts(|fonts| fonts.layout_job(job));
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
    ladder_font(painter.ctx().pixels_per_point(), font)
}

fn ladder_font(pixels_per_point: f32, font: FontId) -> (FontId, f32) {
    let ppp = pixels_per_point.max(0.01);
    let wanted = (font.size * ppp).max(1.0);
    let raster = raster_px(wanted);
    (FontId::new(raster / ppp, font.family), wanted / raster)
}

// ---------------------------------------------------------------------------
// World-space line breaks
// ---------------------------------------------------------------------------

/// Physical pixels of the one raster a line break is shaped at.
///
/// High enough that a one-pixel advance round is a small fraction of a word,
/// and independent of the camera. The world font size only scales the wrap
/// width into this raster and the resulting rows back out.
const REFERENCE_PX: f32 = 64.0;

/// How many world layouts are kept. A camera move does not add a key; an
/// edit does. Past this, the least recently used entry is dropped.
const WORLD_CACHE_CAP: usize = 256;

/// One wrapped line, in the same units as the font size and wrap width
/// (world units, for board text).
#[derive(Clone, Debug)]
pub struct WorldLine {
    /// Bytes of the source this line paints. Excludes a trailing newline.
    pub bytes: std::ops::Range<usize>,
    /// Glyph left edge after alignment.
    pub x: f32,
    /// Top of the line box.
    pub y: f32,
    /// Advance width of the glyphs.
    pub width: f32,
    /// Line box height.
    pub height: f32,
    /// This row ended on a `\n` in the source. The last row of a galley never
    /// does — a trailing newline is its own empty row after this one.
    pub ends_with_newline: bool,
}

/// Line breaks for one string at one world size and wrap width.
#[derive(Clone, Debug)]
pub struct WorldLayout {
    pub lines: Vec<WorldLine>,
    /// Wrap width, or the widest row when the wrap is unbounded.
    pub width: f32,
    /// Block height. The first line starts at y = 0.
    pub height: f32,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum FamilyKey {
    Proportional,
    Monospace,
    Name(Arc<str>),
}

struct Slot {
    text: String,
    family: FamilyKey,
    size_bits: u32,
    wrap_bits: u32,
    align: u8,
    ppp_bits: u32,
    layout: Arc<WorldLayout>,
    used: u64,
}

struct WorldCache {
    buckets: HashMap<u64, Vec<Slot>>,
    len: usize,
    clock: u64,
    shapes: u64,
}

impl WorldCache {
    fn new() -> Self {
        Self {
            buckets: HashMap::new(),
            len: 0,
            clock: 0,
            shapes: 0,
        }
    }
}

thread_local! {
    static WORLD_CACHE: RefCell<WorldCache> = RefCell::new(WorldCache::new());
}

fn family_key(family: &egui::FontFamily) -> FamilyKey {
    match family {
        egui::FontFamily::Proportional => FamilyKey::Proportional,
        egui::FontFamily::Monospace => FamilyKey::Monospace,
        egui::FontFamily::Name(name) => FamilyKey::Name(Arc::clone(name)),
    }
}

fn align_tag(align: egui::Align) -> u8 {
    match align {
        egui::Align::Center => 1,
        egui::Align::Max => 2,
        egui::Align::Min => 0,
    }
}

fn cache_hash(
    text: &str,
    family: &FamilyKey,
    size_bits: u32,
    wrap_bits: u32,
    align: u8,
    ppp_bits: u32,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    family.hash(&mut hasher);
    size_bits.hash(&mut hasher);
    wrap_bits.hash(&mut hasher);
    align.hash(&mut hasher);
    ppp_bits.hash(&mut hasher);
    hasher.finish()
}

/// How many times line breaking has shaped text on this thread.
///
/// A cache hit does not increment it. Painting a cached layout at another
/// zoom goes through [`zoom_galley`] and does not either — that path only
/// rasterizes the lines already chosen.
pub fn world_layout_shapes() -> u64 {
    WORLD_CACHE.with(|cache| cache.borrow().shapes)
}

/// Drop cached line breaks. The next layout of the same text shapes again.
pub fn clear_world_layout_cache() {
    WORLD_CACHE.with(|cache| *cache.borrow_mut() = WorldCache::new());
    ZOOM_CACHE.with(|cache| *cache.borrow_mut() = ZoomCache::default());
}

/// Break `text` in world units. Zoom is not an input.
///
/// `font.size` and `wrap_width` share one unit system. An unbounded wrap
/// (`f32::INFINITY`) keeps each paragraph on one line. The same key returns
/// the cached breaks and does not shape again.
pub fn world_layout(
    ctx: &egui::Context,
    text: &str,
    font: FontId,
    wrap_width: f32,
    align: egui::Align,
) -> Arc<WorldLayout> {
    let wrap = if wrap_width.is_finite() {
        wrap_width.max(0.0)
    } else {
        f32::INFINITY
    };
    let size = font.size.max(1.0e-3);
    let family = family_key(&font.family);
    let size_bits = size.to_bits();
    let wrap_bits = wrap.to_bits();
    let align_bits = align_tag(align);
    let ppp_bits = ctx.pixels_per_point().to_bits();
    let hash = cache_hash(text, &family, size_bits, wrap_bits, align_bits, ppp_bits);

    if let Some(hit) = WORLD_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clock = cache.clock.wrapping_add(1);
        let used = cache.clock;
        let slot = cache.buckets.get_mut(&hash).and_then(|slots| {
            slots.iter_mut().find(|slot| {
                slot.text == text
                    && slot.family == family
                    && slot.size_bits == size_bits
                    && slot.wrap_bits == wrap_bits
                    && slot.align == align_bits
                    && slot.ppp_bits == ppp_bits
            })
        })?;
        slot.used = used;
        Some(Arc::clone(&slot.layout))
    }) {
        return hit;
    }

    let layout = Arc::new(shape_world(ctx, text, font, size, wrap, align));
    WORLD_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.shapes += 1;
        cache.clock += 1;
        let used = cache.clock;
        cache.buckets.entry(hash).or_default().push(Slot {
            text: text.to_owned(),
            family,
            size_bits,
            wrap_bits,
            align: align_bits,
            ppp_bits,
            layout: Arc::clone(&layout),
            used,
        });
        cache.len += 1;
        if cache.len > WORLD_CACHE_CAP {
            evict_oldest(&mut cache);
        }
    });
    layout
}

fn evict_oldest(cache: &mut WorldCache) {
    let mut oldest: Option<(u64, usize, u64)> = None;
    for (hash, slots) in &cache.buckets {
        for (index, slot) in slots.iter().enumerate() {
            let replace = oldest.is_none_or(|(_, _, used)| slot.used < used);
            if replace {
                oldest = Some((*hash, index, slot.used));
            }
        }
    }
    if let Some((hash, index, _)) = oldest {
        if let Some(slots) = cache.buckets.get_mut(&hash) {
            slots.swap_remove(index);
            if slots.is_empty() {
                cache.buckets.remove(&hash);
            }
            cache.len = cache.len.saturating_sub(1);
        }
    }
}

fn shape_world(
    ctx: &egui::Context,
    text: &str,
    font: FontId,
    world_size: f32,
    wrap: f32,
    align: egui::Align,
) -> WorldLayout {
    let ppp = ctx.pixels_per_point().max(0.01);
    let logical = REFERENCE_PX / ppp;
    let to_world = world_size / logical;
    let wrap_logical = if wrap.is_finite() {
        wrap / to_world
    } else {
        f32::INFINITY
    };
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_logical;
    job.halign = egui::Align::LEFT;
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: FontId::new(logical, font.family.clone()),
            color: Color32::PLACEHOLDER,
            ..Default::default()
        },
    );
    let galley = ctx.fonts(|fonts| fonts.layout_job(job));

    let mut byte = 0;
    let mut lines = Vec::with_capacity(galley.rows.len());
    for row in &galley.rows {
        let start = byte;
        for glyph in &row.glyphs {
            let Some(ch) = text[byte..].chars().next() else {
                break;
            };
            debug_assert_eq!(ch, glyph.chr, "shaper glyph diverged from the source");
            byte += ch.len_utf8();
        }
        let end = byte;
        if row.ends_with_newline && text[byte..].starts_with('\n') {
            byte += 1;
        }
        let glyph_w = row.rect.width() * to_world;
        let glyph_left = row.rect.min.x * to_world;
        lines.push(WorldLine {
            bytes: start..end,
            x: glyph_left,
            y: row.rect.min.y * to_world,
            width: glyph_w,
            height: row.rect.height() * to_world,
            ends_with_newline: row.ends_with_newline,
        });
    }
    debug_assert!(
        byte == text.len() || text[byte..].chars().all(|ch| ch == '\n'),
        "line breaks did not cover the source"
    );

    let content_w = lines.iter().map(|line| line.width).fold(0.0, f32::max);
    let box_w = if wrap.is_finite() { wrap } else { content_w };
    for line in &mut lines {
        let dx = match align {
            egui::Align::Center => (box_w - line.width) * 0.5,
            egui::Align::Max => box_w - line.width,
            egui::Align::Min => 0.0,
        };
        line.x += dx;
    }
    let height = galley.rect.height() * to_world;
    WorldLayout {
        lines,
        width: box_w,
        height,
    }
}

/// A screen-space galley of `layout`'s lines.
///
/// Each line is rasterized on the ladder at `screen_font` and placed at
/// `world position × zoom`. The row text is `layout`'s; zoom does not
/// rebreak it. Glyph positions and the mesh are the same geometry, so a
/// caret hit-tested against this galley sits on the ink.
///
/// Cached by layout identity, text, font, color, zoom and pixels per point:
/// a repaint with the camera still returns the same `Arc` without
/// allocating. A zoom gesture misses once per frame per visible text, and
/// those entries age out of the bounded cache.
pub fn zoom_galley(
    ctx: &egui::Context,
    text: &str,
    layout: &Arc<WorldLayout>,
    screen_font: FontId,
    color: Color32,
    zoom: f32,
) -> Arc<Galley> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    let text_hash = hasher.finish();
    let family = family_key(&screen_font.family);
    let size_bits = screen_font.size.to_bits();
    let zoom_bits = zoom.to_bits();
    let ppp_bits = ctx.pixels_per_point().to_bits();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (
        Arc::as_ptr(layout) as usize,
        text_hash,
        size_bits,
        zoom_bits,
    )
        .hash(&mut hasher);
    (ppp_bits, color.to_array(), &family).hash(&mut hasher);
    let key = hasher.finish();
    let hit = ZOOM_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clock = cache.clock.wrapping_add(1);
        let used = cache.clock;
        let slot = cache.buckets.get_mut(&key).and_then(|slots| {
            slots.iter_mut().find(|slot| {
                Arc::ptr_eq(&slot.layout, layout)
                    && slot.text_hash == text_hash
                    && slot.text_len == text.len()
                    && slot.size_bits == size_bits
                    && slot.zoom_bits == zoom_bits
                    && slot.ppp_bits == ppp_bits
                    && slot.color == color
                    && slot.family == family
            })
        })?;
        slot.used = used;
        Some(Arc::clone(&slot.galley))
    });
    if let Some(galley) = hit {
        return galley;
    }
    let galley = build_zoom_galley(ctx, text, layout, screen_font, color, zoom);
    ZOOM_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.builds += 1;
        let used = cache.clock;
        cache.buckets.entry(key).or_default().push(ZoomSlot {
            layout: Arc::clone(layout),
            text_hash,
            text_len: text.len(),
            family,
            size_bits,
            color,
            zoom_bits,
            ppp_bits,
            galley: Arc::clone(&galley),
            used,
        });
        cache.len += 1;
        if cache.len > ZOOM_CACHE_CAP {
            let oldest = cache
                .buckets
                .iter()
                .flat_map(|(key, slots)| {
                    slots
                        .iter()
                        .enumerate()
                        .map(move |(index, slot)| (*key, index, slot.used))
                })
                .min_by_key(|(_, _, used)| *used);
            if let Some((key, index, _)) = oldest {
                if let Some(slots) = cache.buckets.get_mut(&key) {
                    slots.swap_remove(index);
                    if slots.is_empty() {
                        cache.buckets.remove(&key);
                    }
                }
                cache.len -= 1;
            }
        }
    });
    galley
}

/// How many screen galleys [`zoom_galley`] has built on this thread. A
/// repaint at the same zoom does not increment it.
pub fn zoom_galley_builds() -> u64 {
    ZOOM_CACHE.with(|cache| cache.borrow().builds)
}

/// Most screen galleys [`zoom_galley`] keeps.
const ZOOM_CACHE_CAP: usize = 512;

struct ZoomSlot {
    /// Held so the layout's address cannot be reused while this entry lives.
    layout: Arc<WorldLayout>,
    text_hash: u64,
    text_len: usize,
    family: FamilyKey,
    size_bits: u32,
    color: Color32,
    zoom_bits: u32,
    ppp_bits: u32,
    galley: Arc<Galley>,
    used: u64,
}

#[derive(Default)]
struct ZoomCache {
    buckets: HashMap<u64, Vec<ZoomSlot>>,
    len: usize,
    clock: u64,
    builds: u64,
}

thread_local! {
    static ZOOM_CACHE: RefCell<ZoomCache> = RefCell::new(ZoomCache::default());
}

fn build_zoom_galley(
    ctx: &egui::Context,
    text: &str,
    layout: &WorldLayout,
    screen_font: FontId,
    color: Color32,
    zoom: f32,
) -> Arc<Galley> {
    let zoom = if zoom.is_finite() { zoom.max(0.0) } else { 0.0 };
    let (ladder_font, scale) = ladder_font(ctx.pixels_per_point(), screen_font.clone());
    let mut rows = Vec::with_capacity(layout.lines.len());
    let mut mesh_bounds = Rect::NOTHING;
    let mut num_vertices = 0;
    let mut num_indices = 0;

    ctx.fonts(|fonts| {
        for line in &layout.lines {
            let slice = text.get(line.bytes.clone()).unwrap_or("");
            if slice.is_empty() {
                rows.push(empty_row(line, zoom, line.ends_with_newline));
                continue;
            }
            let laid =
                fonts.layout_no_wrap(slice.to_owned(), ladder_font.clone(), Color32::PLACEHOLDER);
            let Some(src) = laid.rows.first() else {
                rows.push(empty_row(line, zoom, line.ends_with_newline));
                continue;
            };
            let origin = egui::vec2(line.x, line.y) * zoom;
            let translate = origin - src.rect.min.to_vec2() * scale;
            let mut glyphs = Vec::with_capacity(src.glyphs.len());
            for glyph in &src.glyphs {
                let mut glyph = *glyph;
                glyph.pos = (glyph.pos.to_vec2() * scale + translate).to_pos2();
                glyph.advance_width *= scale;
                glyph.line_height *= scale;
                glyph.font_ascent *= scale;
                glyph.font_height *= scale;
                glyph.font_impl_ascent *= scale;
                glyph.font_impl_height *= scale;
                glyphs.push(glyph);
            }
            let mut mesh = src.visuals.mesh.clone();
            for vertex in &mut mesh.vertices {
                vertex.pos = (vertex.pos.to_vec2() * scale + translate).to_pos2();
                vertex.color = color;
            }
            let row_mesh_bounds = if mesh.vertices.is_empty() {
                Rect::NOTHING
            } else {
                mesh.calc_bounds()
            };
            mesh_bounds = mesh_bounds.union(row_mesh_bounds);
            num_vertices += mesh.vertices.len();
            num_indices += mesh.indices.len();
            let rect = Rect::from_min_size(
                egui::pos2(line.x * zoom, line.y * zoom),
                egui::vec2(src.rect.width() * scale, line.height * zoom),
            );
            rows.push(egui::epaint::text::Row {
                section_index_at_start: src.section_index_at_start,
                glyphs,
                rect,
                visuals: egui::epaint::text::RowVisuals {
                    mesh,
                    mesh_bounds: row_mesh_bounds,
                    glyph_index_start: src.visuals.glyph_index_start,
                    glyph_vertex_range: src.visuals.glyph_vertex_range.clone(),
                },
                ends_with_newline: line.ends_with_newline,
            });
        }
    });

    if rows.is_empty() {
        rows.push(empty_row(
            &WorldLine {
                bytes: 0..0,
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: screen_font.size.max(0.0),
                ends_with_newline: false,
            },
            1.0,
            false,
        ));
    }

    let job = egui::text::LayoutJob::simple(
        text.to_owned(),
        screen_font,
        Color32::PLACEHOLDER,
        (layout.width * zoom).max(0.0),
    );
    Arc::new(Galley {
        job: Arc::new(job),
        rows,
        elided: false,
        rect: Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(layout.width * zoom, layout.height * zoom),
        ),
        mesh_bounds,
        num_vertices,
        num_indices,
        pixels_per_point: ctx.pixels_per_point(),
    })
}

fn empty_row(line: &WorldLine, zoom: f32, ends_with_newline: bool) -> egui::epaint::text::Row {
    egui::epaint::text::Row {
        section_index_at_start: 0,
        glyphs: Vec::new(),
        rect: Rect::from_min_size(
            egui::pos2(line.x * zoom, line.y * zoom),
            egui::vec2(0.0, line.height * zoom),
        ),
        visuals: Default::default(),
        ends_with_newline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_selection_copies_text_at_multiple_zooms() {
        for scale in [0.5, 1.0, 2.0] {
            let ctx = egui::Context::default();
            let mut time = 0.0;
            let mut frame = |events| {
                time += 0.1;
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let galley = ui.painter().layout_no_wrap(
                                "Selectable reply".into(),
                                FontId::proportional(20.0),
                                Color32::WHITE,
                            );
                            Scaled::from_galley(galley, scale).selectable(
                                ui,
                                egui::Id::new("reply"),
                                Pos2::new(50.0, 50.0),
                                Color32::WHITE,
                            );
                        });
                    },
                )
            };
            frame(vec![]);
            frame(vec![]);
            let first = Pos2::new(50.0, 50.0 + 10.0 * scale);
            let last = Pos2::new(50.0 + 160.0 * scale, first.y);
            frame(vec![
                egui::Event::PointerMoved(first),
                egui::Event::PointerButton {
                    pos: first,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ]);
            frame(vec![egui::Event::PointerMoved(last)]);
            frame(vec![egui::Event::PointerButton {
                pos: last,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }]);
            let output = frame(vec![egui::Event::Copy]);
            assert!(
                output
                    .platform_output
                    .commands
                    .iter()
                    .any(|c| matches!(c,egui::OutputCommand::CopyText(s) if s=="Selectable reply")),
                "copy failed at scale {scale}: {:?}",
                output.platform_output.commands
            );
        }
    }

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

    const ZOOM_SWEEP: [f32; 10] = [0.1, 0.25, 0.37, 0.5, 0.73, 1.0, 1.5, 2.3, 4.0, 8.0];

    fn with_ctx(mut body: impl FnMut(&egui::Context)) {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| body(ctx));
    }

    fn row_text(galley: &Galley) -> Vec<String> {
        galley.rows.iter().map(|row| row.text()).collect()
    }

    fn breaks_at(
        ctx: &egui::Context,
        text: &str,
        size: f32,
        wrap: f32,
        align: egui::Align,
        zoom: f32,
    ) -> Vec<String> {
        let font = FontId::proportional(size);
        let layout = world_layout(ctx, text, font.clone(), wrap, align);
        let screen = FontId::new(size * zoom, font.family);
        let galley = zoom_galley(ctx, text, &layout, screen, Color32::WHITE, zoom);
        let chars: usize = galley
            .rows
            .iter()
            .map(|row| row.char_count_including_newline())
            .sum();
        assert_eq!(chars, text.chars().count(), "caret map dropped a character");
        row_text(&galley)
    }

    #[test]
    fn line_breaks_do_not_change_with_zoom() {
        let samples = [
            (
                "Short.\n\nA longer paragraph with several words that need to wrap inside a narrow column of the board.\nsupercalifragilisticexpialidocious\n尾声",
                11.0,
                160.0,
            ),
            (
                "看板文字在缩放时不得改换行位置即使是连续汉字也一样保持稳定",
                18.0,
                140.0,
            ),
            (
                "alpha bravo charlie delta echo foxtrot golf hotel india\n\nsecond paragraph stays put while the camera moves",
                24.0,
                220.0,
            ),
            ("one two three four five six seven eight nine ten", 8.0, 70.0),
            ("Title that is wider than its box", 48.0, 180.0),
        ];
        with_ctx(|ctx| {
            for (text, size, wrap) in samples {
                for align in [egui::Align::LEFT, egui::Align::Center, egui::Align::RIGHT] {
                    let expected = breaks_at(ctx, text, size, wrap, align, 1.0);
                    let cjk = text.chars().any(|ch| ch > '\u{2E80}');
                    if !cjk {
                        assert!(expected.len() > 1, "sample did not wrap: {text}");
                    }
                    for zoom in ZOOM_SWEEP {
                        let got = breaks_at(ctx, text, size, wrap, align, zoom);
                        assert_eq!(
                            got, expected,
                            "breaks changed at zoom {zoom} size {size} align {align:?}\n{text}"
                        );
                    }
                }
            }
            let mono = "fn main() {\n    println!(\"hi\");\n}\n";
            let expected = {
                let font = FontId::monospace(13.0);
                let layout = world_layout(ctx, mono, font.clone(), 90.0, egui::Align::LEFT);
                let galley = zoom_galley(
                    ctx,
                    mono,
                    &layout,
                    FontId::monospace(13.0),
                    Color32::WHITE,
                    1.0,
                );
                row_text(&galley)
            };
            for zoom in ZOOM_SWEEP {
                let font = FontId::monospace(13.0);
                let layout = world_layout(ctx, mono, font.clone(), 90.0, egui::Align::LEFT);
                let galley = zoom_galley(
                    ctx,
                    mono,
                    &layout,
                    FontId::new(13.0 * zoom, font.family),
                    Color32::WHITE,
                    zoom,
                );
                assert_eq!(row_text(&galley), expected);
            }
        });
    }

    #[test]
    fn a_world_point_stays_on_the_same_line() {
        let text = "alpha bravo charlie delta echo foxtrot golf\nsecond paragraph stays put";
        with_ctx(|ctx| {
            let layout = world_layout(
                ctx,
                text,
                FontId::proportional(16.0),
                150.0,
                egui::Align::LEFT,
            );
            assert!(
                layout.lines.len() >= 2,
                "expected a wrapped first paragraph"
            );
            let line = &layout.lines[1];
            let world = egui::vec2(line.x + 2.0, line.y + line.height * 0.5);
            for zoom in ZOOM_SWEEP {
                let galley = zoom_galley(
                    ctx,
                    text,
                    &layout,
                    FontId::proportional(16.0 * zoom),
                    Color32::WHITE,
                    zoom,
                );
                let cursor = galley.cursor_from_pos(world * zoom);
                assert_eq!(cursor.rcursor.row, 1, "caret left its line at zoom {zoom}");
            }
        });
    }

    #[test]
    fn zoom_and_pan_do_not_reshape() {
        let text = "cached paragraph that wraps more than once across the column";
        with_ctx(|ctx| {
            clear_world_layout_cache();
            let before = world_layout_shapes();
            let font = FontId::proportional(20.0);
            let layout = world_layout(ctx, text, font.clone(), 120.0, egui::Align::LEFT);
            assert_eq!(world_layout_shapes(), before + 1);
            for zoom in ZOOM_SWEEP {
                let _ = world_layout(ctx, text, font.clone(), 120.0, egui::Align::LEFT);
                let _ = zoom_galley(
                    ctx,
                    text,
                    &layout,
                    FontId::proportional(20.0 * zoom),
                    Color32::WHITE,
                    zoom,
                );
            }
            assert_eq!(
                world_layout_shapes(),
                before + 1,
                "pure zoom reshaped the paragraph"
            );
        });
    }

    #[test]
    fn a_still_camera_reuses_the_screen_galley() {
        let text = "a note that repaints every frame while nothing moves";
        with_ctx(|ctx| {
            clear_world_layout_cache();
            let font = FontId::proportional(18.0);
            let paint = |text: &str, zoom: f32| {
                let layout = world_layout(ctx, text, font.clone(), 140.0, egui::Align::LEFT);
                zoom_galley(
                    ctx,
                    text,
                    &layout,
                    FontId::proportional(18.0 * zoom),
                    Color32::WHITE,
                    zoom,
                )
            };
            let first = paint(text, 1.5);
            let builds = zoom_galley_builds();
            let again = paint(text, 1.5);
            assert!(Arc::ptr_eq(&first, &again), "a repaint rebuilt the galley");
            assert_eq!(zoom_galley_builds(), builds);
            let _ = paint(text, 2.0);
            assert_eq!(zoom_galley_builds(), builds + 1, "a new zoom must rebuild");
            let edited = paint("a note that was edited", 1.5);
            assert!(!Arc::ptr_eq(&first, &edited));
            for i in 0..(ZOOM_CACHE_CAP + 8) {
                let _ = paint(text, 1.0 + i as f32 * 0.001);
            }
            ZOOM_CACHE.with(|cache| assert!(cache.borrow().len <= ZOOM_CACHE_CAP));
        });
    }
}
