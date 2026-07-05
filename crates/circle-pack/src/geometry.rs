//! Core circle geometry and enclosing-circle utilities.

use std::f32::consts::PI;

/// A circle in 2D with center `(x, y)` and radius `r`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

impl Circle {
    /// Creates a new circle.
    pub fn new(x: f32, y: f32, r: f32) -> Self {
        Self { x, y, r }
    }

    /// Euclidean distance between circle centers.
    pub fn distance(&self, other: &Circle) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Whether this circle overlaps `other` beyond `epsilon` gap.
    pub fn overlaps(&self, other: &Circle, epsilon: f32) -> bool {
        self.distance(other) + epsilon < self.r + other.r
    }

    /// Whether point `(px, py)` lies inside or on the circle boundary.
    pub fn contains_point(&self, px: f32, py: f32) -> bool {
        let dx = px - self.x;
        let dy = py - self.y;
        dx * dx + dy * dy <= self.r * self.r + 1e-6
    }

    /// Whether circle `other` is fully contained in this circle.
    pub fn contains_circle(&self, other: &Circle, epsilon: f32) -> bool {
        self.distance(other) + other.r <= self.r + epsilon
    }
}

/// Intersection (lens) area of two circles with radii `r1`, `r2` and center distance `d`.
///
/// Returns `0` when the circles are disjoint or one contains the other without a proper lens.
pub fn lens_area(r1: f32, r2: f32, d: f32) -> f32 {
    let r1_sq = r1 * r1;
    let r2_sq = r2 * r2;

    if r1 <= 0.0 || r2 <= 0.0 {
        return 0.0;
    }
    if d <= 0.0 {
        return PI * r1_sq.min(r2_sq);
    }

    if d >= r1 + r2 {
        return 0.0;
    }
    if d <= (r1 - r2).abs() {
        return PI * r1_sq.min(r2_sq);
    }

    let alpha = ((d * d + r1_sq - r2_sq) / (2.0 * d * r1)).clamp(-1.0, 1.0);
    let beta = ((d * d + r2_sq - r1_sq) / (2.0 * d * r2)).clamp(-1.0, 1.0);

    let area1 = r1_sq * alpha.acos();
    let area2 = r2_sq * beta.acos();
    let tri = 0.5
        * ((-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2))
            .max(0.0)
            .sqrt();

    (area1 + area2 - tri).max(0.0)
}

/// Distance between circle centers so the lens area equals `target_area`.
pub fn distance_for_lens_area(r1: f32, r2: f32, target_area: f32) -> f32 {
    if target_area <= 0.0 {
        return r1 + r2 + 1.0;
    }
    let max_lens = lens_area(r1, r2, (r1 - r2).abs().max(0.0));
    if target_area >= max_lens * 0.999 {
        return (r1 - r2).abs().max(0.0) + 1e-3;
    }

    let mut lo = (r1 - r2).abs().max(0.0) + 1e-4;
    let mut hi = r1 + r2 - 1e-4;
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if lens_area(r1, r2, mid) < target_area {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Smallest enclosing circle for a slice of circles (iterative Ritter-style expansion).
pub fn enclosing_circle(circles: &[Circle]) -> Circle {
    match circles.len() {
        0 => Circle::new(0.0, 0.0, 0.0),
        1 => circles[0],
        2 => enclose_two(circles[0], circles[1]),
        _ => {
            let mut enc = enclose_two(circles[0], circles[1]);
            for &c in &circles[2..] {
                enc = enclose_two(enc, c);
            }
            for _ in 0..4 {
                for &c in circles {
                    if !enc.contains_circle(&c, 1e-4) {
                        enc = enclose_two(enc, c);
                    }
                }
            }
            enc
        }
    }
}

fn enclose_two(a: Circle, b: Circle) -> Circle {
    if a.contains_circle(&b, 1e-4) {
        return a;
    }
    if b.contains_circle(&a, 1e-4) {
        return b;
    }
    let d = a.distance(&b).max(1e-6);
    let r = 0.5 * (d + a.r + b.r);
    let t = (r - a.r) / d;
    Circle::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y), r)
}

/// Deterministic xorshift64 PRNG for layout jitter.
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lens_area_disjoint() {
        assert_eq!(lens_area(5.0, 5.0, 20.0), 0.0);
    }

    #[test]
    fn lens_area_increases_as_circles_overlap_more() {
        let a = lens_area(10.0, 10.0, 18.0);
        let b = lens_area(10.0, 10.0, 12.0);
        assert!(b > a);
    }

    #[test]
    fn distance_for_lens_area_monotonic() {
        let d0 = distance_for_lens_area(10.0, 10.0, 0.0);
        let d1 = distance_for_lens_area(10.0, 10.0, 50.0);
        assert!(d1 < d0);
        assert!((lens_area(10.0, 10.0, d1) - 50.0).abs() < 2.0);
    }
}
