//! The second interpreter of the approved SVG props/hats and motion data.
//! XML/path parsing and vector-ink tessellation happen once per display density.
use super::{
    art_geometry::{geometry, inherited, parse_transform, path},
    cached_mesh, AvatarVisual,
};
use eframe::egui::{self, emath::TSTransform, Id, Mesh, Painter};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, OnceLock},
};
use vector_ink::kurbo::{Affine, Point};

#[derive(Deserialize)]
pub struct Hat {
    pub label: String,
    pub svg: String,
    #[serde(default)]
    pub fit: Option<HatFit>,
}
#[derive(Clone, Copy, Deserialize)]
pub struct HatFit {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
}
#[derive(Deserialize)]
struct HatCatalog {
    order: Vec<String>,
    hats: BTreeMap<String, Hat>,
    motion: HatMotion,
}
#[derive(Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HatMotion {
    horizontal_offset: f64,
    vertical_offset: f64,
    yaw_compression: f64,
    pitch_compression: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Departure {
    pub id: String,
    pub label: String,
    pub duration: f64,
    #[serde(default)]
    pub drop: f32,
    #[serde(default)]
    pub bow_until: f32,
    #[serde(default)]
    pub shrink: f32,
    #[serde(default)]
    pub shrink_start: f32,
    #[serde(default)]
    pub fade_start: f32,
    #[serde(default)]
    pub bob: f32,
    #[serde(default)]
    pub bob_turns: f32,
    #[serde(default)]
    pub effect_start: f32,
    #[serde(default)]
    pub effect_end: f32,
    #[serde(default)]
    pub effect_points: f32,
    #[serde(default)]
    pub effect_scale_from: f32,
    #[serde(default)]
    pub effect_scale_travel: f32,
    #[serde(default)]
    pub effect_opacity: f32,
    #[serde(default)]
    pub target_inset: [f32; 2],
    #[serde(default)]
    pub effect_inset: [f32; 2],
    #[serde(default)]
    pub corner_inset: [f32; 2],
    #[serde(default)]
    pub edge_margin: f32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    pub view_box: [f32; 4],
    pub svg: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    fill: String,
    stroke: String,
    stroke_width: f32,
}
#[derive(Clone, Copy, Deserialize)]
pub struct Frame {
    at: f64,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    rotate: f64,
    #[serde(default = "one", rename = "scaleX")]
    scale_x: f64,
    #[serde(default = "one")]
    opacity: f64,
}
fn one() -> f64 {
    1.0
}
#[derive(Deserialize)]
pub struct Motion {
    duration: f64,
    origin: [f64; 2],
    #[serde(default)]
    linear: bool,
    #[serde(default)]
    delays: Vec<f64>,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Master {
    pub head_points: f32,
    pub prop_points: f32,
    pub prop_offset: [f32; 2],
    prop_style: Style,
    props: BTreeMap<String, String>,
    aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub hats: BTreeMap<String, Hat>,
    #[serde(skip)]
    pub hat_order: Vec<String>,
    #[serde(skip)]
    hat_motion: HatMotion,
    motions: BTreeMap<String, Motion>,
    pub departures: Vec<Departure>,
    effects: BTreeMap<String, Effect>,
}
pub fn master() -> &'static Master {
    static MASTER: OnceLock<Master> = OnceLock::new();
    MASTER.get_or_init(|| {
        let mut master: Master =
            serde_json::from_str(include_str!("../../assets/agent-presence.json"))
                .expect("approved presence artwork");
        let hats: HatCatalog = serde_json::from_str(include_str!("../../assets/agent-hats.json"))
            .expect("approved calibrated hats");
        master.hats = hats.hats;
        master.hat_order = hats.order;
        master.hat_motion = hats.motion;
        master
    })
}
fn motion_at(motion: &Motion, t: f64) -> (Affine, f32) {
    let t = t.clamp(0.0, 1.0);
    let next = motion
        .frames
        .iter()
        .position(|f| f.at >= t)
        .unwrap_or(motion.frames.len() - 1);
    let b = motion.frames[next];
    let a = motion.frames[next.saturating_sub(1)];
    let mut u = if b.at > a.at {
        (t - a.at) / (b.at - a.at)
    } else {
        0.0
    };
    if !motion.linear {
        u = u * u * (3.0 - 2.0 * u);
    }
    let mix = |a: f64, b: f64| a + (b - a) * u;
    let origin = (motion.origin[0], motion.origin[1]);
    let transform = Affine::translate((mix(a.x, b.x), mix(a.y, b.y)))
        * Affine::translate(origin)
        * Affine::rotate(mix(a.rotate, b.rotate).to_radians())
        * Affine::scale_non_uniform(mix(a.scale_x, b.scale_x), 1.0)
        * Affine::translate((-origin.0, -origin.1));
    (transform, mix(a.opacity, b.opacity) as f32)
}
struct Part {
    frames: Vec<Arc<Mesh>>,
    duration: f64,
    delay: f64,
}
struct Artwork {
    parts: Vec<Part>,
}
#[derive(Default)]
pub struct ArtCache {
    art: HashMap<Id, Artwork>,
}
impl ArtCache {
    pub fn effect(
        &mut self,
        painter: &Painter,
        id: Id,
        name: &str,
        center: egui::Pos2,
        points: f32,
        opacity: f32,
    ) {
        let Some(effect) = master().effects.get(name) else {
            return;
        };
        let scale = points / effect.view_box[2];
        let key = Id::new((
            "presence-effect",
            name,
            painter.ctx().pixels_per_point().to_bits(),
        ));
        let art = self
            .art
            .entry(key)
            .or_insert_with(|| build(&effect.svg, false, 1.0 / painter.ctx().pixels_per_point()));
        let transform = TSTransform {
            scaling: scale,
            translation: center.to_vec2()
                - egui::vec2(effect.view_box[2] / 2.0, effect.view_box[3] / 2.0) * scale,
        };
        for (index, part) in art.parts.iter().enumerate() {
            cached_mesh(
                painter,
                id.with(index),
                part.frames[0].clone(),
                transform,
                opacity,
            );
        }
    }
    #[cfg(test)]
    pub fn paint(
        &mut self,
        painter: &Painter,
        visual: &AvatarVisual<'_>,
        now: f64,
        reduced: bool,
        head_points: f32,
        draw_scale: f32,
    ) -> bool {
        self.paint_pose(
            painter,
            visual,
            now,
            reduced,
            head_points,
            draw_scale,
            super::rig::gaze(visual.orientation),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn paint_pose(
        &mut self,
        painter: &Painter,
        visual: &AvatarVisual<'_>,
        now: f64,
        reduced: bool,
        head_points: f32,
        draw_scale: f32,
        look: [f64; 2],
    ) -> bool {
        let mut animated = false;
        if let Some(hat) = master()
            .hats
            .get(visual.hat)
            .filter(|hat| !hat.svg.is_empty())
        {
            let scale = head_points / 224.0;
            let key = Id::new((
                "presence-hat",
                visual.hat,
                head_points.to_bits(),
                painter.ctx().pixels_per_point().to_bits(),
            ));
            let artwork = self.art.entry(key).or_insert_with(|| {
                build_hat(hat, 1.0 / (scale * painter.ctx().pixels_per_point()))
            });
            let scale = scale * draw_scale;
            let motion = master().hat_motion;
            let turn = (1.0
                - look[0].abs() * motion.yaw_compression
                - look[1].abs() * motion.pitch_compression) as f32;
            let head = &super::rig::interpreted().head;
            let crown = egui::vec2(head.cx as f32, (head.cy - head.radius) as f32);
            let offset = egui::vec2(
                (look[0] * motion.horizontal_offset) as f32,
                (look[1] * motion.vertical_offset) as f32,
            ) * scale;
            let transform = TSTransform {
                scaling: scale * turn,
                translation: visual.center.to_vec2()
                    - egui::vec2(head.cx as f32, head.cy as f32) * scale
                    + crown * scale * (1.0 - turn)
                    + offset,
            };
            for (index, part) in artwork.parts.iter().enumerate() {
                cached_mesh(
                    painter,
                    Id::new(("presence-hat-slot", visual.id, head_points.to_bits(), index)),
                    part.frames[0].clone(),
                    transform,
                    visual.opacity,
                );
            }
        }
        let prop = master()
            .aliases
            .get(visual.prop)
            .map_or(visual.prop, String::as_str);
        if let Some(svg) = master().props.get(prop) {
            let scale = master().prop_points * (head_points / master().head_points) / 24.0;
            let key = Id::new((
                "presence-prop",
                prop,
                head_points.to_bits(),
                painter.ctx().pixels_per_point().to_bits(),
            ));
            let artwork = self.art.entry(key).or_insert_with(|| {
                build(svg, false, 1.0 / (scale * painter.ctx().pixels_per_point()))
            });
            let scale = scale * draw_scale;
            let center = visual.center
                + egui::vec2(master().prop_offset[0], master().prop_offset[1])
                    * (head_points / master().head_points)
                    * draw_scale;
            let transform = TSTransform {
                scaling: scale,
                translation: center.to_vec2() - egui::vec2(12.0, 12.0) * scale,
            };
            for (index, part) in artwork.parts.iter().enumerate() {
                let phase = if reduced || part.duration <= 0.0 {
                    0
                } else {
                    animated = true;
                    (((now - part.delay).rem_euclid(part.duration) / part.duration)
                        * part.frames.len() as f64) as usize
                        % part.frames.len()
                };
                cached_mesh(
                    painter,
                    Id::new((
                        "presence-prop-slot",
                        visual.id,
                        head_points.to_bits(),
                        index,
                    )),
                    part.frames[phase].clone(),
                    transform,
                    visual.opacity,
                );
            }
        }
        animated
    }
}
fn build_hat(hat: &Hat, feather: f32) -> Artwork {
    let Some(fit) = hat.fit else {
        return build(&hat.svg, true, feather);
    };
    let transform = Affine::translate((
        40.0 + fit.x + fit.width * 0.5,
        30.0 + fit.y + fit.height * 0.5,
    )) * Affine::rotate(fit.rotation.to_radians())
        * Affine::translate((-fit.width * 0.5, -fit.height * 0.5))
        * Affine::scale_non_uniform(fit.width / 100.0, fit.height / 90.0);
    let mut artwork = build(
        &hat.svg,
        true,
        feather / (fit.width / 100.0).max(fit.height / 90.0) as f32,
    );
    for part in &mut artwork.parts {
        for mesh in &mut part.frames {
            for vertex in &mut Arc::make_mut(mesh).vertices {
                let p = transform * Point::new(vertex.pos.x as f64, vertex.pos.y as f64);
                vertex.pos = egui::pos2(p.x as f32, p.y as f32);
            }
        }
    }
    artwork
}
pub(super) fn hat_icon_mesh(hat: &Hat) -> Mesh {
    let art = build(&hat.svg, true, 1.0);
    let mut mesh = Mesh::default();
    for part in art.parts {
        let source = &part.frames[0];
        let base = mesh.vertices.len() as u32;
        mesh.indices.extend(source.indices.iter().map(|i| i + base));
        mesh.vertices.extend(source.vertices.iter().copied());
    }
    mesh
}
fn build(svg: &str, hat: bool, feather: f32) -> Artwork {
    let source = format!("<svg xmlns=\"http://www.w3.org/2000/svg\">{svg}</svg>");
    let document = roxmltree::Document::parse(&source).expect("approved SVG artwork");
    let mut parts = Vec::new();
    for node in document.descendants().filter(|n| n.is_element()) {
        let Some(mut path) = path(node) else {
            continue;
        };
        let ancestors: Vec<_> = node.ancestors().collect();
        for ancestor in ancestors.iter().rev() {
            if let Some(transform) = ancestor.attribute("transform") {
                path = parse_transform(transform) * path;
            }
        }
        let default_fill = if hat {
            "#000000"
        } else {
            master().prop_style.fill.as_str()
        };
        let default_stroke = if hat {
            "none"
        } else {
            master().prop_style.stroke.as_str()
        };
        let fill = inherited(node, "fill").unwrap_or(default_fill);
        let stroke = inherited(node, "stroke").unwrap_or(default_stroke);
        let opacity = ancestors
            .iter()
            .filter_map(|n| n.attribute("opacity"))
            .filter_map(|s| s.parse::<f32>().ok())
            .product::<f32>();
        let width = inherited(node, "stroke-width")
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(master().prop_style.stroke_width);
        let base = geometry(&path, fill, stroke, width, opacity, feather);
        let motion_name = ancestors
            .iter()
            .filter_map(|n| n.attribute("class"))
            .find(|name| master().motions.contains_key(*name));
        let motion = motion_name.and_then(|name| master().motions.get(name));
        let delay = motion
            .and_then(|motion| {
                motion.delays.get(
                    node.prev_siblings()
                        .filter(|n| n.is_element())
                        .count()
                        .saturating_sub(1),
                )
            })
            .copied()
            .unwrap_or(0.0);
        let frames = if let Some(motion) = motion {
            (0..48)
                .map(|index| {
                    let (transform, opacity) = motion_at(motion, index as f64 / 48.0);
                    let mut frame = base.clone();
                    for vertex in &mut frame.vertices {
                        let p = transform * Point::new(vertex.pos.x as f64, vertex.pos.y as f64);
                        vertex.pos = egui::pos2(p.x as f32, p.y as f32);
                        vertex.color = vertex.color.gamma_multiply(opacity);
                    }
                    Arc::new(frame)
                })
                .collect()
        } else {
            vec![Arc::new(base)]
        };
        parts.push(Part {
            frames,
            duration: motion.map_or(0.0, |m| m.duration),
            delay,
        });
    }
    Artwork { parts }
}
#[cfg(test)]
#[path = "art_tests.rs"]
mod tests;
