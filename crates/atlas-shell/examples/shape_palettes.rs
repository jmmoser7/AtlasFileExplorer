//! Native visual fixture: `cargo run -p atlas-shell --example shape_palettes -- --capture <prefix>`.
//! Captures both themes using the production widgets, then closes the fixture window.
use atlas_shell::{canvas_text, icons::Icon, selection_tools as chrome, theme::Palette};
use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Vec2};

struct Preview {
    dark: bool,
    frames: usize,
    capture: Option<String>,
    colors: [chrome::ColorState; 2],
    dimension: Option<chrome::NumberEdit>,
    trimmed: Vec<egui::Mesh>,
    stroke_quality: Option<Vec<egui::Mesh>>,
    fill_quality: bool,
}

impl eframe::App for Preview {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let theme = Palette::for_mode(self.dark);
        ctx.set_visuals(theme.visuals());
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme.bg))
            .show(ctx, |ui| {
                if let Some(meshes) = &self.stroke_quality {
                    let labels = if self.fill_quality {
                        [
                            (35.0, 30.0, "Boolean trim · curved edge without a stroke"),
                            (620.0, 30.0, "Compound fill · curved hole"),
                            (35.0, 270.0, "Translucent trim · no internal triangle seams"),
                            (620.0, 270.0, "Subpixel placement · small curved edges"),
                            (35.0, 510.0, "Angled mesh boundary · gradient fill"),
                            (620.0, 510.0, "Small filled glyphs · shared icon mesh"),
                        ]
                    } else {
                        [
                            (35.0, 30.0, "Dashes retain the spline's curvature"),
                            (620.0, 30.0, "Translucent capsule · clean end caps"),
                            (35.0, 270.0, "Translucent curve · continuous joins"),
                            (620.0, 270.0, "Tapered spline · retained curve samples"),
                            (35.0, 510.0, "Round join · no interior facet buildup"),
                            (620.0, 510.0, "Closed capsule · smooth outline"),
                        ]
                    };
                    for (x, y, title) in labels {
                        canvas_text::text(
                            ui.painter(),
                            Pos2::new(x, y),
                            Align2::LEFT_CENTER,
                            title,
                            egui::FontId::proportional(15.0),
                            theme.ink,
                        );
                    }
                    for mesh in meshes {
                        ui.painter().add(egui::Shape::mesh(mesh.clone()));
                    }
                    if self.fill_quality {
                        for (i, size) in [16.0, 24.0, 36.0, 64.0, 100.0].into_iter().enumerate() {
                            atlas_shell::icons::paint(
                                ui.painter(),
                                Rect::from_min_size(
                                    Pos2::new(630.25 + i as f32 * 100.0, 560.25),
                                    Vec2::splat(size),
                                ),
                                Icon::Select,
                                theme.ink,
                            );
                        }
                    }
                    return;
                }
                let z = 1.5;
                let recent = [
                    [58, 178, 171],
                    [244, 107, 110],
                    [230, 188, 68],
                    [157, 125, 255],
                    [104, 146, 186],
                    [69, 79, 90],
                ];
                for (i, (name, height)) in [
                    ("Fill", chrome::FILL_HEIGHT),
                    ("Stroke", chrome::STROKE_HEIGHT),
                ]
                .iter()
                .enumerate()
                {
                    let y = 45.0 + i as f32 * 305.0;
                    canvas_text::text(
                        ui.painter(),
                        Pos2::new(30.0, y - 20.0),
                        Align2::LEFT_CENTER,
                        name,
                        egui::FontId::proportional(14.0),
                        theme.sub,
                    );
                    let rect = Rect::from_min_size(
                        Pos2::new(30.0, y),
                        Vec2::new(chrome::EDITOR_WIDTH, *height) * z,
                    );
                    ui.push_id(i, |ui| {
                        chrome::color_editor(
                            ui,
                            rect,
                            [45, 212, 191, 255],
                            (i == 1).then_some(2.0),
                            false,
                            &recent,
                            &mut self.colors[i],
                            z,
                            theme,
                        );
                    });
                }
                let corner = Rect::from_min_size(
                    Pos2::new(30.0, 662.0),
                    Vec2::new(chrome::EDITOR_WIDTH, chrome::CORNER_HEIGHT) * z,
                );
                chrome::corner_editor(ui, corner, false, true, 50.0, 100.0, z, theme);
                chrome::wire_editor(
                    ui,
                    Rect::from_min_size(
                        Pos2::new(30.0, 707.0),
                        Vec2::new(chrome::EDITOR_WIDTH, chrome::WIRE_HEIGHT) * z,
                    ),
                    Some(true),
                    Some(true),
                    Some(true),
                    3.0,
                    z,
                    theme,
                );
                // TWIN: slate_doc::scene::PhotoFilter::swatch 2026-09-18
                chrome::filter_editor(
                    ui,
                    Rect::from_min_size(
                        Pos2::new(30.0, 742.0),
                        Vec2::new(chrome::EDITOR_WIDTH, chrome::FILTER_HEIGHT) * z,
                    ),
                    &[
                        chrome::FilterRadio {
                            label: "B&W",
                            fill: [148, 148, 148],
                            fill_b: None,
                        },
                        chrome::FilterRadio {
                            label: "Invert",
                            fill: [244, 244, 244],
                            fill_b: Some([28, 28, 28]),
                        },
                        chrome::FilterRadio {
                            label: "Clarendon",
                            fill: [46, 122, 168],
                            fill_b: None,
                        },
                        chrome::FilterRadio {
                            label: "Juno",
                            fill: [232, 148, 86],
                            fill_b: None,
                        },
                        chrome::FilterRadio {
                            label: "Lark",
                            fill: [214, 198, 92],
                            fill_b: None,
                        },
                    ],
                    Some(2),
                    0.72,
                    z,
                    theme,
                );
                chrome::stringer(
                    ui,
                    Id::new("short_dimension"),
                    [Pos2::new(758.0, 625.0), Pos2::new(778.0, 625.0)],
                    Vec2::new(0.0, 24.0),
                    12.0,
                    "",
                    z,
                    theme,
                    &mut None,
                );
                canvas_text::text(
                    ui.painter(),
                    Pos2::new(758.0, 25.0),
                    Align2::LEFT_CENTER,
                    "Trimmed rectangle · preserved stroke width",
                    egui::FontId::proportional(14.0),
                    theme.sub,
                );
                for mesh in &self.trimmed {
                    ui.painter().add(egui::Shape::mesh(mesh.clone()));
                }
                let shape = Rect::from_min_size(Pos2::new(758.0, 300.0), Vec2::new(300.0, 200.0));
                ui.painter().rect(
                    shape,
                    0.0,
                    Color32::from_rgb(16, 126, 124),
                    egui::Stroke::new(1.0_f32, theme.select),
                    egui::StrokeKind::Inside,
                );
                for (i, icon) in [Icon::Fill, Icon::Ellipse, Icon::Corners]
                    .into_iter()
                    .enumerate()
                {
                    chrome::button(
                        ui,
                        Rect::from_min_size(
                            Pos2::new(
                                shape.center().x - 50.0 + i as f32 * 37.0,
                                shape.top() - 44.0,
                            ),
                            Vec2::splat(30.0),
                        ),
                        Id::new(("button", i)),
                        "Property",
                        icon,
                        i == 0,
                        1.0,
                        theme,
                        1.0,
                        true,
                    );
                }
                chrome::stringer(
                    ui,
                    Id::new("width"),
                    [shape.left_bottom(), shape.right_bottom()],
                    Vec2::new(0.0, 32.0),
                    180.0,
                    "",
                    1.5,
                    theme,
                    &mut self.dimension,
                );
                chrome::stringer(
                    ui,
                    Id::new("height"),
                    [shape.right_top(), shape.right_bottom()],
                    Vec2::new(32.0, 0.0),
                    120.0,
                    "",
                    1.5,
                    theme,
                    &mut None,
                );
                canvas_text::text(
                    ui.painter(),
                    Pos2::new(760.0, 590.0),
                    Align2::LEFT_CENTER,
                    "T switches theme · Click a dimension to edit",
                    egui::FontId::proportional(13.0),
                    theme.sub,
                );
            });
        if ctx.input(|i| i.key_pressed(egui::Key::T)) {
            self.dark = !self.dark;
        }
        if let Some(prefix) = &self.capture {
            self.frames += 1;
            if self.frames == 8 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                let pixels = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, pixels)
                    .unwrap()
                    .save(format!(
                        "{prefix}-{}.png",
                        if self.dark { "dark" } else { "light" }
                    ))
                    .unwrap();
                if self.dark {
                    self.dark = false;
                    self.frames = 0;
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            ctx.request_repaint();
        }
    }
}

fn trimmed_preview() -> Vec<egui::Mesh> {
    let subject = vec![vec![
        [40.0, 30.0],
        [960.0, 30.0],
        [960.0, 410.0],
        [40.0, 410.0],
    ]];
    let cutter = vec![vec![
        [835.0, 0.0],
        [1000.0, 0.0],
        [1000.0, 200.0],
        [835.0, 200.0],
    ]];
    let mut meshes = Vec::new();
    for piece in vector_ink::boolean_difference(&subject, &cutter) {
        let mut path = vector_ink::kurbo::BezPath::new();
        for ring in &piece {
            path.move_to((ring[0][0] as f64, ring[0][1] as f64));
            for p in &ring[1..] {
                path.line_to((p[0] as f64, p[1] as f64));
            }
            path.close_path();
        }
        let (vertices, indices) = vector_ink::fill_triangles(&piece);
        meshes.push(egui::Mesh {
            vertices: vertices
                .into_iter()
                .map(|p| egui::epaint::Vertex {
                    pos: Pos2::new(744.0 + p[0] * 0.4, 45.0 + p[1] * 0.4),
                    uv: egui::epaint::WHITE_UV,
                    color: Color32::from_rgb(43, 64, 162),
                })
                .collect(),
            indices,
            ..Default::default()
        });
        let stroke = vector_ink::stroke_mesh(
            &path,
            &vector_ink::StrokeStyle {
                width: 8.0,
                cap: vector_ink::Cap::Butt,
                join: vector_ink::Join::Miter,
                taper: None,
                dash: None,
            },
            2.5,
            0.02,
        );
        meshes.push(egui::Mesh {
            vertices: stroke
                .vertices
                .into_iter()
                .map(|v| egui::epaint::Vertex {
                    pos: Pos2::new(744.0 + v.pos[0] * 0.4, 45.0 + v.pos[1] * 0.4),
                    uv: egui::epaint::WHITE_UV,
                    color: Color32::from_rgb(45, 212, 191).gamma_multiply(v.alpha),
                })
                .collect(),
            indices: stroke.indices,
            ..Default::default()
        });
    }
    meshes
}

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    let stroke_quality = args.iter().any(|a| a == "--stroke-quality");
    let fill_quality = args.iter().any(|a| a == "--fill-quality");
    // Diagnostic comparison only; production apps always use the shared AA policy.
    let samples = if args.iter().any(|a| a == "--no-msaa") {
        0
    } else {
        atlas_shell::NATIVE_MSAA_SAMPLES
    };
    let capture = args
        .iter()
        .position(|a| a == "--capture")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([1180.0, 820.0]);
    if capture.is_some() {
        viewport = viewport.with_position([-4000.0, -4000.0]);
    }
    eframe::run_native(
        "Slate shape palette review",
        eframe::NativeOptions {
            viewport,
            renderer: eframe::Renderer::Glow,
            multisampling: samples,
            ..Default::default()
        },
        Box::new(move |cc| {
            canvas_text::install(&cc.egui_ctx);
            Ok(Box::new(Preview {
                dark: true,
                frames: 0,
                capture,
                colors: Default::default(),
                dimension: None,
                trimmed: trimmed_preview(),
                stroke_quality: if fill_quality {
                    Some(fill_quality_preview())
                } else {
                    stroke_quality.then(stroke_quality_preview)
                },
                fill_quality,
            }))
        }),
    )
}

fn fill_quality_preview() -> Vec<egui::Mesh> {
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

fn stroke_quality_preview() -> Vec<egui::Mesh> {
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
                taper: Some((1.0, 0.08)),
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
