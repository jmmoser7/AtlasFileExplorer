//! Feathered stroke mesh tessellation.

use crate::geom::{
    add, cumulative_arclength, dot, half_width_at, normalize, perp_left, scale, sub, EPS,
    MITER_LIMIT, ROUND_SEGMENTS,
};
use crate::{Cap, InkMesh, InkVertex, Join, StrokeStyle};

#[derive(Clone, Copy)]
struct Station {
    pos: [f32; 2],
    tangent: [f32; 2],
    half: f32,
}

pub(crate) fn tessellate_run(
    mesh: &mut InkMesh,
    points: &[[f32; 2]],
    style: &StrokeStyle,
    feather: f32,
    closed: bool,
    cap_start: Cap,
    cap_end: Cap,
) {
    if points.len() < 2 || style.width <= 0.0 || !feather.is_finite() || feather < 0.0 {
        return;
    }
    let total_len = cumulative_arclength(points);
    let arc_total = *total_len.last().unwrap_or(&0.0);
    if arc_total <= EPS {
        return;
    }

    let mut stations = Vec::new();
    let n = points.len();

    if closed && n >= 3 {
        for i in 0..n {
            let prev = points[(i + n - 1) % n];
            let cur = points[i];
            let next = points[(i + 1) % n];
            let t = total_len[i] / arc_total;
            let half = half_width_at(style, t);
            if half <= EPS {
                continue;
            }
            let t_in = normalize(sub(cur, prev)).unwrap_or([1.0, 0.0]);
            let t_out = normalize(sub(next, cur)).unwrap_or(t_in);
            push_join(&mut stations, cur, t_in, t_out, half, style.join);
        }
    } else {
        let t0 = normalize(sub(points[1], points[0])).unwrap_or([1.0, 0.0]);
        let half0 = half_width_at(style, 0.0);
        push_cap(&mut stations, points[0], t0, half0, cap_start, true);

        if style.taper.is_some() {
            let steps = ((arc_total / 4.0).ceil() as usize).clamp(8, 64);
            for step in 0..=steps {
                let s = arc_total * step as f32 / steps as f32;
                let pos = sample_at_length(points, &total_len, s);
                let t_frac = if arc_total > EPS { s / arc_total } else { 0.0 };
                let half = half_width_at(style, t_frac);
                let tangent = tangent_at_length(points, s);
                if half <= EPS && step == steps {
                    stations.push(Station {
                        pos,
                        tangent,
                        half: 0.0,
                    });
                } else if half > EPS {
                    stations.push(Station { pos, tangent, half });
                }
            }
        } else {
            for i in 0..n {
                let t_frac = total_len[i] / arc_total;
                let half = half_width_at(style, t_frac);
                if half <= EPS {
                    continue;
                }
                if i > 0 && i < n - 1 {
                    let t_in = normalize(sub(points[i], points[i - 1])).unwrap_or([1.0, 0.0]);
                    let t_out = normalize(sub(points[i + 1], points[i])).unwrap_or(t_in);
                    push_join(&mut stations, points[i], t_in, t_out, half, style.join);
                } else {
                    let tangent = if i == 0 {
                        t0
                    } else {
                        normalize(sub(points[n - 1], points[n - 2])).unwrap_or([1.0, 0.0])
                    };
                    stations.push(Station {
                        pos: points[i],
                        tangent,
                        half,
                    });
                }
            }
        }

        let tend = normalize(sub(points[n - 1], points[n - 2])).unwrap_or([1.0, 0.0]);
        let half_end = half_width_at(style, 1.0);
        push_cap(&mut stations, points[n - 1], tend, half_end, cap_end, false);
    }

    if stations.len() < 2 {
        return;
    }

    emit_strip(mesh, &stations, feather);
}

fn push_cap(
    stations: &mut Vec<Station>,
    pos: [f32; 2],
    tangent: [f32; 2],
    half: f32,
    cap: Cap,
    start: bool,
) {
    if half <= EPS {
        return;
    }
    let dir = if start { scale(tangent, -1.0) } else { tangent };
    match cap {
        Cap::Butt => {
            stations.push(Station {
                pos,
                tangent: dir,
                half,
            });
        }
        Cap::Square => {
            let ext = scale(dir, half);
            stations.push(Station {
                pos: add(pos, ext),
                tangent: dir,
                half,
            });
            stations.push(Station {
                pos,
                tangent: dir,
                half,
            });
        }
        Cap::Round => {
            let n = perp_left(dir);
            let center = pos;
            let seg = ROUND_SEGMENTS / 2;
            for i in 0..=seg {
                let t = i as f32 / seg as f32;
                let angle = std::f32::consts::PI * t;
                let c = angle.cos();
                let s = angle.sin();
                let offset = add(scale(n, c * half), scale(dir, s * half));
                let tan = add(scale(n, -s), scale(dir, c));
                stations.push(Station {
                    pos: add(center, offset),
                    tangent: normalize(tan).unwrap_or(dir),
                    half,
                });
            }
        }
    }
}

fn push_join(
    stations: &mut Vec<Station>,
    pos: [f32; 2],
    t_in: [f32; 2],
    t_out: [f32; 2],
    half: f32,
    join: Join,
) {
    let n_in = perp_left(t_in);
    let n_out = perp_left(t_out);

    match join {
        Join::Bevel => {
            stations.push(Station {
                pos,
                tangent: t_out,
                half,
            });
        }
        Join::Miter => {
            let miter = normalize(add(n_in, n_out));
            if let Some(m) = miter {
                let denom = dot(m, n_in).abs().max(EPS);
                let miter_len = half / denom;
                if miter_len <= half * MITER_LIMIT {
                    stations.push(Station {
                        pos,
                        tangent: t_out,
                        half: miter_len,
                    });
                    return;
                }
            }
            stations.push(Station {
                pos,
                tangent: t_out,
                half,
            });
        }
        Join::Round => {
            let a0 = n_in;
            let a1 = n_out;
            for i in 0..=ROUND_SEGMENTS {
                let t = i as f32 / ROUND_SEGMENTS as f32;
                let nx = a0[0] * (1.0 - t) + a1[0] * t;
                let ny = a0[1] * (1.0 - t) + a1[1] * t;
                if normalize([nx, ny]).is_some() {
                    stations.push(Station {
                        pos,
                        tangent: t_out,
                        half,
                    });
                }
            }
        }
    }
}

fn emit_strip(mesh: &mut InkMesh, stations: &[Station], feather: f32) {
    let feather = feather.max(0.0);
    for (i, st) in stations.iter().enumerate() {
        let tan = st.tangent;
        let n = normalize(perp_left(tan)).unwrap_or([0.0, 1.0]);
        let half = st.half;
        let core = (half - feather * 0.5).max(0.0);
        let outer = half + feather * 0.5;

        let base = mesh.vertices.len() as u32;
        let offsets = [(-outer, 0.0f32), (-core, 1.0), (core, 1.0), (outer, 0.0)];
        for (off, alpha) in offsets {
            let p = add(st.pos, scale(n, off));
            mesh.vertices.push(InkVertex { pos: p, alpha });
        }

        if i > 0 {
            let prev = base - 4;
            for (i0, i1, i2) in [
                (0u32, 1, 5),
                (0, 5, 4),
                (1, 2, 6),
                (1, 6, 5),
                (2, 3, 7),
                (2, 7, 6),
            ] {
                mesh.indices
                    .extend_from_slice(&[prev + i0, prev + i1, prev + i2]);
            }
        }
    }
}

fn sample_at_length(points: &[[f32; 2]], _arc: &[f32], mut s: f32) -> [f32; 2] {
    if points.is_empty() {
        return [0.0, 0.0];
    }
    if s <= 0.0 {
        return points[0];
    }
    for w in points.windows(2) {
        let seg = crate::geom::dist(w[0], w[1]);
        if seg <= EPS {
            continue;
        }
        if s <= seg {
            let t = s / seg;
            return [
                w[0][0] + (w[1][0] - w[0][0]) * t,
                w[0][1] + (w[1][1] - w[0][1]) * t,
            ];
        }
        s -= seg;
    }
    *points.last().unwrap()
}

fn tangent_at_length(points: &[[f32; 2]], mut s: f32) -> [f32; 2] {
    if points.len() < 2 {
        return [1.0, 0.0];
    }
    for i in 0..points.len() - 1 {
        let seg = crate::geom::dist(points[i], points[i + 1]);
        if seg <= EPS {
            continue;
        }
        if s <= seg {
            return normalize(sub(points[i + 1], points[i])).unwrap_or([1.0, 0.0]);
        }
        s -= seg;
    }
    normalize(sub(points[points.len() - 1], points[points.len() - 2])).unwrap_or([1.0, 0.0])
}
