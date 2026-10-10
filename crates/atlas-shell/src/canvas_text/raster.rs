//! Painting a [`WorldLayout`] at a zoom.
//!
//! The zoom picks a rung of the raster ladder and a residual scale. Each rung
//! has one galley per layout: the lines are rasterized at the rung size, and
//! every glyph is moved to its reference position from the layout. The block
//! is therefore the same shape at every zoom; only the bitmap behind each
//! glyph changes, a few times per octave. A zoom gesture does not shape text.

use super::world::{family_key, FamilyKey, WorldLayout, WorldLine};
use super::{ladder_font, Scaled};
use eframe::egui::{self, Color32, FontId, Galley, Pos2, Rect};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Most rung galleys kept. A sweep from 0.25 to 4 visits about sixteen rungs.
const RUNG_CACHE_CAP: usize = 512;

/// Most screen galleys [`zoom_galley`] keeps.
pub(super) const ZOOM_CACHE_CAP: usize = 512;

struct RungSlot {
    /// Held so the layout's address cannot be reused while this entry lives.
    layout: Arc<WorldLayout>,
    family: FamilyKey,
    size_bits: u32,
    ppp_bits: u32,
    galley: Arc<Galley>,
    used: u64,
}

struct ZoomSlot {
    layout: Arc<WorldLayout>,
    color: Color32,
    zoom_bits: u32,
    ppp_bits: u32,
    galley: Arc<Galley>,
    used: u64,
}

struct Lru<T> {
    buckets: HashMap<u64, Vec<T>>,
    len: usize,
    clock: u64,
    builds: u64,
}

impl<T> Default for Lru<T> {
    fn default() -> Self {
        Self {
            buckets: HashMap::new(),
            len: 0,
            clock: 0,
            builds: 0,
        }
    }
}

pub(super) struct Caches {
    rung: Lru<RungSlot>,
    pub(super) zoom: ZoomLru,
}

pub(super) struct ZoomLru(Lru<ZoomSlot>);

impl ZoomLru {
    pub(super) fn len(&self) -> usize {
        self.0.len
    }
}

thread_local! {
    pub(super) static CACHES: RefCell<Caches> = RefCell::new(Caches {
        rung: Lru::default(),
        zoom: ZoomLru(Lru::default()),
    });
}

pub(super) fn clear_raster_caches() {
    CACHES.with(|caches| {
        let mut caches = caches.borrow_mut();
        caches.rung = Lru::default();
        caches.zoom = ZoomLru(Lru::default());
    });
}

trait Used {
    fn used(&self) -> u64;
}
impl Used for RungSlot {
    fn used(&self) -> u64 {
        self.used
    }
}
impl Used for ZoomSlot {
    fn used(&self) -> u64 {
        self.used
    }
}

fn insert<T: Used>(lru: &mut Lru<T>, key: u64, slot: T, cap: usize) {
    lru.builds += 1;
    lru.buckets.entry(key).or_default().push(slot);
    lru.len += 1;
    if lru.len <= cap {
        return;
    }
    let oldest = lru
        .buckets
        .iter()
        .flat_map(|(key, slots)| {
            slots
                .iter()
                .enumerate()
                .map(move |(index, slot)| (*key, index, slot.used()))
        })
        .min_by_key(|(_, _, used)| *used);
    if let Some((key, index, _)) = oldest {
        if let Some(slots) = lru.buckets.get_mut(&key) {
            slots.swap_remove(index);
            if slots.is_empty() {
                lru.buckets.remove(&key);
            }
            lru.len -= 1;
        }
    }
}

fn hash_of(value: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// `layout` ready to paint at `zoom`: the cached galley for this zoom's raster
/// rung and the residual scale to the exact size.
///
/// Glyphs keep their reference positions, so the block scales uniformly and
/// its lines never move against each other. Paint it with [`Scaled::paint`]
/// or [`Scaled::selectable`]. Vertices carry no color: the paint call's color
/// applies.
pub fn world_text(ctx: &egui::Context, layout: &Arc<WorldLayout>, zoom: f32) -> Scaled {
    let zoom = if zoom.is_finite() { zoom.max(0.0) } else { 0.0 };
    let screen = FontId::new(layout.font.size * zoom, layout.font.family.clone());
    let (rung, scale) = ladder_font(ctx.pixels_per_point(), screen);
    Scaled::from_galley(rung_galley(ctx, layout, &rung), scale)
}

/// Wrapped canvas text in one call: [`super::world_layout_rows`] at the world
/// font and wrap width, painted at `zoom` through [`world_text`]. For text a
/// caller does not keep a layout for — failure notes, prompt chips.
pub fn world_wrapped(
    ctx: &egui::Context,
    text: &str,
    world_font: FontId,
    world_wrap: f32,
    max_rows: usize,
    zoom: f32,
) -> Scaled {
    let layout =
        super::world_layout_rows(ctx, text, world_font, world_wrap, egui::Align::LEFT, max_rows);
    world_text(ctx, &layout, zoom)
}

/// How many galleys [`world_text`] and [`zoom_galley`] have rasterized on this
/// thread. A zoom inside one ladder rung does not increment it.
pub fn rung_galley_builds() -> u64 {
    CACHES.with(|caches| caches.borrow().rung.builds)
}

fn rung_galley(ctx: &egui::Context, layout: &Arc<WorldLayout>, rung: &FontId) -> Arc<Galley> {
    let family = family_key(&rung.family);
    let size_bits = rung.size.to_bits();
    let ppp_bits = ctx.pixels_per_point().to_bits();
    let key = hash_of((Arc::as_ptr(layout) as usize, size_bits, ppp_bits, &family));
    let hit = CACHES.with(|caches| {
        let mut caches = caches.borrow_mut();
        let lru = &mut caches.rung;
        lru.clock = lru.clock.wrapping_add(1);
        let used = lru.clock;
        let slot = lru.buckets.get_mut(&key).and_then(|slots| {
            slots.iter_mut().find(|slot| {
                Arc::ptr_eq(&slot.layout, layout)
                    && slot.size_bits == size_bits
                    && slot.ppp_bits == ppp_bits
                    && slot.family == family
            })
        })?;
        slot.used = used;
        Some(Arc::clone(&slot.galley))
    });
    if let Some(galley) = hit {
        return galley;
    }
    let galley = Arc::new(build_rung(ctx, layout, rung));
    CACHES.with(|caches| {
        let mut caches = caches.borrow_mut();
        let used = caches.rung.clock;
        let slot = RungSlot {
            layout: Arc::clone(layout),
            family,
            size_bits,
            ppp_bits,
            galley: Arc::clone(&galley),
            used,
        };
        insert(&mut caches.rung, key, slot, RUNG_CACHE_CAP);
    });
    galley
}

/// Rasterize each line at `rung` and move every glyph to the layout's
/// position for it. Units: the rung's points, `rung.size / world size` per
/// world unit.
fn build_rung(ctx: &egui::Context, layout: &WorldLayout, rung: &FontId) -> Galley {
    let r = rung.size / layout.font.size.max(1.0e-3);
    let mut rows = Vec::with_capacity(layout.lines.len());
    let mut mesh_bounds = Rect::NOTHING;
    let (mut num_vertices, mut num_indices) = (0, 0);
    ctx.fonts(|fonts| {
        for line in &layout.lines {
            let mut slice = layout.text.get(line.bytes.clone()).unwrap_or("").to_owned();
            if line.ellipsis {
                slice.push('…');
            }
            if slice.is_empty() {
                rows.push(empty_row(line, r));
                continue;
            }
            let laid = fonts.layout_no_wrap(slice, rung.clone(), Color32::PLACEHOLDER);
            let Some(src) = laid.rows.first() else {
                rows.push(empty_row(line, r));
                continue;
            };
            let row = place_row(line, src, r);
            mesh_bounds = mesh_bounds.union(row.visuals.mesh_bounds);
            num_vertices += row.visuals.mesh.vertices.len();
            num_indices += row.visuals.mesh.indices.len();
            rows.push(row);
        }
    });
    if rows.is_empty() {
        rows.push(egui::epaint::text::Row {
            section_index_at_start: 0,
            glyphs: Vec::new(),
            rect: Rect::from_min_size(Pos2::ZERO, egui::vec2(0.0, rung.size)),
            visuals: Default::default(),
            ends_with_newline: false,
        });
    }
    let job = egui::text::LayoutJob::simple(
        layout.text.to_string(),
        rung.clone(),
        Color32::PLACEHOLDER,
        layout.width * r,
    );
    Galley {
        job: Arc::new(job),
        rows,
        elided: layout.elided,
        rect: Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(layout.width * r, layout.height * r),
        ),
        mesh_bounds,
        num_vertices,
        num_indices,
        pixels_per_point: ctx.pixels_per_point(),
    }
}

/// `src` is one line rasterized at the rung. Its glyphs and their quads move
/// to `line`'s reference positions; advances become the reference gaps, so a
/// caret hit-test lands where the ink is.
fn place_row(
    line: &WorldLine,
    src: &egui::epaint::text::Row,
    r: f32,
) -> egui::epaint::text::Row {
    let mut glyphs = src.glyphs.clone();
    let mut mesh = src.visuals.mesh.clone();
    let exact = glyphs.len() == line.glyph_x.len();
    let baseline = line.baseline * r;
    let right = (line.x + line.width) * r;
    let wanted: Vec<f32> = (0..glyphs.len())
        .map(|i| {
            if exact {
                (line.x + line.glyph_x[i]) * r
            } else {
                line.x * r + (glyphs[i].pos.x - src.rect.min.x)
            }
        })
        .collect();
    let mut vertex = src.visuals.glyph_vertex_range.start;
    for (i, glyph) in glyphs.iter_mut().enumerate() {
        let delta = egui::vec2(wanted[i] - glyph.pos.x, baseline - glyph.pos.y);
        glyph.pos += delta;
        if exact {
            let next = wanted.get(i + 1).copied().unwrap_or(right);
            glyph.advance_width = (next - wanted[i]).max(0.0);
        }
        if !glyph.uv_rect.is_nothing() {
            if let Some(quad) = mesh.vertices.get_mut(vertex..vertex + 4) {
                for v in quad {
                    v.pos += delta;
                }
            }
            vertex += 4;
        }
    }
    let mesh_bounds = if mesh.vertices.is_empty() {
        Rect::NOTHING
    } else {
        mesh.calc_bounds()
    };
    egui::epaint::text::Row {
        section_index_at_start: src.section_index_at_start,
        glyphs,
        rect: Rect::from_min_size(
            egui::pos2(line.x * r, line.y * r),
            egui::vec2(line.width * r, line.height * r),
        ),
        visuals: egui::epaint::text::RowVisuals {
            mesh,
            mesh_bounds,
            glyph_index_start: src.visuals.glyph_index_start,
            glyph_vertex_range: src.visuals.glyph_vertex_range.clone(),
        },
        ends_with_newline: line.ends_with_newline,
    }
}

fn empty_row(line: &WorldLine, r: f32) -> egui::epaint::text::Row {
    egui::epaint::text::Row {
        section_index_at_start: 0,
        glyphs: Vec::new(),
        rect: Rect::from_min_size(
            egui::pos2(line.x * r, line.y * r),
            egui::vec2(0.0, line.height * r),
        ),
        visuals: Default::default(),
        ends_with_newline: line.ends_with_newline,
    }
}

/// A screen-space galley of `layout` at `zoom`, for callers that hand a galley
/// to egui directly (inline editors hit-test the caret against it).
///
/// It is [`world_text`]'s rung galley with the residual scale applied to every
/// position — a copy of cached vertices, never a shaping pass. Vertices are
/// colored `color`. Cached by layout, color, zoom, and pixels per point, so a
/// still camera returns the same `Arc`.
pub fn zoom_galley(
    ctx: &egui::Context,
    layout: &Arc<WorldLayout>,
    color: Color32,
    zoom: f32,
) -> Arc<Galley> {
    let zoom_bits = zoom.to_bits();
    let ppp_bits = ctx.pixels_per_point().to_bits();
    let key = hash_of((
        Arc::as_ptr(layout) as usize,
        zoom_bits,
        ppp_bits,
        color.to_array(),
    ));
    let hit = CACHES.with(|caches| {
        let mut caches = caches.borrow_mut();
        let lru = &mut caches.zoom.0;
        lru.clock = lru.clock.wrapping_add(1);
        let used = lru.clock;
        let slot = lru.buckets.get_mut(&key).and_then(|slots| {
            slots.iter_mut().find(|slot| {
                Arc::ptr_eq(&slot.layout, layout)
                    && slot.zoom_bits == zoom_bits
                    && slot.ppp_bits == ppp_bits
                    && slot.color == color
            })
        })?;
        slot.used = used;
        Some(Arc::clone(&slot.galley))
    });
    if let Some(galley) = hit {
        return galley;
    }
    let scaled = world_text(ctx, layout, zoom);
    let galley = Arc::new(scale_galley(&scaled.galley(), scaled.scale(), color));
    CACHES.with(|caches| {
        let mut caches = caches.borrow_mut();
        let used = caches.zoom.0.clock;
        let slot = ZoomSlot {
            layout: Arc::clone(layout),
            color,
            zoom_bits,
            ppp_bits,
            galley: Arc::clone(&galley),
            used,
        };
        insert(&mut caches.zoom.0, key, slot, ZOOM_CACHE_CAP);
    });
    galley
}

/// How many screen galleys [`zoom_galley`] has built on this thread. A
/// repaint at the same zoom does not increment it.
pub fn zoom_galley_builds() -> u64 {
    CACHES.with(|caches| caches.borrow().zoom.0.builds)
}

fn scale_galley(src: &Galley, s: f32, color: Color32) -> Galley {
    let scale_rect = |r: Rect| {
        if r == Rect::NOTHING {
            r
        } else {
            Rect::from_min_max((r.min.to_vec2() * s).to_pos2(), (r.max.to_vec2() * s).to_pos2())
        }
    };
    let mut galley = src.clone();
    let job = Arc::make_mut(&mut galley.job);
    job.wrap.max_width *= s;
    for section in &mut job.sections {
        section.format.font_id.size *= s;
    }
    for row in &mut galley.rows {
        row.rect = scale_rect(row.rect);
        for glyph in &mut row.glyphs {
            glyph.pos = (glyph.pos.to_vec2() * s).to_pos2();
            glyph.advance_width *= s;
            glyph.line_height *= s;
            glyph.font_ascent *= s;
            glyph.font_height *= s;
            glyph.font_impl_ascent *= s;
            glyph.font_impl_height *= s;
        }
        for vertex in &mut row.visuals.mesh.vertices {
            vertex.pos = (vertex.pos.to_vec2() * s).to_pos2();
            vertex.color = color;
        }
        row.visuals.mesh_bounds = scale_rect(row.visuals.mesh_bounds);
    }
    galley.rect = scale_rect(galley.rect);
    galley.mesh_bounds = scale_rect(galley.mesh_bounds);
    galley
}
