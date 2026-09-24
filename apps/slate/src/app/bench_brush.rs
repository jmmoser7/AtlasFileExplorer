//! Brush / eraser frame and commit cost on a board of stamped strokes.
//!
//! ```powershell
//! cargo test -p slate --lib bench_brush -- --ignored --nocapture
//! ```
//!
//! Dev profile is the iteration measurement. `--release` is the number to
//! compare with the session log.

use super::board::{brush_prof, BoardTool};
use super::tests::Harness;
use eframe::egui::{self, Pos2};
use slate_doc::scene::{
    NodeKind, PathData, PathSeg, Rgba, ShapeKind, ShapeNode, Stroke, StrokeCap, StrokeJoin,
    WidthProfile, WorldRect,
};
use std::time::Instant;

const STROKES: usize = 5_000;

fn board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.frame();
    h
}

fn pointer_at(pos: Pos2) -> impl FnOnce(&mut egui::RawInput) {
    move |input: &mut egui::RawInput| {
        input.events.push(egui::Event::PointerMoved(pos));
    }
}

fn hover_frame(h: &mut Harness, pos: Pos2) {
    h.frame_with(pointer_at(pos));
}

fn median_ms(mut samples: Vec<f32>) -> f32 {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[samples.len() / 2]
}

fn time_frames(h: &mut Harness, pos: Pos2, n: usize) -> f32 {
    let mut samples = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        hover_frame(h, pos);
        samples.push(t.elapsed().as_secs_f32() * 1000.0);
    }
    median_ms(samples)
}

fn stamp_stroke(i: usize) -> (WorldRect, ShapeNode) {
    let x = ((i * 37) % 180) as f32 * 22.0 - 1800.0;
    let y = ((i * 19) % 120) as f32 * 18.0 - 1000.0;
    let w = 28.0 + (i % 17) as f32;
    let h = 16.0 + (i % 11) as f32;
    let rect = WorldRect::new(x, y, w, h);
    let path = PathData {
        start: [0.1, 0.8],
        segs: vec![
            PathSeg::Cubic {
                c1: [0.3, 0.1],
                c2: [0.6, 0.2],
                to: [0.9, 0.7],
            },
            PathSeg::Line { to: [0.5, 0.4] },
        ],
        closed: false,
        ..PathData::default()
    };
    let shape = ShapeNode {
        shape: ShapeKind::Path,
        fill: None,
        stroke: Stroke {
            width: 8.0 + (i % 5) as f32,
            color: Rgba([20, 20, 20, 220]),
            dash: slate_doc::scene::Dash::Solid,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            profile: WidthProfile::Uniform,
            softness: if i % 4 == 0 { 0.35 } else { 0.0 },
            stamp: true,
            tween_from: None,
        },
        corner: slate_doc::scene::Corner::Square,
        flip: false,
        path: Some(path),
        text: None,
    };
    (rect, shape)
}

fn seed_strokes(h: &mut Harness, n: usize) {
    for i in 0..n {
        let (rect, shape) = stamp_stroke(i);
        let node = h
            .app
            .doc_mut()
            .scene
            .build_node(rect, NodeKind::Shape(shape));
        h.app.doc_mut().scene.nodes.push(node);
    }
    h.app.note_scene_change();
}

fn center(h: &Harness) -> Pos2 {
    h.app.canvas_rect.center()
}

#[test]
fn stroke_commit_does_not_rebuild_the_spatial_index() {
    let mut h = board("brush_commit_index");
    seed_strokes(&mut h, 64);
    let _ = h.app.doc().scene.query_point(0.0, 0.0);
    let rebuilds = h.app.doc().scene.spatial_rebuilds();
    assert!(rebuilds >= 1);
    h.app.finish_freehand_brush(vec![
        Pos2::new(4.0, 4.0),
        Pos2::new(30.0, 12.0),
        Pos2::new(48.0, 6.0),
    ]);
    let added = h.app.doc().scene.nodes.last().unwrap().id;
    let center = {
        let r = h.app.doc().scene.node(added).unwrap().rect;
        Pos2::new(r.x + r.w * 0.5, r.y + r.h * 0.5)
    };
    assert!(h
        .app
        .doc()
        .scene
        .query_point(center.x, center.y)
        .contains(&added));
    assert_eq!(h.app.doc().scene.spatial_rebuilds(), rebuilds);
    assert_eq!(h.app.doc().scene.nodes.len(), 65);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 64);
    assert_eq!(h.app.doc().scene.spatial_rebuilds(), rebuilds);
}

#[test]
fn eraser_hits_only_the_stroke_under_the_tip() {
    let mut h = board("eraser_index");
    seed_strokes(&mut h, 200);
    let line = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 80.0, 4.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Line,
            fill: None,
            stroke: Stroke {
                width: 2.0,
                color: Rgba([0, 0, 0, 255]),
                dash: slate_doc::scene::Dash::Solid,
                cap: StrokeCap::Round,
                join: StrokeJoin::Round,
                profile: WidthProfile::Uniform,
                softness: 0.0,
                stamp: false,
                tween_from: None,
            },
            corner: slate_doc::scene::Corner::Square,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let line_id = line.id;
    h.app.doc_mut().scene.nodes.push(line);
    h.app.note_scene_change();
    h.app.eraser_width = 8.0;
    let hits = h.app.eraser_hits_at(Pos2::new(40.0, 2.0));
    assert_eq!(hits, vec![line_id]);
}

#[test]
#[ignore]
fn bench_brush_input() {
    let profile = if cfg!(debug_assertions) {
        "dev"
    } else {
        "release"
    };
    println!("profile={profile} strokes={STROKES}");
    {
        let mut probe = board("brush_bench_probe");
        let r = probe.app.canvas_rect;
        println!(
            "canvas {}x{} home={} view={:?}",
            r.width(),
            r.height(),
            probe.app.at_home,
            probe.app.doc().view.active_view
        );
        probe.app.bump_grid_fade(1.0);
        probe.app.set_board_tool(BoardTool::Brush);
        let pos = center(&probe);
        brush_prof::begin();
        hover_frame(&mut probe, pos);
        println!("grid-bumped brush sections:");
        for (name, ms) in brush_prof::take() {
            println!("  {name:12} {ms:.2}");
        }
    }

    let mut empty = board("brush_bench_empty");
    let pos = center(&empty);
    empty.app.set_board_tool(BoardTool::Select);
    let select_idle = time_frames(&mut empty, pos, 7);
    empty.app.set_board_tool(BoardTool::Brush);
    brush_prof::begin();
    let brush_empty = {
        let t = Instant::now();
        hover_frame(&mut empty, pos);
        t.elapsed().as_secs_f32() * 1000.0
    };
    println!("empty brush section ms:");
    for (name, ms) in brush_prof::take() {
        println!("  {name:12} {ms:.2}");
    }
    let brush_empty_med = time_frames(&mut empty, pos, 7);
    println!("empty select idle median {select_idle:.2} ms");
    println!("empty brush idle first {brush_empty:.2} ms median {brush_empty_med:.2} ms");

    let mut full = board("brush_bench_full");
    seed_strokes(&mut full, STROKES);
    let pos = center(&full);
    full.app.set_board_tool(BoardTool::Brush);
    // Warm caches (spatial index, stamp textures) then measure.
    hover_frame(&mut full, pos);
    hover_frame(&mut full, pos);
    brush_prof::begin();
    hover_frame(&mut full, pos);
    println!("5000 brush idle section ms:");
    for (name, ms) in brush_prof::take() {
        println!("  {name:12} {ms:.2}");
    }
    let idle = time_frames(&mut full, pos, 5);
    let moved = pos + egui::vec2(12.0, -8.0);
    let move_ms = time_frames(&mut full, moved, 5);

    let before = full.app.doc().scene.nodes.len();
    let t = Instant::now();
    full.app.finish_freehand_brush(vec![
        Pos2::new(10.0, 10.0),
        Pos2::new(40.0, 18.0),
        Pos2::new(70.0, 12.0),
        Pos2::new(110.0, 40.0),
    ]);
    let commit_ms = t.elapsed().as_secs_f32() * 1000.0;
    assert_eq!(full.app.doc().scene.nodes.len(), before + 1);

    full.app.set_board_tool(BoardTool::Eraser);
    let world = full.app.board_xf().s2w(pos);
    full.app.board_drag = Some(full.app.begin_erase(world, false));
    let eraser_ms = time_frames(&mut full, pos + egui::vec2(6.0, 4.0), 5);

    let t = Instant::now();
    full.app.board_undo();
    let undo_ms = t.elapsed().as_secs_f32() * 1000.0;
    assert_eq!(full.app.doc().scene.nodes.len(), before);

    println!("brush idle median {idle:.2} ms");
    println!("pointer move median {move_ms:.2} ms");
    println!("one stroke commit {commit_ms:.2} ms");
    println!("eraser drag frame median {eraser_ms:.2} ms");
    println!("undo one stroke {undo_ms:.2} ms");
}
