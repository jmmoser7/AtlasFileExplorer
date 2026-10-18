//! SVG path interpretation and cached vector meshes for the presence artist.
use super::head;
use eframe::egui::{self, Mesh};
use vector_ink::{
    kurbo::{Affine, BezPath, Circle, Point, Rect, RoundedRect, Shape},
    Cap, Join, StrokeStyle,
};

pub(super) fn inherited<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.ancestors()
        .find_map(|ancestor| ancestor.attribute(name))
}
pub(super) fn parse_transform(value: &str) -> Affine {
    let numbers = |s: &str| {
        s.split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<f64>().ok())
            .collect::<Vec<_>>()
    };
    if let Some(args) = value
        .strip_prefix("rotate(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let args = numbers(args);
        let center = (
            args.get(1).copied().unwrap_or(0.0),
            args.get(2).copied().unwrap_or(0.0),
        );
        Affine::translate(center)
            * Affine::rotate(args.first().copied().unwrap_or(0.0).to_radians())
            * Affine::translate((-center.0, -center.1))
    } else {
        Affine::IDENTITY
    }
}
pub(super) fn path(node: roxmltree::Node<'_, '_>) -> Option<BezPath> {
    let number = |name| {
        node.attribute(name)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    match node.tag_name().name() {
        "path" => BezPath::from_svg(node.attribute("d")?).ok(),
        "rect" => Some(
            RoundedRect::from_rect(
                Rect::new(
                    number("x"),
                    number("y"),
                    number("x") + number("width"),
                    number("y") + number("height"),
                ),
                number("rx"),
            )
            .to_path(0.02),
        ),
        "ellipse" => Some(
            vector_ink::kurbo::Ellipse::new(
                Point::new(number("cx"), number("cy")),
                (number("rx"), number("ry")),
                0.0,
            )
            .to_path(0.02),
        ),
        "circle" => {
            Some(Circle::new(Point::new(number("cx"), number("cy")), number("r")).to_path(0.02))
        }
        _ => None,
    }
}
pub(super) fn geometry(
    path: &BezPath,
    fill: &str,
    stroke: &str,
    width: f32,
    opacity: f32,
    feather: f32,
) -> Mesh {
    let mut mesh = Mesh::default();
    if fill != "none" {
        let (vertices, indices) =
            vector_ink::fill_triangles(&vector_ink::flatten_contours(path, 0.02));
        mesh.indices = indices;
        mesh.vertices = vertices
            .into_iter()
            .map(|p| egui::epaint::Vertex {
                pos: egui::pos2(p[0], p[1]),
                uv: egui::epaint::WHITE_UV,
                color: head::color(fill).gamma_multiply(opacity),
            })
            .collect();
    }
    if stroke != "none" {
        let ink = vector_ink::stroke_mesh(
            path,
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
        let base = mesh.vertices.len() as u32;
        mesh.indices
            .extend(ink.indices.into_iter().map(|i| i + base));
        mesh.vertices
            .extend(ink.vertices.into_iter().map(|v| egui::epaint::Vertex {
                pos: egui::pos2(v.pos[0], v.pos[1]),
                uv: egui::epaint::WHITE_UV,
                color: head::color(stroke).gamma_multiply(opacity * v.alpha),
            }));
    }
    mesh
}
