//! Square wires converging on one node (user, 28 September 2026): each
//! connection gets a bounded zone; wires spread across it at the preferred
//! spacing, pack evenly denser inside it when full, arrive sorted so they
//! do not cross, and export the path the board draws.

use super::board_wire::drawn_connector;
use super::tests::Harness;
use super::SlateApp;
use eframe::egui::Pos2;
use slate_doc::scene::{ConnectorEnd, NodeKind, Side, WorldRect};
use slate_doc::{
    filleted_polyline, ConnectorPath, NodeId, PathCmd, ViewKind, WireRouting, CONNECTION_ZONE,
    ORTHO_CORNER_RADIUS,
};

fn board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.board_wire_routing = WireRouting::Orthogonal;
    h.frame();
    h
}

fn add_box(app: &mut SlateApp, rect: WorldRect) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let node = app.doc_mut().scene.build_node(
        rect,
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: Some(slate_doc::scene::Rgba::WHITE),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    app.add_nodes(vec![node])[0]
}

/// The node every wire converges on: its left edge spans y -80..80, so the
/// port centre is (600, 0) and the zone fits inside the edge.
const TARGET: WorldRect = WorldRect {
    x: 600.0,
    y: -80.0,
    w: 120.0,
    h: 160.0,
};

/// Sources left of the target at `(x, centre y)`, each wired from its right
/// edge to the target's left edge. Returns the wires.
fn fan_in(h: &mut Harness, sources: &[(f32, f32)]) -> Vec<NodeId> {
    let target = add_box(&mut h.app, TARGET);
    let mut wires = Vec::new();
    for &(x, y) in sources {
        let from = add_box(&mut h.app, WorldRect::new(x, y - 20.0, 80.0, 40.0));
        let wire = h.app.build_connector(
            ConnectorEnd::Anchored {
                node: from,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: target,
                side: Side::Left,
                t: 0.5,
            },
        );
        wires.push(h.app.add_nodes(vec![wire])[0]);
    }
    h.frame();
    wires
}

fn route(h: &Harness, wire: NodeId) -> Vec<[f32; 2]> {
    let NodeKind::Connector(c) = &h.app.doc().scene.node(wire).unwrap().kind else {
        panic!("a connector");
    };
    match h.app.connector_path_visible(wire, c) {
        Some(ConnectorPath::Orthogonal(pts)) => pts,
        other => panic!("a square route, not {other:?}"),
    }
}

/// Where each wire meets the target's edge, along it, from the port centre.
fn arrivals(h: &Harness, wires: &[NodeId]) -> Vec<f32> {
    wires
        .iter()
        .map(|w| {
            let end = *route(h, *w).last().unwrap();
            assert!((end[0] - TARGET.x).abs() < 0.5, "ends on the left edge");
            end[1]
        })
        .collect()
}

fn sorted(mut v: Vec<f32>) -> Vec<f32> {
    v.sort_by(f32::total_cmp);
    v
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

/// Axis-aligned segments touch when their boxes overlap, within `slack`.
fn segments_touch(p: [[f32; 2]; 2], q: [[f32; 2]; 2], slack: f32) -> bool {
    let bx = |s: [[f32; 2]; 2]| {
        (
            s[0][0].min(s[1][0]),
            s[0][0].max(s[1][0]),
            s[0][1].min(s[1][1]),
            s[0][1].max(s[1][1]),
        )
    };
    let (a, b) = (bx(p), bx(q));
    a.0 <= b.1 + slack && b.0 <= a.1 + slack && a.2 <= b.3 + slack && b.2 <= a.3 + slack
}

/// No two wires cross or run on top of each other, and none runs through a
/// node body.
fn assert_clean_fan(h: &Harness, wires: &[NodeId]) {
    let routes: Vec<Vec<[f32; 2]>> = wires.iter().map(|w| route(h, *w)).collect();
    for i in 0..routes.len() {
        for j in i + 1..routes.len() {
            for p in routes[i].windows(2) {
                for q in routes[j].windows(2) {
                    assert!(
                        !segments_touch([p[0], p[1]], [q[0], q[1]], 0.5),
                        "wires {i} and {j} meet: {p:?} {q:?}"
                    );
                }
            }
        }
    }
    let bodies: Vec<WorldRect> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| !matches!(n.kind, NodeKind::Connector(_)))
        .map(|n| n.rect)
        .collect();
    for (w, pts) in routes.iter().enumerate() {
        for seg in pts.windows(2) {
            for r in &bodies {
                for k in 0..=16 {
                    let t = k as f32 / 16.0;
                    let x = seg[0][0] + (seg[1][0] - seg[0][0]) * t;
                    let y = seg[0][1] + (seg[1][1] - seg[0][1]) * t;
                    let inside = x > r.x + 0.5
                        && x < r.x + r.w - 0.5
                        && y > r.y + 0.5
                        && y < r.y + r.h - 0.5;
                    assert!(!inside, "wire {w} runs through a node at ({x}, {y})");
                }
            }
        }
    }
}

/// Each wire's exported `<path d>`, relative to its start, matches the
/// board's drawn path for the same wire (Art. IV).
pub(super) fn export_matches_board(h: &Harness, wires: &[NodeId]) {
    let relative =
        |v: Vec<f32>| -> Vec<f32> { v.iter().enumerate().map(|(i, x)| x - v[i % 2]).collect() };
    let mut board: Vec<Vec<f32>> = Vec::new();
    let mut css = String::new();
    for &wire in wires {
        let NodeKind::Connector(c) = &h.app.doc().scene.node(wire).unwrap().kind else {
            panic!("a connector");
        };
        // `render_html` passes no theme, so unauthored wires take the light gray.
        css = c
            .paint_color(slate_doc::scene::Rgba::opaque(0x6e, 0x76, 0x80))
            .css();
        let path = h.app.connector_path_visible(wire, c).unwrap();
        let (path, _) = drawn_connector(&h.app.doc().scene, path, c);
        let pts: Vec<[f32; 2]> = match &path {
            ConnectorPath::Bezier(b) => vec![b.p0, b.c1, b.c2, b.p3],
            ConnectorPath::Orthogonal(pts) => filleted_polyline(pts, ORTHO_CORNER_RADIUS)
                .into_iter()
                .flat_map(|cmd| match cmd {
                    PathCmd::Move(p) | PathCmd::Line(p) => vec![p],
                    PathCmd::Cubic { c1, c2, to } => vec![c1, c2, to],
                })
                .collect(),
        };
        board.push(relative(pts.into_iter().flatten().collect()));
    }
    let html = slate_artifact::render_html(h.app.doc(), &slate_artifact::AssetMap::default());
    let tail = format!("\" fill=\"none\" stroke=\"{css}\"");
    let exported: Vec<Vec<f32>> = html
        .match_indices(&tail)
        .map(|(at, _)| {
            let d = &html[..at];
            let d = &d[d.rfind("<path d=\"").unwrap() + 9..];
            relative(
                d.split_whitespace()
                    .filter_map(|t| t.parse().ok())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(exported.len(), board.len(), "every wire exports");
    for want in &board {
        assert!(
            exported.iter().any(|got| got.len() == want.len()
                && got.iter().zip(want).all(|(g, w)| (g - w).abs() < 0.2)),
            "the export draws the board's routed path: {want:?} in {exported:?}"
        );
    }
}

#[test]
fn three_converging_wires_spread_across_the_zone_at_the_preferred_spacing() {
    let mut h = board("lanes_three");
    let wires = fan_in(&mut h, &[(0.0, -150.0), (0.0, 0.0), (0.0, 150.0)]);
    let gap = CONNECTION_ZONE.spacing.preferred;
    let at = sorted(arrivals(&h, &wires));
    assert!(
        close(at[0], -gap) && close(at[1], 0.0) && close(at[2], gap),
        "preferred spacing, centred on the port: {at:?}"
    );
    assert_clean_fan(&h, &wires);
    export_matches_board(&h, &wires);
}

#[test]
fn twelve_converging_wires_pack_evenly_denser_inside_the_same_zone() {
    let mut h = board("lanes_twelve");
    let sources: Vec<(f32, f32)> = (0..12).map(|k| (0.0, -330.0 + 60.0 * k as f32)).collect();
    let wires = fan_in(&mut h, &sources);
    let half = CONNECTION_ZONE.width * 0.5;
    let at = sorted(arrivals(&h, &wires));
    assert!(
        at.iter().all(|y| y.abs() <= half + 0.05),
        "every wire lands inside the zone: {at:?}"
    );
    let steps: Vec<f32> = at.windows(2).map(|w| w[1] - w[0]).collect();
    let dense = half / 6.0;
    assert!(dense < CONNECTION_ZONE.spacing.preferred);
    for (i, s) in steps.iter().enumerate() {
        // The port centre stays free for a wire that runs straight in.
        let want = if i == 5 { dense * 2.0 } else { dense };
        assert!(close(*s, want), "even, denser spacing: {steps:?}");
    }
    assert_clean_fan(&h, &wires);
    export_matches_board(&h, &wires);
}

#[test]
fn converging_wires_arrive_sorted_and_do_not_cross() {
    let mut h = board("lanes_sorted");
    // Staggered in depth and out of order on the board, so id order and
    // arrival order disagree.
    let wires = fan_in(
        &mut h,
        &[
            (120.0, 60.0),
            (-200.0, -260.0),
            (60.0, 220.0),
            (-80.0, -90.0),
            (-260.0, 380.0),
        ],
    );
    let at = arrivals(&h, &wires);
    let source_y = [60.0, -260.0, 220.0, -90.0, 380.0];
    let mut by_source: Vec<usize> = (0..wires.len()).collect();
    by_source.sort_by(|a, b| f32::total_cmp(&source_y[*a], &source_y[*b]));
    let ordered: Vec<f32> = by_source.iter().map(|&i| at[i]).collect();
    assert!(
        ordered.windows(2).all(|w| w[0] < w[1]),
        "arrivals keep the sources' order along the edge: {ordered:?}"
    );
    assert_clean_fan(&h, &wires);
    export_matches_board(&h, &wires);
}

/// Before and after of a many-to-one fan-in, as PNGs under
/// `target/wire-preview/`. "Before" replays the removed midpoint-rail rule.
#[test]
#[ignore = "writes preview images"]
fn fan_in_preview_png() {
    use slate_doc::{connector_ortho_path, scene_wire_hosts, scene_wire_obstacles, OrthoLane};
    let mut h = board("lanes_preview");
    let sources: Vec<(f32, f32)> = (0..7)
        .map(|k| (-60.0 * (k % 3) as f32, -270.0 + 90.0 * k as f32))
        .collect();
    let wires = fan_in(&mut h, &sources);
    let scene = h.app.doc().scene.clone();
    let after: Vec<Vec<[f32; 2]>> = wires.iter().map(|w| route(&h, *w)).collect();
    // The rule this change replaced: nearest dest innermost, a midpoint
    // rail per rank, and a fixed 8-unit fan.
    let port = [TARGET.x, 0.0];
    let mut pos: Vec<(f32, usize)> = Vec::new();
    let mut neg: Vec<(f32, usize)> = Vec::new();
    for (i, &(_, y)) in sources.iter().enumerate() {
        let d = y - port[1];
        if d > 12.0 {
            pos.push((d, i));
        } else if d < -12.0 {
            neg.push((-d, i));
        }
    }
    let mut old = vec![OrthoLane::default(); wires.len()];
    for (list, sign) in [(&mut pos, 1.0f32), (&mut neg, -1.0)] {
        list.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (r, &(_, i)) in list.iter().enumerate() {
            old[i].end_along = sign * (r as f32 + 1.0) * 8.0;
            old[i].rail = sign * (r as f32 + 1.0) * 10.0;
        }
    }
    let obstacles = scene_wire_obstacles(&scene);
    let before: Vec<Vec<[f32; 2]>> = wires
        .iter()
        .zip(&old)
        .map(|(w, lane)| {
            let NodeKind::Connector(c) = &scene.node(*w).unwrap().kind else {
                unreachable!()
            };
            connector_ortho_path(&c.a, &c.b, scene_wire_hosts(&scene), &obstacles, *lane).unwrap()
        })
        .collect();
    let boxes: Vec<WorldRect> = scene
        .nodes
        .iter()
        .filter(|n| !matches!(n.kind, NodeKind::Connector(_)))
        .map(|n| n.rect)
        .collect();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/wire-preview");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, routes) in [("fan-in-before.png", &before), ("fan-in-after.png", &after)] {
        let path = dir.join(name);
        draw_preview(&boxes, routes).save(&path).unwrap();
        println!("{}", path.canonicalize().unwrap().display());
    }
}

/// Boxes and wires on white, fit to the picture, 2 px wires.
fn draw_preview(boxes: &[WorldRect], routes: &[Vec<[f32; 2]>]) -> image::RgbImage {
    let (w, h) = (900u32, 700u32);
    let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for r in boxes {
        min = min.min(Pos2::new(r.x, r.y));
        max = max.max(Pos2::new(r.x + r.w, r.y + r.h));
    }
    let pad = 30.0;
    let s =
        ((w as f32 - 2.0 * pad) / (max.x - min.x)).min((h as f32 - 2.0 * pad) / (max.y - min.y));
    let at = |p: [f32; 2]| ((p[0] - min.x) * s + pad, (p[1] - min.y) * s + pad);
    let mut img = image::RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]));
    let dot = |img: &mut image::RgbImage, x: f32, y: f32, c: [u8; 3], r: i32| {
        for dy in -r..=r {
            for dx in -r..=r {
                let (px, py) = (x.round() as i32 + dx, y.round() as i32 + dy);
                if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                    img.put_pixel(px as u32, py as u32, image::Rgb(c));
                }
            }
        }
    };
    for r in boxes {
        let (x0, y0) = at([r.x, r.y]);
        let (x1, y1) = at([r.x + r.w, r.y + r.h]);
        for x in x0 as i32..=x1 as i32 {
            dot(&mut img, x as f32, y0, [90, 90, 90], 0);
            dot(&mut img, x as f32, y1, [90, 90, 90], 0);
        }
        for y in y0 as i32..=y1 as i32 {
            dot(&mut img, x0, y as f32, [90, 90, 90], 0);
            dot(&mut img, x1, y as f32, [90, 90, 90], 0);
        }
    }
    let palette = [
        [214, 39, 40],
        [31, 119, 180],
        [44, 160, 44],
        [148, 103, 189],
        [255, 127, 14],
        [23, 190, 207],
        [140, 86, 75],
    ];
    for (i, pts) in routes.iter().enumerate() {
        let c = palette[i % palette.len()];
        for seg in pts.windows(2) {
            let (a, b) = (at(seg[0]), at(seg[1]));
            let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) as usize).max(1);
            for k in 0..=n {
                let t = k as f32 / n as f32;
                dot(&mut img, a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, c, 1);
            }
        }
    }
    img
}
