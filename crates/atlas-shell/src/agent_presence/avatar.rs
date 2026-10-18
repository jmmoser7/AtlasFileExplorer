use super::{cached_mesh, head, rig, AvatarVisual};
use eframe::egui::{self, emath::TSTransform, Color32, Id, Mesh, Painter, Pos2};
use std::{collections::HashMap, sync::Arc};
const SAMPLES: usize = 160;
// 32 active collaborators have two visual slots (board/capsule), with room for
// departures and a density change. Fresh session identities cannot grow forever.
const STATE_LIMIT: usize = 128;
pub(super) fn slot_id(owner: &str, slot: &str) -> String {
    format!("{owner}\u{1f}{slot}")
}
struct State {
    id: String,
    expression: rig::Channel<12>,
    look: rig::Channel<2>,
    last: ([f64; 12], [f64; 2]),
    mesh: Arc<Mesh>,
    last_seen: f64,
}
#[derive(Default)]
pub struct AvatarCache {
    states: HashMap<Id, State>,
    heads: HashMap<(u32, u32), Arc<Mesh>>,
}
impl AvatarCache {
    pub fn retain(&mut self, ids: &[&str]) {
        self.states.retain(|_, state| {
            ids.contains(
                &state
                    .id
                    .split_once('\u{1f}')
                    .map_or(state.id.as_str(), |(owner, _)| owner),
            )
        });
    }
    pub fn paint(
        &mut self,
        painter: &Painter,
        visual: &AvatarVisual<'_>,
        now: f64,
        reduced: bool,
        head_points: f32,
        draw_scale: f32,
    ) -> (bool, [f64; 2]) {
        let rig = rig::interpreted();
        let density = painter.ctx().pixels_per_point();
        let scale = head_points / (rig.head.radius as f32 * 2.0);
        let feather = 1.0 / (density * scale);
        let key = (head_points.to_bits(), density.to_bits());
        let head = self
            .heads
            .entry(key)
            .or_insert_with(|| Arc::new(head::build(feather)))
            .clone();
        let id = Id::new(("agent-presence-avatar", visual.id, key));
        if self.states.len() >= STATE_LIMIT && !self.states.contains_key(&id) {
            if let Some(oldest) = self
                .states
                .iter()
                .min_by(|a, b| a.1.last_seen.total_cmp(&b.1.last_seen))
                .map(|(id, _)| *id)
            {
                self.states.remove(&oldest);
            }
        }
        let expression = rig::shape(visual.expression);
        let look = rig::gaze(visual.orientation);
        let state = self.states.entry(id).or_insert_with(|| State {
            id: visual.id.into(),
            expression: rig::Channel::new(expression),
            look: rig::Channel::new(look),
            last: (expression, look),
            mesh: Arc::new(eye_mesh(rig::eyes(expression, look), feather)),
            last_seen: now,
        });
        state.last_seen = now;
        state
            .expression
            .target(expression, now, if reduced { 0.0 } else { rig.motion[0] });
        state
            .look
            .target(look, now, if reduced { 0.0 } else { rig.motion[1] });
        let values = (state.expression.value, state.look.value);
        if values != state.last {
            update_eyes(
                Arc::make_mut(&mut state.mesh),
                rig::eyes(values.0, values.1),
                feather,
            );
            state.last = values;
        }
        let scale = scale * draw_scale;
        let transform = TSTransform {
            scaling: scale,
            translation: visual.center.to_vec2()
                - egui::vec2(rig.head.cx as f32, rig.head.cy as f32) * scale,
        };
        cached_mesh(painter, id.with("shell"), head, transform, visual.opacity);
        cached_mesh(
            painter,
            id.with("eyes"),
            state.mesh.clone(),
            transform,
            visual.opacity,
        );
        (
            state.expression.active || state.look.active,
            state.look.value,
        )
    }
}
fn smooth_points(eye: rig::Eye) -> [Pos2; SAMPLES] {
    let contour = &rig::interpreted().contour;
    let samples = contour.samples;
    // Sample each authored contour point once. Smooth interpolation and fringe
    // normals then use fixed stack arrays rather than repeatedly reading the rig.
    let raw: [Pos2; SAMPLES] = std::array::from_fn(|n| {
        if n < samples {
            rig::contour(eye, n as f64 / samples as f64)
        } else {
            Pos2::ZERO
        }
    });
    let divisor = contour.spline_divisor as f32;
    std::array::from_fn(|index| {
        let t = index as f64 / SAMPLES as f64;
        let n = (t * samples as f64).floor() as usize;
        let u = (t * samples as f64).fract() as f32;
        let at = |offset: usize| raw[offset % samples];
        let (a, b, c, d) = (at(n + samples - 1), at(n), at(n + 1), at(n + 2));
        let p = b + (c - a) / divisor;
        let q = c - (d - b) / divisor;
        let v = 1.0 - u;
        egui::pos2(
            v * v * v * b.x + 3.0 * v * v * u * p.x + 3.0 * v * u * u * q.x + u * u * u * c.x,
            v * v * v * b.y + 3.0 * v * v * u * p.y + 3.0 * v * u * u * q.y + u * u * u * c.y,
        )
    })
}
fn eye_mesh(eyes: [rig::Eye; 2], feather: f32) -> Mesh {
    let mut mesh = Mesh::default();
    mesh.vertices.resize(
        SAMPLES * 4 + 4,
        egui::epaint::Vertex {
            pos: Pos2::ZERO,
            uv: egui::epaint::WHITE_UV,
            color: Color32::BLACK,
        },
    );
    for eye in 0..2 {
        let base = (eye * SAMPLES * 2) as u32;
        for n in 0..SAMPLES / 2 {
            let (lo, ln, up, un) = (
                n,
                (n + 1) % SAMPLES,
                (SAMPLES - n) % SAMPLES,
                (SAMPLES - n - 1) % SAMPLES,
            );
            mesh.add_triangle(base + lo as u32, base + ln as u32, base + un as u32);
            mesh.add_triangle(base + lo as u32, base + un as u32, base + up as u32);
        }
        for n in 0..SAMPLES {
            let j = (n + 1) % SAMPLES;
            mesh.add_triangle(
                base + n as u32,
                base + j as u32,
                base + (SAMPLES + j) as u32,
            );
            mesh.add_triangle(
                base + n as u32,
                base + (SAMPLES + j) as u32,
                base + (SAMPLES + n) as u32,
            );
        }
    }
    let base = (SAMPLES * 4) as u32;
    for n in 0..2 {
        let j = n + 1;
        mesh.add_triangle(base, base + j, base + j + 1);
    }
    update_eyes(&mut mesh, eyes, feather);
    mesh
}
fn update_eyes(mesh: &mut Mesh, eyes: [rig::Eye; 2], feather: f32) {
    let rig = rig::interpreted();
    let ink = rig.ink;
    for (i, eye) in eyes.into_iter().enumerate() {
        let points = smooth_points(eye);
        for n in 0..SAMPLES {
            let p = points[n];
            let before = points[(n + SAMPLES - 1) % SAMPLES];
            let after = points[(n + 1) % SAMPLES];
            let tangent = (after - before).normalized();
            let normal = egui::vec2(tangent.y, -tangent.x);
            let base = i * SAMPLES * 2 + n;
            mesh.vertices[base].pos = p - normal * feather * 0.5;
            mesh.vertices[base].color = ink;
            mesh.vertices[base + SAMPLES].pos = p + normal * feather * 0.5;
            mesh.vertices[base + SAMPLES].color = Color32::TRANSPARENT;
        }
    }
    let [a, b] = rig::bridge(eyes);
    let dir = (b - a).normalized();
    let normal = egui::vec2(-dir.y, dir.x) * (rig.settings.bridge_width as f32 * 0.5);
    let base = SAMPLES * 4;
    for (n, p) in [a - normal, b - normal, b + normal, a + normal]
        .into_iter()
        .enumerate()
    {
        mesh.vertices[base + n].pos = p;
        mesh.vertices[base + n].color = ink;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retired_session_identities_cannot_grow_the_avatar_cache_without_bound() {
        let ctx = egui::Context::default();
        let mut cache = AvatarCache::default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                ..Default::default()
            },
            |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Middle,
                    Id::new("presence-cache-bound"),
                ));
                for n in 0..STATE_LIMIT + 20 {
                    cache.paint(
                        &painter,
                        &AvatarVisual {
                            id: &format!("fresh-session-{n}"),
                            center: egui::pos2(100.0, 90.0),
                            expression: "neutral",
                            orientation: "forward",
                            hat: "none",
                            prop: "none",
                            opacity: 1.0,
                        },
                        n as f64,
                        false,
                        32.0,
                        1.0,
                    );
                    assert!(cache.states.len() <= STATE_LIMIT);
                }
            },
        );
        assert!(!cache.states.values().any(|s| s.id == "fresh-session-0"));
        assert!(cache
            .states
            .values()
            .any(|s| s.id == format!("fresh-session-{}", STATE_LIMIT + 19)));
        assert_eq!(
            cache.heads.len(),
            1,
            "all identities reuse one shell at this density"
        );
    }
    #[test]
    fn closed_resting_mesh_has_fixed_topology_and_valid_indices() {
        let mut mesh = eye_mesh(rig::eyes(rig::shape("neutral"), [0.0, 0.0]), 7.0);
        let indices = mesh.indices.clone();
        for expression in rig::rig()["expressions"].as_array().unwrap() {
            update_eyes(
                &mut mesh,
                rig::eyes(rig::shape(expression["id"].as_str().unwrap()), [0.0, 0.0]),
                7.0,
            );
            assert_eq!(mesh.indices, indices);
            assert!(mesh.vertices.iter().all(|v| v.pos.is_finite()));
            assert!(mesh
                .indices
                .iter()
                .all(|i| (*i as usize) < mesh.vertices.len()));
            assert!(
                (0..mesh.vertices.len()).all(|index| mesh.indices.contains(&(index as u32))),
                "cached geometry must not retain unused origin vertices"
            );
        }
    }
    #[test]
    fn native_layer_transform_keeps_a_marble_head_and_reuses_meshes_while_moving() {
        let ctx = egui::Context::default();
        let mut cache = AvatarCache::default();
        let mut source = None;
        for center in [egui::pos2(100.0, 90.0), egui::pos2(225.0, 140.0)] {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(400.0, 300.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    let painter = ctx.layer_painter(egui::LayerId::new(
                        egui::Order::Middle,
                        Id::new("presence-test"),
                    ));
                    cache.paint(
                        &painter,
                        &AvatarVisual {
                            id: "Dot",
                            center,
                            expression: "neutral",
                            orientation: "forward",
                            hat: "none",
                            prop: "none",
                            opacity: 1.0,
                        },
                        0.0,
                        false,
                        32.0,
                        1.0,
                    );
                },
            );
            let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
            let mut bounds = egui::Rect::NOTHING;
            for primitive in primitives {
                if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                    for vertex in mesh.vertices.iter().filter(|v| v.color.a() > 0) {
                        bounds.extend_with(vertex.pos);
                    }
                }
            }
            assert!(
                (28.0..=36.0).contains(&bounds.width()),
                "native painted head bounds: {bounds:?}"
            );
            assert!((bounds.center().x - center.x).abs() < 0.1);
            assert_eq!(cache.states.len(), 1);
            assert_eq!(cache.heads.len(), 1);
            let head = cache.heads.values().next().unwrap();
            if let Some(before) = &source {
                assert!(Arc::ptr_eq(before, head));
            } else {
                source = Some(head.clone());
            }
        }
    }
}
