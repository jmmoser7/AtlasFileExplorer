//! Pen fit and Smoothing brush acceptance through the app (pen.md GP3,
//! smooth.md golden paths and behavior matrix).

use super::board::BoardTool;
use super::board_path;
use super::tests::Harness;
use eframe::egui::{self, Pos2};
use slate_doc::scene::{NodeKind, PathSeg, ShapeKind};
use slate_doc::{NodeId, StrokeTool, ViewKind};
use vector_ink::kurbo::BezPath;

fn board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.tab_mut().cam.z = 1.0;
    h
}

fn jitter(seed: &mut u64, amp: f32) -> f32 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let unit = ((*seed >> 33) as f32) / ((1u64 << 31) as f32);
    (unit * 2.0 - 1.0) * amp
}

/// A hand-drawn three-period wave inside the default frame.
fn jittery_wave() -> Vec<Pos2> {
    let mut seed = 3;
    (0..=300)
        .map(|i| {
            let x = 100.0 + i as f32 * 2.0;
            let y = 220.0 + 40.0 * (std::f32::consts::TAU * (x - 100.0) / 200.0).sin();
            Pos2::new(x + jitter(&mut seed, 0.8), y + jitter(&mut seed, 0.8))
        })
        .collect()
}

fn seed_frame(h: &mut Harness) {
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 800.0, 450.0),
        NodeKind::Frame(slate_doc::scene::FrameNode {
            title: "Slide 1".into(),
            order: 0,
            fill: slate_doc::scene::Rgba::WHITE,
            fill_authored: false,
            assignments: std::collections::BTreeMap::new(),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
        }),
    );
    h.app.add_nodes(vec![node]);
}

fn node(h: &Harness, id: NodeId) -> slate_doc::scene::Node {
    h.app.doc().scene.node(id).expect("node exists").clone()
}

fn path_of(n: &slate_doc::scene::Node) -> (ShapeKind, slate_doc::scene::PathData) {
    let NodeKind::Shape(s) = &n.kind else {
        panic!("a shape");
    };
    (s.shape, s.path.as_deref().cloned().unwrap_or_default())
}

fn world_bez(n: &slate_doc::scene::Node) -> BezPath {
    let (_, path) = path_of(n);
    board_path::path_data_to_world_bez(&path, n.rect, n.rotation_deg)
}

/// Uniform arc-length resample of a path, `step` world units apart.
fn resample(bez: &BezPath, step: f32) -> Vec<[f32; 2]> {
    let flat = vector_ink::flatten(bez, 0.01);
    let mut out = vec![flat[0]];
    let mut carry = 0.0_f32;
    for w in flat.windows(2) {
        let (a, b) = (w[0], w[1]);
        let seg = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let mut t = step - carry;
        while t <= seg {
            let f = t / seg;
            out.push([a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]);
            t += step;
        }
        carry = seg - (t - step);
    }
    out
}

fn arc_length(bez: &BezPath) -> f32 {
    let flat = vector_ink::flatten(bez, 0.01);
    flat.windows(2)
        .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
        .sum()
}

/// Largest turn, in degrees, between consecutive chords of a uniform resample.
fn max_turn_deg(bez: &BezPath) -> f32 {
    max_turn_deg_near(bez, [0.0, 0.0], f32::INFINITY)
}

/// Largest turn between 2-unit samples whose middle sample lies within
/// `radius` of `center`.
fn max_turn_deg_near(bez: &BezPath, center: [f32; 2], radius: f32) -> f32 {
    let pts = resample(bez, 2.0);
    pts.windows(3)
        .filter(|w| {
            let (dx, dy) = (w[1][0] - center[0], w[1][1] - center[1]);
            (dx * dx + dy * dy).sqrt() <= radius
        })
        .map(|w| {
            let a = [w[1][0] - w[0][0], w[1][1] - w[0][1]];
            let b = [w[2][0] - w[1][0], w[2][1] - w[1][1]];
            let la = (a[0] * a[0] + a[1] * a[1]).sqrt();
            let lb = (b[0] * b[0] + b[1] * b[1]).sqrt();
            if la < 1e-6 || lb < 1e-6 {
                return 0.0;
            }
            ((a[0] * b[0] + a[1] * b[1]) / (la * lb))
                .clamp(-1.0, 1.0)
                .acos()
                .to_degrees()
        })
        .fold(0.0, f32::max)
}

fn dist_to_path(p: [f32; 2], bez: &BezPath) -> f32 {
    let flat = vector_ink::flatten(bez, 0.01);
    flat.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = [b[0] - a[0], b[1] - a[1]];
            let len2 = ab[0] * ab[0] + ab[1] * ab[1];
            let t = if len2 > 0.0 {
                (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
        })
        .fold(f32::INFINITY, f32::min)
}

fn start_end(bez: &BezPath) -> ([f32; 2], [f32; 2]) {
    let flat = vector_ink::flatten(bez, 0.01);
    (flat[0], *flat.last().unwrap())
}

fn add_path(h: &mut Harness, bez: &BezPath, closed: bool) -> NodeId {
    let (rect, data) = board_path::bezpath_to_path_data(bez, closed);
    h.app
        .commit_path_node(StrokeTool::Polyline, rect, data, closed);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(BoardTool::Smooth);
    id
}

fn polyline(points: &[(f64, f64)]) -> BezPath {
    let mut bez = BezPath::new();
    bez.move_to(points[0]);
    for p in &points[1..] {
        bez.line_to(*p);
    }
    bez
}

fn drag(h: &mut Harness, pts: &[Pos2], mods: egui::Modifiers) {
    h.app.board_drag = h.app.begin_gesture_for_test(pts[0], pts[0], mods);
    for p in &pts[1..] {
        h.app.update_gesture_for_test(*p, mods);
    }
    h.app.end_gesture_for_test(*pts.last().unwrap(), None, mods);
}

fn undo_depth(h: &Harness) -> usize {
    h.app.tab().journal.undo_depth()
}

// ---------- Pen (pen.md GP3; user finding 2026-09-26) ----------

/// Stated: the pen never looks segmented, and the exported SVG path is the
/// same cubic chain the board stores.
#[test]
fn pen_stroke_stores_and_exports_one_cubic_chain() {
    let mut h = board("pen_export_cubics");
    seed_frame(&mut h);
    h.app.set_board_tool(BoardTool::Pen);
    h.app.finish_freehand_pen(jittery_wave());
    let n = h.app.doc().scene.nodes.last().unwrap().clone();
    let (shape, path) = path_of(&n);
    assert_eq!(shape, ShapeKind::Path);
    assert!(path.segs.len() >= 4);
    assert!(
        path.segs.iter().all(|s| matches!(s, PathSeg::Cubic { .. })),
        "a curved pen stroke stores only cubic spans: {:?}",
        path.segs
    );

    let html = slate_artifact::render_html(h.app.doc(), &slate_artifact::AssetMap::default());
    let d = html
        .split("<path d=\"")
        .skip(1)
        .map(|s| &s[..s.find('"').unwrap()])
        .find(|d| d.contains(" C "))
        .expect("the stroke exports as an SVG path");
    assert!(
        !d.contains(" L ") && !d.contains(" Q "),
        "export adds no straight spans: {d}"
    );
    let nums: Vec<f32> = d
        .split_whitespace()
        .filter_map(|t| t.parse::<f32>().ok())
        .collect();
    let mut want = vec![path.start];
    for s in &path.segs {
        if let PathSeg::Cubic { c1, c2, to } = s {
            want.extend([*c1, *c2, *to]);
        }
    }
    assert_eq!(
        nums.len(),
        want.len() * 2,
        "one M and three points per C: {d}"
    );
    for (i, p) in want.iter().enumerate() {
        let (x, y) = (p[0] * n.rect.w, p[1] * n.rect.h);
        assert!(
            (nums[i * 2] - x).abs() <= 0.06 && (nums[i * 2 + 1] - y).abs() <= 0.06,
            "point {i}: exported ({}, {}) != stored ({x}, {y})",
            nums[i * 2],
            nums[i * 2 + 1]
        );
    }
}

/// Stated (P1.curve.width-chord): a width change mid-stroke still maps one
/// tip per fitted vertex, narrow before the change and wide after it.
#[test]
fn variable_width_pen_tips_map_onto_the_fitted_vertices() {
    let mut h = board("pen_tips_map");
    h.app.set_board_tool(BoardTool::Pen);
    let wave = jittery_wave();
    let widths: Vec<f32> = (0..wave.len())
        .map(|i| if i < 150 { 2.0 } else { 9.0 })
        .collect();
    h.app.finish_freehand_pen_widths(wave, &widths);
    let n = h.app.doc().scene.nodes.last().unwrap().clone();
    let (_, path) = path_of(&n);
    assert!(path.segs.iter().all(|s| matches!(s, PathSeg::Cubic { .. })));
    assert_eq!(path.tips.len(), path.segs.len() + 1, "one tip per vertex");
    let w: Vec<f32> = path.tips.iter().map(|t| t.width).collect();
    let steps = w.windows(2).filter(|p| p[0] != p[1]).count();
    assert_eq!(steps, 1, "one narrow run then one wide run: {w:?}");
    assert_eq!(w[0], 2.0);
    assert_eq!(*w.last().unwrap(), 9.0);
}

// ---------- Smoothing a sparse Bézier (user finding 2026-09-26) ----------

/// Two humps meeting at a cusp at (300, 300).
fn two_segment_bezier() -> BezPath {
    let mut bez = BezPath::new();
    bez.move_to((100.0, 300.0));
    bez.curve_to((150.0, 150.0), (250.0, 150.0), (300.0, 300.0));
    bez.curve_to((350.0, 150.0), (450.0, 150.0), (500.0, 300.0));
    bez
}

fn wiggle_around(c: Pos2, n: usize) -> Vec<Pos2> {
    (0..n)
        .map(|i| {
            let a = i as f32 * 0.9;
            c + egui::vec2(a.cos() * 4.0, a.sin() * 4.0)
        })
        .collect()
}

#[test]
fn smoothing_a_two_segment_bezier_rounds_it_without_collapsing() {
    let mut h = board("smooth_sparse_bezier");
    let original = two_segment_bezier();
    let id = add_path(&mut h, &original, false);
    h.app.smooth_width = 80.0;
    h.app.smooth_strength = 1.0;
    let before_turn = max_turn_deg(&original);
    let before_len = arc_length(&original);

    let mut pts = vec![Pos2::new(300.0, 300.0)];
    pts.extend(wiggle_around(Pos2::new(300.0, 296.0), 12));
    drag(&mut h, &pts, egui::Modifiers::NONE);

    let after = world_bez(&node(&h, id));
    let after_turn = max_turn_deg(&after);
    assert!(
        after_turn <= before_turn / 3.0,
        "the cusp under the brush smooths: {before_turn:.1} -> {after_turn:.1} deg"
    );
    let after_len = arc_length(&after);
    assert!(
        after_len >= before_len * 0.9,
        "no collapse: length {before_len:.1} -> {after_len:.1}"
    );
    let (s0, e0) = start_end(&original);
    let (s1, e1) = start_end(&after);
    for (a, b) in [(s0, s1), (e0, e1)] {
        assert!(
            (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3,
            "endpoints stay fixed: {a:?} -> {b:?}"
        );
    }
    // Far from the brush, both humps keep their shape.
    let segs: Vec<_> = original.segments().collect();
    for (seg, range) in [(segs[0], 0.0..0.5), (segs[1], 0.5..1.0)] {
        for i in 0..=20 {
            let t = range.start + (range.end - range.start) * i as f64 / 20.0;
            let p = vector_ink::kurbo::ParamCurve::eval(&seg, t);
            let d = dist_to_path([p.x as f32, p.y as f32], &after);
            assert!(d <= 0.3, "untouched span moved by {d} at t={t}");
        }
    }
}

/// A sparse, already-smooth Bézier relaxes at the brush's scale: the control
/// polygon the brush smooths is spaced to the brush, not to a dense flatten
/// whose Laplacian barely moves a smooth curve.
#[test]
fn smoothing_a_smooth_sparse_bezier_relaxes_it_under_the_brush() {
    let mut h = board("smooth_sparse_s_curve");
    let mut original = BezPath::new();
    original.move_to((100.0, 300.0));
    original.curve_to((150.0, 200.0), (250.0, 200.0), (300.0, 300.0));
    original.curve_to((350.0, 400.0), (450.0, 400.0), (500.0, 300.0));
    let id = add_path(&mut h, &original, false);
    h.app.smooth_width = 80.0;
    h.app.smooth_strength = 1.0;
    let before_len = arc_length(&original);

    let hump = Pos2::new(200.0, 225.0);
    let mut pts = vec![hump];
    pts.extend(wiggle_around(hump, 12));
    drag(&mut h, &pts, egui::Modifiers::NONE);

    let after = world_bez(&node(&h, id));
    let moved = dist_to_path([hump.x, hump.y], &after);
    assert!(moved >= 2.0, "the hump under the brush relaxed by {moved}");
    assert!(arc_length(&after) >= before_len * 0.9, "no collapse");
    let (s0, e0) = start_end(&original);
    let (s1, e1) = start_end(&after);
    for (a, b) in [(s0, s1), (e0, e1)] {
        assert!(
            (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3,
            "endpoints stay fixed: {a:?} -> {b:?}"
        );
    }
    let far = original.segments().nth(1).unwrap();
    for i in 4..=20 {
        let p = vector_ink::kurbo::ParamCurve::eval(&far, i as f64 / 20.0);
        let d = dist_to_path([p.x as f32, p.y as f32], &after);
        assert!(d <= 0.3, "the far hump moved by {d}");
    }
}

// ---------- Smoothing golden paths (smooth.md) ----------

fn zigzag() -> BezPath {
    polyline(&[
        (100.0, 300.0),
        (150.0, 260.0),
        (200.0, 300.0),
        (250.0, 260.0),
        (300.0, 300.0),
    ])
}

/// GP1 + D02 + D11: a pass over a polyline smooths it, the tool stays armed,
/// and one undo restores the exact original.
#[test]
fn smooth_gp1_polyline_pass_smooths_and_one_undo_restores() {
    let mut h = board("smooth_gp1");
    let id = add_path(&mut h, &zigzag(), false);
    h.app.smooth_width = 60.0;
    h.app.smooth_strength = 1.0;
    let before = node(&h, id);
    let depth = undo_depth(&h);
    let brush = ([200.0, 300.0], 30.0);
    let before_turn = max_turn_deg_near(&world_bez(&before), brush.0, brush.1);
    drag(
        &mut h,
        &[
            Pos2::new(200.0, 300.0),
            Pos2::new(201.0, 299.0),
            Pos2::new(202.0, 298.0),
        ],
        egui::Modifiers::NONE,
    );
    let after = node(&h, id);
    assert_ne!(after, before, "the pass changed the polyline");
    let after_turn = max_turn_deg_near(&world_bez(&after), brush.0, brush.1);
    assert!(
        after_turn < before_turn * 0.75,
        "the corner under the brush rounds: {before_turn:.1} -> {after_turn:.1} deg"
    );
    assert_eq!(h.app.board_tool, BoardTool::Smooth, "D02: sticky");
    assert_eq!(undo_depth(&h), depth + 1, "D11: one undo group");
    h.app.board_undo();
    assert_eq!(node(&h, id), before, "undo restores the exact original");
    h.frame();
}

/// GP3 + D12: Esc mid-drag drops the pass; no journal, no preview.
#[test]
fn smooth_gp3_esc_mid_drag_cancels_without_journal() {
    let mut h = board("smooth_gp3");
    let id = add_path(&mut h, &zigzag(), false);
    h.app.smooth_width = 60.0;
    let before = node(&h, id);
    let depth = undo_depth(&h);
    let mods = egui::Modifiers::NONE;
    let p = Pos2::new(200.0, 300.0);
    h.app.board_drag = h.app.begin_gesture_for_test(p, p, mods);
    h.app.update_gesture_for_test(Pos2::new(205.0, 295.0), mods);
    assert!(
        !h.app.smooth_preview.is_empty(),
        "D09: live preview while dragging"
    );
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
    });
    assert!(h.app.board_drag.is_none(), "Esc ends the pass");
    assert!(h.app.smooth_preview.is_empty(), "preview dropped");
    assert_eq!(node(&h, id), before, "scene unchanged");
    assert_eq!(undo_depth(&h), depth, "no journal");
}

/// D04: a click without movement still runs one pass at the press point.
#[test]
fn smooth_click_runs_one_pass() {
    let mut h = board("smooth_click");
    let id = add_path(&mut h, &zigzag(), false);
    h.app.smooth_width = 60.0;
    h.app.smooth_strength = 1.0;
    let before = node(&h, id);
    drag(&mut h, &[Pos2::new(200.0, 300.0)], egui::Modifiers::NONE);
    assert_ne!(node(&h, id), before);
}

/// D03: the brush sweeps; a drag that starts on empty canvas smooths every
/// stroke it crosses, as one undo group (D11).
#[test]
#[ignore = "item 3: sweep hit-test"]
fn smooth_drag_sweeps_every_stroke_it_crosses() {
    let mut h = board("smooth_sweep");
    let a = add_path(&mut h, &zigzag(), false);
    let b = add_path(
        &mut h,
        &polyline(&[
            (100.0, 400.0),
            (150.0, 360.0),
            (200.0, 400.0),
            (250.0, 360.0),
            (300.0, 400.0),
        ]),
        false,
    );
    h.app.smooth_width = 60.0;
    h.app.smooth_strength = 1.0;
    let (a0, b0) = (node(&h, a), node(&h, b));
    let depth = undo_depth(&h);
    let pts: Vec<Pos2> = (0..=40)
        .map(|i| Pos2::new(200.0, 200.0 + i as f32 * 6.0))
        .collect();
    drag(&mut h, &pts, egui::Modifiers::NONE);
    assert_ne!(node(&h, a), a0, "the first stroke crossed is smoothed");
    assert_ne!(node(&h, b), b0, "the second stroke crossed is smoothed");
    assert_eq!(undo_depth(&h), depth + 1);
    h.app.board_undo();
    assert_eq!(node(&h, a), a0);
    assert_eq!(node(&h, b), b0);
}

/// D03: no paint on empty canvas.
#[test]
fn smooth_on_empty_canvas_changes_nothing() {
    let mut h = board("smooth_empty");
    let id = add_path(&mut h, &zigzag(), false);
    let before = node(&h, id);
    let depth = undo_depth(&h);
    drag(
        &mut h,
        &[Pos2::new(600.0, 100.0), Pos2::new(650.0, 120.0)],
        egui::Modifiers::NONE,
    );
    assert_eq!(node(&h, id), before);
    assert_eq!(undo_depth(&h), depth);
}

/// D03: Shift+drag is a straight pass from the end of the last pass; the
/// stroke under that segment is smoothed even though no pointer sample
/// lands on it.
#[test]
#[ignore = "item 3: straight pass"]
fn smooth_shift_drag_is_a_straight_pass_along_the_segment() {
    let mut h = board("smooth_straight");
    let id = add_path(&mut h, &zigzag(), false);
    h.app.smooth_width = 60.0;
    h.app.smooth_strength = 1.0;
    h.app.smooth_anchor = Some(Pos2::new(200.0, 150.0));
    let before = node(&h, id);
    let shift = egui::Modifiers::SHIFT;
    drag(
        &mut h,
        &[Pos2::new(200.0, 450.0), Pos2::new(201.0, 450.0)],
        shift,
    );
    assert_ne!(node(&h, id), before, "the straight pass crossed the zigzag");
}

/// D17: hidden and locked strokes are skipped.
#[test]
fn smooth_skips_locked_strokes() {
    let mut h = board("smooth_locked");
    let id = add_path(&mut h, &zigzag(), false);
    h.app.doc_mut().scene.node_mut(id).unwrap().locked = true;
    h.app.smooth_width = 60.0;
    let before = node(&h, id);
    let depth = undo_depth(&h);
    drag(&mut h, &[Pos2::new(200.0, 300.0)], egui::Modifiers::NONE);
    assert_eq!(node(&h, id), before);
    assert_eq!(undo_depth(&h), depth);
}

/// D14: a smoothed line becomes a cubic path.
#[test]
fn smoothed_line_becomes_a_cubic_path() {
    let mut h = board("smooth_line");
    let mods = egui::Modifiers::NONE;
    let id = h
        .app
        .commit_line(Pos2::new(100.0, 100.0), Pos2::new(300.0, 100.0))
        .expect("a line");
    assert!(super::board_line::line_endpoints(&node(&h, id)).is_some());
    h.app.set_board_tool(BoardTool::Smooth);
    h.app.smooth_width = 40.0;
    drag(&mut h, &[Pos2::new(200.0, 100.0)], mods);
    let (shape, path) = path_of(&node(&h, id));
    assert_eq!(shape, ShapeKind::Path);
    assert!(!path.segs.is_empty());
    assert!(path.segs.iter().all(|s| matches!(s, PathSeg::Cubic { .. })));
}

/// Smoothing a closed shape keeps it closed.
#[test]
#[ignore = "item 3: closed flag"]
fn smoothing_a_closed_path_keeps_it_closed() {
    let mut h = board("smooth_closed");
    let mut square = polyline(&[
        (100.0, 100.0),
        (200.0, 100.0),
        (200.0, 200.0),
        (100.0, 200.0),
    ]);
    square.close_path();
    let id = add_path(&mut h, &square, true);
    assert!(path_of(&node(&h, id)).1.closed);
    h.app.smooth_width = 60.0;
    h.app.smooth_strength = 1.0;
    drag(&mut h, &[Pos2::new(200.0, 200.0)], egui::Modifiers::NONE);
    let (_, path) = path_of(&node(&h, id));
    assert!(path.closed, "still a closed shape after smoothing");
}
