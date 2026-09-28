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

    /// A bench whose board paints are logged for coverage checks.
    fn logged(n: usize) -> Self {
        super::board_path::tiles::log_paints();
        let mut b = Self::new(n);
        b.app.brush_tiles_enabled = true;
        b
    }

    fn frame(&mut self) -> Duration {
        self.frame_with(Vec::new())
    }

    fn frame_with(&mut self, events: Vec<egui::Event>) -> Duration {
        let start = Instant::now();
        let _ = self.frame_out(events);
        start.elapsed()
    }

    fn frame_out(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1920.0, 1080.0))),
            events,
            ..Default::default()
        };
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        ctx.run(input, |c| app.update_app(c))
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
        gaussian_blur: 0.0,
        arrow_end: false,
        texture: Default::default(),
    };
    scene.build_node(
        WorldRect::new(x, y, w, h),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke,
            corner: Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(std::sync::Arc::new(PathData {
                start: [0.06, 0.5],
                segs,
                ..PathData::default()
            })),
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
    // Tiles rasterize on worker threads; a busy CI runner needs wall time, not frames.
    let mut settled = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        b.frame();
        if b.app.brush_tiles.last.settled && b.app.brush_tiles.last.ready_tiles > 0 {
            settled = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
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

/// Tiles stamp without a blur pass, so a blurred stroke paints from its own
/// blurred raster (the one the HTML artifact embeds), never from a tile.
#[test]
fn a_blurred_brush_stroke_paints_with_its_blur() {
    let mut b = Bench::new(6);
    b.app.brush_tiles_enabled = true;
    let id = b.app.doc().scene.nodes[0].id;
    if let NodeKind::Shape(shape) = &mut b.app.doc_mut().scene.nodes[0].kind {
        shape.stroke.gaussian_blur = 6.0;
    }
    b.app.note_scene_change();
    assert!(settle(&mut b), "tiles did not settle");
    assert!(
        b.app.brush_tiles.tiles_with(id).is_empty(),
        "a tile baked the blurred stroke without its blur"
    );
    assert!(
        b.app.brush_stamps.contains_key(&b.app.stroke_cache_id(id)),
        "the blurred stroke has no raster of its own"
    );
}

fn settle(b: &mut Bench) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        b.frame();
        if b.app.brush_tiles.last.settled && b.app.brush_tiles.last.ready_tiles > 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn primary(pos: Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        },
    ]
}

/// Drag one brush stroke across the canvas center with real pointer input.
fn drag_brush_stroke(b: &mut Bench) -> slate_doc::NodeId {
    b.app.set_board_tool(super::board::BoardTool::Brush);
    b.frame();
    let c = b.app.canvas_rect.center();
    let start = c + EVec2::new(-90.0, -20.0);
    b.frame_with(primary(start, true));
    for i in 1..=12 {
        let p = start + EVec2::new(i as f32 * 15.0, (i as f32 * 0.6).sin() * 25.0);
        b.frame_with(vec![egui::Event::PointerMoved(p)]);
    }
    let before = b.app.doc().scene.nodes.len();
    b.frame_with(primary(start + EVec2::new(180.0, 0.0), false));
    assert_eq!(
        b.app.doc().scene.nodes.len(),
        before + 1,
        "stroke committed"
    );
    b.app.doc().scene.nodes.last().unwrap().id
}

/// Committing a stroke must not blank the strokes already on screen: every
/// frame from the release until the new tiles land draws each earlier stroke,
/// and tiles the new stroke does not touch keep their textures.
#[test]
fn committing_a_stroke_keeps_earlier_strokes_on_screen() {
    let mut b = Bench::logged(40);
    assert!(settle(&mut b), "fixture tiles did not settle");
    let before = b.app.brush_tiles.drawn_ids();
    assert!(before.len() >= 10, "fixture drew {} strokes", before.len());
    let textures = b.app.brush_tiles.tile_textures();

    let added = drag_brush_stroke(&mut b);
    let mut frame = 0;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let drawn = b.app.brush_tiles.drawn_ids();
        let lost = before.difference(&drawn).count();
        assert_eq!(
            lost,
            0,
            "frame {frame} after the commit left {lost} of {} earlier strokes undrawn",
            before.len()
        );
        if b.app.brush_tiles.last.settled || Instant::now() > deadline {
            break;
        }
        frame += 1;
        std::thread::sleep(Duration::from_millis(5));
        b.frame();
    }
    assert!(b.app.brush_tiles.last.settled, "tiles did not settle");
    let touched = b.app.brush_tiles.tiles_with(added);
    let after = b.app.brush_tiles.tile_textures();
    let kept = textures
        .iter()
        .filter(|(key, _)| !touched.contains(*key))
        .filter(|(key, tex)| after.get(*key) == Some(*tex))
        .count();
    let untouched = textures.keys().filter(|k| !touched.contains(*k)).count();
    assert_eq!(
        kept, untouched,
        "tiles away from the new stroke were rebuilt"
    );
}

/// Erasing across a stroke must not blank the strokes the eraser never
/// reached: every frame of the pass and after the release draws each of them.
#[test]
fn erasing_keeps_untouched_strokes_on_screen() {
    let mut b = Bench::logged(40);
    assert!(settle(&mut b), "fixture tiles did not settle");
    let before = b.app.brush_tiles.drawn_ids();
    let target = b.app.doc().scene.nodes[20].clone();
    let at = Pos2::new(
        target.rect.x + 0.06 * target.rect.w,
        target.rect.y + 0.5 * target.rect.h,
    );
    b.app.set_board_tool(super::board::BoardTool::Eraser);
    b.app.eraser_width = 16.0;
    b.frame();
    let xf = b.app.board_xf();
    let start = xf.w2s(at);
    b.frame_with(vec![egui::Event::PointerMoved(start)]);
    b.frame_with(primary(start, true));
    let mut reached: HashSet<slate_doc::NodeId> = HashSet::new();
    let mut check = |b: &Bench, when: &str| {
        if let Some(super::board::BoardDrag::Erase { spot, .. }) = &b.app.board_drag {
            reached.extend(spot.iter().copied());
        }
        let drawn = b.app.brush_tiles.drawn_ids();
        let lost: Vec<_> = before
            .iter()
            .filter(|id| !reached.contains(id) && **id != target.id && !drawn.contains(id))
            .collect();
        assert!(
            lost.is_empty(),
            "{when}: {} strokes the eraser never reached went undrawn",
            lost.len()
        );
    };
    for i in 1..=8 {
        let p = start + EVec2::new(i as f32 * 4.0, 0.0);
        b.frame_with(vec![egui::Event::PointerMoved(p)]);
        check(&b, &format!("move {i}"));
    }
    b.frame_with(primary(start + EVec2::new(32.0, 0.0), false));
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut frame = 0;
    loop {
        check(&b, &format!("frame {frame} after release"));
        if b.app.brush_tiles.last.settled || Instant::now() > deadline {
            break;
        }
        frame += 1;
        std::thread::sleep(Duration::from_millis(5));
        b.frame();
    }
}

/// A single-dab brush stroke (a path with no segments) centered at `at`.
fn dab_node(
    scene: &mut slate_doc::scene::Scene,
    at: [f32; 2],
    width: f32,
    softness: f32,
    blur: f32,
) -> Node {
    let stroke = Stroke {
        width,
        color: Rgba([30, 90, 200, 255]),
        dash: Dash::Solid,
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        profile: WidthProfile::Uniform,
        softness,
        stamp: true,
        tween_from: None,
        gaussian_blur: blur,
        arrow_end: false,
        texture: Default::default(),
    };
    scene.build_node(
        WorldRect::new(at[0] - width * 0.5, at[1] - width * 0.5, width, width),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke,
            corner: Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(std::sync::Arc::new(PathData {
                start: [0.5, 0.5],
                segs: Vec::new(),
                ..PathData::default()
            })),
            text: None,
        }),
    )
}

fn view_world(b: &Bench) -> [f32; 4] {
    let xf = b.app.board_xf();
    let a = xf.s2w(b.app.canvas_rect.min);
    let c = xf.s2w(b.app.canvas_rect.max);
    [a.x, a.y, c.x, c.y]
}

/// Samples of each stroke that the last paint left bare or drew twice.
fn coverage_faults(b: &Bench, ids: impl IntoIterator<Item = slate_doc::NodeId>) -> Vec<String> {
    use super::board_path::tiles::{coverage, stroke_ink_rect};
    let view = view_world(b);
    let mut out = Vec::new();
    for id in ids {
        let Some(ink) = b.app.doc().scene.node(id).and_then(stroke_ink_rect) else {
            continue;
        };
        let region = [
            ink[0].max(view[0]),
            ink[1].max(view[1]),
            ink[2].min(view[2]),
            ink[3].min(view[3]),
        ];
        if region[0] >= region[2] || region[1] >= region[3] {
            continue;
        }
        let (n, bare, doubled) = coverage(id, region, 12);
        if bare > 0 || doubled > 0 {
            out.push(format!("{id:?}: {bare} bare, {doubled} doubled of {n}"));
        }
    }
    out
}

/// The hand-off from the live brush preview to tiles never skips a frame:
/// from the release until the tiles land, the new stroke is painted exactly
/// once everywhere it reaches. The view is empty board centered on a tile
/// corner, so the stroke needs four new tiles that land over several frames.
#[test]
fn a_committed_stroke_paints_every_frame_until_its_tiles_land() {
    let mut b = Bench::logged(40);
    assert!(settle(&mut b), "fixture tiles did not settle");
    b.app.tab_mut().cam.offset = EVec2::new(5120.0, 512.0);
    assert!(settle(&mut b), "empty view did not settle");

    let added = drag_brush_stroke(&mut b);
    let mut frame = 0;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let faults = coverage_faults(&b, [added]);
        assert!(
            faults.is_empty(),
            "frame {frame} after the release: {faults:?}"
        );
        if b.app.brush_tiles.last.settled || Instant::now() > deadline {
            break;
        }
        frame += 1;
        std::thread::sleep(Duration::from_millis(2));
        b.frame();
    }
    assert!(b.app.brush_tiles.last.settled, "tiles did not settle");
    assert!(
        !b.app.brush_tiles.tiles_with(added).is_empty(),
        "the new stroke never reached a tile"
    );
}

/// During an erase drag and after its release, every stroke on screen is
/// painted exactly once each frame: from its old tile, its new tile, or its
/// live eraser preview, never none and never two of them.
#[test]
fn erasing_paints_every_stroke_exactly_once_each_frame() {
    let mut b = Bench::logged(40);
    assert!(settle(&mut b), "fixture tiles did not settle");
    let target = b.app.doc().scene.nodes[20].clone();
    let at = Pos2::new(
        target.rect.x + 0.06 * target.rect.w,
        target.rect.y + 0.5 * target.rect.h,
    );
    b.app.set_board_tool(super::board::BoardTool::Eraser);
    b.app.eraser_width = 16.0;
    b.frame();
    let start = b.app.board_xf().w2s(at);
    b.frame_with(vec![egui::Event::PointerMoved(start)]);
    b.frame_with(primary(start, true));
    let check = |b: &Bench, when: &str| {
        let ids: Vec<_> = b.app.doc().scene.nodes.iter().map(|n| n.id).collect();
        let faults = coverage_faults(b, ids);
        assert!(faults.is_empty(), "{when}: {faults:?}");
    };
    check(&b, "press");
    for i in 1..=24 {
        let p = start + EVec2::new(i as f32 * 6.0, (i as f32 * 0.5).sin() * 20.0);
        b.frame_with(vec![egui::Event::PointerMoved(p)]);
        check(&b, &format!("move {i}"));
    }
    b.frame_with(primary(start + EVec2::new(144.0, 0.0), false));
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut frame = 0;
    loop {
        check(&b, &format!("frame {frame} after release"));
        if b.app.brush_tiles.last.settled || Instant::now() > deadline {
            break;
        }
        frame += 1;
        std::thread::sleep(Duration::from_millis(2));
        b.frame();
    }
    assert!(b.app.brush_tiles.last.settled, "tiles did not settle");
}

/// A stroke that leaves the tiles (selected, or under the eraser) splits
/// its run in two. The run that keeps the old cache never paints a tile at
/// a coordinate its own strokes no longer reach: once the other half's new
/// tile lands there, each stroke still paints exactly once.
#[test]
fn a_split_run_never_paints_its_old_tiles_over_the_other_half() {
    let mut b = Bench::logged(0);
    let scene = &mut b.app.doc_mut().scene;
    let p = dab_node(scene, [100.0, 100.0], 24.0, 0.0, 0.0);
    let x = dab_node(scene, [700.0, 100.0], 24.0, 0.0, 0.0);
    let q = dab_node(scene, [760.0, 100.0], 24.0, 0.0, 0.0);
    let ids = [p.id, x.id, q.id];
    b.app.add_nodes(vec![p, x, q]);
    assert!(settle(&mut b), "fixture tiles did not settle");
    assert!(
        ids.iter()
            .all(|id| !b.app.brush_tiles.tiles_with(*id).is_empty()),
        "fixture strokes not tiled"
    );

    b.app.board_sel.insert(ids[1]);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut frame = 0;
    loop {
        b.frame();
        let faults = coverage_faults(&b, ids);
        assert!(
            faults.is_empty(),
            "frame {frame} after the split: {faults:?}"
        );
        if b.app.brush_tiles.last.settled || Instant::now() > deadline {
            break;
        }
        frame += 1;
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(b.app.brush_tiles.last.settled, "tiles did not settle");
    for f in 0..3 {
        b.frame();
        let faults = coverage_faults(&b, ids);
        assert!(faults.is_empty(), "settled frame {f}: {faults:?}");
    }
}

/// A horizontal brush bar `length` long, centered at `at`.
fn bar_node(
    scene: &mut slate_doc::scene::Scene,
    at: [f32; 2],
    length: f32,
    width: f32,
    blur: f32,
) -> Node {
    let mut node = dab_node(scene, at, width, 0.0, blur);
    node.rect = WorldRect::new(at[0] - length * 0.5, at[1] - width * 0.5, length, width);
    if let NodeKind::Shape(shape) = &mut node.kind {
        shape.path = Some(std::sync::Arc::new(PathData {
            start: [0.0, 0.5],
            segs: vec![PathSeg::Line { to: [1.0, 0.5] }],
            ..PathData::default()
        }));
    }
    node
}

fn mesh_textures(shape: &egui::Shape, out: &mut Vec<egui::TextureId>) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| mesh_textures(s, out)),
        egui::Shape::Mesh(mesh) => out.push(mesh.texture_id),
        _ => {}
    }
}

/// A stroke under the eraser keeps its look. Every frame of the drag it is
/// painted once, from its live preview, and the preview is the committed
/// bitmap (blur included) with the pass cut out: untouched pixels are the
/// committed ones, fully covered pixels are transparent. On release the
/// committed raster takes over on the next frame with the preview's pixels.
#[test]
fn an_erased_stroke_previews_as_committed_minus_the_pass() {
    for blur in [8.0_f32, 0.0] {
        let mut b = Bench::logged(0);
        b.frame();
        let view = view_world(&b);
        let center = [(view[0] + view[2]) * 0.5, (view[1] + view[3]) * 0.5];
        let bar = bar_node(&mut b.app.doc_mut().scene, center, 240.0, 30.0, blur);
        let id = bar.id;
        b.app.add_nodes(vec![bar]);
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            b.frame();
            let ready = if blur > 0.0 {
                b.app.brush_stamps.contains_key(&b.app.stroke_cache_id(id))
            } else {
                b.app.brush_tiles.last.settled && !b.app.brush_tiles.tiles_with(id).is_empty()
            };
            if ready {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "blur {blur}: the bar never settled"
            );
            std::thread::sleep(Duration::from_millis(2));
        }

        let node = b.app.doc().scene.node(id).cloned().unwrap();
        let NodeKind::Shape(shape) = &node.kind else {
            unreachable!()
        };
        let pixel = super::board_path::stamp_pixel_for_zoom(b.app.board_xf().z, 1.0);
        let committed =
            super::board_path::stroke_stamp(&node, shape, shape.path.as_ref().unwrap(), pixel)
                .expect("the bar's stamp");
        if blur > 0.0 {
            let (w, h) = (committed.width, committed.height);
            let halo = (0..h).any(|y| {
                (0..w).any(|x| {
                    let wy = committed.origin[1] + (y as f32 + 0.5) * committed.pixel;
                    (wy - center[1]).abs() > 15.0 + 2.0
                        && committed.rgba[((y * w + x) * 4 + 3) as usize] > 0
                })
            });
            assert!(halo, "the fixture's committed bitmap has no blur halo");
        }

        b.app.set_board_tool(super::board::BoardTool::Eraser);
        b.app.eraser_width = 16.0;
        b.app.eraser_softness = 0.0;
        b.app.eraser_opacity = 1.0;
        b.frame();
        let xf = b.app.board_xf();
        let at = |t: f32| xf.w2s(Pos2::new(center[0], center[1] - 60.0 + t * 120.0));
        b.frame_with(vec![egui::Event::PointerMoved(at(0.0))]);
        let mut events = primary(at(0.0), true);
        let mut last_shown = Vec::new();
        for step in 0..=12 {
            if step > 0 {
                events = vec![egui::Event::PointerMoved(at(step as f32 / 12.0))];
            }
            let out = b.frame_out(events.clone());
            let when = format!("blur {blur} step {step}");
            let faults = coverage_faults(&b, [id]);
            assert!(faults.is_empty(), "{when}: {faults:?}");
            let Some(live) = b.app.erase_live.get(&id) else {
                let gap = (60.0 - step as f32 * 10.0).abs();
                let reach = 15.0 + 3.0 * blur + 8.0;
                assert!(gap > reach - 10.0, "{when}: no live preview");
                continue;
            };
            let mut painted = Vec::new();
            for clipped in &out.shapes {
                mesh_textures(&clipped.shape, &mut painted);
            }
            assert!(
                painted.contains(&live.texture()),
                "{when}: the live preview was not painted"
            );
            if let Some((_, gpu)) = b.app.brush_stamps.get(&b.app.stroke_cache_id(id)) {
                assert!(
                    !painted.contains(&gpu.tex.id()),
                    "{when}: the committed raster was painted under the preview"
                );
            }
            let mask = live.mask();
            assert_eq!(
                (mask.width, mask.height, mask.origin, mask.pixel),
                (
                    committed.width,
                    committed.height,
                    committed.origin,
                    committed.pixel
                ),
                "{when}: the preview is not the committed bitmap"
            );
            let shown = live.shown();
            let (mut kept, mut cut) = (0usize, 0usize);
            for i in (0..shown.len()).step_by(4) {
                match mask.rgba[i + 3] {
                    0 => {
                        assert_eq!(
                            &shown[i..i + 4],
                            &committed.rgba[i..i + 4],
                            "{when}: pixel {} differs from the committed stroke",
                            i / 4
                        );
                        kept += 1;
                    }
                    255 => {
                        assert_eq!(shown[i + 3], 0, "{when}: erased pixel {} shows", i / 4);
                        cut += 1;
                    }
                    _ => {}
                }
            }
            assert!(kept > 0, "{when}: nothing kept");
            if step == 12 {
                assert!(cut > 0, "{when}: nothing erased");
            }
            last_shown = shown.to_vec();
        }
        assert!(!last_shown.is_empty(), "blur {blur}: never previewed");

        b.frame_with(primary(at(1.0), false));
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut frame = 0;
        loop {
            let faults = coverage_faults(&b, [id]);
            assert!(
                faults.is_empty(),
                "blur {blur}: frame {frame} after release: {faults:?}"
            );
            if b.app.brush_tiles.last.settled || Instant::now() > deadline {
                break;
            }
            frame += 1;
            std::thread::sleep(Duration::from_millis(2));
            b.frame();
        }
        let node = b
            .app
            .doc()
            .scene
            .node(id)
            .cloned()
            .expect("bar erased away");
        let NodeKind::Shape(shape) = &node.kind else {
            unreachable!()
        };
        let after =
            super::board_path::stroke_stamp(&node, shape, shape.path.as_ref().unwrap(), pixel)
                .expect("the erased bar's stamp");
        assert_eq!(
            after.rgba.len(),
            last_shown.len(),
            "blur {blur}: bitmap resized"
        );
        let jumps = after
            .rgba
            .iter()
            .skip(3)
            .step_by(4)
            .zip(last_shown.iter().skip(3).step_by(4))
            .filter(|(a, s)| a.abs_diff(**s) > 16)
            .count();
        assert!(
            jumps * 100 <= after.rgba.len() / 4,
            "blur {blur}: {jumps} of {} pixels jump on release",
            after.rgba.len() / 4
        );
    }
}

/// A heavily blurred dab keeps its dot: the blur spreads the ink inside a
/// bitmap wide enough for the falloff (no clipped square), keeps its mass,
/// peaks at the center, and has the same world-space spread at every zoom.
#[test]
fn a_heavily_blurred_dab_keeps_its_dot() {
    let mut b = Bench::new(0);
    let scene = &mut b.app.doc_mut().scene;
    let sharp = dab_node(scene, [100.0, 100.0], 24.0, 0.5, 0.0);
    let blurred = dab_node(scene, [100.0, 100.0], 24.0, 0.5, 24.0);
    let stamp = |node: &Node, pixel: f32| {
        let NodeKind::Shape(shape) = &node.kind else {
            unreachable!()
        };
        super::board_path::stroke_stamp(node, shape, shape.path.as_ref().unwrap(), pixel)
            .expect("a dab stamp")
    };
    // (mass in world units², peak alpha at the center, max alpha, max on the
    // bitmap's outer ring, x spread in world units)
    let measure = |s: &vector_ink::StampImage| {
        let (w, h) = (s.width as usize, s.height as usize);
        let a = |x: usize, y: usize| s.rgba[(y * w + x) * 4 + 3] as f32;
        let mut mass = 0.0;
        let mut mx = 0.0;
        let mut ring: f32 = 0.0;
        let mut most: f32 = 0.0;
        for y in 0..h {
            for x in 0..w {
                let v = a(x, y);
                mass += v;
                mx += v * (s.origin[0] + (x as f32 + 0.5) * s.pixel);
                most = most.max(v);
                if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                    ring = ring.max(v);
                }
            }
        }
        let cx = mx / mass.max(1.0);
        let mut var = 0.0;
        for y in 0..h {
            for x in 0..w {
                let dx = s.origin[0] + (x as f32 + 0.5) * s.pixel - cx;
                var += a(x, y) * dx * dx;
            }
        }
        let center_x = ((100.0 - s.origin[0]) / s.pixel) as usize;
        let center_y = ((100.0 - s.origin[1]) / s.pixel) as usize;
        let peak = a(center_x.min(w - 1), center_y.min(h - 1));
        (
            mass * s.pixel * s.pixel,
            peak,
            most,
            ring,
            (var / mass.max(1.0)).sqrt(),
        )
    };
    let (sharp_mass, _, _, _, sharp_spread) = measure(&stamp(&sharp, 1.0));
    for pixel in [0.5_f32, 1.0, 2.0] {
        let (mass, peak, most, ring, spread) = measure(&stamp(&blurred, pixel));
        assert!(
            ring <= 3.0,
            "pixel {pixel}: the blur is cut off by the bitmap edge (edge alpha {ring})"
        );
        assert!(
            (mass - sharp_mass).abs() <= sharp_mass * 0.1,
            "pixel {pixel}: blur changed the dab's ink from {sharp_mass:.0} to {mass:.0}"
        );
        assert!(
            peak >= most * 0.9,
            "pixel {pixel}: the dot is gone (center {peak} vs max {most})"
        );
        let added = (spread * spread - sharp_spread * sharp_spread)
            .max(0.0)
            .sqrt();
        assert!(
            (added - 24.0).abs() <= 24.0 * 0.15,
            "pixel {pixel}: blur spread {added:.1} world units, want 24"
        );
    }
}

/// A blurred dab painted on its own keeps painting once, with the same
/// pixels, while the user keeps drawing: from the moment it is blurred, no
/// frame shows it twice (an old tile plus its own raster) or not at all, and
/// once it settles each new stroke leaves its raster untouched.
#[test]
fn a_blurred_dab_stays_put_while_strokes_follow() {
    let mut b = Bench::logged(40);
    let dab = dab_node(&mut b.app.doc_mut().scene, [800.0, 450.0], 24.0, 0.5, 0.0);
    let id = dab.id;
    b.app.add_nodes(vec![dab]);
    assert!(settle(&mut b), "fixture tiles did not settle");
    assert!(
        !b.app.brush_tiles.tiles_with(id).is_empty(),
        "dab not tiled"
    );

    let index = b
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .position(|n| n.id == id)
        .unwrap();
    if let NodeKind::Shape(shape) = &mut b.app.doc_mut().scene.nodes[index].kind {
        shape.stroke.gaussian_blur = 24.0;
    }
    b.app.note_scene_change();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut frame = 0;
    loop {
        b.frame();
        let faults = coverage_faults(&b, [id]);
        assert!(
            faults.is_empty(),
            "frame {frame} after the blur: {faults:?}"
        );
        if b.app.brush_tiles.last.settled || Instant::now() > deadline {
            break;
        }
        frame += 1;
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(b.app.brush_tiles.last.settled, "tiles did not settle");
    let tex = b.app.brush_stamps.get(&b.app.stroke_cache_id(id)).map(|(_, g)| g.tex.id());
    assert!(tex.is_some(), "the blurred dab has no raster of its own");

    for i in 0..12 {
        let extra = stroke_node(&mut b.app.doc_mut().scene, 7_000 + i);
        b.app.add_nodes(vec![extra]);
        for f in 0..3 {
            b.frame();
            let faults = coverage_faults(&b, [id]);
            assert!(faults.is_empty(), "stroke {i} frame {f}: {faults:?}");
            assert_eq!(
                b.app.brush_stamps.get(&b.app.stroke_cache_id(id)).map(|(_, g)| g.tex.id()),
                tex,
                "stroke {i} frame {f}: the dab's pixels changed"
            );
        }
    }
}

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

/// 5,000 strokes: rest, pan, zoom, and one more commit, old path then tiles.
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
