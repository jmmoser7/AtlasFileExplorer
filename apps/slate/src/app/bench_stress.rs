//! Repeatable mixed-board CPU stress audit, including egui tessellation.
//!
//! `cargo test --locked --release -p slate --lib bench_mixed_board -- --ignored --nocapture --test-threads=1`
//! Optional: SLATE_STRESS_COUNTS=100,1000,5000; SLATE_STRESS_FRAMES=120;
//! SLATE_STRESS_LABEL=baseline; SLATE_STRESS_DOWNLOADS=1 includes resident assets.
//! Outputs fixtures and raw JSON under target/slate-perf-audit. No GPU submission
//! is measured: use bench_model3d for live 3D and delivered-frame validation.

use super::board::BoardTool;
use super::tests::Harness;
use eframe::egui::{self, Pos2, Vec2};
use slate_doc::scene::{
    Corner, FrameNode, ImageNode, NodeKind, PathData, PathSeg, Rgba, ShapeKind, ShapeNode, Stroke,
    StrokeCap, StrokeJoin, TextNode, WorldRect,
};
use slate_doc::ItemId;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod cache_probe;
mod imports;
mod input;
mod reporting;
use reporting::{persist, report};

#[derive(serde::Serialize)]
struct Sample {
    app_ms: f64,
    tessellation_ms: f64,
    cpu_ms: f64,
    meshes: usize,
    vertices: usize,
    indices: usize,
    texture_upload_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    path_mesh: Option<super::board_path::cache_probe::Snapshot>,
    camera: [f32; 3],
}

#[derive(serde::Serialize)]
struct ResultRow {
    nodes: usize,
    scenario: String,
    mutation_ms: f64,
    samples: Vec<Sample>,
}

fn frame(h: &mut Harness, events: Vec<egui::Event>) -> Sample {
    h.app.path_mesh_cache.begin_probe_frame();
    let start = Instant::now();
    let out = h.frame_output(|input| input.events = events);
    let app_ms = start.elapsed().as_secs_f64() * 1000.0;
    let upload = out
        .textures_delta
        .set
        .iter()
        .map(|(_, d)| {
            let [w, h] = d.image.size();
            w * h * 4
        })
        .sum();
    let tessellation = Instant::now();
    let meshes = h.ctx.tessellate(out.shapes, out.pixels_per_point);
    let tessellation_ms = tessellation.elapsed().as_secs_f64() * 1000.0;
    let (mut vertices, mut indices) = (0, 0);
    for mesh in &meshes {
        if let egui::epaint::Primitive::Mesh(mesh) = &mesh.primitive {
            vertices += mesh.vertices.len();
            indices += mesh.indices.len();
        }
    }
    Sample {
        app_ms,
        tessellation_ms,
        cpu_ms: start.elapsed().as_secs_f64() * 1000.0,
        meshes: meshes.len(),
        vertices,
        indices,
        texture_upload_bytes: upload,
        path_mesh: h.app.path_mesh_cache.probe_snapshot(),
        camera: [
            h.app.tab().cam.offset.x,
            h.app.tab().cam.offset.y,
            h.app.tab().cam.z,
        ],
    }
}

fn synthetic_assets(base: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    std::fs::create_dir_all(base).unwrap();
    let images = (0..24)
        .map(|i| {
            let p = base.join(format!("image-{i:02}.png"));
            if !p.exists() {
                let size = [128, 512, 1024][i % 3];
                image::RgbaImage::from_fn(size, size, |x, y| {
                    image::Rgba([
                        (x.wrapping_mul(13) + i as u32 * 31) as u8,
                        (y.wrapping_mul(7) + i as u32 * 17) as u8,
                        ((x ^ y).wrapping_mul(3)) as u8,
                        255,
                    ])
                })
                .save(&p)
                .unwrap();
            }
            p
        })
        .collect();
    let mut models = Vec::new();
    for side in [16usize, 128] {
        let path = base.join(format!("grid-{side}.obj"));
        if !path.exists() {
            let mut f = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
            for y in 0..=side {
                for x in 0..=side {
                    writeln!(f, "v {x} {y} {}", ((x * 3 + y * 7) % 13) as f32 * 0.1).unwrap();
                }
            }
            for y in 0..side {
                for x in 0..side {
                    let a = y * (side + 1) + x + 1;
                    let b = a + side + 1;
                    writeln!(f, "f {a} {} {b}\nf {} {} {b}", a + 1, a + 1, b + 1).unwrap();
                }
            }
        }
        models.push(path);
    }
    (images, models)
}

fn resident_downloads() -> (Vec<PathBuf>, Vec<PathBuf>) {
    if std::env::var_os("SLATE_STRESS_DOWNLOADS").is_none() {
        return (Vec::new(), Vec::new());
    }
    let Some(home) = std::env::var_os("USERPROFILE") else {
        return (Vec::new(), Vec::new());
    };
    let dir = PathBuf::from(home).join("Downloads");
    let mut images = Vec::new();
    let mut models = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (images, models);
    };
    let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "obj" | "stl" | "3dm" | "gltf" | "glb"
        ) {
            continue;
        }
        if atlas_core::cloud::is_dehydrated(&path) {
            continue;
        }
        let Ok(meta) = path.metadata() else {
            continue;
        };
        // Bound the audit's imports, independently of its number of placed nodes.
        if !meta.is_file() || meta.len() > 200_000_000 {
            continue;
        }
        if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
            if images.len() < 8 {
                images.push(path);
            }
        } else if models.len() < 10 {
            models.push(path);
        }
    }
    (images, models)
}

fn stroke(i: usize, brush: bool) -> NodeKind {
    let segments = 24 + i % 72;
    let path = PathData {
        start: [0.0, 0.5],
        segs: (1..=segments)
            .map(|j| PathSeg::Line {
                to: [
                    j as f32 / segments as f32,
                    0.5 + ((j * 13 + i) as f32 * 0.31).sin() * 0.35,
                ],
            })
            .collect(),
        ..Default::default()
    };
    NodeKind::Shape(ShapeNode {
        shape: ShapeKind::Path,
        fill: None,
        stroke: Stroke {
            width: if brush { 6.0 + (i % 5) as f32 } else { 2.0 },
            color: Rgba([25, 90, 150, if i.is_multiple_of(3) { 140 } else { 255 }]),
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            stamp: brush,
            softness: if brush { 0.25 } else { 0.0 },
            ..Stroke::default()
        },
        corner: Corner::Square,
        sides: slate_doc::scene::default_regular_sides(),
        phase_deg: 0.0,
        flip: false,
        path: Some(path.into()),
        text: None,
    })
}

fn seed(h: &mut Harness, n: usize, image_items: &[ItemId], model_items: &[ItemId]) -> WorldRect {
    let columns = (n as f32).sqrt().ceil() as usize;
    let mut nodes = Vec::with_capacity(n);
    for i in 0..n {
        let mut rect = WorldRect::new(
            (i % columns) as f32 * 200.0,
            (i / columns) as f32 * 140.0,
            175.0,
            110.0,
        );
        let kind = match i % 20 {
            0 => {
                rect.w = 195.0;
                rect.h = 135.0;
                NodeKind::Frame(FrameNode {
                    title: format!("Frame {i}"),
                    order: (i / 20) as u32,
                    fill: Rgba::WHITE,
                    fill_authored: false,
                    assignments: Default::default(),
                    stroke: Stroke::none(),
                    corner: Corner::Square,
                })
            }
            1 => NodeKind::Text(TextNode {
                text: format!("Review {i}\nMixed board image model pen brush frame"),
                style: Default::default(),
                family: Default::default(),
                size: 16.0,
                color: Rgba::BLACK,
                align: Default::default(),
                fill: None,
                stroke: Stroke::none(),
                agent: None,
            }),
            2..=3 => NodeKind::Image(ImageNode::new(model_items[i % model_items.len()])),
            4..=11 => NodeKind::Image(ImageNode::new(image_items[i % image_items.len()])),
            12..=16 => stroke(i, true),
            _ => stroke(i, false),
        };
        nodes.push(h.app.doc_mut().scene.build_node(rect, kind));
    }
    h.app.add_nodes(nodes);
    WorldRect::new(
        0.0,
        0.0,
        columns as f32 * 200.0,
        n.div_ceil(columns) as f32 * 140.0,
    )
}

fn click(p: Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(p),
        egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        },
    ]
}

#[test]
#[ignore]
fn bench_mixed_board() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/slate-perf-audit");
    let (mut image_paths, mut model_paths) = synthetic_assets(&base.join("assets"));
    let (images, models) = resident_downloads();
    image_paths.extend(images);
    model_paths.extend(models);
    let counts: Vec<usize> = std::env::var("SLATE_STRESS_COUNTS")
        .unwrap_or_else(|_| "100,1000,5000".into())
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    let frames: usize = std::env::var("SLATE_STRESS_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);
    assert!(frames >= 10 && counts.iter().all(|n| *n >= 20));
    let mut rows = Vec::new();
    for count in counts {
        let mut h = Harness::new("mixed_perf");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        let images = h.app.add_paths(&image_paths);
        let models = h.app.add_paths(&model_paths);
        assert!(!images.is_empty() && !models.is_empty());
        let start = Instant::now();
        let bounds = seed(&mut h, count, &images, &models);
        let seed_ms = start.elapsed().as_secs_f64() * 1000.0;
        h.app
            .doc()
            .save_to(&base.join(format!("mixed-{count}.slate")))
            .unwrap();
        report(
            &mut rows,
            count,
            "app_cold_first_paint",
            vec![frame(&mut h, Vec::new())],
            seed_ms,
        );
        let mut warm = Vec::new();
        for _ in 0..120 {
            warm.push(frame(&mut h, Vec::new()));
            std::thread::sleep(Duration::from_millis(2));
        }
        report(&mut rows, count, "warmup", warm, 0.0);
        report(
            &mut rows,
            count,
            "idle_detail",
            (0..frames).map(|_| frame(&mut h, Vec::new())).collect(),
            0.0,
        );
        let original = h.app.tab().cam;
        report(
            &mut rows,
            count,
            "pan_detail",
            (0..frames)
                .map(|i| {
                    h.app.tab_mut().cam.offset = original.offset
                        + Vec2::new(
                            (i as f32 * 0.07).sin() * 180.0,
                            (i as f32 * 0.04).cos() * 100.0,
                        );
                    frame(&mut h, Vec::new())
                })
                .collect(),
            0.0,
        );
        h.app.tab_mut().cam = original;
        report(
            &mut rows,
            count,
            "zoom_detail",
            (0..frames)
                .map(|i| {
                    h.app.tab_mut().cam.z = original.z * (1.0 + (i as f32 * 0.04).sin() * 0.5);
                    frame(&mut h, Vec::new())
                })
                .collect(),
            0.0,
        );
        h.app.zoom_to_rect(bounds);
        report(
            &mut rows,
            count,
            "overview_first_paint",
            vec![frame(&mut h, Vec::new())],
            0.0,
        );
        for _ in 0..60 {
            frame(&mut h, Vec::new());
            std::thread::sleep(Duration::from_millis(2));
        }
        report(
            &mut rows,
            count,
            "idle_overview",
            (0..frames).map(|_| frame(&mut h, Vec::new())).collect(),
            0.0,
        );
        input::navigation(&mut h, &mut rows, count, frames);
        let canvas = h.app.canvas_rect;
        report(
            &mut rows,
            count,
            "pointer_overview",
            (0..frames)
                .map(|i| {
                    let pos = canvas.min
                        + Vec2::new(80.0 + (i * 37 % 700) as f32, 60.0 + (i * 23 % 500) as f32);
                    frame(&mut h, vec![egui::Event::PointerMoved(pos)])
                })
                .collect(),
            0.0,
        );
        let p = canvas.center();
        report(
            &mut rows,
            count,
            "selection_overview",
            (0..20)
                .flat_map(|i| {
                    let pos = p + Vec2::new((i * 17 % 150) as f32, (i * 29 % 100) as f32);
                    [
                        frame(&mut h, click(pos, true)),
                        frame(&mut h, click(pos, false)),
                    ]
                })
                .collect(),
            0.0,
        );
        h.app.set_board_tool(BoardTool::Brush);
        let start = Instant::now();
        h.app.finish_freehand_brush(
            (0..200)
                .map(|i| {
                    Pos2::new(
                        100.0 + i as f32 * 1.5,
                        100.0 + (i as f32 * 0.2).sin() * 45.0,
                    )
                })
                .collect(),
        );
        let commit = start.elapsed().as_secs_f64() * 1000.0;
        report(
            &mut rows,
            count,
            "brush_commit_followup",
            (0..30).map(|_| frame(&mut h, Vec::new())).collect(),
            commit,
        );
        h.app.set_board_tool(BoardTool::Select);
        let start = Instant::now();
        h.app.board_undo();
        let undo = start.elapsed().as_secs_f64() * 1000.0;
        report(
            &mut rows,
            count,
            "undo_followup",
            (0..10).map(|_| frame(&mut h, Vec::new())).collect(),
            undo,
        );
        let start = Instant::now();
        h.app.board_redo();
        let redo = start.elapsed().as_secs_f64() * 1000.0;
        report(
            &mut rows,
            count,
            "redo_followup",
            (0..10).map(|_| frame(&mut h, Vec::new())).collect(),
            redo,
        );
        persist(&base, &rows, &image_paths, &model_paths);
    }
}
