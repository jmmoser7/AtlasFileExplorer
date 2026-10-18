//! Production vector coverage fixtures.
use super::*;

pub(super) fn fill_quality_preview() -> Vec<egui::Mesh> {
    use vector_ink::kurbo::{RoundedRect, Shape as _};
    let rounded = |x, y, w, h, r| {
        vector_ink::flatten_contours(&RoundedRect::new(x, y, x + w, y + h, r).to_path(0.02), 0.02)
    };
    let outer = rounded(50.25, 65.25, 470.0, 165.0, 45.0);
    let notch = rounded(365.25, 35.25, 210.0, 130.0, 30.0);
    let mut cases: Vec<(vector_ink::Polygon, Color32, bool)> = Vec::new();
    for piece in vector_ink::boolean_difference(&outer, &notch) {
        cases.push((piece.clone(), Color32::RED, false));
        let shifted = piece
            .into_iter()
            .map(|ring| ring.into_iter().map(|p| [p[0], p[1] + 240.0]).collect())
            .collect();
        cases.push((
            shifted,
            Color32::from_rgba_unmultiplied(255, 0, 0, 115),
            false,
        ));
    }
    for piece in vector_ink::boolean_difference(
        &rounded(640.25, 65.25, 440.0, 165.0, 45.0),
        &rounded(750.25, 95.25, 220.0, 105.0, 52.5),
    ) {
        cases.push((piece, Color32::RED, false));
    }
    for (i, radius) in [3.0_f64, 6.0, 12.0, 24.0, 40.0].into_iter().enumerate() {
        cases.push((
            rounded(
                640.25 + i as f64 * 90.0,
                345.25,
                72.0,
                80.0,
                radius.min(36.0),
            ),
            Color32::RED,
            false,
        ));
    }
    cases.push((
        vec![vec![
            [60.25, 690.25],
            [120.25, 545.25],
            [490.25, 570.25],
            [535.25, 665.25],
        ]],
        Color32::RED,
        true,
    ));
    cases
        .into_iter()
        .map(|(poly, color, gradient)| {
            let (vertices, indices) = vector_ink::fill_triangles(&poly);
            egui::Mesh {
                vertices: vertices
                    .into_iter()
                    .map(|p| egui::epaint::Vertex {
                        pos: Pos2::new(p[0], p[1]),
                        uv: egui::epaint::WHITE_UV,
                        color: if gradient {
                            Color32::from_rgb(
                                (255.0 * (1.0 - (p[0] - 60.25) / 475.0)) as u8,
                                35,
                                (255.0 * (p[0] - 60.25) / 475.0) as u8,
                            )
                        } else {
                            color
                        },
                    })
                    .collect(),
                indices,
                ..Default::default()
            }
        })
        .collect()
}

pub(super) fn stroke_quality_preview() -> Vec<egui::Mesh> {
    use vector_ink::{
        kurbo::{BezPath, RoundedRect, Shape as _},
        Cap, Join, StrokeStyle,
    };
    let base = StrokeStyle {
        width: 22.0,
        cap: Cap::Round,
        join: Join::Round,
        taper: None,
        dash: None,
    };
    [
        (
            BezPath::from_svg("M50 200C130 10 330 360 510 160").unwrap(),
            StrokeStyle {
                width: 8.0,
                dash: Some((vec![70.0, 25.0], 0.0)),
                ..base.clone()
            },
        ),
        (
            BezPath::from_svg("M670 140H1080").unwrap(),
            StrokeStyle {
                width: 88.0,
                ..base.clone()
            },
        ),
        (
            BezPath::from_svg("M70 400C190 220 330 560 510 350").unwrap(),
            base.clone(),
        ),
        (
            BezPath::from_svg("M650 400C770 220 910 560 1090 350").unwrap(),
            StrokeStyle {
                width: 64.0,
                taper: Some(vector_ink::Taper::Linear(1.0, 0.08)),
                ..base.clone()
            },
        ),
        (
            BezPath::from_svg("M90 590H430V685").unwrap(),
            StrokeStyle {
                width: 48.0,
                ..base.clone()
            },
        ),
        (
            RoundedRect::new(640.0, 555.0, 1090.0, 705.0, 75.0).to_path(0.05),
            StrokeStyle {
                width: 14.0,
                ..base
            },
        ),
    ]
    .into_iter()
    .map(|(path, style)| {
        let ink = vector_ink::stroke_mesh(&path, &style, 1.25, 0.12);
        egui::Mesh {
            vertices: ink
                .vertices
                .into_iter()
                .map(|v| egui::epaint::Vertex {
                    pos: Pos2::new(v.pos[0], v.pos[1]),
                    uv: egui::epaint::WHITE_UV,
                    color: Color32::from_rgba_unmultiplied(45, 180, 175, 150)
                        .gamma_multiply(v.alpha),
                })
                .collect(),
            indices: ink.indices,
            ..Default::default()
        }
    })
    .collect()
}
