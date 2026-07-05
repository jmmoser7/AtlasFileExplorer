//! Venn-diagram layout: tag circles and item thumbnail placement.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::geometry::{distance_for_lens_area, Circle, Rng};
use crate::pack::pack_in_circle;

/// One tag set; `weight` drives circle radius (~ member count).
#[derive(Debug, Clone, PartialEq)]
pub struct VennSet {
    pub id: u64,
    pub weight: f32,
}

/// One item belonging to one or more tag sets.
#[derive(Debug, Clone, PartialEq)]
pub struct VennItem {
    pub id: u64,
    pub sets: Vec<u64>,
    pub r: f32,
}

/// Layout output: set circles and item circles keyed by id.
#[derive(Debug, Clone, PartialEq)]
pub struct VennLayout {
    pub set_circles: Vec<(u64, Circle)>,
    pub item_circles: Vec<(u64, Circle)>,
}

const SET_RADIUS_FLOOR: f32 = 12.0;
const SET_RADIUS_SCALE: f32 = 4.0;
const RELAX_ITERS: usize = 60;
const EPSILON: f32 = 1e-3;

/// Compute a Venn layout for the given tag sets and items.
///
/// Set circles are sized from [`VennSet::weight`] and positioned so shared tags
/// overlap proportionally to their shared-item fraction. Items are packed inside
/// their member regions and relaxed to reduce collisions.
///
/// Items with an empty [`VennItem::sets`] vector are omitted from the result.
pub fn venn_layout(sets: &[VennSet], items: &[VennItem]) -> VennLayout {
    let active_items: Vec<&VennItem> = items.iter().filter(|it| !it.sets.is_empty()).collect();
    if sets.is_empty() {
        return VennLayout {
            set_circles: Vec::new(),
            item_circles: Vec::new(),
        };
    }

    let set_radii = compute_set_radii(sets);
    let shared_fractions = compute_shared_fractions(sets, &active_items);
    let set_positions = position_sets(sets, &set_radii, &shared_fractions);

    let mut set_circles: Vec<(u64, Circle)> = sets
        .iter()
        .map(|s| {
            let pos = set_positions[&s.id];
            (s.id, Circle::new(pos.0, pos.1, set_radii[&s.id]))
        })
        .collect();
    set_circles.sort_by_key(|(id, _)| *id);

    let set_circle_map: HashMap<u64, Circle> = set_circles.iter().copied().collect();

    let groups = group_items_by_sets(&active_items);
    let mut item_circles: Vec<(u64, Circle)> = Vec::new();

    for (combo, group_items) in groups {
        let placed = place_item_group(&combo, &group_items, &set_circle_map);
        item_circles.extend(placed);
    }

    item_circles.sort_by_key(|(id, _)| *id);
    relax_items(&mut item_circles, &active_items, &set_circle_map);

    VennLayout {
        set_circles,
        item_circles,
    }
}

fn compute_set_radii(sets: &[VennSet]) -> HashMap<u64, f32> {
    sets.iter()
        .map(|s| {
            let r = SET_RADIUS_FLOOR + SET_RADIUS_SCALE * s.weight.max(0.0).sqrt();
            (s.id, r)
        })
        .collect()
}

fn compute_shared_fractions(sets: &[VennSet], items: &[&VennItem]) -> HashMap<(u64, u64), f32> {
    let mut counts: HashMap<u64, usize> = HashMap::new();
    let mut pair_counts: HashMap<(u64, u64), usize> = HashMap::new();

    for item in items {
        let mut ids: Vec<u64> = item.sets.clone();
        ids.sort_unstable();
        ids.dedup();
        for &id in &ids {
            *counts.entry(id).or_default() += 1;
        }
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let key = (ids[i], ids[j]);
                *pair_counts.entry(key).or_default() += 1;
            }
        }
    }

    let mut fractions = HashMap::new();
    for i in 0..sets.len() {
        for j in (i + 1)..sets.len() {
            let a = sets[i].id;
            let b = sets[j].id;
            let key = (a, b);
            let shared = *pair_counts.get(&key).unwrap_or(&0) as f32;
            let denom = counts
                .get(&a)
                .copied()
                .unwrap_or(0)
                .min(counts.get(&b).copied().unwrap_or(0)) as f32;
            let frac = if denom > 0.0 { shared / denom } else { 0.0 };
            fractions.insert(key, frac);
        }
    }
    fractions
}

fn position_sets(
    sets: &[VennSet],
    radii: &HashMap<u64, f32>,
    shared: &HashMap<(u64, u64), f32>,
) -> HashMap<u64, (f32, f32)> {
    match sets.len() {
        0 => HashMap::new(),
        1 => {
            let id = sets[0].id;
            let mut m = HashMap::new();
            m.insert(id, (0.0, 0.0));
            m
        }
        2 => position_two_sets(sets, radii, shared),
        3 => position_three_sets(sets, radii, shared),
        _ => position_many_sets(sets, radii, shared),
    }
}

fn pair_key(a: u64, b: u64) -> (u64, u64) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

fn target_distance(r1: f32, r2: f32, frac: f32) -> f32 {
    if frac <= 1e-6 {
        return r1 + r2 + 8.0;
    }
    let max_area = std::f32::consts::PI * r1.min(r2).powi(2);
    let target = frac * max_area;
    distance_for_lens_area(r1, r2, target)
}

fn position_two_sets(
    sets: &[VennSet],
    radii: &HashMap<u64, f32>,
    shared: &HashMap<(u64, u64), f32>,
) -> HashMap<u64, (f32, f32)> {
    let a = sets[0].id;
    let b = sets[1].id;
    let r1 = radii[&a];
    let r2 = radii[&b];
    let frac = *shared.get(&pair_key(a, b)).unwrap_or(&0.0);
    let d = target_distance(r1, r2, frac);

    let mut m = HashMap::new();
    m.insert(a, (-0.5 * d, 0.0));
    m.insert(b, (0.5 * d, 0.0));
    m
}

fn position_three_sets(
    sets: &[VennSet],
    radii: &HashMap<u64, f32>,
    shared: &HashMap<(u64, u64), f32>,
) -> HashMap<u64, (f32, f32)> {
    let ids: Vec<u64> = sets.iter().map(|s| s.id).collect();
    let r: Vec<f32> = ids.iter().map(|id| radii[id]).collect();

    let d01 = target_distance(
        r[0],
        r[1],
        *shared.get(&pair_key(ids[0], ids[1])).unwrap_or(&0.0),
    );
    let d02 = target_distance(
        r[0],
        r[2],
        *shared.get(&pair_key(ids[0], ids[2])).unwrap_or(&0.0),
    );
    let d12 = target_distance(
        r[1],
        r[2],
        *shared.get(&pair_key(ids[1], ids[2])).unwrap_or(&0.0),
    );

    let mut positions = HashMap::new();
    positions.insert(ids[0], (-0.5 * d01, 0.0));
    positions.insert(ids[1], (0.5 * d01, 0.0));

    let x2 = (d02 * d02 - d12 * d12 + d01 * d01) / (2.0 * d01);
    let y2_sq = (d02 * d02 - x2 * x2).max(0.0);
    let y2 = y2_sq.sqrt();
    positions.insert(ids[2], (x2 - 0.5 * d01, y2));

    for _ in 0..12 {
        relax_set_positions(&mut positions, &ids, radii, shared);
    }

    positions
}

fn position_many_sets(
    sets: &[VennSet],
    radii: &HashMap<u64, f32>,
    shared: &HashMap<(u64, u64), f32>,
) -> HashMap<u64, (f32, f32)> {
    let ids: Vec<u64> = sets.iter().map(|s| s.id).collect();
    let n = ids.len();
    let mut rng = Rng::new(0xC1EC1EC1E);

    let mut positions: HashMap<u64, (f32, f32)> = HashMap::new();
    let spread = ids.iter().map(|id| radii[id]).sum::<f32>() * 0.5;
    for (i, &id) in ids.iter().enumerate() {
        let angle = 2.0 * std::f32::consts::PI * (i as f32) / (n as f32);
        let jitter = rng.range(-5.0, 5.0);
        positions.insert(
            id,
            (
                angle.cos() * (spread + jitter),
                angle.sin() * (spread + jitter),
            ),
        );
    }

    for _ in 0..80 {
        let mut forces: HashMap<u64, (f32, f32)> = ids.iter().map(|&id| (id, (0.0, 0.0))).collect();

        for i in 0..n {
            for j in (i + 1)..n {
                let a = ids[i];
                let b = ids[j];
                let (ax, ay) = positions[&a];
                let (bx, by) = positions[&b];
                let r1 = radii[&a];
                let r2 = radii[&b];
                let dx = bx - ax;
                let dy = by - ay;
                let d = (dx * dx + dy * dy).sqrt().max(1e-4);
                let ux = dx / d;
                let uy = dy / d;

                let frac = *shared.get(&pair_key(a, b)).unwrap_or(&0.0);
                let target = target_distance(r1, r2, frac);
                let gap = d - target;

                let strength = if frac > 1e-6 { 0.08 } else { 0.15 };
                let fx = strength * gap * ux;
                let fy = strength * gap * uy;

                let fa = forces.get_mut(&a).unwrap();
                fa.0 += fx;
                fa.1 += fy;
                let fb = forces.get_mut(&b).unwrap();
                fb.0 -= fx;
                fb.1 -= fy;

                if frac <= 1e-6 && d < r1 + r2 + 4.0 {
                    let push = 0.2 * (r1 + r2 + 4.0 - d);
                    let fa = forces.get_mut(&a).unwrap();
                    fa.0 -= push * ux;
                    fa.1 -= push * uy;
                    let fb = forces.get_mut(&b).unwrap();
                    fb.0 += push * ux;
                    fb.1 += push * uy;
                }
            }
        }

        for &id in &ids {
            let (fx, fy) = forces[&id];
            let (x, y) = positions.get_mut(&id).unwrap();
            *x += fx;
            *y += fy;
        }
    }

    positions
}

fn relax_set_positions(
    positions: &mut HashMap<u64, (f32, f32)>,
    ids: &[u64],
    radii: &HashMap<u64, f32>,
    shared: &HashMap<(u64, u64), f32>,
) {
    let n = ids.len();
    let mut deltas: HashMap<u64, (f32, f32)> = ids.iter().map(|&id| (id, (0.0, 0.0))).collect();

    for i in 0..n {
        for j in (i + 1)..n {
            let a = ids[i];
            let b = ids[j];
            let (ax, ay) = positions[&a];
            let (bx, by) = positions[&b];
            let r1 = radii[&a];
            let r2 = radii[&b];
            let dx = bx - ax;
            let dy = by - ay;
            let d = (dx * dx + dy * dy).sqrt().max(1e-4);
            let ux = dx / d;
            let uy = dy / d;
            let frac = *shared.get(&pair_key(a, b)).unwrap_or(&0.0);
            let target = target_distance(r1, r2, frac);
            let gap = d - target;
            let k = 0.25;
            let fx = k * gap * ux;
            let fy = k * gap * uy;
            let da = deltas.get_mut(&a).unwrap();
            da.0 += fx;
            da.1 += fy;
            let db = deltas.get_mut(&b).unwrap();
            db.0 -= fx;
            db.1 -= fy;
        }
    }

    for &id in ids {
        let (dx, dy) = deltas[&id];
        let (x, y) = positions.get_mut(&id).unwrap();
        *x += dx;
        *y += dy;
    }
}

fn group_items_by_sets<'a>(items: &[&'a VennItem]) -> BTreeMap<Vec<u64>, Vec<&'a VennItem>> {
    let mut groups: BTreeMap<Vec<u64>, Vec<&VennItem>> = BTreeMap::new();
    for item in items {
        let mut sets = item.sets.clone();
        sets.sort_unstable();
        sets.dedup();
        groups.entry(sets).or_default().push(item);
    }
    groups
}

fn seed_for_combo(combo: &[u64], set_circles: &HashMap<u64, Circle>) -> (f32, f32) {
    match combo.len() {
        0 => (0.0, 0.0),
        1 => {
            let c = set_circles[&combo[0]];
            let (mut sx, mut sy) = (c.x, c.y);
            for (id, other) in set_circles.iter() {
                if *id == combo[0] {
                    continue;
                }
                let dx = sx - other.x;
                let dy = sy - other.y;
                let d = (dx * dx + dy * dy).sqrt().max(1e-4);
                let bias = 0.55 * c.r;
                sx += bias * dx / d;
                sy += bias * dy / d;
            }
            (sx, sy)
        }
        2 => {
            let a = set_circles[&combo[0]];
            let b = set_circles[&combo[1]];
            ((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
        }
        _ => {
            let mut sx = 0.0;
            let mut sy = 0.0;
            for &id in combo {
                let c = set_circles[&id];
                sx += c.x;
                sy += c.y;
            }
            let n = combo.len() as f32;
            (sx / n, sy / n)
        }
    }
}

fn place_item_group(
    combo: &[u64],
    items: &[&VennItem],
    set_circles: &HashMap<u64, Circle>,
) -> Vec<(u64, Circle)> {
    if items.is_empty() {
        return Vec::new();
    }

    let (seed_x, seed_y) = seed_for_combo(combo, set_circles);
    let mut radii: Vec<f32> = items.iter().map(|it| it.r).collect();

    let member_sets: Vec<Circle> = combo.iter().map(|id| set_circles[id]).collect();
    let available_r = max_uniform_radius_in_region(&member_sets, seed_x, seed_y);
    let max_item_r = radii.iter().copied().fold(0.0_f32, f32::max);
    if max_item_r > available_r && available_r > 0.0 {
        let scale = (available_r / max_item_r).min(1.0);
        for r in &mut radii {
            *r *= scale;
        }
    }

    let packing = pack_in_circle(&radii);
    let mut out = Vec::with_capacity(items.len());
    for (item, packed) in items.iter().zip(packing.circles.iter()) {
        out.push((
            item.id,
            Circle::new(seed_x + packed.x, seed_y + packed.y, packed.r),
        ));
    }
    out
}

fn max_uniform_radius_in_region(sets: &[Circle], x: f32, y: f32) -> f32 {
    let mut min_margin = f32::INFINITY;
    for s in sets {
        let d = ((x - s.x).powi(2) + (y - s.y).powi(2)).sqrt();
        min_margin = min_margin.min(s.r - d);
    }
    min_margin.max(0.0)
}

fn relax_items(
    item_circles: &mut [(u64, Circle)],
    items: &[&VennItem],
    set_circles: &HashMap<u64, Circle>,
) {
    let membership: HashMap<u64, BTreeSet<u64>> = items
        .iter()
        .map(|it| {
            let s: BTreeSet<u64> = it.sets.iter().copied().collect();
            (it.id, s)
        })
        .collect();

    let all_set_ids: BTreeSet<u64> = set_circles.keys().copied().collect();

    for _ in 0..RELAX_ITERS {
        // Collision separation among items
        let n = item_circles.len();
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = item_circles[j].1.x - item_circles[i].1.x;
                let dy = item_circles[j].1.y - item_circles[i].1.y;
                let d = (dx * dx + dy * dy).sqrt().max(1e-4);
                let overlap = item_circles[i].1.r + item_circles[j].1.r - d;
                if overlap > 0.0 {
                    let ux = dx / d;
                    let uy = dy / d;
                    let push = 0.5 * overlap;
                    item_circles[i].1.x -= push * ux;
                    item_circles[i].1.y -= push * uy;
                    item_circles[j].1.x += push * ux;
                    item_circles[j].1.y += push * uy;
                }
            }
        }

        // Hard constraint: stay inside member set circles
        for (id, c) in item_circles.iter_mut() {
            let member = membership.get(id).cloned().unwrap_or_default();
            for set_id in &member {
                if let Some(sc) = set_circles.get(set_id) {
                    project_inside(c, sc);
                }
            }
        }

        // Soft constraint: nudge out of non-member sets, then re-clamp inside
        for (id, c) in item_circles.iter_mut() {
            let member = membership.get(id).cloned().unwrap_or_default();
            for set_id in all_set_ids.difference(&member) {
                if let Some(sc) = set_circles.get(set_id) {
                    project_outside(c, sc);
                }
            }
            for set_id in &member {
                if let Some(sc) = set_circles.get(set_id) {
                    project_inside(c, sc);
                }
            }
        }
    }
}

fn project_inside(item: &mut Circle, set: &Circle) {
    let dx = item.x - set.x;
    let dy = item.y - set.y;
    let d = (dx * dx + dy * dy).sqrt();
    let max_d = (set.r - item.r).max(0.0);
    if d > max_d && d > 1e-4 {
        let scale = max_d / d;
        item.x = set.x + dx * scale;
        item.y = set.y + dy * scale;
    }
}

fn project_outside(item: &mut Circle, set: &Circle) {
    let dx = item.x - set.x;
    let dy = item.y - set.y;
    let d = (dx * dx + dy * dy).sqrt();
    let min_d = set.r + item.r + EPSILON;
    if d < min_d {
        if d < 1e-4 {
            item.x = set.x + min_d;
            item.y = set.y;
        } else {
            let scale = min_d / d;
            item.x = set.x + dx * scale;
            item.y = set.y + dy * scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(id: u64, sets: Vec<u64>, r: f32) -> VennItem {
        VennItem { id, sets, r }
    }

    fn set(id: u64, weight: f32) -> VennSet {
        VennSet { id, weight }
    }

    fn circle_for(layout: &VennLayout, id: u64) -> Circle {
        layout
            .item_circles
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, c)| *c)
            .expect("item not found")
    }

    fn set_circle_for(layout: &VennLayout, id: u64) -> Circle {
        layout
            .set_circles
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, c)| *c)
            .expect("set not found")
    }

    #[test]
    fn venn_two_sets_shared_and_exclusive() {
        let sets = [set(1, 10.0), set(2, 10.0)];
        let items = [
            item(10, vec![1, 2], 3.0),
            item(11, vec![1], 3.0),
            item(12, vec![2], 3.0),
        ];
        let layout = venn_layout(&sets, &items);
        assert_eq!(layout.set_circles.len(), 2);
        assert_eq!(layout.item_circles.len(), 3);

        let s1 = set_circle_for(&layout, 1);
        let s2 = set_circle_for(&layout, 2);
        let shared = circle_for(&layout, 10);
        let only1 = circle_for(&layout, 11);
        let only2 = circle_for(&layout, 12);

        assert!(s1.contains_circle(&shared, 0.5));
        assert!(s2.contains_circle(&shared, 0.5));
        assert!(s1.contains_circle(&only1, 0.5));
        assert!(!s2.contains_circle(&only1, 0.5));
        assert!(s2.contains_circle(&only2, 0.5));
        assert!(!s1.contains_circle(&only2, 0.5));
    }

    #[test]
    fn venn_three_sets_center_item() {
        let sets = [set(1, 8.0), set(2, 8.0), set(3, 8.0)];
        let items = [
            item(1, vec![1], 2.5),
            item(2, vec![2], 2.5),
            item(3, vec![3], 2.5),
            item(4, vec![1, 2], 2.5),
            item(5, vec![1, 3], 2.5),
            item(6, vec![2, 3], 2.5),
            item(7, vec![1, 2, 3], 2.5),
        ];
        let layout = venn_layout(&sets, &items);
        let center = circle_for(&layout, 7);
        for id in [1u64, 2, 3] {
            let sc = set_circle_for(&layout, id);
            assert!(
                sc.contains_circle(&center, 1.0),
                "center item should be inside set {id}"
            );
        }
    }

    #[test]
    fn venn_disjoint_sets_separated() {
        let sets = [set(1, 5.0), set(2, 5.0)];
        let items = [item(1, vec![1], 2.0), item(2, vec![2], 2.0)];
        let layout = venn_layout(&sets, &items);
        let s1 = set_circle_for(&layout, 1);
        let s2 = set_circle_for(&layout, 2);
        let gap = s1.distance(&s2) - (s1.r + s2.r);
        assert!(gap > 0.0, "disjoint sets should not overlap, gap={gap}");
    }

    #[test]
    fn venn_skips_empty_sets_item() {
        let sets = [set(1, 5.0)];
        let items = [item(1, vec![], 2.0), item(2, vec![1], 2.0)];
        let layout = venn_layout(&sets, &items);
        assert_eq!(layout.item_circles.len(), 1);
        assert_eq!(layout.item_circles[0].0, 2);
    }

    #[test]
    fn venn_deterministic() {
        let sets = [set(1, 12.0), set(2, 8.0), set(3, 6.0)];
        let items = [
            item(1, vec![1], 2.0),
            item(2, vec![2], 2.0),
            item(3, vec![1, 2], 2.0),
            item(4, vec![1, 2, 3], 2.0),
        ];
        let a = venn_layout(&sets, &items);
        let b = venn_layout(&sets, &items);
        assert_eq!(a, b);
    }
}
