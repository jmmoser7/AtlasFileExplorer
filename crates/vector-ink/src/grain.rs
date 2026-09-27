//! Paper and pigment fields for textured brush tips.
//!
//! Each field is periodic value noise baked once per process into a table
//! and sampled by world position, like a brush texture pattern in a paint
//! program. Stamping never evaluates noise per dab: the grain is applied once
//! per pixel of the finished stroke ([`crate::stamp::finish_grain`]).

use std::sync::OnceLock;

/// Texels per side. A power of two, so wrapping is a mask.
const SIDE: usize = 512;
/// World units per texel. The pattern repeats every `SIDE * TEXEL` units.
const TEXEL: f32 = 0.5;

/// One baked field, `0..=255` spread over the full range.
pub(crate) struct Field(Vec<u8>);

impl Field {
    /// Bilinear sample at world `(wx, wy)`, in `0..=1`.
    #[inline]
    pub(crate) fn at(&self, wx: f32, wy: f32) -> f32 {
        let u = wx / TEXEL;
        let v = wy / TEXEL;
        let (x0, y0) = (u.floor(), v.floor());
        let (fx, fy) = (u - x0, v - y0);
        let mask = SIDE as i32 - 1;
        let (ix, iy) = (x0 as i32 & mask, y0 as i32 & mask);
        let (jx, jy) = ((ix + 1) & mask, (iy + 1) & mask);
        let row = |y: i32| y as usize * SIDE;
        let t = &self.0;
        let a = t[row(iy) + ix as usize] as f32;
        let b = t[row(iy) + jx as usize] as f32;
        let c = t[row(jy) + ix as usize] as f32;
        let d = t[row(jy) + jx as usize] as f32;
        let top = a + (b - a) * fx;
        let bottom = c + (d - c) * fx;
        (top + (bottom - top) * fy) * (1.0 / 255.0)
    }
}

pub(crate) struct Fields {
    /// Fine paper tooth (1 and 2 unit features): Graphite.
    pub tooth: Field,
    /// Sharper, finer tooth on another seed: Pencil.
    pub pencil: Field,
    /// Fine pigment settling into the paper: Watercolor granulation.
    pub granule: Field,
    /// Broad blooms (8 and 16 unit features): Watercolor density.
    pub bloom: Field,
    /// Edge wander (4 and 8 unit features): Watercolor rim.
    pub wander: Field,
}

/// Bake the fields now (tens of milliseconds, once), so the first textured
/// stroke does not pay for it on the frame loop. Call from a worker.
pub fn warm_grain_fields() {
    fields();
}

pub(crate) fn fields() -> &'static Fields {
    static FIELDS: OnceLock<Fields> = OnceLock::new();
    FIELDS.get_or_init(|| Fields {
        tooth: bake(&[(1.0, 0.7, 11), (2.0, 0.3, 12)]),
        pencil: bake(&[(1.0, 0.8, 21), (4.0, 0.2, 22)]),
        granule: bake(&[(1.0, 0.6, 41), (2.0, 0.4, 42)]),
        bloom: bake(&[(16.0, 0.6, 43), (8.0, 0.4, 44)]),
        wander: bake(&[(8.0, 0.7, 45), (4.0, 0.3, 46)]),
    })
}

/// Sum of periodic value-noise octaves `(feature size in world units,
/// weight, seed)`, stretched so the 1st and 99th percentiles reach 0 and 255.
/// Feature sizes divide the period, so the table tiles without a seam.
fn bake(octaves: &[(f32, f32, u32)]) -> Field {
    let period = SIDE as f32 * TEXEL;
    let mut raw = vec![0.0_f32; SIDE * SIDE];
    for &(size, weight, seed) in octaves {
        let cells = (period / size).round().max(1.0) as i32;
        let per_texel = cells as f32 / SIDE as f32;
        for y in 0..SIDE {
            for x in 0..SIDE {
                raw[y * SIDE + x] +=
                    weight * periodic_noise(x as f32 * per_texel, y as f32 * per_texel, cells, seed);
            }
        }
    }
    let mut sorted = raw.clone();
    let cmp = |a: &f32, b: &f32| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal);
    let n = sorted.len();
    let lo = *sorted.select_nth_unstable_by(n / 100, cmp).1;
    let hi = *sorted.select_nth_unstable_by(n * 99 / 100, cmp).1;
    let span = (hi - lo).max(1.0e-6);
    Field(
        raw.iter()
            .map(|v| (((v - lo) / span).clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect(),
    )
}

fn lattice(ix: i32, iy: i32, seed: u32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(0x8da6_b343)
        ^ (iy as u32).wrapping_mul(0xd816_3841)
        ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_215.0
}

/// Smooth value noise in `0..=1` on a unit lattice that wraps every `cells`.
fn periodic_noise(x: f32, y: f32, cells: i32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (ix, iy) = (x0 as i32, y0 as i32);
    let w = |i: i32| i.rem_euclid(cells);
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let a = lattice(w(ix), w(iy), seed);
    let b = lattice(w(ix + 1), w(iy), seed);
    let c = lattice(w(ix), w(iy + 1), seed);
    let d = lattice(w(ix + 1), w(iy + 1), seed);
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    top + (bottom - top) * sy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_tile_without_a_seam_and_use_the_full_range() {
        let f = fields();
        let period = SIDE as f32 * TEXEL;
        for field in [&f.tooth, &f.pencil, &f.granule, &f.bloom, &f.wander] {
            for i in 0..50 {
                let (x, y) = (i as f32 * 3.7 + 0.3, i as f32 * 1.9 + 0.1);
                assert!((field.at(x, y) - field.at(x + period, y - period)).abs() < 1e-3);
            }
            let lo = field.0.iter().filter(|v| **v < 26).count();
            let hi = field.0.iter().filter(|v| **v > 229).count();
            assert!(lo > field.0.len() / 100 && hi > field.0.len() / 100);
        }
    }
}
