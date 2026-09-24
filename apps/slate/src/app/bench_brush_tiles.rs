//! Frame-time bench for thousands of committed brush stamps.
//!
//! ```powershell
//! cargo test -p slate --release --lib bench_brush_tiles -- --ignored --nocapture
//! ```
//!
//! The same board is measured twice: once on the per-stroke texture path
//! (`brush_tiles_enabled = false`, the design at 594f389) and once with
//! world-aligned tiles. Release numbers are the ones to keep.

use super::*;
use eframe::egui::{Pos2, Rect as ERect, Vec2 as EVec2};
use slate_doc::scene::{
    Corner, Dash, PathData, PathSeg, Rgba, ShapeKind, ShapeNode, Stroke, StrokeCap, StrokeJoin,
    WidthProfile, WorldRect,
};
use slate_doc::{Node, NodeKind, ViewKind};
use std::time::{Duration, Instant};

const STROKES: usize = 5_000;

struct Bench {
    ctx: egui::Context,
    app: SlateApp,
}

impl Bench {
    fn new(n: usize) -> Self {
        let ctx = egui::Context::default();
        let mut app = SlateApp::with_ctx(&ctx, None);
        app.kits = kits::KitState::builtin_only();
        app.leave_home();
        app.ensure_work_tab();
        app.doc_mut().view.active_view = ViewKind::Board;
        let mut nodes = Vec::with_capacity(n);
        for i in 0..n {
            nodes.push(stroke_node(&mut app.doc_mut().scene, i as u64 + 1));
        }
        app.add_nodes(nodes);
        let cam = &mut app.tab_mut().cam;
        cam.z = 1.0;
        cam.offset = EVec2::new(900.0, 500.0);
        Self { ctx, app }
    }

    fn frame(&mut self) -> Duration {
        let input = egui::RawInput {
            screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1920.0, 1080.0))),
            ..Default::default()
        };
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let start = Instant::now();
        let _ = ctx.run(input, |c| app.update_app(c));
        start.elapsed()
    }
}

fn stroke_node(scene: &mut slate_doc::scene::Scene, seed: u64) -> Node {
    let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let npts = 20 + (next() % 181) as usize;
    let x = (next() % 1600) as f32;
    let y = (next() % 900) as f32;
    let w = 48.0 + (next() % 160) as f32;
    let h = 28.0 + (next() % 90) as f32;
    let width = 6.0 + (next() % 28) as f32;
    let softness = (next() % 101) as f32 / 100.0;
    let alpha = [255u8, 210, 140, 90][(next() % 4) as usize];
    let mut py = 0.5_f32;
    let mut segs = Vec::with_capacity(npts);
    for i in 0..npts {
        let along = (0.06 + i as f32 / npts as f32 * 0.9).min(0.97);
        py = (py + ((next() % 100) as f32 - 50.0) / 350.0).clamp(0.06, 0.94);
        segs.push(PathSeg::Line { to: [along, py] });
    }
    let stroke = Stroke {
        width,
        color: Rgba([
            (next() % 256) as u8,
            (next() % 256) as u8,
            (next() % 256) as u8,
            alpha,
        ]),
        dash: Dash::Solid,
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        profile: WidthProfile::Uniform,
        softness,
        stamp: true,
        tween_from: None,
    };
    scene.build_node(
        WorldRect::new(x, y, w, h),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke,
            corner: Corner::Square,
            flip: false,
            path: Some(PathData {
                start: [0.06, 0.5],
                segs,
                ..PathData::default()
            }),
            text: None,
        }),
    )
}

fn ms(d: Duration) -> f32 {
    d.as_secs_f32() * 1000.0
}

fn legacy_stats(app: &SlateApp, want: f32) -> (usize, usize, usize) {
    let bytes = app.brush_stamps.values().map(|(_, g)| g.bytes).sum();
    let stale = app
        .brush_stamps
        .values()
        .filter(|(_, g)| (g.wanted_pixel - want).abs() > f32::EPSILON)
        .count();
    (bytes, app.brush_stamps.len(), stale)
}

fn report_tiles(label: &str, app: &SlateApp) {
    let s = &app.brush_tiles.last;
    println!(
        "{label}: gpu {:.1} MB, ready {}, pending {}, fallback {}, individuals {}, settled {}",
        s.gpu_bytes as f32 / (1024.0 * 1024.0),
        s.ready_tiles,
        s.pending_jobs,
        s.drew_fallback,
        s.individuals,
        s.settled
    );
}

/// A few strokes must become exact tiles without staying on the per-stroke path.
#[test]
fn a_few_brush_strokes_settle_into_tiles() {
    let mut b = Bench::new(6);
    b.app.brush_tiles_enabled = true;
    let mut settled = false;
    for _ in 0..90 {
        b.frame();
        if b.app.brush_tiles.last.settled && b.app.brush_tiles.last.ready_tiles > 0 {
            settled = true;
            break;
        }
    }
    assert!(
        settled,
        "tiles did not settle: {:?}",
        (
            b.app.brush_tiles.last.ready_tiles,
            b.app.brush_tiles.last.pending_jobs,
            b.app.brush_tiles.last.individuals,
            b.app.brush_tiles.last.drew_fallback,
        )
    );
    assert!(b.app.brush_tiles.last.gpu_bytes > 0);
}

/// 5,000 strokes: rest, pan, zoom, and one more commit, old path then tiles.
/// Saves the bench board to `SLATE_BRUSH_FIXTURE` so a GUI run can open it.
#[test]
#[ignore]
fn write_brush_fixture() {
    let Ok(path) = std::env::var("SLATE_BRUSH_FIXTURE") else {
        return;
    };
    let bench = Bench::new(STROKES);
    bench
        .app
        .doc()
        .save_to(std::path::Path::new(&path))
        .expect("write the brush fixture");
}

#[test]
#[ignore]
fn bench_brush_five_thousand() {
    let mut b = Bench::new(STROKES);
    println!("built {STROKES} strokes");

    b.app.brush_tiles_enabled = false;
    let first = b.frame();
    println!(
        "legacy first frame (builds missing stamps) {:.1} ms",
        ms(first)
    );
    let rest: Vec<_> = (0..5).map(|_| b.frame()).collect();
    println!(
        "legacy rest frames ms: {}",
        rest.iter()
            .map(|d| format!("{:.2}", ms(*d)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let pan: Vec<_> = (0..5)
        .map(|i| {
            b.app.tab_mut().cam.offset.x += if i % 2 == 0 { 30.0 } else { -30.0 };
            b.frame()
        })
        .collect();
    println!(
        "legacy pan frames ms: {}",
        pan.iter()
            .map(|d| format!("{:.2}", ms(*d)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    b.app.tab_mut().cam.z = 2.0;
    let zoom_start = Instant::now();
    let mut zoom_frames = 0usize;
    let mut stale = usize::MAX;
    for _ in 0..240 {
        zoom_frames += 1;
        let dt = b.frame();
        let (_bytes, count, left) =
            legacy_stats(&b.app, super::board_path::stamp_pixel_for_zoom(2.0, 1.0));
        stale = left;
        if left == 0 {
            println!(
                "legacy zoom settled in {zoom_frames} frames / {:.1} ms (last frame {:.1} ms, {count} textures)",
                ms(zoom_start.elapsed()),
                ms(dt)
            );
            break;
        }
    }
    if stale != 0 {
        let (_bytes, count, left) =
            legacy_stats(&b.app, super::board_path::stamp_pixel_for_zoom(2.0, 1.0));
        println!(
            "legacy zoom NOT settled after {zoom_frames} frames / {:.1} ms — {left} of {count} stamps still stale (3 rebuilds/frame)",
            ms(zoom_start.elapsed())
        );
    }
    let (bytes, count, _) = legacy_stats(&b.app, 0.5);
    println!(
        "legacy textures: {count}, {:.1} MB",
        bytes as f32 / (1024.0 * 1024.0)
    );

    b.app.tab_mut().cam.z = 1.0;
    b.app.brush_stamps.clear();
    b.app.brush_tiles_enabled = true;
    b.app.brush_tiles.clear();
    let tile_first = b.frame();
    println!("tile first frame {:.1} ms", ms(tile_first));
    let tile_start = Instant::now();
    let mut tile_frames = 0usize;
    while tile_start.elapsed() < Duration::from_secs(45) && tile_frames < 2000 {
        tile_frames += 1;
        b.frame();
        if b.app.brush_tiles.last.settled {
            break;
        }
    }
    println!(
        "tile settle {tile_frames} frames / {:.1} ms",
        ms(tile_start.elapsed())
    );
    report_tiles("tile after settle", &b.app);
    let rest: Vec<_> = (0..5).map(|_| b.frame()).collect();
    println!(
        "tile rest frames ms: {}",
        rest.iter()
            .map(|d| format!("{:.2}", ms(*d)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    super::board::brush_prof::begin();
    b.frame();
    println!("tile rest sections ms:");
    for (name, ms) in super::board::brush_prof::take() {
        println!("  {name:<14} {ms:.2}");
    }
    let pan: Vec<_> = (0..5)
        .map(|i| {
            b.app.tab_mut().cam.offset.x += if i % 2 == 0 { 40.0 } else { -40.0 };
            b.frame()
        })
        .collect();
    println!(
        "tile pan frames ms: {}",
        pan.iter()
            .map(|d| format!("{:.2}", ms(*d)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    report_tiles("tile after pan", &b.app);

    b.app.tab_mut().cam.z = 2.0;
    let zoom_start = Instant::now();
    let mut zoom_frames = 0usize;
    while zoom_start.elapsed() < Duration::from_secs(45) && zoom_frames < 20_000 {
        zoom_frames += 1;
        b.frame();
        if b.app.brush_tiles.last.settled {
            break;
        }
    }
    println!(
        "tile zoom {zoom_frames} frames / {:.1} ms settled {}",
        ms(zoom_start.elapsed()),
        b.app.brush_tiles.last.settled
    );
    report_tiles("tile after zoom", &b.app);

    b.app.tab_mut().cam.z = 1.0;
    let back = Instant::now();
    while back.elapsed() < Duration::from_secs(45) {
        b.frame();
        if b.app.brush_tiles.last.settled {
            break;
        }
    }
    let extra = stroke_node(&mut b.app.doc_mut().scene, 99_001);
    let commit_at = Instant::now();
    b.app.add_nodes(vec![extra]);
    let commit = commit_at.elapsed();
    let show = b.frame();
    println!(
        "tile commit one more stroke: journal {:.2} ms, next frame {:.2} ms, individuals {}, settled {}",
        ms(commit),
        ms(show),
        b.app.brush_tiles.last.individuals,
        b.app.brush_tiles.last.settled
    );
    report_tiles("tile after commit", &b.app);
}
