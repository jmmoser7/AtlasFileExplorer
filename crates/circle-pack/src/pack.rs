//! Front-chain circle packing (d3-hierarchy / Wang-Chi style).

use crate::geometry::{enclosing_circle, Circle};

/// Result of packing circles of given radii around the origin.
#[derive(Debug, Clone, PartialEq)]
pub struct Packing {
    /// Packed circles in the same order as the input radii.
    pub circles: Vec<Circle>,
    /// Smallest circle enclosing all packed circles.
    pub enclosing: Circle,
}

const INTERSECT_EPS: f32 = 1e-6;

/// Pack `radii` tightly around the origin using a front-chain algorithm.
///
/// Circles are placed tangent to prior circles without overlap. The result is
/// deterministic for a given input order.
pub fn pack_in_circle(radii: &[f32]) -> Packing {
    let n = radii.len();
    if n == 0 {
        return Packing {
            circles: Vec::new(),
            enclosing: Circle::new(0.0, 0.0, 0.0),
        };
    }

    let mut circles: Vec<Circle> = radii.iter().map(|&r| Circle::new(0.0, 0.0, r)).collect();

    if n == 1 {
        let enc = circles[0];
        return Packing {
            circles,
            enclosing: enc,
        };
    }

    circles[0] = Circle::new(-circles[1].r, 0.0, circles[0].r);
    circles[1].x = circles[0].r;

    if n == 2 {
        let enclosing = enclosing_circle(&circles);
        translate_to_origin(&mut circles, &enclosing);
        return Packing {
            circles,
            enclosing: Circle::new(0.0, 0.0, enclosing.r),
        };
    }

    let b0 = circles[1];
    let a0 = circles[0];
    place_tangent(&b0, &a0, &mut circles[2]);

    let mut nodes: Vec<ChainNode> = (0..n).map(ChainNode::new).collect();
    link3(&mut nodes, 0, 1, 2);

    let mut a = 0usize;
    let mut b = 1usize;

    let mut i = 3usize;
    while i < n {
        let mut c = Circle::new(0.0, 0.0, circles[i].r);
        let b_circ = circles[b];
        let a_circ = circles[a];
        place_tangent(&b_circ, &a_circ, &mut c);

        let mut j = nodes[b].next;
        let mut k = nodes[a].prev;
        let mut sj = circles[b].r;
        let mut sk = circles[a].r;
        let mut backtrack = false;

        loop {
            if sj <= sk {
                if intersects(&circles[j], &c) {
                    b = j;
                    nodes[a].next = b;
                    nodes[b].prev = a;
                    backtrack = true;
                    break;
                }
                sj += circles[j].r;
                j = nodes[j].next;
            } else {
                if intersects(&circles[k], &c) {
                    a = k;
                    nodes[a].next = b;
                    nodes[b].prev = a;
                    backtrack = true;
                    break;
                }
                sk += circles[k].r;
                k = nodes[k].prev;
            }
            if j == nodes[k].next {
                break;
            }
        }

        if backtrack {
            continue;
        }

        circles[i] = c;
        insert_between(&mut nodes, a, b, i);
        b = i;

        let mut best_a = a;
        let mut best_score = chain_score(&circles, a, nodes[a].next);
        let mut cur = nodes[a].next;
        while cur != b {
            let next = nodes[cur].next;
            let s = chain_score(&circles, cur, next);
            if s < best_score {
                best_a = cur;
                best_score = s;
            }
            cur = next;
        }
        a = best_a;
        b = nodes[a].next;

        i += 1;
    }

    let enclosing = enclosing_circle(&circles);
    translate_to_origin(&mut circles, &enclosing);
    resolve_overlaps(&mut circles, 48);
    let enclosing = enclosing_circle(&circles);
    translate_to_origin(&mut circles, &enclosing);

    Packing {
        circles,
        enclosing: Circle::new(0.0, 0.0, enclosing.r),
    }
}

fn translate_to_origin(circles: &mut [Circle], enclosing: &Circle) {
    for c in circles.iter_mut() {
        c.x -= enclosing.x;
        c.y -= enclosing.y;
    }
}

/// Place `c` tangent to circles `b` and `a` (d3 `place` order).
fn place_tangent(b: &Circle, a: &Circle, c: &mut Circle) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let d2 = dx * dx + dy * dy;
    if d2 < 1e-12 {
        c.x = a.x + c.r;
        c.y = a.y;
        return;
    }

    let a2 = (a.r + c.r).powi(2);
    let b2 = (b.r + c.r).powi(2);

    if a2 > b2 {
        let x = (d2 + b2 - a2) / (2.0 * d2);
        let y = (b2 / d2 - x * x).max(0.0).sqrt();
        c.x = b.x - x * dx - y * dy;
        c.y = b.y - x * dy + y * dx;
    } else {
        let x = (d2 + a2 - b2) / (2.0 * d2);
        let y = (a2 / d2 - x * x).max(0.0).sqrt();
        c.x = a.x + x * dx - y * dy;
        c.y = a.y + x * dy + y * dx;
    }
}

fn intersects(a: &Circle, b: &Circle) -> bool {
    let dr = a.r + b.r - INTERSECT_EPS;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    dr > 0.0 && dr * dr > dx * dx + dy * dy
}

fn chain_score(circles: &[Circle], a: usize, b: usize) -> f32 {
    let ca = circles[a];
    let cb = circles[b];
    let ab = ca.r + cb.r;
    let dx = (ca.x * cb.r + cb.x * ca.r) / ab;
    let dy = (ca.y * cb.r + cb.y * ca.r) / ab;
    dx * dx + dy * dy
}

#[derive(Clone, Copy)]
struct ChainNode {
    next: usize,
    prev: usize,
}

impl ChainNode {
    fn new(_id: usize) -> Self {
        Self { next: 0, prev: 0 }
    }
}

fn link3(nodes: &mut [ChainNode], a: usize, b: usize, c: usize) {
    nodes[a].next = b;
    nodes[a].prev = c;
    nodes[b].next = c;
    nodes[b].prev = a;
    nodes[c].next = a;
    nodes[c].prev = b;
}

fn insert_between(nodes: &mut [ChainNode], a: usize, old_b: usize, c: usize) {
    nodes[c].prev = a;
    nodes[c].next = old_b;
    nodes[old_b].prev = c;
    nodes[a].next = c;
}

fn resolve_overlaps(circles: &mut [Circle], iterations: usize) {
    for _ in 0..iterations {
        let n = circles.len();
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = circles[j].x - circles[i].x;
                let dy = circles[j].y - circles[i].y;
                let d = (dx * dx + dy * dy).sqrt().max(1e-6);
                let overlap = circles[i].r + circles[j].r - d;
                if overlap > 0.0 {
                    let ux = dx / d;
                    let uy = dy / d;
                    let push = 0.5 * overlap + 1e-4;
                    circles[i].x -= push * ux;
                    circles[i].y -= push * uy;
                    circles[j].x += push * ux;
                    circles[j].y += push * uy;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-2;

    fn assert_no_overlaps(circles: &[Circle]) {
        for i in 0..circles.len() {
            for j in (i + 1)..circles.len() {
                let gap = circles[i].distance(&circles[j]) - (circles[i].r + circles[j].r);
                assert!(gap >= -EPSILON, "circles {i} and {j} overlap by {}", -gap);
            }
        }
    }

    fn assert_inside_enclosing(circles: &[Circle], enclosing: &Circle) {
        for c in circles {
            assert!(
                enclosing.contains_circle(c, EPSILON),
                "circle at ({}, {}) r={} outside enclosing ({}, {}) r={}",
                c.x,
                c.y,
                c.r,
                enclosing.x,
                enclosing.y,
                enclosing.r
            );
        }
    }

    #[test]
    fn pack_empty() {
        let p = pack_in_circle(&[]);
        assert!(p.circles.is_empty());
        assert_eq!(p.enclosing.r, 0.0);
    }

    #[test]
    fn pack_one() {
        let p = pack_in_circle(&[5.0]);
        assert_eq!(p.circles.len(), 1);
        assert!((p.circles[0].r - 5.0).abs() < 1e-5);
        assert!((p.enclosing.r - 5.0).abs() < 0.1);
    }

    #[test]
    fn pack_two() {
        let p = pack_in_circle(&[4.0, 6.0]);
        assert_eq!(p.circles.len(), 2);
        assert_no_overlaps(&p.circles);
        assert_inside_enclosing(&p.circles, &p.enclosing);
    }

    #[test]
    fn pack_seven_mixed() {
        let radii = [3.0, 5.0, 2.0, 7.0, 4.0, 6.0, 1.0];
        let p = pack_in_circle(&radii);
        assert_eq!(p.circles.len(), 7);
        for (c, &r) in p.circles.iter().zip(radii.iter()) {
            assert!((c.r - r).abs() < 1e-5);
        }
        assert_no_overlaps(&p.circles);
        assert_inside_enclosing(&p.circles, &p.enclosing);
    }

    #[test]
    fn pack_fifty() {
        let radii: Vec<f32> = (1..=50).map(|i| (i % 7 + 1) as f32).collect();
        let p = pack_in_circle(&radii);
        assert_eq!(p.circles.len(), 50);
        assert_no_overlaps(&p.circles);
        assert_inside_enclosing(&p.circles, &p.enclosing);
    }

    #[test]
    fn pack_deterministic() {
        let radii = [2.0, 5.0, 3.0, 8.0, 1.0, 4.0];
        let a = pack_in_circle(&radii);
        let b = pack_in_circle(&radii);
        assert_eq!(a.circles, b.circles);
        assert_eq!(a.enclosing, b.enclosing);
    }
}
