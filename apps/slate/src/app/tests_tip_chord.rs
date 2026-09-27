//! P1.curve.tip-chord: a tip change made mid-draw through the right-button
//! HUD (size, color, opacity) on Line, Arc, Polyline, Bézier span, Pen and
//! Brush, driven through real frames. The live preview shows the blend, the
//! committed curve keeps it per vertex, and one Ctrl+Z removes the curve.

use super::board::{BoardDrag, BoardTool};
use super::board_color::BrushHud;
use super::board_path;
use super::tests::Harness;
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

fn enter(h: &mut Harness) {
    events(
        h,
        Modifiers::NONE,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
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
