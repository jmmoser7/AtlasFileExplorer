//! Tangential round closures for constant and variable-width rails.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn push_cap(
    out: &mut Vec<Station>,
    pos: [f32; 2],
    tangent: [f32; 2],
    half: f32,
    slope: f32,
    cap: Cap,
    start: bool,
    tolerance: f64,
    feather: f32,
) {
    let normal = perp_left(tangent);
    match cap {
        Cap::Butt => out.push(section(pos, normal, half)),
        Cap::Square => {
            let extended = section(
                add(pos, scale(tangent, if start { -half } else { half })),
                normal,
                half,
            );
            let end = section(pos, normal, half);
            if start {
                out.extend([extended, end]);
            } else {
                out.extend([end, extended]);
            }
        }
        Cap::Round => {
            // A circle tangent to y = half + slope*x is centred at
            // x = half*slope and has radius half*sqrt(1+slope²). Keeping
            // the contact at the endpoint preserves the authored width.
            let slope = slope.clamp(-8.0, 8.0);
            let radius = half * (1.0 + slope * slope).sqrt();
            let center = add(pos, scale(tangent, half * slope));
            let contact = std::f32::consts::FRAC_PI_2 + slope.atan();
            let angle_span = if start {
                std::f32::consts::PI - contact
            } else {
                contact
            };
            let count = arc_steps(radius + feather * 0.5, angle_span, tolerance);
            for i in 0..=count {
                // Longitudinal sections travel from tip to end (start cap), or
                // end to tip (end cap). No rotating diameters/bow-tie triangles.
                let fraction = i as f32 / count as f32;
                let angle = if start {
                    std::f32::consts::PI - angle_span * fraction
                } else {
                    contact * (1.0 - fraction)
                };
                let (sin, cos) = angle.sin_cos();
                let axial = scale(tangent, cos);
                let edges = [-1.0, 1.0].map(|sign| {
                    let outward = add(axial, scale(normal, sign * sin));
                    Edge {
                        pos: add(center, scale(outward, radius)),
                        normal: outward,
                    }
                });
                out.push(Station { edges, half, at: 0 });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thick_taper_cap_is_tangent_to_the_straight_rails() {
        for start in [false, true] {
            let mut out = Vec::new();
            push_cap(
                &mut out,
                [0.0, 0.0],
                [1.0, 0.0],
                100.0,
                0.5,
                Cap::Round,
                start,
                0.001,
                0.0,
            );
            let contact = if start { out.last().unwrap() } else { &out[0] };
            let neighbor = if start { &out[out.len() - 2] } else { &out[1] };
            let p = contact.edges[1].pos;
            assert!(p[0].abs() < 0.001 && (p[1] - 100.0).abs() < 0.001);
            let tangent = sub(neighbor.edges[1].pos, p);
            assert!((tangent[1] / tangent[0] - 0.5).abs() < 0.01);
        }
    }
}
