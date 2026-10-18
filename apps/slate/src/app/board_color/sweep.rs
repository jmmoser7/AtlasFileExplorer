//! Cached canonical geometry for testing successive eraser path segments.
use super::{board_path, dist_point_segment, line_endpoints};
use eframe::egui::Pos2;
use slate_doc::scene::{Node, NodeKind, ShapeKind, WorldRect};
use vector_ink::CachedStrokeHit;

pub(crate) const MAX_ERASER_SWEEP_WORK: usize = 2_000_000;
pub(crate) const MAX_ERASER_SWEEP_SAMPLES: usize = 8_192;

pub(crate) fn next_eraser_sweep_work(
    current: usize,
    sample_count: usize,
    segment_count: usize,
) -> Option<usize> {
    let next = current.saturating_add(sample_count.saturating_mul(segment_count));
    (next <= MAX_ERASER_SWEEP_WORK).then_some(next)
}

/// Upper bound on sample positions tested by the canonical segment sweep.
pub(crate) fn eraser_sweep_sample_count(points: &[Pos2], radius: f32) -> usize {
    let denominator = (radius * 0.5).max(0.25) as f64;
    points.windows(2).fold(0usize, |total, pair| {
        let steps = ((pair[1] - pair[0]).length() as f64 / denominator).ceil() as usize;
        total.saturating_add(steps.saturating_add(1))
    })
}

enum Geometry {
    Path { hit: CachedStrokeHit },
    Line { a: Pos2, b: Pos2 },
}

/// A path/line eraser query with its hit-test geometry built once. Reusing it
/// across a sweep avoids tessellating the same target once per path segment.
pub(crate) struct EraserSweepQuery {
    rect: WorldRect,
    reach: f32,
    pad: f32,
    radius: f32,
    geometry: Geometry,
}

impl EraserSweepQuery {
    pub(crate) fn prepare(node: &Node, radius: f32, zoom: f32) -> Option<Self> {
        if node.hidden || node.locked {
            return None;
        }
        let NodeKind::Shape(shape) = &node.kind else {
            return None;
        };
        let pad = radius + shape.stroke.width.max(1.0) * 0.5;
        let rect = node.rect.normalized();
        let reach = if node.rotation_deg.abs() > 0.01 {
            pad + rect.w.max(rect.h)
        } else {
            pad
        };
        let geometry = match shape.shape {
            ShapeKind::Path => {
                let path = shape.path.as_ref()?;
                if path.is_empty() || shape.stroke.paints_as_stamp() {
                    return None;
                }
                let bez = board_path::path_data_to_world_bez(path, node.rect, node.rotation_deg);
                let style = board_path::stroke_style_world(&shape.stroke, zoom);
                Geometry::Path {
                    hit: CachedStrokeHit::new(&bez, &style)?,
                }
            }
            ShapeKind::Line => {
                let (a, b) = line_endpoints(node.rect, shape.flip, node.rotation_deg);
                Geometry::Line { a, b }
            }
            _ => return None,
        };
        Some(Self {
            rect,
            reach,
            pad,
            radius,
            geometry,
        })
    }

    /// Test one world-space sweep segment with the board's normal hit math.
    pub(crate) fn hits(&self, from: Pos2, to: Pos2) -> bool {
        let steps = ((to - from).length() / (self.radius * 0.5).max(0.25)).ceil() as usize;
        let mut samples = (0..=steps)
            .map(|i| from.lerp(to, i as f32 / steps.max(1) as f32))
            .filter(|p| self.near(*p))
            .peekable();
        if samples.peek().is_none() {
            return false;
        }
        match &self.geometry {
            Geometry::Path { hit } => samples.any(|p| hit.hit([p.x, p.y], self.radius)),
            Geometry::Line { a, b } => samples.any(|p| dist_point_segment(p, *a, *b) <= self.pad),
        }
    }

    pub(crate) fn segment_count(&self) -> usize {
        match &self.geometry {
            Geometry::Path { hit } => hit.segment_count(),
            Geometry::Line { .. } => 1,
        }
    }

    fn near(&self, p: Pos2) -> bool {
        p.x >= self.rect.x - self.reach
            && p.x <= self.rect.x + self.rect.w + self.reach
            && p.y >= self.rect.y - self.reach
            && p.y <= self.rect.y + self.rect.h + self.reach
    }
}
