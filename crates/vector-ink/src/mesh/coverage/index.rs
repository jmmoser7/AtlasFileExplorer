//! Private broad phase for the coverage owner's existing AABB predicate.

use super::bounds_overlap;

const LEAF_SIZE: usize = 8;

#[derive(Clone, Copy)]
struct Node {
    bounds: [f64; 4],
    start: usize,
    end: usize,
    left: Option<usize>,
    right: Option<usize>,
}

pub(super) struct CandidateIndex {
    entry_bounds: Vec<[f64; 4]>,
    ids: Vec<usize>,
    nodes: Vec<Node>,
    linear_fallback: bool,
}

impl CandidateIndex {
    pub(super) fn new(bounds: impl IntoIterator<Item = [f64; 4]>) -> Self {
        let entry_bounds: Vec<_> = bounds.into_iter().collect();
        let linear_fallback = entry_bounds.iter().any(|bounds| !valid_bounds(*bounds));
        let mut index = Self {
            ids: (0..entry_bounds.len()).collect(),
            nodes: Vec::new(),
            entry_bounds,
            linear_fallback,
        };
        if !index.linear_fallback && !index.ids.is_empty() {
            let end = index.ids.len();
            index.build_node(0, end);
        }
        index
    }

    /// Append original triangle IDs. The caller sorts them into source order.
    pub(super) fn query(&self, bounds: [f64; 4], output: &mut Vec<usize>) {
        output.clear();
        if self.linear_fallback || !valid_bounds(bounds) {
            output.extend(0..self.entry_bounds.len());
        } else if !self.nodes.is_empty() {
            self.query_node(0, bounds, output);
        }
    }

    fn build_node(&mut self, start: usize, end: usize) -> usize {
        let bounds = self.ids[start..end]
            .iter()
            .map(|&id| self.entry_bounds[id])
            .reduce(union_bounds)
            .expect("BVH nodes are nonempty");
        let node_index = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            start,
            end,
            left: None,
            right: None,
        });
        if end - start > LEAF_SIZE {
            let axis = split_axis(bounds);
            self.ids[start..end].sort_by(|&a, &b| {
                centroid(self.entry_bounds[a], axis)
                    .total_cmp(&centroid(self.entry_bounds[b], axis))
                    .then_with(|| a.cmp(&b))
            });
            let middle = start + (end - start) / 2;
            let left = self.build_node(start, middle);
            let right = self.build_node(middle, end);
            self.nodes[node_index].left = Some(left);
            self.nodes[node_index].right = Some(right);
        }
        node_index
    }

    fn query_node(&self, node_index: usize, query: [f64; 4], output: &mut Vec<usize>) {
        let node = self.nodes[node_index];
        if !bounds_overlap(node.bounds, query) {
            return;
        }
        if let (Some(left), Some(right)) = (node.left, node.right) {
            self.query_node(left, query, output);
            self.query_node(right, query, output);
        } else {
            output.extend(
                self.ids[node.start..node.end]
                    .iter()
                    .copied()
                    .filter(|&id| bounds_overlap(self.entry_bounds[id], query)),
            );
        }
    }
}

fn valid_bounds(bounds: [f64; 4]) -> bool {
    bounds.iter().all(|value| value.is_finite()) && bounds[0] < bounds[2] && bounds[1] < bounds[3]
}

fn union_bounds(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

fn split_axis(bounds: [f64; 4]) -> usize {
    let x_span = bounds[2] - bounds[0];
    let y_span = bounds[3] - bounds[1];
    usize::from(y_span > x_span)
}

fn centroid(bounds: [f64; 4], axis: usize) -> f64 {
    bounds[axis] * 0.5 + bounds[axis + 2] * 0.5
}

#[cfg(test)]
mod tests {
    use super::CandidateIndex;
    use crate::mesh::coverage::bounds_overlap;

    fn parity(bounds: &[[f64; 4]], queries: &[[f64; 4]]) {
        let index = CandidateIndex::new(bounds.iter().copied());
        let mut found = Vec::new();
        for &query in queries {
            index.query(query, &mut found);
            found.sort_unstable();
            let expected: Vec<_> = bounds
                .iter()
                .enumerate()
                .filter_map(|(id, &entry)| bounds_overlap(entry, query).then_some(id))
                .collect();
            let actual: Vec<_> = found
                .iter()
                .copied()
                .filter(|&id| bounds_overlap(bounds[id], query))
                .collect();
            assert_eq!(actual, expected);
        }
    }

    fn assert_linear_fallback(bounds: &[[f64; 4]], query: [f64; 4]) {
        let index = CandidateIndex::new(bounds.iter().copied());
        assert!(index.linear_fallback);
        let mut found = Vec::new();
        index.query(query, &mut found);
        assert_eq!(found, (0..bounds.len()).collect::<Vec<_>>());
        let expected: Vec<_> = bounds
            .iter()
            .enumerate()
            .filter_map(|(id, &entry)| bounds_overlap(entry, query).then_some(id))
            .collect();
        let actual: Vec<_> = found
            .iter()
            .copied()
            .filter(|&id| bounds_overlap(bounds[id], query))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn candidates_match_brute_force_for_touching_thin_negative_and_tied_bounds() {
        let bounds = [
            [-4.0, -2.0, -3.0, -1.0],
            [-3.0, -2.0, -2.0, -1.0],
            [0.0, 0.0, 0.000_001, 5.0],
            [2.0, 2.0, 3.0, 3.0],
            [2.0, 2.0, 3.0, 3.0],
            [8.0, -5.0, 9.0, -4.0],
            [-8.0, 4.0, -7.0, 5.0],
            [12.0, 1.0, 13.0, 2.0],
            [0.0, 10.0, 1.0, 11.0],
        ];
        let queries = [
            [-3.0, -2.0, -2.0, -1.0],
            [-4.0, -2.0, -3.0, -1.0],
            [0.0, 0.0, 0.000_001, 5.0],
            [2.0, 2.0, 3.0, 3.0],
            [-9.0, 3.0, -6.0, 6.0],
        ];
        parity(&bounds, &queries);
        let index = CandidateIndex::new(bounds);
        let mut first = Vec::new();
        let mut second = Vec::new();
        index.query(queries[0], &mut first);
        index.query(queries[0], &mut second);
        assert_eq!(first, second);
    }

    #[test]
    fn seeded_multilevel_candidates_match_brute_force() {
        fn next(seed: &mut u64) -> u32 {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            (*seed >> 16) as u32
        }

        fn coordinate(seed: &mut u64) -> f64 {
            (next(seed) % 4001) as f64 / 16.0 - 125.0
        }

        fn extent(seed: &mut u64) -> f64 {
            (next(seed) % 160) as f64 / 16.0 + 1.0 / 1024.0
        }

        let mut seed = 0x7a91_d3e5_b4c6_2810;
        let bounds: Vec<_> = (0..257)
            .map(|_| {
                let x = coordinate(&mut seed);
                let y = coordinate(&mut seed);
                [x, y, x + extent(&mut seed), y + extent(&mut seed)]
            })
            .collect();
        let queries: Vec<_> = (0..96)
            .map(|_| {
                let x = coordinate(&mut seed);
                let y = coordinate(&mut seed);
                [x, y, x + extent(&mut seed), y + extent(&mut seed)]
            })
            .collect();
        parity(&bounds, &queries);
    }

    #[test]
    fn empty_input_has_no_candidates() {
        let index = CandidateIndex::new(std::iter::empty());
        let mut found = vec![99];
        index.query([0.0, 0.0, 1.0, 1.0], &mut found);
        assert!(found.is_empty());
    }

    #[test]
    fn nonfinite_entry_uses_linear_fallback_and_parity_filter() {
        let bounds = [[0.0, 0.0, 1.0, 1.0], [f64::NAN, 0.0, 2.0, 1.0]];
        assert_linear_fallback(&bounds, [10.0, 10.0, 11.0, 11.0]);
    }

    #[test]
    fn zero_width_and_height_entries_use_linear_fallback() {
        let bounds = [
            [0.0, 0.0, 1.0, 1.0],
            [2.0, 2.0, 2.0, 3.0],
            [4.0, 4.0, 5.0, 4.0],
        ];
        assert_linear_fallback(&bounds, [10.0, 10.0, 11.0, 11.0]);
    }

    #[test]
    fn reversed_entry_uses_linear_fallback() {
        let bounds = [[0.0, 0.0, 1.0, 1.0], [2.0, 2.0, 1.0, 3.0]];
        assert_linear_fallback(&bounds, [10.0, 10.0, 11.0, 11.0]);
    }

    #[test]
    fn invalid_query_uses_linear_fallback_and_parity_filter() {
        let bounds = [[0.0, 0.0, 1.0, 1.0], [2.0, 2.0, 3.0, 3.0]];
        let index = CandidateIndex::new(bounds);
        let mut found = Vec::new();
        index.query([f64::INFINITY, 0.0, 2.0, 1.0], &mut found);
        assert_eq!(found, [0, 1]);
    }
}
