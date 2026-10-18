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
                        assert_eq!(pixels.get_pixel(x, y).0[3], 0,
                            "Ellipse counter covers host pixels at {size}/{ppp}, dark={dark}, {x}/{y}");
                    }
                }
            }
        }
    }
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
        "geometry_cost": cold_geometry_metrics(),
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

fn cold_geometry_metrics() -> Vec<serde_json::Value> {
    let mut records = Vec::new();
    for &icon in CASES {
        for &size in SIZES {
            for ppp in [1.0, 1.5, 2.0] {
                records.push(
                    std::thread::spawn(move || {
                        // Each fresh thread starts with an empty production geometry
                        // cache. Thread creation and PNG rasterization are not timed.
                        let start = std::time::Instant::now();
                        let mesh = density::media::mesh(icon, size * ppp);
                        let cold_us = start.elapsed().as_micros();
                        let start = std::time::Instant::now();
                        let warm = density::media::mesh(icon, size * ppp);
                        serde_json::json!({
                            "icon": icon.name(), "slot_points": size, "ppp": ppp,
                            "cold_us": cold_us, "warm_us": start.elapsed().as_micros(),
                            "vertices": mesh.vertices.len(), "triangles": mesh.indices.len() / 3,
                            "warm_reused_geometry": Arc::ptr_eq(&mesh, &warm),
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
