//! P1.curve.tip-chord: a tip change made mid-draw through the right-button
//! HUD (size, color, opacity) on Line, Arc, Polyline, Bézier span, Pen and
//! Brush, driven through real frames. The live preview shows the blend, the
//! committed curve keeps it per vertex, and one Ctrl+Z removes the curve.

use super::board::{BoardDrag, BoardTool};
use super::board_color::BrushHud;
use super::board_path;
use super::tests::{capture_frame, rasterize, FrameRaster, Harness};
use eframe::egui::{self, Modifiers, Pos2, Vec2};
use slate_doc::scene::{NodeKind, Rgba, StrokeSpan};
use slate_doc::vertex_style::PlacedTip;
use slate_doc::{StrokeTool, ViewKind};
use vector_ink::kurbo::{BezPath, Point};

fn board(tag: &str, tool: BoardTool) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.app.board_ortho = false;
    h.frame();
    h.frame();
    h.app.set_board_tool(tool);
    h.frame();
    h
}

const ALT: Modifiers = Modifiers {
    alt: true,
    ctrl: false,
    shift: false,
    mac_cmd: false,
    command: false,
};
const CTRL: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};
const SHIFT: Modifiers = Modifiers {
    alt: false,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: false,
};

fn screen(h: &Harness, world: Pos2) -> Pos2 {
    h.app.board_xf().w2s(world)
}

/// One frame, 100 ms after the last, so separate clicks never read as a
/// double-click.
fn events(h: &mut Harness, mods: Modifiers, events: Vec<egui::Event>) {
    let t = h.ctx.input(|i| i.time) + 0.1;
    h.frame_with(|i| {
        i.time = Some(t);
        i.modifiers = mods;
        i.events = events;
    });
}

fn button(pos: Pos2, button: egui::PointerButton, pressed: bool, m: Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: m,
    }
}

fn hover(h: &mut Harness, world: Pos2) {
    let p = screen(h, world);
    events(h, Modifiers::NONE, vec![egui::Event::PointerMoved(p)]);
}

fn click(h: &mut Harness, world: Pos2) {
    let p = screen(h, world);
    let left = egui::PointerButton::Primary;
    events(h, Modifiers::NONE, vec![egui::Event::PointerMoved(p)]);
    events(
        h,
        Modifiers::NONE,
        vec![
            button(p, left, true, Modifiers::NONE),
            button(p, left, false, Modifiers::NONE),
        ],
    );
}

/// Modifier + right-drag from `at` (screen) through `offsets`, then release.
fn chord(h: &mut Harness, m: Modifiers, at: Pos2, offsets: &[Vec2]) {
    let right = egui::PointerButton::Secondary;
    events(h, m, vec![egui::Event::PointerMoved(at)]);
    events(h, m, vec![button(at, right, true, m)]);
    let mut last = at;
    for o in offsets {
        last = at + *o;
        events(h, m, vec![egui::Event::PointerMoved(last)]);
    }
    events(h, m, vec![button(last, right, false, m)]);
    events(h, Modifiers::NONE, vec![egui::Event::PointerMoved(at)]);
    assert!(h.app.brush_hud.is_none(), "the HUD closed on release");
}

/// Alt+right-drag 40 px to the right: a wider tip.
fn widen(h: &mut Harness, at: Pos2) {
    chord(h, ALT, at, &[Vec2::new(20.0, 0.0), Vec2::new(40.0, 0.0)]);
}

/// Ctrl+right-drag into the color wheel's disk: a new color.
fn recolor(h: &mut Harness, at: Pos2) {
    let right = egui::PointerButton::Secondary;
    events(h, CTRL, vec![egui::Event::PointerMoved(at)]);
    events(h, CTRL, vec![button(at, right, true, CTRL)]);
    let Some(BrushHud::Wheel { center, .. }) = h.app.brush_hud else {
        panic!("Ctrl+right opens the wheel, got {:?}", h.app.brush_hud);
    };
    let target = center + Vec2::new(-40.0, 40.0);
    events(h, CTRL, vec![egui::Event::PointerMoved(target)]);
    events(h, CTRL, vec![button(target, right, false, CTRL)]);
    events(h, Modifiers::NONE, vec![egui::Event::PointerMoved(at)]);
    assert!(h.app.brush_hud.is_none(), "the wheel closed on release");
}

/// Shift+right-drag 50 px down: about half opacity.
fn fade(h: &mut Harness, at: Pos2) {
    chord(h, SHIFT, at, &[Vec2::new(0.0, 25.0), Vec2::new(0.0, 50.0)]);
}

fn ctrl_z(h: &mut Harness) {
    let m = Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    events(
        h,
        m,
        vec![egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: m,
        }],
    );
}

fn key(h: &mut Harness, key: egui::Key) {
    events(
        h,
        Modifiers::NONE,
        vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
}

fn enter(h: &mut Harness) {
    key(h, egui::Key::Enter);
}

fn escape(h: &mut Harness) {
    key(h, egui::Key::Escape);
}

/// The one committed curve: its node opacity and its tips as painted
/// (hard strokes at painted width).
fn committed(h: &Harness) -> (f32, Vec<StrokeSpan>) {
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "one committed curve");
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Shape(s) = &node.kind else {
        panic!("a shape");
    };
    let path = s.path.as_ref().expect("a path");
    let tips = slate_doc::vertex_style::grip_tips(path, &s.stroke, node.rect, node.rotation_deg)
        .expect("grips");
    (node.opacity, tips)
}

fn alpha(tip: &StrokeSpan) -> u8 {
    tip.color.0[3]
}

fn path_preview(h: &Harness, tool: StrokeTool, cursor: Pos2) -> (BezPath, bool, Vec<PlacedTip>) {
    let draft = h.app.board_path_draft.as_ref().expect("still drawing");
    board_path::path_draft_preview(draft, Some(cursor), h.app.placed_tip(tool)).expect("a preview")
}

fn undo_removes_the_curve(h: &mut Harness) {
    ctrl_z(h);
    assert!(
        h.app.doc().scene.nodes.is_empty(),
        "one Ctrl+Z removes the curve (nodes left: {})",
        h.app.doc().scene.nodes.len()
    );
}

fn assert_differs(a: Rgba, b: Rgba, what: &str) {
    assert_ne!(a.0[..3], b.0[..3], "{what}");
}

/// The mesh a draft paints with `tips` along `bez` carries a color per
/// vertex, and the ends' colors differ. The rasterizer blends vertex colors
/// straight across each triangle, which is the whole blend on a straight
/// span; an `eased` blend also needs vertices painting colors between the
/// ends'.
fn assert_preview_blends(bez: &BezPath, closed: bool, tips: &[PlacedTip], eased: bool) {
    let (ink, _) = board_path::draft_stroke_ink(bez, closed, tips, 1.0);
    assert_eq!(
        ink.colors.len(),
        ink.vertices.len(),
        "the preview paints a color per vertex"
    );
    let near = |p: Point| {
        let i = (0..ink.vertices.len())
            .min_by(|a, b| {
                let d = |k: usize| {
                    let v = ink.vertices[k].pos;
                    (v[0] as f64 - p.x).hypot(v[1] as f64 - p.y)
                };
                d(*a).total_cmp(&d(*b))
            })
            .expect("a mesh");
        ink.colors[i]
    };
    let ends: Vec<Point> = bez
        .elements()
        .iter()
        .filter_map(|e| e.end_point())
        .collect();
    let (a, b) = (near(ends[0]), near(*ends.last().unwrap()));
    let c = (0..3)
        .max_by(|x, y| (a[*x] - b[*x]).abs().total_cmp(&(a[*y] - b[*y]).abs()))
        .unwrap();
    assert!((a[c] - b[c]).abs() > 0.05, "the ends differ: {a:?} {b:?}");
    if eased {
        let (lo, hi) = (a[c].min(b[c]) + 0.01, a[c].max(b[c]) - 0.01);
        let between = ink.colors.iter().filter(|k| k[c] > lo && k[c] < hi).count();
        assert!(between >= 6, "vertices paint the eased blend ({between})");
    }
}

fn assert_freehand_blends<T: Copy>(tips: &[T], width: impl Fn(&T) -> f32) {
    let (a, b) = (width(&tips[0]), width(tips.last().unwrap()));
    assert!(b > a + 10.0, "the stroke widens: {a} → {b}");
    let between = tips
        .iter()
        .filter(|t| width(t) > a + 0.5 && width(t) < b - 0.5)
        .count();
    assert!(
        between >= 3,
        "the stroke blends into the new tip ({between} samples)"
    );
}

/// A freehand tip change blends by smoothstep over `TIP_BLEND_PX` (screen)
/// from the last drawn tip, and the fit splits at the blend's two ends so
/// the fitted vertices keep the eased tips.
#[test]
fn a_freehand_tip_change_eases_by_smoothstep_and_the_fit_keeps_it() {
    let tip = |width| PlacedTip {
        width,
        color: Rgba::opaque(0, 0, 0),
        opacity: 1.0,
    };
    let mut drawn = board_path::FreehandTips::new(Pos2::ZERO, tip(2.0));
    for i in 1..=10 {
        drawn.push(Pos2::new(2.0 * i as f32, 0.0), tip(2.0), 1.0);
    }
    for i in 11..=50 {
        drawn.push(Pos2::new(2.0 * i as f32, 0.0), tip(10.0), 1.0);
    }
    let len = board_path::TIP_BLEND_PX;
    let smooth = |d: f32| {
        let t = (d / len).clamp(0.0, 1.0);
        2.0 + 8.0 * t * t * (3.0 - 2.0 * t)
    };
    for (p, t) in drawn.points.iter().zip(&drawn.tips) {
        let want = smooth(p.x - 20.0);
        assert!(
            (t.width - want).abs() < 1e-4,
            "{} at {}, want {want}",
            t.width,
            p.x
        );
    }
    assert_eq!(drawn.breaks, vec![10, 22], "the blend's two ends");
    let (_, fitted) = drawn.fit(0.5, 4.0).expect("the stroke fits");
    assert_eq!(fitted[0].width, 2.0);
    assert_eq!(fitted.last().unwrap().width, 10.0);
    let ends = fitted.iter().filter(|t| t.width == 2.0).count();
    assert!(
        ends >= 2,
        "a vertex holds the old tip where the blend starts: {fitted:?}"
    );
}

/// User report (27 September 2026): "drawing a line changing brush
/// properties half for one side of line verses another does not create an
/// interpolation or tweening between the two properties".
#[test]
fn a_line_tweens_width_color_and_opacity_changed_mid_draw() {
    let mut h = board("chord_line", BoardTool::Line);
    let (a, b) = (Pos2::new(0.0, 0.0), Pos2::new(200.0, 0.0));
    click(&mut h, a);
    assert!(h.app.line_draft.is_some(), "the first click starts a line");
    hover(&mut h, Pos2::new(120.0, 0.0));
    let at = screen(&h, Pos2::new(120.0, 0.0));
    widen(&mut h, at);
    recolor(&mut h, at);
    fade(&mut h, at);
    let d = h.app.line_draft.clone().expect("the chords keep the draft");
    let mut bez = BezPath::new();
    bez.move_to((a.x as f64, a.y as f64));
    bez.line_to((b.x as f64, b.y as f64));
    let tips = [d.start_tip, h.app.placed_tip(StrokeTool::Line)];
    assert_preview_blends(&bez, false, &tips, false);
    click(&mut h, b);
    let (opacity, tips) = committed(&h);
    assert_eq!(tips.len(), 2);
    assert!(tips[1].width > tips[0].width + 10.0, "{tips:?}");
    assert_differs(tips[0].color, tips[1].color, "the end takes the new color");
    assert!(alpha(&tips[1]) < alpha(&tips[0]), "the end takes the fade");
    assert!((opacity - 1.0).abs() < 1e-3, "the start is fully opaque");
    undo_removes_the_curve(&mut h);
}

#[test]
fn an_arc_tweens_the_tip_along_its_sweep() {
    let mut h = board("chord_arc", BoardTool::Arc);
    click(&mut h, Pos2::new(0.0, 0.0));
    let at = screen(&h, Pos2::new(100.0, 30.0));
    recolor(&mut h, at);
    click(&mut h, Pos2::new(200.0, 0.0));
    widen(&mut h, at);
    let (bez, closed, tips) = path_preview(&h, StrokeTool::Arc, Pos2::new(100.0, -100.0));
    assert!(
        tips[1].width > tips[0].width + 10.0,
        "the through point is wide"
    );
    assert_preview_blends(&bez, closed, &tips, false);
    click(&mut h, Pos2::new(100.0, -100.0));
    let (_, tips) = committed(&h);
    let widths: Vec<f32> = tips.iter().map(|t| t.width).collect();
    assert_eq!(tips.len(), 3, "start, through, end");
    assert!(widths[1] > widths[0] + 10.0 && (widths[0] - widths[2]).abs() < 1e-3);
    assert_differs(tips[0].color, tips[2].color, "the end took the new color");
    assert_eq!(tips[1].color, tips[2].color, "the through point too");
    undo_removes_the_curve(&mut h);
}

/// User request (26 September 2026): "if the user draws a starting point of
/// a polyline with a small brush then increases the brush size for their
/// second point … taper between the two points … same principle applies to
/// other properties … such as color".
#[test]
fn a_polyline_tweens_each_placed_points_tip() {
    let mut h = board("chord_polyline", BoardTool::Polyline);
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(150.0, 0.0),
        Pos2::new(150.0, 150.0),
    ];
    let at = screen(&h, Pos2::new(60.0, 60.0));
    click(&mut h, pts[0]);
    widen(&mut h, at);
    click(&mut h, pts[1]);
    recolor(&mut h, at);
    let (bez, closed, tips) = path_preview(&h, StrokeTool::Polyline, pts[2]);
    assert!(tips[1].width > tips[0].width + 10.0, "point 2 is wide");
    assert_preview_blends(&bez, closed, &tips, false);
    click(&mut h, pts[2]);
    enter(&mut h);
    let (_, tips) = committed(&h);
    assert_eq!(tips.len(), 3);
    assert!(tips[1].width > tips[0].width + 10.0, "{tips:?}");
    assert!((tips[2].width - tips[1].width).abs() < 1e-3);
    assert_eq!(tips[0].color, tips[1].color, "color changed after point 2");
    assert_differs(tips[1].color, tips[2].color, "point 3 took the new color");
    undo_removes_the_curve(&mut h);
}

#[test]
fn a_bezier_span_tweens_each_placed_anchors_tip() {
    let mut h = board("chord_bezier", BoardTool::BezierSpan);
    let at = screen(&h, Pos2::new(60.0, 80.0));
    let left = egui::PointerButton::Primary;
    // Press at `world`, drag its handle out by `handle`; `release` or not.
    let place = |h: &mut Harness, world: Pos2, handle: Vec2, release: bool| {
        let p = screen(h, world);
        events(h, Modifiers::NONE, vec![egui::Event::PointerMoved(p)]);
        events(
            h,
            Modifiers::NONE,
            vec![button(p, left, true, Modifiers::NONE)],
        );
        for f in [0.5, 1.0] {
            let q = p + handle * f;
            events(h, Modifiers::NONE, vec![egui::Event::PointerMoved(q)]);
        }
        if release {
            let q = p + handle;
            events(
                h,
                Modifiers::NONE,
                vec![button(q, left, false, Modifiers::NONE)],
            );
        }
    };
    place(&mut h, Pos2::new(0.0, 0.0), Vec2::new(40.0, -60.0), true);
    widen(&mut h, at);
    place(&mut h, Pos2::new(160.0, 0.0), Vec2::new(40.0, 60.0), true);
    recolor(&mut h, at);
    fade(&mut h, at);
    // The span previews the anchor being placed while its press is down.
    let third = Pos2::new(320.0, 0.0);
    place(&mut h, third, Vec2::new(40.0, -60.0), false);
    let (bez, closed, tips) = path_preview(&h, StrokeTool::Bezier, third);
    assert_eq!(tips.len(), 3);
    assert!(
        tips[2].opacity < tips[1].opacity,
        "the anchor being placed fades"
    );
    assert_preview_blends(&bez, closed, &tips, true);
    let up = screen(&h, third) + Vec2::new(40.0, -60.0);
    events(
        &mut h,
        Modifiers::NONE,
        vec![button(up, left, false, Modifiers::NONE)],
    );
    enter(&mut h);
    let (opacity, tips) = committed(&h);
    assert_eq!(tips.len(), 3);
    assert!(tips[1].width > tips[0].width + 10.0, "{tips:?}");
    assert_differs(tips[1].color, tips[2].color, "the last anchor's color");
    assert!(alpha(&tips[2]) < alpha(&tips[1]), "the last anchor's fade");
    assert!((opacity - 1.0).abs() < 1e-3);
    undo_removes_the_curve(&mut h);
}

/// Left-press at `from`, drag through `to` in frames (the button stays down).
fn stroke_start(h: &mut Harness, from: Pos2) {
    let p = screen(h, from);
    let left = egui::PointerButton::Primary;
    events(h, Modifiers::NONE, vec![egui::Event::PointerMoved(p)]);
    events(
        h,
        Modifiers::NONE,
        vec![button(p, left, true, Modifiers::NONE)],
    );
}

fn stroke_through(h: &mut Harness, pts: impl IntoIterator<Item = Pos2>) {
    for w in pts {
        let p = screen(h, w);
        events(h, Modifiers::NONE, vec![egui::Event::PointerMoved(p)]);
    }
}

fn stroke_end(h: &mut Harness, at: Pos2) {
    let p = screen(h, at);
    let left = egui::PointerButton::Primary;
    events(
        h,
        Modifiers::NONE,
        vec![
            egui::Event::PointerMoved(p),
            button(p, left, false, Modifiers::NONE),
        ],
    );
}

fn run(from: f32, to: f32) -> impl Iterator<Item = Pos2> {
    (0..=20).map(move |i| Pos2::new(from + (to - from) * i as f32 / 20.0, 0.0))
}

/// Sep 25: "allowing for mid stroke adjustments in drawing tip diameter";
/// Sep 27: the same for color, and the committed stroke keeps them.
#[test]
fn a_pen_stroke_tweens_width_and_color_changed_mid_stroke() {
    let mut h = board("chord_pen", BoardTool::Pen);
    stroke_start(&mut h, Pos2::new(0.0, 0.0));
    stroke_through(&mut h, run(0.0, 200.0));
    let at = screen(&h, Pos2::new(200.0, 0.0));
    widen(&mut h, at);
    recolor(&mut h, at);
    stroke_through(&mut h, run(200.0, 400.0));
    let Some(BoardDrag::FreehandPen { stroke }) = &h.app.board_drag else {
        panic!("the pen stroke is live");
    };
    assert_freehand_blends(&stroke.tips, |t| t.width);
    assert!(
        stroke.points.iter().all(|p| p.y.abs() < 1e-3),
        "the scrubs drew no ink"
    );
    let mut bez = BezPath::new();
    bez.move_to((0.0, 0.0));
    for p in &stroke.points[1..] {
        bez.line_to((p.x as f64, p.y as f64));
    }
    assert_preview_blends(&bez, false, &stroke.tips, true);
    stroke_end(&mut h, Pos2::new(400.0, 0.0));
    let (_, tips) = committed(&h);
    let (first, last) = (tips[0], *tips.last().unwrap());
    assert!(last.width > first.width + 10.0, "{first:?} → {last:?}");
    assert_differs(first.color, last.color, "the rest of the stroke's color");
    undo_removes_the_curve(&mut h);
}

/// Shift+right-drag 150 px down from full at `world`: past 0 %, which it
/// holds.
fn fade_out(h: &mut Harness, world: Pos2) {
    let at = screen(h, world);
    chord(h, SHIFT, at, &[Vec2::new(0.0, 75.0), Vec2::new(0.0, 150.0)]);
}

/// The one committed curve is clear, and a Select click on its geometry
/// (after a click on empty board clears the selection) selects it.
fn assert_clear_and_picked(h: &mut Harness, on: Pos2) {
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "one committed curve");
    let node = &h.app.doc().scene.nodes[0];
    let id = node.id;
    assert_eq!(node.opacity, 0.0, "the curve commits at 0 %");
    if h.app.board_tool != BoardTool::Select {
        escape(h);
    }
    assert_eq!(h.app.board_tool, BoardTool::Select);
    click(h, Pos2::new(on.x, on.y + 300.0));
    assert!(
        h.app.board_sel.is_empty(),
        "empty board clears the selection"
    );
    click(h, on);
    assert_eq!(
        h.app.board_sel.iter().copied().collect::<Vec<_>>(),
        vec![id],
        "a 0 % curve is picked by its geometry"
    );
}

/// User decision (27 September 2026): "Opacity reaches 0 %" on every tool,
/// and a 0 % stroke is still picked by its geometry.
#[test]
fn a_line_armed_at_zero_opacity_commits_clear_and_still_picks() {
    let mut h = board("zero_line", BoardTool::Line);
    fade_out(&mut h, Pos2::new(100.0, 100.0));
    assert_eq!(h.app.opacity_for_tool(StrokeTool::Line), 0.0);
    click(&mut h, Pos2::new(0.0, 0.0));
    hover(&mut h, Pos2::new(200.0, 0.0));
    click(&mut h, Pos2::new(200.0, 0.0));
    assert_clear_and_picked(&mut h, Pos2::new(100.0, 0.0));
}

#[test]
fn a_polyline_armed_at_zero_opacity_commits_clear_and_still_picks() {
    let mut h = board("zero_polyline", BoardTool::Polyline);
    fade_out(&mut h, Pos2::new(100.0, 100.0));
    for p in [
        Pos2::new(0.0, 0.0),
        Pos2::new(150.0, 0.0),
        Pos2::new(150.0, 150.0),
    ] {
        hover(&mut h, p - Vec2::new(0.0, 20.0));
        hover(&mut h, p);
        click(&mut h, p);
    }
    assert!(h.app.board_path_draft.is_some(), "the polyline is drawing");
    enter(&mut h);
    assert_clear_and_picked(&mut h, Pos2::new(75.0, 0.0));
}

#[test]
fn a_pen_stroke_armed_at_zero_opacity_commits_clear_and_still_picks() {
    let mut h = board("zero_pen", BoardTool::Pen);
    fade_out(&mut h, Pos2::new(100.0, 100.0));
    assert_eq!(h.app.opacity_for_tool(StrokeTool::Pen), 0.0);
    stroke_start(&mut h, Pos2::new(0.0, 0.0));
    stroke_through(&mut h, run(0.0, 200.0));
    stroke_end(&mut h, Pos2::new(200.0, 0.0));
    assert_clear_and_picked(&mut h, Pos2::new(100.0, 0.0));
}

/// Hover near `p`, then onto it, then click: separate clicks never pair
/// into a double-click.
fn place_point(h: &mut Harness, p: Pos2) {
    hover(h, p - Vec2::new(0.0, 20.0));
    hover(h, p);
    click(h, p);
}

#[test]
fn an_arc_armed_at_zero_opacity_commits_clear_and_still_picks() {
    let mut h = board("zero_arc", BoardTool::Arc);
    fade_out(&mut h, Pos2::new(300.0, 100.0));
    assert_eq!(h.app.opacity_for_tool(StrokeTool::Arc), 0.0);
    for p in [
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 0.0),
        Pos2::new(100.0, -100.0),
    ] {
        place_point(&mut h, p);
    }
    assert!(h.app.board_path_draft.is_none(), "the third click commits");
    assert_clear_and_picked(&mut h, Pos2::new(100.0, -100.0));
}

#[test]
fn a_bezier_span_armed_at_zero_opacity_commits_clear_and_still_picks() {
    let mut h = board("zero_bezier", BoardTool::BezierSpan);
    fade_out(&mut h, Pos2::new(300.0, 100.0));
    assert_eq!(h.app.opacity_for_tool(StrokeTool::Bezier), 0.0);
    for p in [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(200.0, 0.0),
    ] {
        place_point(&mut h, p);
    }
    assert!(h.app.board_path_draft.is_some(), "the span is drawing");
    enter(&mut h);
    assert_clear_and_picked(&mut h, Pos2::new(50.0, 0.0));
}

/// One still frame, 100 ms after the last, drawn into `raster`.
fn raster_frame(h: &mut Harness, raster: &mut FrameRaster) {
    let t = h.ctx.input(|i| i.time) + 0.1;
    let out = capture_frame(h, raster, |i| i.time = Some(t));
    rasterize(h, raster, out);
}

/// How far the pixel at `screen` moved from `base`, summed over RGB.
fn lit(raster: &FrameRaster, base: &[[f32; 4]], screen: Pos2) -> f32 {
    let i = screen.y as usize * raster.w + screen.x as usize;
    (0..3).map(|c| (raster.px[i][c] - base[i][c]).abs()).sum()
}

/// A curve at 0 % paints nothing: its pixels match the empty board, while
/// the same Line at full opacity lights them.
#[test]
fn a_zero_opacity_curve_leaves_its_pixels_unlit() {
    let mut h = board("zero_raster", BoardTool::Line);
    let mut raster = FrameRaster::new(1440, 900);
    let park = Pos2::new(-400.0, 300.0);
    hover(&mut h, park);
    raster_frame(&mut h, &mut raster);
    let base = raster.px.clone();
    let draw = |h: &mut Harness, y: f32| {
        place_point(h, Pos2::new(-200.0, y));
        place_point(h, Pos2::new(200.0, y));
    };
    draw(&mut h, -100.0);
    h.app.set_board_tool(BoardTool::Line);
    fade_out(&mut h, Pos2::new(350.0, 0.0));
    assert_eq!(h.app.opacity_for_tool(StrokeTool::Line), 0.0);
    draw(&mut h, 100.0);
    assert_eq!(h.app.doc().scene.nodes.len(), 2, "two lines");
    escape(&mut h);
    click(&mut h, park);
    assert!(h.app.board_sel.is_empty(), "nothing selected");
    raster_frame(&mut h, &mut raster);
    let band = |raster: &FrameRaster, y: f32| {
        let mut most = 0.0f32;
        for x in (-150..=150).step_by(5) {
            for dy in -6..=6 {
                let s = screen(&h, Pos2::new(x as f32, y + dy as f32));
                most = most.max(lit(raster, &base, s));
            }
        }
        most
    };
    assert!(band(&raster, -100.0) > 0.2, "the full-opacity line paints");
    let clear = band(&raster, 100.0);
    assert!(clear < 1e-3, "the 0 % line paints nothing ({clear})");
}

/// A translucent Pen stroke drawn in pieces shows no seam: at each joint
/// between pieces the columns cover as much as their neighbours, where one
/// side is a plain stroke and the other blends a mid-stroke color change.
#[test]
fn a_translucent_pen_stroke_has_no_seam_at_its_piece_joints() {
    let mut h = board("pen_seams", BoardTool::Pen);
    let mut raster = FrameRaster::new(1440, 900);
    let side = screen(&h, Pos2::new(0.0, 250.0));
    widen(&mut h, side);
    fade(&mut h, side);
    let tip = h.app.placed_tip(StrokeTool::Pen);
    assert!(
        tip.width > 8.0 && (0.3..0.7).contains(&tip.opacity),
        "a wide, half-clear tip: {tip:?}"
    );
    let park = Pos2::new(0.0, 250.0);
    hover(&mut h, park);
    raster_frame(&mut h, &mut raster);
    let base = raster.px.clone();
    // Above the empty board's hint text.
    let at = |i: usize| Pos2::new(-500.0 + 2.0 * i as f32 + 0.25, -150.37);
    stroke_start(&mut h, at(0));
    stroke_through(&mut h, (1..=70).map(at));
    let turn = screen(&h, at(70));
    recolor(&mut h, turn);
    stroke_through(&mut h, (71..=400).map(at));
    let pieces = h.app.draft_ink.pen_pieces();
    assert!(pieces >= 5, "the stroke paints in pieces ({pieces})");
    let Some(BoardDrag::FreehandPen { stroke }) = &h.app.board_drag else {
        panic!("the pen stroke is live");
    };
    let pts = stroke.points.clone();
    let tips = stroke.tips.clone();
    assert!(
        tips[64..129].iter().any(|t| *t != tips[64]) && tips[129..].iter().all(|t| *t == tips[129]),
        "the second piece blends the color change, the rest are plain"
    );
    raster_frame(&mut h, &mut raster);
    let column = |x: i32| -> f32 {
        let y0 = screen(&h, pts[0]).y as i32;
        (y0 - 60..=y0 + 60)
            .map(|y| lit(&raster, &base, Pos2::new(x as f32 + 0.5, y as f32 + 0.5)))
            .sum()
    };
    let mut checked = 0;
    for k in 1..pieces - 1 {
        let joint = pts[k * 64].lerp(pts[k * 64 + 1], 0.5);
        let x = screen(&h, joint).x.floor() as i32;
        let around = (column(x - 6) + column(x + 6)) / 2.0;
        assert!(
            around > 1.0,
            "joint {k}: the stroke is on screen ({around})"
        );
        for dx in -1..=1 {
            let c = column(x + dx);
            assert!(
                (c - around).abs() <= 0.02 * around,
                "joint {k}, column {}: {c} against {around} beside it",
                x + dx
            );
        }
        checked += 1;
    }
    assert!(checked >= 3, "checked {checked} joints");
}

/// Art. II: a draft preview repaints its cached mesh on a frame where
/// nothing changed, rebuilds it once when the pointer moves, and a Pen
/// stroke's move rebuilds only the piece still being drawn.
#[test]
fn an_unchanged_draft_frame_does_not_re_tessellate() {
    let still = |h: &mut Harness, what: &str| {
        let built = h.app.draft_ink.builds;
        for _ in 0..4 {
            h.frame();
        }
        assert_eq!(h.app.draft_ink.builds, built, "{what}: a still frame");
    };
    let mut h = board("draft_cache_polyline", BoardTool::Polyline);
    click(&mut h, Pos2::new(0.0, 0.0));
    hover(&mut h, Pos2::new(80.0, 0.0));
    hover(&mut h, Pos2::new(150.0, 0.0));
    click(&mut h, Pos2::new(150.0, 0.0));
    hover(&mut h, Pos2::new(150.0, 120.0));
    assert!(h.app.board_path_draft.is_some(), "the polyline is drawing");
    still(&mut h, "polyline");
    let built = h.app.draft_ink.builds;
    hover(&mut h, Pos2::new(160.0, 130.0));
    h.frame();
    assert_eq!(h.app.draft_ink.builds, built + 1, "a move rebuilds it once");
    still(&mut h, "polyline after the move");

    let mut h = board("draft_cache_line", BoardTool::Line);
    click(&mut h, Pos2::new(0.0, 0.0));
    hover(&mut h, Pos2::new(120.0, 40.0));
    assert!(h.app.line_draft_preview().is_some(), "the line previews");
    still(&mut h, "line");
    // A tip chord or a zoom with the pointer held still rebuilds the mesh
    // once, into the one a fresh build of the same draft makes.
    let fresh = |h: &Harness, what: &str| {
        let (a, b, tips) = h.app.line_draft_preview().expect("the line previews");
        let mut bez = BezPath::new();
        bez.move_to((a.x as f64, a.y as f64));
        bez.line_to((b.x as f64, b.y as f64));
        let (ink, color) = board_path::draft_stroke_ink(&bez, false, &tips, h.app.board_xf().z);
        let (mesh, cached) = h.app.draft_ink.draft_mesh().expect("a cached mesh");
        assert_eq!(cached, color, "{what}: the cached color");
        assert!(
            *mesh == board_path::CachedInkMesh::from(ink),
            "{what}: the cached mesh is the fresh one"
        );
    };
    // What the rubber band is built from. It follows the pointer while a
    // HUD scrubs, so each chord below scrubs across a Tab lock: the end
    // stays put and only the tip changes.
    let inputs = |h: &Harness| {
        let d = h.app.line_draft.as_ref().expect("the line is drawing");
        let z = h.app.board_xf().z;
        (d.cursor, d.start_tip, h.app.placed_tip(StrokeTool::Line), z)
    };
    // One frame of `events`; returns whether it changed the inputs.
    let step = |h: &mut Harness, m: Modifiers, ev: Vec<egui::Event>| {
        let before = inputs(h);
        events(h, m, ev);
        inputs(h) != before
    };
    let right = egui::PointerButton::Secondary;
    // Lock the rubber band along `end` from the start, and hover there.
    let lock = |h: &mut Harness, end: Pos2| {
        if h.app.draft_lock.is_some() {
            key(h, egui::Key::Tab);
        }
        hover(h, end);
        key(h, egui::Key::Tab);
        assert!(h.app.draft_lock.is_some(), "Tab locks the rubber band");
        hover(h, end);
        still(h, "locked");
    };
    let (down, right_of) = (Pos2::new(0.0, 120.0), Pos2::new(120.0, 0.0));
    for (what, m, end, across) in [
        ("Alt+right", ALT, down, Vec2::X * 40.0),
        ("Ctrl+right", CTRL, down, Vec2::X * -40.0),
        ("Shift+right", SHIFT, right_of, Vec2::Y * 50.0),
    ] {
        lock(&mut h, end);
        let at = screen(&h, end);
        let (built, tip) = (h.app.draft_ink.builds, h.app.placed_tip(StrokeTool::Line));
        let mut changed = 0u32;
        changed += step(&mut h, m, vec![egui::Event::PointerMoved(at)]) as u32;
        changed += step(&mut h, m, vec![button(at, right, true, m)]) as u32;
        assert!(h.app.brush_hud.is_some(), "{what}: the HUD opened");
        let to = at + across;
        changed += step(&mut h, m, vec![egui::Event::PointerMoved(to)]) as u32;
        changed += step(&mut h, m, vec![button(to, right, false, m)]) as u32;
        changed += step(&mut h, Modifiers::NONE, vec![egui::Event::PointerMoved(at)]) as u32;
        assert!(h.app.brush_hud.is_none(), "{what}: the HUD closed");
        assert_ne!(
            h.app.placed_tip(StrokeTool::Line),
            tip,
            "{what}: the tip changed"
        );
        assert_eq!(changed, 1, "{what}: only the tip changed, once");
        assert_eq!(h.app.draft_ink.builds, built + 1, "{what}: one rebuild");
        fresh(&h, what);
        still(&mut h, what);
    }
    let built = h.app.draft_ink.builds;
    let mut changed = 0u32;
    changed += step(
        &mut h,
        Modifiers::NONE,
        vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, 60.0),
            modifiers: Modifiers::NONE,
        }],
    ) as u32;
    let mut steady = 0;
    while steady < 4 {
        if step(&mut h, Modifiers::NONE, vec![]) {
            changed += 1;
            steady = 0;
        } else {
            steady += 1;
        }
    }
    assert!(changed >= 1, "the wheel zooms the board");
    assert_eq!(
        h.app.draft_ink.builds - built,
        changed,
        "one rebuild for each frame the zoom changed"
    );
    fresh(&h, "zoom");
    still(&mut h, "after the zoom");

    let mut h = board("draft_cache_pen", BoardTool::Pen);
    stroke_start(&mut h, Pos2::new(0.0, 0.0));
    stroke_through(
        &mut h,
        (1..=300).map(|i| Pos2::new(i as f32 * 2.0, (i as f32 * 0.05).sin() * 40.0)),
    );
    let pieces = h.app.draft_ink.pen_pieces();
    assert!(pieces >= 4, "a long stroke paints in pieces ({pieces})");
    let hashed = h.app.draft_ink.pen_hashes;
    still(&mut h, "pen");
    assert_eq!(
        h.app.draft_ink.pen_hashes,
        hashed + 4,
        "a still frame hashes only the live piece, not all {pieces}"
    );
    let built = h.app.draft_ink.builds;
    stroke_through(&mut h, [Pos2::new(601.0, 0.0)]);
    h.frame();
    let rebuilt = h.app.draft_ink.builds - built;
    assert!(
        (1..=2).contains(&rebuilt),
        "a move rebuilds only the live piece, not all {pieces} ({rebuilt})"
    );
    stroke_end(&mut h, Pos2::new(601.0, 0.0));
    assert_eq!(
        h.app.draft_ink.pen_pieces(),
        0,
        "the release drops the pieces"
    );
}

#[test]
fn a_brush_stroke_tweens_size_color_and_opacity_changed_mid_stroke() {
    let mut h = board("chord_brush", BoardTool::Brush);
    h.app.brush_width = 8.0;
    h.app.brush_opacity = 1.0;
    stroke_start(&mut h, Pos2::new(0.0, 0.0));
    stroke_through(&mut h, run(0.0, 200.0));
    let at = screen(&h, Pos2::new(200.0, 0.0));
    widen(&mut h, at);
    recolor(&mut h, at);
    fade(&mut h, at);
    stroke_through(&mut h, run(200.0, 400.0));
    let Some(BoardDrag::FreehandBrush { stroke }) = &h.app.board_drag else {
        panic!("the brush stroke is live");
    };
    assert_freehand_blends(&stroke.tips, |t| t.width);
    let (first, last) = (stroke.tips[0], *stroke.tips.last().unwrap());
    assert_differs(
        first.color,
        last.color,
        "the live stroke takes the new color",
    );
    assert!(
        alpha(&last) < alpha(&first),
        "the live stroke takes the fade"
    );
    assert_eq!(stroke.tips.len(), stroke.points.len(), "a tip per sample");
    stroke_end(&mut h, Pos2::new(400.0, 0.0));
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "one brush stroke");
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Shape(s) = &node.kind else {
        panic!("a shape");
    };
    let path = s.path.as_ref().unwrap();
    assert_eq!(path.tips.len(), path.segs.len() + 1, "one tip per vertex");
    let (first, last) = (path.tips[0], *path.tips.last().unwrap());
    assert!(last.width > first.width + 10.0, "{first:?} → {last:?}");
    assert_differs(first.color, last.color, "the rest of the stroke's color");
    assert!(
        alpha(&last) < alpha(&first),
        "the rest of the stroke's fade"
    );
    let r = node.rect;
    assert!(
        r.y > -1.0 && r.y + r.h < 1.0,
        "the HUD scrubs drew no ink: {r:?}"
    );
    assert!(h.app.board_drag.is_none());
    undo_removes_the_curve(&mut h);
}
