//! Ephemeral selection fields (P1.node.hit-regions). No scene or document access.
//! Screen-space policy uses projected host size, independent of world units/zoom.
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};

pub const CORNER_REACH: f32 = 7.0;
pub const EDGE_REACH: f32 = 6.0;
pub const ROTATE_REACH: f32 = 9.0;
pub const ROTATE_OFFSET: f32 = 18.0;
pub const FILLET_REACH: f32 = 9.0;
pub const WIRE_REACH: f32 = 8.0;
pub const WIRE_EXTENSION: f32 = 1.25;
pub const TYPED_PORT_RADIUS: f32 = 6.0;
pub const ORDINARY_PORT_RADIUS: f32 = 3.0;
pub const MINIMUM_REACH: f32 = 2.0;
pub const MOVE_SIZE: f32 = 18.0;
pub const COMPACT_FRAME: f32 = 72.0;
pub const CURSOR_RANGE: f32 = 42.0;
pub const CURSOR_GAIN: f32 = 0.65;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    Wire,
    Resize,
    Rotate,
    Move,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Prominence {
    pub wire: f32,
    pub resize: f32,
    pub rotate: f32,
}

pub fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Domain {
    pub fn prominence(self, shortest: f32) -> f32 {
        if !shortest.is_finite() {
            return 0.0;
        }
        let (low, high) = match self {
            Self::Wire => (36.0, 88.0),
            Self::Resize => (12.0, 48.0),
            Self::Rotate => (3.0, 16.0),
            Self::Move => return 1.0,
        };
        smoothstep(low, high, shortest)
    }

    /// Dormancy uses resting reach, before any cursor expansion. Dormant
    /// controls cannot reappear merely because the cursor crosses their anchor.
    pub fn reach(self, shortest: f32, base: f32) -> Option<f32> {
        let reach = base * self.prominence(shortest);
        (reach >= MINIMUM_REACH).then_some(reach)
    }
}

impl Prominence {
    pub fn new(shortest: f32) -> Self {
        Self {
            wire: Domain::Wire.prominence(shortest),
            resize: Domain::Resize.prominence(shortest),
            rotate: Domain::Rotate.prominence(shortest),
        }
    }
}

/// Transient port markers share the hit domain instead of scaling with an
/// authored node's world units. The host still supplies its actual port site.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortPresentation {
    pub radius: f32,
    pub prominence: f32,
}

impl PortPresentation {
    pub fn typed(shortest: f32) -> Option<Self> {
        Self::new(shortest, TYPED_PORT_RADIUS)
    }

    pub fn ordinary(shortest: f32) -> Option<Self> {
        Self::new(shortest, ORDINARY_PORT_RADIUS)
    }

    fn new(shortest: f32, radius: f32) -> Option<Self> {
        Domain::Wire.reach(shortest, WIRE_REACH)?;
        let prominence = Domain::Wire.prominence(shortest);
        Some(Self {
            radius: radius * prominence,
            prominence,
        })
    }
}

/// Dot, radial clearance and shoulder scale as one ephemeral port control.
/// A fully relevant typed 6px disc has a 2px gap and a 4px shoulder.
pub fn wire_notch_geometry(dot_radius: f32) -> (f32, f32) {
    (dot_radius * (4.0 / 3.0), dot_radius * (2.0 / 3.0))
}

pub fn shortest_side(corners: &[Pos2; 4]) -> f32 {
    (0..4)
        .map(|i| corners[i].distance(corners[(i + 1) % 4]))
        .fold(f32::INFINITY, f32::min)
}

pub fn edge_midpoints(c: &[Pos2; 4]) -> [Pos2; 4] {
    std::array::from_fn(|i| c[i] + (c[(i + 1) % 4] - c[i]) * 0.5)
}

fn unit(v: Vec2) -> Vec2 {
    let length = v.length();
    if length > 1e-6 {
        v / length
    } else {
        Vec2::ZERO
    }
}

/// Blends a compact frame around the true rectangle. Callers keep the authored
/// corners separately: this frame must never become a transform's origin.
pub fn control_corners(actual: &[Pos2; 4]) -> [Pos2; 4] {
    let shortest = shortest_side(actual);
    let pad = ((COMPACT_FRAME - shortest) * 0.5).max(0.0) * Domain::Resize.prominence(shortest);
    let x = unit(actual[1] - actual[0]);
    let y = unit(actual[3] - actual[0]);
    [
        actual[0] - (x + y) * pad,
        actual[1] + (x - y) * pad,
        actual[2] + (x + y) * pad,
        actual[3] + (y - x) * pad,
    ]
}

pub fn rotation_points(corners: &[Pos2; 4], prominence: f32) -> [Pos2; 4] {
    let center = corners[0] + (corners[2] - corners[0]) * 0.5;
    let offset = rotation_radius(prominence);
    corners.map(|corner| corner + unit(corner - center) * offset)
}

pub fn rotation_radius(prominence: f32) -> f32 {
    (ROTATE_OFFSET * prominence).max(12.0)
}

pub fn nearest_on_segment(p: Pos2, a: Pos2, b: Pos2) -> Pos2 {
    let ab = b - a;
    let t = if ab.length_sq() > 1e-8 {
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    a + ab * t
}

pub fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    p.distance(nearest_on_segment(p, a, b))
}

#[derive(Clone, Copy, Debug)]
pub enum Field {
    Disc(Pos2),
    Segment {
        a: Pos2,
        b: Pos2,
    },
    /// A 90-degree outside-corner arc centered on `center`.
    Arc {
        center: Pos2,
        bearing: f32,
        radius: f32,
    },
}

impl Field {
    pub fn nearest(self, point: Pos2) -> Pos2 {
        match self {
            Self::Disc(anchor) => anchor,
            Self::Segment { a, b } => nearest_on_segment(point, a, b),
            Self::Arc {
                center,
                bearing,
                radius,
            } => {
                let angle = (point.y - center.y).atan2(point.x - center.x);
                let delta = (angle - bearing).sin().atan2((angle - bearing).cos());
                let a = bearing
                    + delta.clamp(-std::f32::consts::FRAC_PI_4, std::f32::consts::FRAC_PI_4);
                center + Vec2::angled(a) * radius
            }
        }
    }

    /// Normalized distance inside a continuous field. Cursor proximity grows
    /// the whole field, rather than moving it to a reserved floating point.
    pub fn pick(self, point: Pos2, cursor: Pos2, resting_reach: f32) -> Option<f32> {
        if resting_reach < MINIMUM_REACH {
            return None;
        }
        let reach = cursor_reach(resting_reach, cursor.distance(self.nearest(cursor)));
        let normalized = point.distance(self.nearest(point)) / reach;
        (normalized <= 1.0).then_some(normalized)
    }
}

pub fn cursor_reach(resting_reach: f32, distance: f32) -> f32 {
    resting_reach * (1.0 + CURSOR_GAIN * (1.0 - smoothstep(0.0, CURSOR_RANGE, distance)))
}

/// Conservative exterior bound for layout avoiding a cursor-expanded wire field.
pub fn wire_outer_reach(shortest: f32) -> Option<f32> {
    Domain::Wire
        .reach(shortest, WIRE_REACH)
        .map(|resting| resting * WIRE_EXTENSION + cursor_reach(resting, 0.0))
}

/// Signed distance to the true rectangular host, useful for adjacent-corner
/// ties. Negative values are inside; controls are never the geometry authority.
pub fn body_distance(point: Pos2, corners: &[Pos2; 4]) -> f32 {
    let x_axis = corners[1] - corners[0];
    let y_axis = corners[3] - corners[0];
    let center = corners[0] + (corners[2] - corners[0]) * 0.5;
    let delta = point - center;
    let x = delta.dot(unit(x_axis)).abs() - x_axis.length() * 0.5;
    let y = delta.dot(unit(y_axis)).abs() - y_axis.length() * 0.5;
    Vec2::new(x.max(0.0), y.max(0.0)).length() + x.max(y).min(0.0)
}

/// Hit-only body envelope. Its minimum 18 px does not enlarge authored paint.
pub fn body_contains(point: Pos2, corners: &[Pos2; 4]) -> bool {
    let center = corners[0] + (corners[2] - corners[0]) * 0.5;
    let delta = point - center;
    let x = corners[1] - corners[0];
    let y = corners[3] - corners[0];
    // Degenerate authored rectangles still have an honest move target.
    let ux = if x.length_sq() > 1e-12 {
        unit(x)
    } else {
        Vec2::X
    };
    let uy = if y.length_sq() > 1e-12 {
        unit(y)
    } else {
        Vec2::Y
    };
    delta.dot(ux).abs() <= x.length().max(MOVE_SIZE) * 0.5
        && delta.dot(uy).abs() <= y.length().max(MOVE_SIZE) * 0.5
}

pub fn in_move_core(point: Pos2, corners: &[Pos2; 4]) -> bool {
    let center = corners[0] + (corners[2] - corners[0]) * 0.5;
    let delta = point - center;
    let x = corners[1] - corners[0];
    let y = corners[3] - corners[0];
    delta.dot(unit(x)).abs() < 9.0_f32.min(x.length() / 3.0)
        && delta.dot(unit(y)).abs() < 9.0_f32.min(y.length() / 3.0)
}

/// The named P0.9 exception paints only the ephemeral interaction frame.
/// Authored selection silhouettes remain supplied and painted by their owner.
pub fn paint_control_frame(
    painter: &egui::Painter,
    corners: &[Pos2; 4],
    ink: Color32,
    opacity: f32,
) {
    if opacity > 0.0 {
        painter.add(egui::Shape::closed_line(
            corners.to_vec(),
            Stroke::new(1.0_f32, ink.gamma_multiply(opacity * 0.45)),
        ));
    }
}

/// Label and surface for a buried selection's outside move affordance.
pub fn paint_move_header(painter: &egui::Painter, rect: egui::Rect, ink: Color32) {
    painter.rect(
        rect,
        4.0,
        ink.gamma_multiply(0.12),
        Stroke::new(1.0_f32, ink.gamma_multiply(0.6)),
        egui::StrokeKind::Inside,
    );
    crate::canvas_text::text(
        painter,
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "Move selection",
        egui::FontId::proportional(11.0),
        ink,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: f32, h: f32) -> [Pos2; 4] {
        [
            Pos2::ZERO,
            Pos2::new(w, 0.0),
            Pos2::new(w, h),
            Pos2::new(0.0, h),
        ]
    }

    #[test]
    fn zoom_domains_are_continuous_monotone_and_ordered() {
        let mut previous = Prominence::default();
        for i in 0..=1024 {
            let next = Prominence::new(i as f32 / 8.0);
            assert!(
                next.wire >= previous.wire
                    && next.resize >= previous.resize
                    && next.rotate >= previous.rotate
            );
            assert!(next.wire <= next.resize && next.resize <= next.rotate);
            assert!((next.wire - previous.wire).abs() < 0.02);
            previous = next;
        }
        assert_eq!(Prominence::new(1.0), Prominence::default());
        assert_eq!(
            Prominence::new(128.0),
            Prominence {
                wire: 1.0,
                resize: 1.0,
                rotate: 1.0
            }
        );
    }

    #[test]
    fn fine_controls_stay_dormant_until_the_host_is_large_on_screen() {
        assert_eq!(Domain::Wire.reach(1.0, 8.0), None);
        assert_eq!(Domain::Wire.reach(128.0, 8.0), Some(8.0));
        assert_eq!(Domain::Resize.reach(18.0, CORNER_REACH), None);
        assert!(Domain::Rotate.reach(18.0, ROTATE_REACH).is_some());
        assert_eq!(Domain::Rotate.reach(1.0, ROTATE_REACH), None);
        assert_eq!(
            Field::Disc(Pos2::ZERO).pick(Pos2::ZERO, Pos2::ZERO, 1.99),
            None
        );
    }

    #[test]
    fn cursor_expands_a_continuous_edge_band() {
        let edge = Field::Segment {
            a: Pos2::ZERO,
            b: Pos2::new(200.0, 0.0),
        };
        for x in [18.0, 31.0, 79.0, 133.0, 180.0] {
            let p = Pos2::new(x, 8.5);
            assert!(edge.pick(p, p, EDGE_REACH).is_some());
            assert!(edge.pick(p, p + Vec2::splat(100.0), EDGE_REACH).is_none());
        }
        assert_eq!(cursor_reach(10.0, 0.0), 16.5);
        assert_eq!(cursor_reach(10.0, CURSOR_RANGE), 10.0);
    }

    #[test]
    fn rotation_field_is_an_outside_arc_not_a_corner_point() {
        let arc = Field::Arc {
            center: Pos2::ZERO,
            bearing: -std::f32::consts::FRAC_PI_4,
            radius: 18.0,
        };
        for angle in [-1.5_f32, -0.8, -0.1] {
            let p = Pos2::ZERO + Vec2::angled(angle) * 18.0;
            assert!(arc.pick(p, p, ROTATE_REACH).is_some());
        }
        assert!(arc
            .pick(Pos2::new(20.0, 20.0), Pos2::new(20.0, 20.0), ROTATE_REACH)
            .is_none());
    }

    #[test]
    fn compact_frame_never_changes_the_authored_geometry() {
        let actual = rect(48.0, 36.0);
        let control = control_corners(&actual);
        assert!(control[0].x < actual[0].x);
        let center = control[0] + (control[2] - control[0]) * 0.5;
        assert!(
            center.distance(Pos2::new(24.0, 18.0)) < 1e-5,
            "symmetric floating-point padding must preserve the center: {center:?}"
        );
        assert_eq!(control_corners(&rect(240.0, 112.0)), rect(240.0, 112.0));
        assert!(body_contains(Pos2::new(7.0, 7.0), &rect(1.0, 1.0)));
        assert!(!body_contains(Pos2::new(11.0, 11.0), &rect(1.0, 1.0)));
    }

    #[test]
    fn body_distance_uses_geometry_for_adjacent_hosts() {
        let left = rect(100.0, 100.0);
        let right = left.map(|p| p + Vec2::new(100.0, 0.0));
        let p = Pos2::new(99.0, -3.0);
        assert!(body_distance(p, &left) < body_distance(p, &right));
        assert_eq!(
            body_distance(Pos2::new(100.0, -3.0), &left),
            body_distance(Pos2::new(100.0, -3.0), &right)
        );
        assert!(body_distance(Pos2::new(50.0, 50.0), &left) < 0.0);
    }

    #[test]
    fn port_markers_follow_projected_size_and_remain_bounded_at_extreme_zoom() {
        for (world_shortest, zoom) in [(1.0_f32, 128.0_f32), (640.0, 0.2), (128.0, 1.0)] {
            let typed = PortPresentation::typed(world_shortest * zoom).unwrap();
            let ordinary = PortPresentation::ordinary(world_shortest * zoom).unwrap();
            assert_eq!(typed.radius, 6.0);
            assert_eq!(ordinary.radius, 3.0);
            assert_eq!(wire_notch_geometry(typed.radius), (8.0, 4.0));
        }
        assert!(PortPresentation::typed(18.0).is_none());
        let partial = PortPresentation::typed(60.0).unwrap();
        assert!(partial.radius > 0.0 && partial.radius < 6.0);
        assert!(partial.prominence > 0.0 && partial.prominence < 1.0);
        assert_eq!(partial.radius, 6.0 * partial.prominence);
        assert_eq!(PortPresentation::typed(1_000_000.0).unwrap().radius, 6.0);
    }
}
