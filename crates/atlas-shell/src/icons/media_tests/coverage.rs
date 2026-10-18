use super::*;

const CASES: &[Icon] = &[
    Icon::Image,
    Icon::Ellipse,
    Icon::Sliders,
    Icon::Agent,
    Icon::Fill,
    Icon::Corners,
    Icon::Brush,
    Icon::Media,
    Icon::Grid,
    Icon::Filters,
    Icon::TextDoc,
    Icon::Model,
    Icon::Video,
];
const SIZES: &[f32] = &[15.6, 16.0, 20.0, 24.0, 31.2];
const CELL: f32 = 40.0;

#[test]
fn shared_rasterizer_owns_each_triangle_edge_once() {
    for reverse in [false, true] {
        let mut mesh = egui::Mesh::default();
        for pos in [
            egui::pos2(1.0, 1.0),
            egui::pos2(9.0, 1.0),
            egui::pos2(9.0, 9.0),
            egui::pos2(1.0, 9.0),
        ] {
            mesh.vertices.push(egui::epaint::Vertex {
                pos,
                uv: egui::epaint::WHITE_UV,
                color: Color32::from_rgba_unmultiplied(200, 150, 100, 64),
            });
        }
        mesh.indices = if reverse {
            vec![2, 1, 0, 3, 2, 0]
        } else {
            vec![0, 1, 2, 0, 2, 3]
        };
        let pixels = super::super::tests::rasterize(&[mesh], 10.0, 10.0, 1.0, Color32::TRANSPARENT);
        for y in 1..9 {
            for x in 1..9 {
                assert_eq!(
                    pixels.get_pixel(x, y).0[3],
                    64,
                    "shared diagonal must have one owner, reverse={reverse}, at {x}/{y}"
                );
            }
        }
    }
}

fn media_mesh(icon: Icon, size: f32, ppp: f32, alpha: u8, dark: bool) -> Vec<egui::Mesh> {
    let theme = crate::theme::Palette::for_mode(dark);
    let ink = theme.ink;
    // Fractional placement exercises subpixel AA without placing every shared
    // triangle edge exactly on a supersample center.
    let slot = Rect::from_center_size(egui::pos2(20.137, 20.271), egui::vec2(size, size));
    paint_frame_at(ppp, |painter| {
        paint_on_media(
            painter,
            slot,
            icon,
            Color32::from_rgba_unmultiplied(ink.r(), ink.g(), ink.b(), alpha),
            theme,
        );
    })
}

#[test]
fn faded_media_icons_do_not_accumulate_opacity_at_crossings_or_counters() {
    for &icon in CASES {
        for &size in SIZES {
            for ppp in [1.0, 1.5, 2.0] {
                let opaque = super::super::tests::rasterize(
                    &media_mesh(icon, size, ppp, 255, true),
                    CELL,
                    CELL,
                    ppp,
                    Color32::TRANSPARENT,
                );
                for alpha in [64, 128] {
                    let meshes = media_mesh(icon, size, ppp, alpha, true);
                    assert_eq!(meshes.len(), 1, "{icon:?} at {size}/{ppp}");
                    let pixels = super::super::tests::rasterize(
                        &meshes,
                        CELL,
                        CELL,
                        ppp,
                        Color32::TRANSPARENT,
                    );
                    let max = pixels.pixels().map(|p| p.0[3]).max().unwrap();
                    assert!(max > 0, "missing {icon:?} at {size}/{ppp}");
                    assert!(
                        max <= alpha + 1,
                        "composited {icon:?} alpha {max} exceeds caller {alpha} at {size}/{ppp}"
                    );
                    for ((x, y, actual), full) in pixels.enumerate_pixels().zip(opaque.pixels()) {
                        let expected = (full.0[3] as f32 * alpha as f32 / 255.0).round() as i16;
                        let error = (actual.0[3] as i16 - expected).abs();
                        assert!(
                            error <= 2,
                            "{icon:?} fade differs by {error} alpha units at {size}/{ppp}/{alpha}, pixel {x}/{y}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn outline_counter_preserves_the_host_image() {
    for &size in SIZES {
        for ppp in [1.0, 1.5, 2.0] {
            for dark in [false, true] {
                let pixels = super::super::tests::rasterize(
                    &media_mesh(Icon::Ellipse, size, ppp, 255, dark),
                    CELL,
                    CELL,
                    ppp,
                    Color32::TRANSPARENT,
                );
                let center = (CELL * ppp * 0.5) as u32;
                for y in center - 1..=center + 1 {
                    for x in center - 1..=center + 1 {
                        assert_eq!(
                            pixels.get_pixel(x, y).0[3],
                            0,
                            "Ellipse counter covers host pixels at {size}/{ppp}, dark={dark}, {x}/{y}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "set ATLAS_MEDIA_ICON_AUDIT_DIR to retain the exact failed Agent mesh"]
fn export_agent_media_failure_for_numeric_audit() {
    let dir = std::path::PathBuf::from(
        std::env::var("ATLAS_MEDIA_ICON_AUDIT_DIR").expect("audit output directory"),
    );
    std::fs::create_dir_all(&dir).unwrap();
    let encode = |mesh: &egui::Mesh| {
        serde_json::json!({
            "vertex_columns": ["point_x_bits", "point_y_bits", "raster_x_bits", "raster_y_bits", "premult_rgba"],
            "vertices": mesh.vertices.iter().map(|v| (
                v.pos.x.to_bits(), v.pos.y.to_bits(),
                (v.pos.x * 1.5_f32).to_bits(), (v.pos.y * 1.5_f32).to_bits(),
                v.color.to_array(),
            )).collect::<Vec<_>>(),
            "indices": &mesh.indices,
        })
    };
    let mut cases = Vec::new();
    for alpha in [64, 255] {
        let meshes = media_mesh(Icon::Agent, 24.0, 1.5, alpha, true);
        let pixels = super::super::tests::rasterize(&meshes, CELL, CELL, 1.5, Color32::TRANSPARENT);
        let max = pixels.pixels().map(|p| p.0[3]).max().unwrap();
        cases.push(serde_json::json!({
            "caller_alpha": alpha,
            "meshes": meshes.iter().map(&encode).collect::<Vec<_>>(),
            "max_alpha": max,
            "max_pixels": pixels.enumerate_pixels().filter(|(_, _, p)| p.0[3] == max)
                .map(|(x, y, p)| (x, y, p.0)).collect::<Vec<_>>(),
            "raster_width": pixels.width(), "raster_height": pixels.height(),
            "pixel_alpha": pixels.pixels().map(|p| p.0[3]).collect::<Vec<_>>(),
        }));
    }
    let material = density::media::mesh(Icon::Agent, 36.0);
    let input = density::media::source_material_for_audit(Icon::Agent, 36.0);
    let output = vector_ink::max_coverage_mesh(&input);
    let encode_ink = |mesh: &vector_ink::InkMesh| {
        serde_json::json!({
            "vertex_columns": ["master_x_bits", "master_y_bits", "coverage_alpha_bits"],
            "vertices": mesh.vertices.iter().map(|v| (
                v.pos[0].to_bits(), v.pos[1].to_bits(), v.alpha.to_bits(),
            )).collect::<Vec<_>>(),
            "color_columns": ["material_red_bits", "green_bits", "blue_bits", "alpha_bits"],
            "colors": mesh.colors.iter().map(|color| color.map(f32::to_bits)).collect::<Vec<_>>(),
            "indices": &mesh.indices,
        })
    };
    let evidence = serde_json::json!({
        "source": "Exact production Agent 24pt/1.5ppp failing case; unchanged shared rasterizer",
        "material_physical_size": 36.0, "material": encode(&material),
        "pre_envelope": encode_ink(&input),
        "post_envelope": encode_ink(&output),
        "placed_cases": cases,
    });
    std::fs::write(
        dir.join("agent-24pt-1_5ppp-exact-mesh.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}

/// Actual production material meshes, rasterized at native pixel density. The
/// manifest maps rows to icons/opacity and columns to slot size; no SVG surrogate.
#[test]
#[ignore = "set ATLAS_MEDIA_ICON_AUDIT_DIR to write actual media mesh PNGs"]
fn export_media_icon_pixels_for_visual_audit() {
    let dir = std::path::PathBuf::from(
        std::env::var("ATLAS_MEDIA_ICON_AUDIT_DIR").expect("audit output directory"),
    );
    std::fs::create_dir_all(&dir).unwrap();
    let mut outputs = Vec::new();
    for dark in [false, true] {
        for ppp in [1.0, 1.5, 2.0] {
            for light_host in [false, true] {
                let cell = (CELL * ppp) as u32;
                let background = if light_host {
                    Color32::from_gray(245)
                } else {
                    Color32::from_gray(25)
                };
                let mut atlas = image::RgbaImage::from_pixel(
                    cell * SIZES.len() as u32,
                    cell * CASES.len() as u32 * 3,
                    image::Rgba(background.to_srgba_unmultiplied()),
                );
                for (row, &icon) in CASES.iter().enumerate() {
                    for (fade, alpha) in [64, 128, 255].into_iter().enumerate() {
                        for (col, &size) in SIZES.iter().enumerate() {
                            let meshes = media_mesh(icon, size, ppp, alpha, dark);
                            let pixels = super::super::tests::rasterize(
                                &meshes, CELL, CELL, ppp, background,
                            );
                            image::imageops::overlay(
                                &mut atlas,
                                &pixels,
                                (col as u32 * cell) as i64,
                                ((row * 3 + fade) as u32 * cell) as i64,
                            );
                        }
                    }
                }
                let name = format!(
                    "media-{}-host-{}-{ppp}x.png",
                    if dark { "dark" } else { "light" },
                    if light_host { "light" } else { "dark" }
                );
                atlas.save(dir.join(&name)).unwrap();
                outputs.push(name);
            }
        }
    }
    let ellipse = density::media::mesh(Icon::Ellipse, 24.0);
    let manifest = serde_json::json!({
        "source": "icons::paint_on_media production meshes; shared native mesh rasterizer",
        "columns_slot_points": SIZES,
        "rows_icons": CASES.iter().map(|i| i.name()).collect::<Vec<_>>(),
        "rows_per_icon_alpha": [64, 128, 255], "cell_points": CELL,
        "geometry_cost": cold_geometry_metrics(&dir),
        "ellipse_material_mesh_24px": {
            "vertex_columns": ["master_x", "master_y", "premult_material_red", "coverage_alpha"],
            "vertices": ellipse.vertices.iter().map(|v| [v.pos.x, v.pos.y, v.color.r() as f32, v.color.a() as f32]).collect::<Vec<_>>(),
            "indices": &ellipse.indices,
        },
        "outputs": outputs,
    });
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

fn cold_geometry_metrics(dir: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::create_dir_all(dir.join("geometry")).unwrap();
    let mut records = Vec::new();
    for &icon in CASES {
        for &size in SIZES {
            for ppp in [1.0, 1.5, 2.0] {
                let mesh_file = format!(
                    "geometry/{}-{:08x}-{:08x}.mesh",
                    icon.name(),
                    size.to_bits(),
                    f32::to_bits(ppp),
                );
                let mesh_path = dir.join(&mesh_file);
                records.push(
                    std::thread::spawn(move || {
                        // Each fresh thread starts with an empty production geometry
                        // cache. Thread creation and PNG rasterization are not timed.
                        let start = std::time::Instant::now();
                        let mesh = density::media::mesh(icon, size * ppp);
                        let cold_us = start.elapsed().as_micros();
                        let start = std::time::Instant::now();
                        let warm = density::media::mesh(icon, size * ppp);
                        let warm_us = start.elapsed().as_micros();
                        // Serialization is outside both production timing intervals.
                        write_material_mesh_bits(&mesh_path, &mesh);
                        serde_json::json!({
                            "icon": icon.name(), "slot_points": size, "ppp": ppp,
                            "cold_us": cold_us, "warm_us": warm_us,
                            "vertices": mesh.vertices.len(), "triangles": mesh.indices.len() / 3,
                            "warm_reused_geometry": Arc::ptr_eq(&mesh, &warm),
                            "exact_mesh_file": mesh_file,
                            "texture_id": format!("{:?}", mesh.texture_id),
                        })
                    })
                    .join()
                    .expect("geometry audit worker"),
                );
            }
        }
    }
    records
}

/// MIM1: magic, little-endian u32 counts, position/UV f32 bits and RGBA bytes,
/// then little-endian u32 indices. Retains exact production output for comparison.
fn write_material_mesh_bits(path: &std::path::Path, mesh: &egui::Mesh) {
    let mut bytes = Vec::with_capacity(12 + mesh.vertices.len() * 20 + mesh.indices.len() * 4);
    bytes.extend_from_slice(b"MIM1");
    bytes.extend_from_slice(&(mesh.vertices.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(mesh.indices.len() as u32).to_le_bytes());
    for vertex in &mesh.vertices {
        for value in [vertex.pos.x, vertex.pos.y, vertex.uv.x, vertex.uv.y] {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes.extend_from_slice(&vertex.color.to_array());
    }
    for index in &mesh.indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}

/// Shared CPU paint only; the real app, tessellation and GPU remain separate gates.
#[test]
#[ignore = "set ATLAS_MEDIA_ICON_AUDIT_DIR to record changing-zoom component paint"]
fn export_media_changing_zoom_paint_cost() {
    let dir = std::path::PathBuf::from(
        std::env::var("ATLAS_MEDIA_ICON_AUDIT_DIR").expect("audit output directory"),
    );
    std::fs::create_dir_all(&dir).unwrap();
    let mut records = Vec::new();
    for ppp in [1.0_f32, 1.5, 2.0] {
        records.extend(
            std::thread::spawn(move || {
                use crate::selection_tools::{blister_layout, BlisterPresentation};
                let icons: Vec<_> = CASES
                    .iter()
                    .copied()
                    .filter(|icon| {
                        matches!(
                            icon,
                            Icon::Fill
                                | Icon::Ellipse
                                | Icon::Corners
                                | Icon::Sliders
                                | Icon::Brush
                                | Icon::Media
                                | Icon::Agent
                        )
                    })
                    .collect();
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(ppp);
                let theme = crate::theme::Palette::dark();
                let mut seen = std::collections::HashSet::new();
                let mut frames = Vec::new();
                for frame in 0..=40 {
                    let step = if frame <= 20 { frame } else { 40 - frame };
                    let z = 0.5 + step as f32 * 0.025;
                    let repeated_size = !seen.insert(z.to_bits());
                    // No measured glyph or context frame is prewarmed.
                    let start = std::time::Instant::now();
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(900.0, 700.0),
                            )),
                            time: Some(frame as f64 / 60.0),
                            ..Default::default()
                        },
                        |ctx| {
                            assert_eq!(ctx.pixels_per_point(), ppp);
                            let host = Rect::from_center_size(
                                egui::pos2(450.0, 300.0),
                                egui::vec2(680.0, 380.0) * z,
                            );
                            let layout =
                                blister_layout(host, z, icons.len(), BlisterPresentation::Overlay);
                            let painter = ctx.layer_painter(egui::LayerId::new(
                                egui::Order::Foreground,
                                egui::Id::new("media-changing-zoom-paint"),
                            ));
                            for (index, &icon) in icons.iter().enumerate() {
                                layout.paint_icon(&painter, index, icon, theme.ink, theme);
                            }
                        },
                    );
                    let paint_us = start.elapsed().as_micros();
                    let (meshes, vertices, triangles) =
                        output
                            .shapes
                            .iter()
                            .fold((0, 0, 0), |counts, shape| match &shape.shape {
                                egui::Shape::Mesh(mesh) => (
                                    counts.0 + 1,
                                    counts.1 + mesh.vertices.len(),
                                    counts.2 + mesh.indices.len() / 3,
                                ),
                                _ => counts,
                            });
                    assert_eq!(meshes, 7, "all seven visible image glyphs were painted");
                    assert!(triangles > 0);
                    frames.push(serde_json::json!({
                        "ppp": ppp, "frame": frame, "zoom": z,
                        "phase": if frame <= 20 { "outbound-new-size" } else { "returning-size" },
                        "repeated_size": repeated_size, "paint_us": paint_us,
                        "meshes": meshes, "vertices": vertices, "triangles": triangles,
                    }));
                }
                frames
            })
            .join()
            .expect("changing-zoom paint worker"),
        );
    }
    let evidence = serde_json::json!({
        "source": "production BlisterLayout::paint_icon overlay, seven actual image glyphs",
        "timed": "Context::run including layout and icon paint; no glyph prewarm",
        "excluded": "Context construction, output counting, serialization, tessellation, GPU and app frame work",
        "cache_scope": "fresh thread/context per DPI; returning sizes are identified, not presumed cache hits",
        "boundary": "shared CPU component measurement only; no timing pass or app/native smoothness inference",
        "frames": records,
    });
    std::fs::write(
        dir.join("changing-zoom-paint.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}
