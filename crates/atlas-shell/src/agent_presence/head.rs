use super::rig;
use eframe::egui::{self, Color32, Mesh, Pos2};
use vector_ink::{
    kurbo::{Circle, Shape},
    Cap, Join, StrokeStyle,
};
pub fn color(hex: &str) -> Color32 {
    let value =
        u32::from_str_radix(hex.trim_start_matches('#'), 16).expect("approved avatar color");
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}
fn percent(value: &serde_json::Value) -> f32 {
    value
        .as_str()
        .unwrap()
        .trim_end_matches('%')
        .parse::<f32>()
        .unwrap()
        / 100.0
}
pub fn build(feather: f32) -> Mesh {
    let mut mesh = Mesh::default();
    let r = rig::spec(&["head", "radius"]) as f32;
    let center = egui::pos2(
        rig::spec(&["head", "cx"]) as f32,
        rig::spec(&["head", "cy"]) as f32,
    );
    let gradient = &rig::rig()["geometry"]["head"]["gradient"];
    let gc = center
        + egui::vec2(
            (percent(&gradient["cx"]) - 0.5) * r * 2.0,
            (percent(&gradient["cy"]) - 0.5) * r * 2.0,
        );
    let gr = percent(&gradient["radius"]) * r * 2.0;
    let stops: Vec<(f32, Color32)> = gradient["stops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["offset"].as_str().unwrap().parse().unwrap(),
                color(v["color"].as_str().unwrap()),
            )
        })
        .collect();
    let gradient_color = |p: Pos2| {
        let value = (p - gc).length() / gr;
        let next = stops
            .iter()
            .position(|(at, _)| *at >= value)
            .unwrap_or(stops.len() - 1);
        if next == 0 {
            stops[0].1
        } else {
            let (a, ca) = stops[next - 1];
            let (b, cb) = stops[next];
            blend(ca, cb, ((value - a) / (b - a)).clamp(0.0, 1.0))
        }
    };
    // Soft ground shadow uses the same transparent radial field as the SVG.
    let shadow = &rig::rig()["geometry"]["shadow"];
    let shadow_color = color(shadow["color"].as_str().unwrap());
    let sc = egui::pos2(
        shadow["cx"].as_f64().unwrap() as f32,
        shadow["cy"].as_f64().unwrap() as f32,
    );
    ellipse(
        &mut mesh,
        sc,
        shadow["rx"].as_f64().unwrap() as f32,
        shadow["ry"].as_f64().unwrap() as f32,
        8,
        |p| {
            let d = ((p.x - sc.x) / (shadow["rx"].as_f64().unwrap() as f32))
                .hypot((p.y - sc.y) / (shadow["ry"].as_f64().unwrap() as f32));
            shadow_color
                .gamma_multiply(((1.0 - d) * shadow["opacity"].as_f64().unwrap() as f32).max(0.0))
        },
    );
    let first = mesh.vertices.len();
    ellipse(&mut mesh, center, r, r, 16, gradient_color);
    let boundary: Vec<u32> = (0..96).map(|n| (first + 16 * 96 + n) as u32).collect();
    crate::mesh_aa::feather_boundary(&mut mesh, &boundary, feather);
    // SVG's centered shell stroke is part of the approved material. Reuse the
    // shared stroke owner and cache its AA ring with the gradient, once/density.
    let outline = outline_mesh(feather);
    let base = mesh.vertices.len() as u32;
    mesh.vertices.extend(outline.vertices);
    mesh.indices
        .extend(outline.indices.into_iter().map(|index| index + base));
    mesh
}
fn outline_mesh(feather: f32) -> Mesh {
    let head = &rig::rig()["geometry"]["head"];
    let radius = head["radius"].as_f64().unwrap();
    let width = head["strokeWidth"].as_f64().unwrap() as f32;
    let circle = Circle::new(
        (head["cx"].as_f64().unwrap(), head["cy"].as_f64().unwrap()),
        radius,
    );
    let source = vector_ink::stroke_mesh(
        &circle.to_path(0.02),
        &StrokeStyle {
            width,
            cap: Cap::Round,
            join: Join::Round,
            taper: None,
            dash: None,
        },
        feather,
        0.02,
    );
    let ink = color(head["stroke"].as_str().unwrap());
    // A stroke thinner than one device pixel has no opaque plateau. Preserve
    // its integrated coverage when the shared AA strip collapses to a ridge.
    let coverage = if width < feather {
        (2.0 * width / (width + feather)).min(1.0)
    } else {
        1.0
    };
    Mesh {
        indices: source.indices,
        vertices: source
            .vertices
            .into_iter()
            .map(|v| egui::epaint::Vertex {
                pos: egui::pos2(v.pos[0], v.pos[1]),
                uv: egui::epaint::WHITE_UV,
                color: ink.gamma_multiply(v.alpha * coverage),
            })
            .collect(),
        ..Default::default()
    }
}
fn ellipse(
    mesh: &mut Mesh,
    center: Pos2,
    rx: f32,
    ry: f32,
    rings: usize,
    color: impl Fn(Pos2) -> Color32,
) {
    let base = mesh.vertices.len() as u32;
    for ring in 0..=rings {
        for n in 0..96 {
            let angle = n as f32 / 96.0 * std::f32::consts::TAU;
            let k = ring as f32 / rings as f32;
            let p = center + egui::vec2(angle.cos() * rx * k, angle.sin() * ry * k);
            mesh.vertices.push(egui::epaint::Vertex {
                pos: p,
                uv: egui::epaint::WHITE_UV,
                color: color(p),
            });
        }
    }
    for ring in 0..rings {
        for n in 0..96 {
            let next = (n + 1) % 96;
            let (a, b, c, d) = (
                base + (ring * 96 + n) as u32,
                base + (ring * 96 + next) as u32,
                base + ((ring + 1) * 96 + n) as u32,
                base + ((ring + 1) * 96 + next) as u32,
            );
            mesh.add_triangle(a, b, c);
            mesh.add_triangle(b, d, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_shell_outline_is_centered_has_its_authored_width_and_subpixel_coverage() {
        let radius = rig::spec(&["head", "radius"]) as f32;
        let width = rig::spec(&["head", "strokeWidth"]) as f32;
        let center = egui::pos2(
            rig::spec(&["head", "cx"]) as f32,
            rig::spec(&["head", "cy"]) as f32,
        );
        let outline = outline_mesh(0.0);
        assert!(!outline.indices.is_empty());
        let distances: Vec<_> = outline
            .vertices
            .iter()
            .map(|v| v.pos.distance(center))
            .collect();
        let inner = distances.iter().copied().fold(f32::INFINITY, f32::min);
        let outer = distances.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!((inner - (radius - width / 2.0)).abs() < 0.04);
        assert!((outer - (radius + width / 2.0)).abs() < 0.04);
        let soft = outline_mesh(7.0);
        let max_alpha = soft.vertices.iter().map(|v| v.color.a()).max().unwrap();
        assert!(
            (1..255).contains(&max_alpha),
            "subpixel outline must not become an opaque heavy border"
        );
        let integrated = build(7.0);
        assert!(integrated.vertices.ends_with(&soft.vertices));
        let expected = color(rig::rig()["geometry"]["head"]["stroke"].as_str().unwrap());
        for vertex in soft.vertices.iter().filter(|v| v.color.a() > 0) {
            assert!(
                (vertex.color.r() as f32 / vertex.color.a() as f32 - expected.r() as f32 / 255.0)
                    .abs()
                    < 0.03
            );
        }
    }
}
