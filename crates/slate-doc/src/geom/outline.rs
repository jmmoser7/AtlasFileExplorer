//! Visible closed-node outlines shared by pick, trim, and bumpers.
use super::*;
use crate::scene::{NodeId, Scene};
use crate::SlateDoc;
use std::path::Path;
use std::{collections::HashMap, sync::Arc};

/// The world region of a closed node: a trim clip when it has one, else the
/// rect (with its corner treatment), ellipse, closed path, or the rotated
/// box of a text, image, frame, portal, or dock strip.
pub fn node_closed_poly(n: &Node, tolerance: f32) -> Option<Polygon> {
    node_closed_poly_with_path(n, tolerance, None)
}

fn node_closed_poly_with_path(n: &Node, tolerance: f32, path: Option<&Path>) -> Option<Polygon> {
    if let Some(clip) = &n.clip {
        let bez = path_data_to_world_bez(clip, n.rect, n.rotation_deg);
        let contours = flatten_contours(&bez, tolerance as f64);
        return (!contours.is_empty()).then_some(contours);
    }
    match &n.kind {
        NodeKind::Shape(s) => {
            if let Some(styled) = crate::vertex_style::closed_form_paint_shape(s, n.rect) {
                let bez = path_data_to_world_bez_with_fillet(
                    styled.path.as_deref()?,
                    n.rect,
                    n.rotation_deg,
                    styled.corner,
                );
                return Some(flatten_contours(&bez, tolerance as f64));
            }
            match s.shape {
                ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::RegularPolygon => {
                    let outline = match s.shape {
                        ShapeKind::Rect => s
                            .corner
                            .outline(n.rect, tolerance)
                            .into_iter()
                            .map(|p| n.rect.rotate_point(p, n.rotation_deg))
                            .collect(),
                        ShapeKind::Ellipse => n
                            .rect
                            .ellipse_outline(tolerance)
                            .into_iter()
                            .map(|p| n.rect.rotate_point(p, n.rotation_deg))
                            .collect(),
                        ShapeKind::RegularPolygon => regular_polygon_world_outline(
                            n.rect,
                            n.rotation_deg,
                            s.sides,
                            s.phase_deg,
                            s.corner,
                            tolerance,
                        ),
                        _ => unreachable!(),
                    };
                    Some(vec![outline])
                }
                ShapeKind::Path => {
                    let path = s.path.as_ref()?;
                    if !path.closed {
                        return None;
                    }
                    let bez =
                        path_data_to_world_bez_with_fillet(path, n.rect, n.rotation_deg, s.corner);
                    let contours = flatten_contours(&bez, tolerance as f64);
                    (!contours.is_empty()).then_some(contours)
                }
                ShapeKind::Line => None,
            }
        }
        NodeKind::Text(_)
        | NodeKind::Image(_)
        | NodeKind::Frame(_)
        | NodeKind::Portal(_)
        | NodeKind::DockStrip(_) => Some(vec![crate::scene::resolved_corner(n, path)
            .outline(n.rect, tolerance)
            .into_iter()
            .map(|p| n.rect.rotate_point(p, n.rotation_deg))
            .collect()]),
        _ => None,
    }
}

/// Distance to the actual closed silhouette, including rotated/rounded hosts
/// and trim holes. The pointer inside the visible region has zero distance.
pub fn node_outline_distance(n: &Node, point: [f32; 2], tolerance: f32) -> f32 {
    ClosedOutline::new(n, tolerance, None).map_or(f32::INFINITY, |outline| outline.distance(point))
}

/// Whether a world point lies inside the host's visible outline (trim clip when set).
pub fn point_in_node_outline(host: &Node, px: f32, py: f32, tolerance: f32) -> bool {
    node_closed_poly(host, tolerance).is_some_and(|poly| point_in_polygon(&poly, [px, py]))
}

/// Positive-area silhouette overlap. Shared edges and shared vertices are
/// adjacency; they must not turn a geometry-distance tie into layer priority.
pub fn node_outlines_overlap(a: &Node, b: &Node, tolerance: f32) -> bool {
    let Some(first) = node_closed_poly(a, tolerance) else {
        return false;
    };
    let Some(second) = node_closed_poly(b, tolerance) else {
        return false;
    };
    positive_intersection(&first, &second, tolerance)
}

#[derive(Debug, Clone)]
struct ClosedOutline {
    region: Polygon,
    boundary: Polygon,
}

impl ClosedOutline {
    fn new(node: &Node, tolerance: f32, path: Option<&Path>) -> Option<Self> {
        let region = node_closed_poly_with_path(node, tolerance, path)?;
        let mut boundary = region.clone();
        for ring in &mut boundary {
            if let Some(first) = ring.first().copied() {
                if ring.last() != Some(&first) {
                    ring.push(first);
                }
            }
        }
        Some(Self { region, boundary })
    }

    fn distance(&self, point: [f32; 2]) -> f32 {
        if point_in_polygon(&self.region, point) {
            return 0.0;
        }
        self.boundary_distance(point)
    }

    fn boundary_distance(&self, point: [f32; 2]) -> f32 {
        vector_ink::closest_polyline_span(&self.boundary, point)
            .map_or(f32::INFINITY, |hit| hit.dist)
    }

    fn contains_strict(&self, point: [f32; 2], tolerance: f32) -> bool {
        point_in_polygon(&self.region, point)
            && self.boundary_distance(point) > (tolerance * 0.01).max(1e-6)
    }
}

fn ring_area(ring: &[[f32; 2]]) -> f64 {
    ring.iter()
        .zip(ring.iter().cycle().skip(1))
        .take(ring.len())
        .map(|(p, q)| f64::from(p[0]) * f64::from(q[1]) - f64::from(q[0]) * f64::from(p[1]))
        .sum::<f64>()
        .abs()
        * 0.5
}

fn region_area(polygon: &Polygon) -> f64 {
    polygon.first().map_or(0.0, |outer| {
        (ring_area(outer)
            - polygon
                .iter()
                .skip(1)
                .map(|ring| ring_area(ring))
                .sum::<f64>())
        .max(0.0)
    })
}

fn area_epsilon(tolerance: f32) -> f64 {
    f64::from(tolerance.max(1e-5)).powi(2) * 0.01
}

fn positive_intersection(a: &Polygon, b: &Polygon, tolerance: f32) -> bool {
    vector_ink::boolean_intersection(a, b)
        .iter()
        .any(|polygon| region_area(polygon) > area_epsilon(tolerance))
}

/// One active flattening bucket, rounded down so it never loses requested detail.
/// A pointer query does work once per generation/bucket, never once per frame.
#[derive(Debug, Clone, Default)]
pub(crate) struct OutlineCache {
    stamp: Option<(u64, usize, i16, Option<(u64, usize)>)>,
    outlines: HashMap<NodeId, Option<Arc<ClosedOutline>>>,
    overlaps: HashMap<(NodeId, NodeId), bool>,
    neighbors: HashMap<NodeId, Arc<[NodeId]>>,
    covering: HashMap<NodeId, Arc<[NodeId]>>,
    #[cfg(test)]
    builds: usize,
    #[cfg(test)]
    intersections: usize,
}

impl OutlineCache {
    fn prepare(&mut self, query: &OutlineQueries<'_>, tolerance: f32) -> f32 {
        let tolerance = if tolerance.is_finite() {
            tolerance.clamp(1e-5, 1e5)
        } else {
            0.25
        };
        // Drop low mantissa bits: four exact, monotone buckets per octave.
        // Unlike log2/powf, re-bucketing this value cannot round into a neighbor.
        let bucket = (tolerance.to_bits() >> 21) as i16;
        let stamp = (
            query.scene.scene_gen(),
            query.scene.nodes.len(),
            bucket,
            query
                .doc
                .map(|doc| (doc.item_paths_revision(), doc.items.len())),
        );
        if self.stamp != Some(stamp) {
            self.outlines.clear();
            self.overlaps.clear();
            self.neighbors.clear();
            self.covering.clear();
            self.stamp = Some(stamp);
        }
        f32::from_bits((bucket as u32) << 21)
    }

    fn outline(
        &mut self,
        node: &Node,
        tolerance: f32,
        doc: Option<&SlateDoc>,
    ) -> Option<Arc<ClosedOutline>> {
        self.outlines
            .entry(node.id)
            .or_insert_with(|| {
                #[cfg(test)]
                {
                    self.builds += 1;
                }
                let path = match &node.kind {
                    NodeKind::Image(image) => doc
                        .and_then(|doc| doc.item(image.item))
                        .map(|item| item.path.as_path()),
                    _ => None,
                };
                ClosedOutline::new(node, tolerance, path).map(Arc::new)
            })
            .clone()
    }
}

/// Read-only geometry queries with the workbook's linked-source defaults.
/// Paths are consulted only when an outline is first built; revisions keep
/// warmed pointer queries independent of the number of linked items.
pub struct OutlineQueries<'a> {
    scene: &'a Scene,
    doc: Option<&'a SlateDoc>,
}

impl SlateDoc {
    pub fn outline_queries(&self) -> OutlineQueries<'_> {
        OutlineQueries {
            scene: &self.scene,
            doc: Some(self),
        }
    }
}

impl Scene {
    fn outline_queries(&self) -> OutlineQueries<'_> {
        OutlineQueries {
            scene: self,
            doc: None,
        }
    }

    /// Pathless scene geometry; workbook interactions use `SlateDoc::outline_queries`.
    pub fn node_outline_distance(&self, id: NodeId, point: [f32; 2], tolerance: f32) -> f32 {
        self.outline_queries()
            .node_outline_distance(id, point, tolerance)
    }
    pub fn node_outline_contains(&self, id: NodeId, point: [f32; 2], tolerance: f32) -> bool {
        self.outline_queries()
            .node_outline_contains(id, point, tolerance)
    }
    pub fn node_outlines_overlap(&self, a: NodeId, b: NodeId, tolerance: f32) -> bool {
        self.outline_queries()
            .node_outlines_overlap(a, b, tolerance)
    }
    pub fn outline_overlaps(&self, id: NodeId, tolerance: f32) -> Arc<[NodeId]> {
        self.outline_queries().outline_overlaps(id, tolerance)
    }
    pub fn outline_covering_nodes(&self, id: NodeId, tolerance: f32) -> Arc<[NodeId]> {
        self.outline_queries().outline_covering_nodes(id, tolerance)
    }
}

impl OutlineQueries<'_> {
    /// Cached distance to the actual rounded/rotated/trimmed silhouette.
    /// Uses the same geometry as [`node_closed_poly`]; live edits invalidate it.
    pub fn node_outline_distance(&self, id: NodeId, point: [f32; 2], tolerance: f32) -> f32 {
        self.cached_outline(id, tolerance)
            .map_or(f32::INFINITY, |(outline, _)| outline.distance(point))
    }

    /// Strict interior of the visible silhouette, excluding outer and hole boundaries.
    pub fn node_outline_contains(&self, id: NodeId, point: [f32; 2], tolerance: f32) -> bool {
        self.cached_outline(id, tolerance)
            .is_some_and(|(outline, tolerance)| outline.contains_strict(point, tolerance))
    }

    fn cached_outline(&self, id: NodeId, tolerance: f32) -> Option<(Arc<ClosedOutline>, f32)> {
        let node = self.scene.node(id)?;
        let mut cache = self.scene.outline_cache.borrow_mut();
        let tolerance = cache.prepare(self, tolerance);
        Some((cache.outline(node, tolerance, self.doc)?, tolerance))
    }

    /// Cached positive-area silhouette overlap; mere edge/vertex contact is false.
    pub fn node_outlines_overlap(&self, a: NodeId, b: NodeId, tolerance: f32) -> bool {
        let (Some(first), Some(second)) = (self.scene.node(a), self.scene.node(b)) else {
            return false;
        };
        let pair = if a <= b { (a, b) } else { (b, a) };
        let mut cache = self.scene.outline_cache.borrow_mut();
        let tolerance = cache.prepare(self, tolerance);
        if let Some(hit) = cache.overlaps.get(&pair) {
            return *hit;
        }
        let hit = match (
            cache.outline(first, tolerance, self.doc),
            cache.outline(second, tolerance, self.doc),
        ) {
            (Some(first), Some(second)) => {
                #[cfg(test)]
                {
                    cache.intersections += 1;
                }
                positive_intersection(&first.region, &second.region, tolerance)
            }
            _ => false,
        };
        cache.overlaps.insert(pair, hit);
        hit
    }

    /// Exact silhouette neighbors in ascending scene order. The existing spatial
    /// index provides the broad phase; repeated pointer queries share this slice.
    pub fn outline_overlaps(&self, id: NodeId, tolerance: f32) -> Arc<[NodeId]> {
        {
            let mut cache = self.scene.outline_cache.borrow_mut();
            cache.prepare(self, tolerance);
            if let Some(neighbors) = cache.neighbors.get(&id) {
                return neighbors.clone();
            }
        }
        let Some(node) = self.scene.node(id) else {
            return Arc::from([]);
        };
        let neighbors: Arc<[NodeId]> = self
            .scene
            .query_rect(crate::spatial::node_aabb(node))
            .into_iter()
            .filter(|other| *other != id && self.node_outlines_overlap(id, *other, tolerance))
            .collect::<Vec<_>>()
            .into();
        self.scene
            .outline_cache
            .borrow_mut()
            .neighbors
            .insert(id, neighbors.clone());
        neighbors
    }

    /// Higher visible bodies whose union completely covers this node's silhouette.
    /// Empty means some real silhouette remains visible. Rounded cutouts, trim
    /// holes and coverage shared between several hosts are preserved by difference.
    pub fn outline_covering_nodes(&self, id: NodeId, tolerance: f32) -> Arc<[NodeId]> {
        {
            let mut cache = self.scene.outline_cache.borrow_mut();
            cache.prepare(self, tolerance);
            if let Some(covering) = cache.covering.get(&id) {
                return covering.clone();
            }
        }
        let covering = self.compute_outline_cover(id, tolerance);
        self.scene
            .outline_cache
            .borrow_mut()
            .covering
            .insert(id, covering.clone());
        covering
    }

    fn compute_outline_cover(&self, id: NodeId, tolerance: f32) -> Arc<[NodeId]> {
        let Some(index) = self.scene.index_of(id) else {
            return Arc::from([]);
        };
        let (mut remainder, tolerance) = {
            let mut cache = self.scene.outline_cache.borrow_mut();
            let tolerance = cache.prepare(self, tolerance);
            let Some(outline) = cache.outline(&self.scene.nodes[index], tolerance, self.doc) else {
                return Arc::from([]);
            };
            // Canonical pieces distinguish extra disjoint islands from holes.
            (
                vector_ink::boolean_union(&outline.region, &outline.region),
                tolerance,
            )
        };
        let mut contributors = Vec::new();
        for candidate in self.outline_overlaps(id, tolerance).iter().copied() {
            let Some(other_index) = self.scene.index_of(candidate) else {
                continue;
            };
            let node = &self.scene.nodes[other_index];
            if other_index <= index
                || node.hidden
                || node.is_frame()
                || matches!(node.kind, NodeKind::Connector(_))
            {
                continue;
            }
            let outline = self
                .scene
                .outline_cache
                .borrow_mut()
                .outline(node, tolerance, self.doc)
                .unwrap();
            let before: f64 = remainder.iter().map(region_area).sum();
            let next: Vec<Polygon> = remainder
                .iter()
                .flat_map(|piece| vector_ink::boolean_difference(piece, &outline.region))
                .collect();
            let after: f64 = next.iter().map(region_area).sum();
            if before - after > area_epsilon(tolerance) {
                contributors.push(candidate);
            }
            remainder = next;
            if after <= area_epsilon(tolerance) {
                return contributors.into();
            }
        }
        Arc::from([])
    }
}

#[cfg(test)]
#[path = "outline_tests.rs"]
mod tests;
