//! Pen fit and Smoothing brush acceptance through the app (pen.md GP3,
//! smooth.md golden paths and behavior matrix).

use super::board::BoardTool;
use super::tests::Harness;
use eframe::egui::Pos2;
use slate_doc::scene::{NodeKind, PathSeg, ShapeKind};
use slate_doc::ViewKind;

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

fn path_of(n: &slate_doc::scene::Node) -> (ShapeKind, slate_doc::scene::PathData) {
    let NodeKind::Shape(s) = &n.kind else {
        panic!("a shape");
    };
    (s.shape, s.path.as_deref().cloned().unwrap_or_default())
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
