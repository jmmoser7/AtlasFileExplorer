//! World-space line breaks. A string is shaped once, at a fixed reference
//! raster, for its world font size and world wrap width. The result records
//! every line's bytes and every glyph's position in world units, so a zoom
//! can only scale it: [`super::world_text`] and [`super::zoom_galley`] place
//! glyphs from these numbers and never break or advance text themselves.

use eframe::egui::{self, Color32, FontId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Physical pixels of the one raster a line break is shaped at.
///
/// High enough that a one-pixel advance round is a small fraction of a word,
/// and independent of the camera. The world font size only scales the wrap
/// width into this raster and the resulting rows back out.
const REFERENCE_PX: f32 = 64.0;

/// How many world layouts are kept. A camera move does not add a key; an
/// edit does. Past this, the least recently used entry is dropped.
const WORLD_CACHE_CAP: usize = 256;

/// Wrap widths are keyed in steps of 1/64 world unit. A wrap measured from a
/// screen rect and divided by the zoom carries float noise; that noise must
/// not become a new key, or every frame of a zoom would shape again.
const WRAP_STEPS_PER_UNIT: f32 = 64.0;

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
    /// Baseline, measured from the top of the block.
    pub baseline: f32,
    /// Each glyph's left edge, measured from `x`. One entry per painted glyph,
    /// including the trailing `…` of an elided line.
    pub glyph_x: Vec<f32>,
    /// A row limit cut this line short; it paints `…` after its bytes.
    pub ellipsis: bool,
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
    /// The source the line byte ranges index into.
    pub text: Arc<str>,
    /// The world font the breaks were shaped for.
    pub font: FontId,
    /// A row limit dropped text after the last line.
    pub elided: bool,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) enum FamilyKey {
    Proportional,
    Monospace,
    Name(Arc<str>),
}

pub(super) fn family_key(family: &egui::FontFamily) -> FamilyKey {
    match family {
        egui::FontFamily::Proportional => FamilyKey::Proportional,
        egui::FontFamily::Monospace => FamilyKey::Monospace,
        egui::FontFamily::Name(name) => FamilyKey::Name(Arc::clone(name)),
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    family: FamilyKey,
    size_bits: u32,
    wrap_bits: u32,
    align: u8,
    max_rows: usize,
    ppp_bits: u32,
}

struct Slot {
    key: Key,
    layout: Arc<WorldLayout>,
    used: u64,
}

#[derive(Default)]
struct WorldCache {
    buckets: HashMap<u64, Vec<Slot>>,
    len: usize,
    clock: u64,
    shapes: u64,
}

thread_local! {
    static WORLD_CACHE: RefCell<WorldCache> = RefCell::new(WorldCache::default());
}

fn align_tag(align: egui::Align) -> u8 {
    match align {
        egui::Align::Center => 1,
        egui::Align::Max => 2,
        egui::Align::Min => 0,
    }
}

/// How many times line breaking has shaped text on this thread.
///
/// A cache hit does not increment it. Painting a cached layout at another
/// zoom goes through [`super::world_text`] or [`super::zoom_galley`] and does
/// not either — those only rasterize the lines already chosen.
pub fn world_layout_shapes() -> u64 {
    WORLD_CACHE.with(|cache| cache.borrow().shapes)
}

/// Drop cached line breaks. The next layout of the same text shapes again.
pub fn clear_world_layout_cache() {
    WORLD_CACHE.with(|cache| *cache.borrow_mut() = WorldCache::default());
    super::raster::clear_raster_caches();
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
    world_layout_rows(ctx, text, font, wrap_width, align, usize::MAX)
}

/// [`world_layout`] limited to `max_rows`, ending in `…` when text remains.
pub fn world_layout_rows(
    ctx: &egui::Context,
    text: &str,
    font: FontId,
    wrap_width: f32,
    align: egui::Align,
    max_rows: usize,
) -> Arc<WorldLayout> {
    let wrap = if wrap_width.is_finite() {
        (wrap_width.max(0.0) * WRAP_STEPS_PER_UNIT).round() / WRAP_STEPS_PER_UNIT
    } else {
        f32::INFINITY
    };
    let size = font.size.max(1.0e-3);
    let key = Key {
        family: family_key(&font.family),
        size_bits: size.to_bits(),
        wrap_bits: wrap.to_bits(),
        align: align_tag(align),
        max_rows: max_rows.max(1),
        ppp_bits: ctx.pixels_per_point().to_bits(),
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    key.hash(&mut hasher);
    let hash = hasher.finish();

    if let Some(hit) = WORLD_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clock = cache.clock.wrapping_add(1);
        let used = cache.clock;
        let slot = cache.buckets.get_mut(&hash).and_then(|slots| {
            slots
                .iter_mut()
                .find(|slot| slot.key == key && &*slot.layout.text == text)
        })?;
        slot.used = used;
        Some(Arc::clone(&slot.layout))
    }) {
        return hit;
    }

    let font = FontId::new(size, font.family);
    let layout = Arc::new(shape_world(ctx, text, font, wrap, align, key.max_rows));
    WORLD_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.shapes += 1;
        cache.clock += 1;
        let used = cache.clock;
        cache.buckets.entry(hash).or_default().push(Slot {
            key,
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
    let oldest = cache
        .buckets
        .iter()
        .flat_map(|(hash, slots)| {
            slots
                .iter()
                .enumerate()
                .map(move |(index, slot)| (*hash, index, slot.used))
        })
        .min_by_key(|(_, _, used)| *used);
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
    wrap: f32,
    align: egui::Align,
    max_rows: usize,
) -> WorldLayout {
    let ppp = ctx.pixels_per_point().max(0.01);
    let logical = REFERENCE_PX / ppp;
    let to_world = font.size / logical;
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = if wrap.is_finite() {
        wrap / to_world
    } else {
        f32::INFINITY
    };
    job.wrap.max_rows = max_rows;
    job.wrap.overflow_character = Some('…');
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
        let left = row.rect.min.x;
        let mut glyph_x = Vec::with_capacity(row.glyphs.len());
        let mut ellipsis = false;
        for glyph in &row.glyphs {
            glyph_x.push((glyph.pos.x - left) * to_world);
            match text[byte..].chars().next() {
                Some(ch) if ch == glyph.chr => byte += ch.len_utf8(),
                _ => ellipsis = true,
            }
        }
        let end = byte;
        if row.ends_with_newline && text[byte..].starts_with('\n') {
            byte += 1;
        }
        let baseline = row
            .glyphs
            .first()
            .map_or(row.rect.max.y, |glyph| glyph.pos.y);
        lines.push(WorldLine {
            bytes: start..end,
            x: left * to_world,
            y: row.rect.min.y * to_world,
            width: row.rect.width() * to_world,
            height: row.rect.height() * to_world,
            baseline: baseline * to_world,
            glyph_x,
            ellipsis,
            ends_with_newline: row.ends_with_newline,
        });
    }
    debug_assert!(
        galley.elided || byte == text.len() || text[byte..].chars().all(|ch| ch == '\n'),
        "line breaks did not cover the source"
    );

    let content_w = lines.iter().map(|line| line.width).fold(0.0, f32::max);
    let box_w = if wrap.is_finite() { wrap } else { content_w };
    for line in &mut lines {
        line.x += match align {
            egui::Align::Center => (box_w - line.width) * 0.5,
            egui::Align::Max => box_w - line.width,
            egui::Align::Min => 0.0,
        };
    }
    WorldLayout {
        lines,
        width: box_w,
        height: galley.rect.height() * to_world,
        text: Arc::from(text),
        font,
        elided: galley.elided,
    }
}
