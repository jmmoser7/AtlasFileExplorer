//! The native interpreter of the one approved avatar model; no preset is authored here.
use eframe::egui::{Color32, Pos2};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, sync::OnceLock};
pub fn rig() -> &'static Value {
    static RIG: OnceLock<Value> = OnceLock::new();
    RIG.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/agent-avatar.json"))
            .expect("approved avatar rig")
    })
}
pub fn number(value: &Value, path: &[&str]) -> f64 {
    path.iter()
        .fold(value, |v, key| &v[*key])
        .as_f64()
        .expect("numeric avatar field")
}
pub fn spec(path: &[&str]) -> f64 {
    number(&rig()["geometry"], path)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub eye_radius: f64,
    pub eye_spacing: f64,
    pub bridge_width: f64,
    expression_strength: f64,
    gaze_amount: f64,
}
#[derive(Deserialize)]
pub struct Head {
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Gaze {
    far_compression: f64,
    near_compression: f64,
    separation_compression: f64,
    vertical_compression: f64,
    vertical_offset: f64,
    horizontal_offset: f64,
    diagonal_offset: f64,
    edge_radius: f64,
    edge_minimum_squared: f64,
    min_eye_height: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contour {
    pub samples: usize,
    pub spline_divisor: f64,
    closed_blend_minimum: f64,
    closed_blend_radius: f64,
    cap_max_radius: f64,
    cap_safety: f64,
    bridge_reach: f64,
    bridge_bend_offset: f64,
}
pub struct InterpretedRig {
    pub settings: Settings,
    pub head: Head,
    pub contour: Contour,
    pub motion: [f64; 2],
    pub ink: Color32,
    gaze: Gaze,
    base_radius: f64,
    shapes: HashMap<String, [f64; 12]>,
    orientations: HashMap<String, [f64; 2]>,
}
/// Interpret the canonical numeric data once. Animated contours must not walk
/// JSON maps for every sampled point; the source and formulas stay unchanged.
pub fn interpreted() -> &'static InterpretedRig {
    static CACHE: OnceLock<InterpretedRig> = OnceLock::new();
    CACHE.get_or_init(|| {
        let source = rig();
        InterpretedRig {
            settings: serde_json::from_value(source["presentationDefaults"]["settings"].clone())
                .expect("approved presentation settings"),
            head: serde_json::from_value(source["geometry"]["head"].clone())
                .expect("approved head geometry"),
            contour: serde_json::from_value(source["geometry"]["contour"].clone())
                .expect("approved contour geometry"),
            gaze: serde_json::from_value(source["geometry"]["gaze"].clone())
                .expect("approved gaze geometry"),
            motion: ["expressionDuration", "lookDuration"]
                .map(|key| number(source, &["presentationDefaults", "motion", key]) / 1000.0),
            ink: super::head::color(source["geometry"]["ink"].as_str().unwrap()),
            base_radius: number(source, &["defaults", "eyeRadius"]),
            shapes: source["shapes"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(id, shape)| {
                    (
                        id.clone(),
                        std::array::from_fn(|n| shape[n / 6][n % 6].as_f64().unwrap()),
                    )
                })
                .collect(),
            orientations: source["orientations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| {
                    (
                        item["id"].as_str().unwrap().into(),
                        [item["x"].as_f64().unwrap(), item["y"].as_f64().unwrap()],
                    )
                })
                .collect(),
        }
    })
}
pub fn shape(id: &str) -> [f64; 12] {
    let shapes = &interpreted().shapes;
    *shapes.get(id).unwrap_or_else(|| &shapes["neutral"])
}
pub fn gaze(id: &str) -> [f64; 2] {
    interpreted()
        .orientations
        .get(id)
        .copied()
        .unwrap_or([0.0, 0.0])
}
#[derive(Clone, Copy, Debug)]
pub struct Eye {
    pub cx: f64,
    pub cy: f64,
    pub rx: f64,
    pub ry: f64,
    pub bend: f64,
    pub slope: f64,
    pub upper: f64,
}
pub fn eyes(shape: [f64; 12], gaze: [f64; 2]) -> [Eye; 2] {
    let rig = interpreted();
    let (s, g, head) = (&rig.settings, &rig.gaze, &rig.head);
    let gx = gaze[0] * s.gaze_amount;
    let gy = gaze[1] * s.gaze_amount;
    let strength = s.expression_strength;
    let radius = s.eye_radius;
    let radius_scale = radius / rig.base_radius;
    let base = rig.shapes["neutral"];
    let resting = rig.shapes["resting"];
    std::array::from_fn(|i| {
        let side = if i == 0 { -1.0 } else { 1.0 };
        let mut values: [f64; 6] = std::array::from_fn(|k| {
            base[i * 6 + k] + (shape[i * 6 + k] - base[i * 6 + k]) * strength
        });
        let lid_weight = (-shape[i * 6 + 3] / -resting[i * 6 + 3]).clamp(0.0, 1.0);
        values[1] += (values[1].max(shape[i * 6 + 1]) - values[1]) * lid_weight;
        let far = side * gx > 0.0;
        let width = 1.0
            - gx.abs()
                * if far {
                    g.far_compression
                } else {
                    g.near_compression
                };
        let separation = s.eye_spacing * 0.5 * (1.0 - gx.abs() * g.separation_compression);
        let rx = radius * values[0] * width;
        let ry =
            (radius * values[1] * (1.0 - gy.abs() * g.vertical_compression)).max(g.min_eye_height);
        let cy = head.cy
            + gy * g.vertical_offset
            + values[2] * radius_scale
            + side * gx * gy * g.diagonal_offset;
        let edge = (g.edge_radius.powi(2) - (cy - head.cy).powi(2))
            .max(g.edge_minimum_squared)
            .sqrt()
            - rx;
        Eye {
            cx: head.cx + (side * separation + gx * g.horizontal_offset).clamp(-edge, edge),
            cy,
            rx,
            ry,
            bend: values[3] * radius_scale,
            slope: values[4] * radius_scale,
            upper: values[5] * radius_scale,
        }
    })
}
pub fn contour(eye: Eye, t: f64) -> Pos2 {
    use std::f64::consts::PI;
    let spec = &interpreted().contour;
    let angle = t * PI * 2.0;
    let (x, y) = (angle.cos(), angle.sin());
    let mut point = [
        eye.cx + eye.rx * x,
        eye.cy
            + eye.ry * y
            + eye.bend * (x * x - 1.0)
            + eye.slope * x
            + if y < 0.0 { eye.upper * x * (-y) } else { 0.0 },
    ];
    let weight = (-eye.bend
        / spec
            .closed_blend_minimum
            .max(eye.rx * spec.closed_blend_radius))
    .clamp(0.0, 1.0);
    if weight > 0.0 {
        let depth = (-eye.bend).max(0.0);
        let safe = eye.rx + depth - (depth * depth + 2.0 * eye.rx * depth).sqrt();
        let radius = eye
            .ry
            .min(eye.rx * spec.cap_max_radius)
            .min(safe * spec.cap_safety);
        let span = eye.rx - radius;
        let (u, side, angle) = if (0.125..0.375).contains(&t) {
            (1.0 - (t - 0.125) * 8.0, 1.0, 0.0)
        } else if (0.625..0.875).contains(&t) {
            (-1.0 + (t - 0.625) * 8.0, -1.0, 0.0)
        } else {
            (
                if (0.375..0.625).contains(&t) {
                    -1.0
                } else {
                    1.0
                },
                0.0,
                if t < 0.125 {
                    t * 4.0 * PI
                } else if t < 0.625 {
                    PI / 2.0 + (t - 0.375) * 4.0 * PI
                } else {
                    PI * 1.5 + (t - 0.875) * 4.0 * PI
                },
            )
        };
        let dy = -2.0 * depth * u + eye.slope;
        let length = span.hypot(dy);
        let (tx, ty) = (span / length, dy / length);
        let (cx, cy) = (
            eye.cx + span * u,
            eye.cy + depth * (1.0 - u * u) + eye.slope * u,
        );
        let rounded = if side != 0.0 {
            [cx - radius * ty * side, cy + radius * tx * side]
        } else {
            [
                cx + radius * (tx * angle.cos() - ty * angle.sin()),
                cy + radius * (ty * angle.cos() + tx * angle.sin()),
            ]
        };
        point = std::array::from_fn(|i| point[i] + (rounded[i] - point[i]) * weight);
    }
    Pos2::new(point[0] as f32, point[1] as f32)
}
pub fn bridge(eyes: [Eye; 2]) -> [Pos2; 2] {
    let contour = &interpreted().contour;
    std::array::from_fn(|i| {
        let eye = eyes[i];
        let side = if i == 0 { 1.0 } else { -1.0 };
        let reach = contour.bridge_reach;
        Pos2::new(
            (eye.cx + eye.rx * side * reach) as f32,
            (eye.cy - eye.bend * contour.bridge_bend_offset + eye.slope * side * reach) as f32,
        )
    })
}

#[derive(Clone)]
pub struct Channel<const N: usize> {
    pub value: [f64; N],
    from: [f64; N],
    target: [f64; N],
    start: f64,
    duration: f64,
    pub active: bool,
}
impl<const N: usize> Channel<N> {
    pub fn new(value: [f64; N]) -> Self {
        Self {
            value,
            from: value,
            target: value,
            start: 0.0,
            duration: 0.0,
            active: false,
        }
    }
    pub fn sample(&mut self, now: f64) {
        if !self.active {
            return;
        }
        let t = ((now - self.start) / self.duration).clamp(0.0, 1.0);
        let ease = t * t * (3.0 - 2.0 * t);
        self.value = std::array::from_fn(|n| self.from[n] + (self.target[n] - self.from[n]) * ease);
        if t >= 1.0 {
            self.active = false;
        }
    }
    pub fn target(&mut self, value: [f64; N], now: f64, duration: f64) {
        self.sample(now);
        if value == self.target && duration > 0.0 {
            return;
        }
        self.target = value;
        self.from = self.value;
        self.start = now;
        self.duration = duration;
        if duration <= 0.0 {
            self.value = value;
            self.active = false;
        } else {
            self.active = self.value != value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_geometry_conforms_to_all_81_approved_browser_poses() {
        // Generated by running the approved JS interpreter with the shared
        // presentation defaults, then reading its painted SVG and geometry.
        let records: Value = serde_json::from_str(include_str!("avatar_conformance.json")).unwrap();
        assert_eq!(records.as_array().unwrap().len(), 81);
        for record in records.as_array().unwrap() {
            let eyes = eyes(
                shape(record["expression"].as_str().unwrap()),
                gaze(record["orientation"].as_str().unwrap()),
            );
            for (i, eye) in eyes.into_iter().enumerate() {
                for (field, value) in [
                    ("cx", eye.cx),
                    ("cy", eye.cy),
                    ("rx", eye.rx),
                    ("ry", eye.ry),
                    ("bend", eye.bend),
                    ("slope", eye.slope),
                    ("upper", eye.upper),
                ] {
                    let expected = record["eyes"][i][field].as_f64().unwrap();
                    assert!(
                        (value - expected).abs() < 1e-9,
                        "{} / {} / {field}",
                        record["expression"],
                        record["orientation"]
                    );
                }
                for n in 0..40 {
                    let p = contour(eye, n as f64 / 40.0);
                    let expected = &record["contours"][i][n];
                    assert!((p.x as f64 - expected[0].as_f64().unwrap()).abs() < 0.001);
                    assert!((p.y as f64 - expected[1].as_f64().unwrap()).abs() < 0.001);
                }
            }
            for (i, p) in bridge(eyes).into_iter().enumerate() {
                let x = record["bridge"][if i == 0 { "x1" } else { "x2" }]
                    .as_str()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap();
                let y = record["bridge"][if i == 0 { "y1" } else { "y2" }]
                    .as_str()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap();
                assert!((p.x as f64 - x).abs() < 0.001);
                assert!((p.y as f64 - y).abs() < 0.001);
            }
        }
    }
    #[test]
    fn presentation_not_raw_defaults() {
        let cached = interpreted();
        assert_eq!(cached.settings.eye_radius, 16.5);
        assert_eq!(cached.settings.expression_strength, 1.35);
        assert_eq!(cached.motion, [0.18, 0.475]);
        assert_eq!(shape("missing-expression"), shape("neutral"));
        assert_eq!(gaze("missing-direction"), [0.0, 0.0]);
    }
    #[test]
    fn every_approved_expression_direction_is_finite_and_has_continuous_contours() {
        for expression in rig()["expressions"].as_array().unwrap() {
            for orientation in rig()["orientations"].as_array().unwrap() {
                let eyes = eyes(
                    shape(expression["id"].as_str().unwrap()),
                    gaze(orientation["id"].as_str().unwrap()),
                );
                for eye in eyes {
                    for n in 0..160 {
                        let p = contour(eye, n as f64 / 160.0);
                        assert!(p.is_finite());
                        assert!((p - contour(eye, (n + 1) as f64 / 160.0 % 1.0)).length() < 8.0);
                    }
                }
                assert!(bridge(eyes).iter().all(|p| p.is_finite()));
            }
        }
    }
    #[test]
    fn tracks_remain_independent_and_interrupt_at_the_sampled_value() {
        let mut expression = Channel::new([0.0]);
        let mut look = Channel::new([0.0]);
        expression.target([1.0], 0.0, 0.18);
        look.target([1.0], 0.0, 0.475);
        expression.sample(0.18);
        look.sample(0.18);
        assert!(!expression.active);
        assert!(look.active);
        let prior = look.value;
        expression.target([0.0], 0.18, 0.18);
        assert_eq!(look.value, prior);
        assert_eq!(look.start, 0.0);
        look.target([-1.0], 0.18, 0.475);
        assert_eq!(look.value, prior);
        assert_eq!(look.from, prior);
        look.target([-1.0], 0.2, 0.0);
        assert_eq!(look.value, [-1.0]);
        assert!(!look.active);
    }
}
