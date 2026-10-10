//! World-space line breaks. A string is shaped once, at a fixed reference
//! raster, for its world font size and world wrap width. The result records
//! every line's bytes and every glyph's position in world units, so a zoom
//! can only scale it: [`super::world_text`] and [`super::zoom_galley`] place
//! glyphs from these numbers and never break or advance text themselves.

use super::lru::Lru;
use eframe::egui::{self, Color32, FontId};
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Physical pixels of the one raster a line break is shaped at.
///
/// High enough that a one-pixel advance round is a small fraction of a word,
/// and independent of the camera. The world font size only scales the wrap
/// width into this raster and the resulting rows back out.
const REFERENCE_PX: f32 = 64.0;

/// World layouts kept before a sweep drops those no recent frame painted. A
/// camera move does not add a key; an edit does.
const WORLD_CACHE_CAP: usize = 256;
/// Most world layouts one frame may hold.
const WORLD_CACHE_HARD_CAP: usize = 16_384;

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

/// What a line break depends on besides the text. Every length shares the
/// font size's unit — world units for board text.
#[derive(Clone, Debug)]
pub struct WorldSpec {
    pub font: FontId,
    /// Wrap width. `f32::INFINITY` keeps each paragraph on one line.
    pub wrap: f32,
    pub align: egui::Align,
    /// Rows kept; a cut row ends in `…`.
    pub max_rows: usize,
    /// Extra advance after every glyph.
    pub tracking: f32,
    /// Break inside a word even where a space would fit.
    pub break_anywhere: bool,
    /// The block is as wide as its widest line, not the wrap width.
    pub hug: bool,
}

impl WorldSpec {
    /// Wrapped, left-aligned text whose block is the wrap width wide.
    pub fn new(font: FontId, wrap: f32) -> Self {
        Self {
            font,
            wrap,
            align: egui::Align::LEFT,
            max_rows: usize::MAX,
            tracking: 0.0,
            break_anywhere: false,
            hug: false,
        }
    }

    /// One line cut with `…` where it would pass `width`, as wide as its text.
    pub fn label(font: FontId, width: f32) -> Self {
        Self {
            max_rows: 1,
            break_anywhere: true,
            hug: true,
            ..Self::new(font, width)
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    family: FamilyKey,
    size_bits: u32,
    wrap_bits: u32,
    tracking_bits: u32,
    align: u8,
    max_rows: usize,
    break_anywhere: bool,
    hug: bool,
    ppp_bits: u32,
}

thread_local! {
    static WORLD_CACHE: RefCell<Lru<(Key, Arc<WorldLayout>)>> =
        RefCell::new(Lru::new(WORLD_CACHE_CAP, WORLD_CACHE_HARD_CAP));
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
    WORLD_CACHE.with(|cache| cache.borrow().builds())
}

/// Drop cached line breaks. The next layout of the same text shapes again.
pub fn clear_world_layout_cache() {
    WORLD_CACHE.with(|cache| {
        *cache.borrow_mut() = Lru::new(WORLD_CACHE_CAP, WORLD_CACHE_HARD_CAP);
    });
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
    let spec = WorldSpec {
        align,
        max_rows,
        ..WorldSpec::new(font, wrap_width)
    };
    world_layout_spec(ctx, text, &spec)
}

/// Break `text` as `spec` says. Zoom is not an input; the same text and spec
/// return the cached breaks without shaping again.
pub fn world_layout_spec(ctx: &egui::Context, text: &str, spec: &WorldSpec) -> Arc<WorldLayout> {
    let wrap = if spec.wrap.is_finite() {
        (spec.wrap.max(0.0) * WRAP_STEPS_PER_UNIT).round() / WRAP_STEPS_PER_UNIT
    } else {
        f32::INFINITY
    };
    let size = spec.font.size.max(1.0e-3);
    let key = Key {
        family: family_key(&spec.font.family),
        size_bits: size.to_bits(),
        wrap_bits: wrap.to_bits(),
        tracking_bits: spec.tracking.to_bits(),
        align: align_tag(spec.align),
        max_rows: spec.max_rows.max(1),
        break_anywhere: spec.break_anywhere,
        hug: spec.hug,
        ppp_bits: ctx.pixels_per_point().to_bits(),
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    key.hash(&mut hasher);
    let hash = hasher.finish();
    let pass = ctx.cumulative_pass_nr();

    if let Some(hit) = WORLD_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let found = cache.get(hash, pass, |(k, layout)| *k == key && &*layout.text == text);
        found.map(|(_, layout)| Arc::clone(layout))
    }) {
        return hit;
    }

    let shaped = WorldSpec {
        font: FontId::new(size, spec.font.family.clone()),
        wrap,
        max_rows: key.max_rows,
        ..spec.clone()
    };
    let layout = Arc::new(shape_world(ctx, text, &shaped));
    WORLD_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .insert(hash, pass, (key, Arc::clone(&layout)));
    });
    layout
}

fn shape_world(ctx: &egui::Context, text: &str, spec: &WorldSpec) -> WorldLayout {
    let ppp = ctx.pixels_per_point().max(0.01);
    let logical = REFERENCE_PX / ppp;
    let to_world = spec.font.size / logical;
    let wrap = spec.wrap;
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = if wrap.is_finite() {
        wrap / to_world
    } else {
        f32::INFINITY
    };
    job.wrap.max_rows = spec.max_rows;
    job.wrap.break_anywhere = spec.break_anywhere;
    job.wrap.overflow_character = Some('…');
    job.halign = egui::Align::LEFT;
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: FontId::new(logical, spec.font.family.clone()),
            extra_letter_spacing: spec.tracking / to_world,
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
    let box_w = if wrap.is_finite() && !spec.hug {
        wrap
    } else {
        content_w
    };
    for line in &mut lines {
        line.x += match spec.align {
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
        font: spec.font.clone(),
        elided: galley.elided,
    }
}
