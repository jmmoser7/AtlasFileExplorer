//! Join golden-path tests.

use super::*;

/// GP1 — two open segments join at nearest ends (existing style rule).
#[test]
fn join_gp1_open_paths() {
    super::path_edit::join_two_open_paths_keeps_first_style();
}

/// GP2 — overlapping filled rects union into one region.
#[test]
fn join_gp2_closed_union() {
    let mut h = join_board("join_gp2");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 80.0, 80.0);
    let b = add_filled_rect(&mut h.app, 40.0, 40.0, 80.0, 80.0);
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h.app.cmd_join());
    let (_, contours) = joined_contours(&h.app);
    assert!(vector_ink::point_in_polygon(&contours, [10.0, 10.0]));
    assert!(vector_ink::point_in_polygon(&contours, [100.0, 100.0]));
    assert!(vector_ink::point_in_polygon(&contours, [50.0, 50.0]));
    assert!(!vector_ink::point_in_polygon(&contours, [10.0, 100.0]));
    assert_axis_aligned_world(&contours);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    h.frame();
}

/// Joined concave union must earcut both lobes (egui PathShape fans tear this).
#[test]
fn join_fill_mesh_covers_concave_union() {
    let mut h = join_board("join_fill_mesh");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 80.0, 80.0);
    let b = add_filled_rect(&mut h.app, 40.0, 40.0, 80.0, 80.0);
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h.app.cmd_join());
    let (_, contours) = joined_contours(&h.app);
    let (verts, idx) = vector_ink::fill_triangles(&contours);
    assert!(vector_ink::point_in_mesh(&verts, &idx, [10.0, 10.0]));
    assert!(vector_ink::point_in_mesh(&verts, &idx, [100.0, 100.0]));
    assert!(vector_ink::point_in_mesh(&verts, &idx, [50.0, 50.0]));
    assert!(!vector_ink::point_in_mesh(&verts, &idx, [10.0, 100.0]));
    h.frame();
}

/// GP3 — open stroke + filled rect: the ribbon outside the rect is kept.
#[test]
fn join_gp3_open_plus_closed() {
    let mut h = join_board("join_gp3");
    let rect = add_filled_rect(&mut h.app, 40.0, 20.0, 40.0, 40.0);
    let line = add_seg(&mut h.app, Pos2::new(0.0, 40.0), Pos2::new(140.0, 40.0));
    h.app.board_sel = [rect, line].into_iter().collect();
    assert!(h.app.cmd_join());
    let (_, contours) = joined_contours(&h.app);
    assert!(
        vector_ink::point_in_polygon(&contours, [10.0, 40.0]),
        "stroke ribbon left of the rect must remain"
    );
    assert!(vector_ink::point_in_polygon(&contours, [60.0, 40.0]));
    h.frame();
}

/// GP4 — nested rects union to the outer (no hole).
#[test]
fn join_gp4_nested_union() {
    let mut h = join_board("join_gp4");
    let outer = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    let inner = add_filled_rect(&mut h.app, 30.0, 30.0, 20.0, 20.0);
    h.app.board_sel = [outer, inner].into_iter().collect();
    assert!(h.app.cmd_join());
    let (_, contours) = joined_contours(&h.app);
    assert!(vector_ink::point_in_polygon(&contours, [10.0, 10.0]));
    assert!(
        vector_ink::point_in_polygon(&contours, [40.0, 40.0]),
        "inner rect must not become a hole"
    );
    h.frame();
}

/// GP5 — disjoint rects are a no-op (Rhino; Group is Ctrl+G).
#[test]
fn join_gp5_disjoint_is_noop() {
    let mut h = join_board("join_gp5");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 20.0, 20.0);
    let b = add_filled_rect(&mut h.app, 80.0, 80.0, 20.0, 20.0);
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(!h.app.cmd_join());
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    assert!(h.app.toasts.iter().any(|(m, _)| m.contains("do not touch")));
    h.frame();
}

/// A touching pair unions; a far object stays its own node.
#[test]
fn join_leaves_a_far_object() {
    let mut h = join_board("join_far");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    let b = add_filled_rect(&mut h.app, 20.0, 20.0, 40.0, 40.0);
    let c = add_filled_rect(&mut h.app, 200.0, 200.0, 20.0, 20.0);
    h.app.board_sel = [a, b, c].into_iter().collect();
    assert!(h.app.cmd_join());
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    assert!(
        h.app.doc().scene.node(c).is_some(),
        "far rect must stay editable"
    );
    h.frame();
}

/// GP6 — typed / dock command is the same as Ctrl+J.
#[test]
fn join_gp6_command_id() {
    let mut h = join_board("join_gp6");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    let b = add_filled_rect(&mut h.app, 20.0, 20.0, 40.0, 40.0);
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.path.join"), None));
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.frame();
}

/// One closed shape alone is a no-op (inferred D11).
#[test]
fn join_one_closed_is_noop() {
    let mut h = join_board("join_one_closed");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    h.app.board_sel = [a].into_iter().collect();
    assert!(!h.app.cmd_join());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.frame();
}

/// Text is skipped; two rects still union (P2.RhinoJoin.skip).
#[test]
fn join_skips_text() {
    use slate_doc::scene::{TextAlign, TextNode, Typeface};
    let mut h = join_board("join_skips_text");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    let b = add_filled_rect(&mut h.app, 20.0, 20.0, 40.0, 40.0);
    let text = {
        let rect = slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 24.0);
        let node = h.app.doc_mut().scene.build_node(
            rect,
            slate_doc::scene::NodeKind::Text(TextNode {
                text: "keep".into(),
                family: Typeface::Sans,
                size: 14.0,
                color: slate_doc::scene::Rgba::BLACK,
                align: TextAlign::Left,
                fill: None,
                stroke: Default::default(),
                agent: None,
            }),
        );
        h.app.add_nodes(vec![node])[0]
    };
    h.app.board_sel = [a, b, text].into_iter().collect();
    assert!(h.app.cmd_join());
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        2,
        "text remains, rects union"
    );
    assert!(h.app.doc().scene.node(text).is_some());
    h.frame();
}

/// A click on a host selects the host, not the wire painted underneath it.
#[test]
fn wire_under_node_does_not_steal_pick() {
    use slate_doc::scene::{ConnectorEnd, Side};
    let mut h = Harness::new("wire_under");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 200.0, 0.0);
    h.app
        .add_connector(
            ConnectorEnd::Anchored {
                node: a,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: b,
                side: Side::Left,
                t: 0.5,
            },
        )
        .expect("wire");
    let hit = board_path::board_pick_node_routed(
        &h.app.doc().scene,
        40.0,
        30.0,
        1.0,
        false,
        h.app.board_wire_routing,
    );
    assert_eq!(hit, Some(a), "host under the pointer beats the wire");

    let wire = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| n.id != a && n.id != b)
        .unwrap()
        .id;
    let beside = board_path::board_pick_node_routed(
        &h.app.doc().scene,
        84.0,
        30.0,
        1.0,
        false,
        h.app.board_wire_routing,
    );
    assert_ne!(
        beside,
        Some(wire),
        "the pick slop on the neighboring node must not select the wire"
    );
    let mid = board_path::board_pick_node_routed(
        &h.app.doc().scene,
        140.0,
        30.0,
        1.0,
        false,
        h.app.board_wire_routing,
    );
    assert_eq!(mid, Some(wire), "the open span of the wire still hits");
}

#[test]
fn wire_routing_toggle_is_session_not_journaled() {
    let mut h = Harness::new("wire_routing");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    assert_eq!(h.app.board_wire_routing, slate_doc::WireRouting::Bezier);
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.wire.orthogonal"),
        None
    ));
    assert_eq!(h.app.board_wire_routing, slate_doc::WireRouting::Orthogonal);
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.wire.routing"),
        None
    ));
    assert_eq!(h.app.board_wire_routing, slate_doc::WireRouting::Bezier);
}

#[test]
fn a_shift_chain_extends_one_stroke_so_joints_do_not_stack() {
    let mut h = Harness::new("brush_chain");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 12.0;
    h.app.brush_opacity = 0.5;
    h.app
        .finish_freehand_brush(vec![Pos2::new(0.0, 0.0), Pos2::new(60.0, 0.0)]);
    let before = h.app.doc().scene.nodes.len();
    let anchor = h.app.brush_line_anchor().expect("anchor after a stroke");
    let first = anchor.node.expect("anchor names the stroke");
    h.app
        .commit_tween_line(anchor.pos, Pos2::new(60.0, 50.0), anchor.tip, anchor.node);
    let anchor = h.app.brush_line_anchor().unwrap();
    h.app
        .commit_tween_line(anchor.pos, Pos2::new(10.0, 10.0), anchor.tip, anchor.node);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        before,
        "segments extend the stroke instead of stacking new nodes"
    );
    let node = h.app.doc().scene.node(first).unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("path");
    };
    let path = shape.path.clone().unwrap();
    assert_eq!(path.tips.len(), path.segs.len() + 1);
    // One stamp for the whole chain: its opacity never exceeds the brush's.
    let contours = board_path::stamped_contours(node, shape, &path, 0.25);
    let img = vector_ink::stamp_tipped(&contours, 1.0).unwrap();
    let top = img.rgba.iter().skip(3).step_by(4).copied().max().unwrap();
    assert_eq!(top, shape.stroke.color.0[3]);
    // Undo removes only the last segment.
    h.app.board_undo();
    let node = h.app.doc().scene.node(first).unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("path");
    };
    assert_eq!(shape.path.as_ref().unwrap().tips.len(), path.tips.len() - 1);
}

/// Soft brush at 50 % opacity: the moving preview mesh starts butt at the
/// joint with a stamped chain, so nothing behind the joint doubles, and its
/// edge fades over softness × radius as the stamp's does. It is still an
/// approximation of the exact stamp that replaces it.
#[test]
fn a_soft_translucent_moving_preview_does_not_double_behind_the_joint() {
    let mut h = brush_board("r10_soft_joint");
    h.app.board_colors.fg.0 = [255, 40, 40, 255];
    h.app.brush_opacity = 0.5;
    h.app.brush_softness = 0.5;
    h.app.brush_width = 60.0;
    h.app.brush_texture = slate_doc::scene::BrushTexture::Smooth;
    let mut raster = FrameRaster::new(1440, 900);
    capture_frame(&mut h, &mut raster, |_| {});
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let freehand = [
        p(-300.0, -200.0),
        p(-200.0, -200.0),
        p(-100.0, -200.0),
        p(0.0, -200.0),
    ];
    raster_drag(&mut h, &mut raster, egui::Modifiers::NONE, &freehand, false);
    settle_brush_live(&mut h, &mut raster);
    h.app
        .brush_live
        .as_mut()
        .expect("parked canvas")
        .hold_previews = true;
    let xf = h.app.board_xf();
    let shift = egui::Modifiers::SHIFT;
    let mut out = None;
    for (i, w) in [
        p(0.0, -200.0),
        p(0.0, -200.0),
        p(150.0, -200.0),
        p(300.0, -200.0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut events = vec![egui::Event::PointerMoved(xf.w2s(w))];
        if i == 1 {
            events.push(egui::Event::PointerButton {
                pos: xf.w2s(w),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: shift,
            });
        }
        out = Some(capture_frame(&mut h, &mut raster, |inp| {
            inp.modifiers = shift;
            inp.events = events;
        }));
    }
    let canvas = h.app.brush_live.as_ref().expect("live canvas");
    assert!(
        canvas.showing_line() && !canvas.line_exact(),
        "the preview is the moving mesh"
    );
    draw_now(&h, &mut raster, out.unwrap());
    // Estimated from red over the board's background.
    let red = |raster: &FrameRaster, w: Pos2| {
        let s = xf.w2s(w);
        raster.px[s.y as usize * raster.w + s.x as usize][0]
    };
    let bg = red(&raster, p(0.0, 100.0));
    let alpha = |raster: &FrameRaster, w: Pos2| (red(raster, w) - bg) / (1.0 - bg);
    let r = 30.0;
    let chain = alpha(&raster, p(-150.0, -200.0));
    let mesh = alpha(&raster, p(150.0, -200.0));
    assert!((chain - 0.5).abs() < 0.05, "the chain's body is {chain}");
    assert!(
        (mesh - 0.5).abs() < 0.05,
        "the moving mesh's body is {mesh}"
    );
    for back in [2.0, r * 0.5] {
        let a = alpha(&raster, p(-back, -200.0));
        assert!(
            a <= chain + 2.0 / 255.0,
            "{back} behind the joint the preview is {a}, over the body's {chain}"
        );
    }
    let edge = alpha(&raster, p(150.0, -200.0 + 0.75 * r));
    assert!(
        edge > 0.1 * mesh && edge < 0.9 * mesh,
        "the mesh edge at 0.75 r is {edge}: it does not fade as the stamp does"
    );
    let ahead = alpha(&raster, p(r * 0.25, -200.0));
    eprintln!(
        "soft joint: chain {chain:.3} mesh {mesh:.3} edge {edge:.3} r/4 ahead of the joint {ahead:.3}"
    );
    h.app.brush_live.as_mut().unwrap().hold_previews = false;
    wait_brush_live(&mut h, &mut raster, shift, |c| c.line_exact());
    let out = capture_frame(&mut h, &mut raster, |inp| inp.modifiers = shift);
    draw_now(&h, &mut raster, out);
    let exact = alpha(&raster, p(r * 0.25, -200.0));
    assert!(
        exact <= chain + 2.0 / 255.0,
        "the exact stamp doubles at the joint: {exact}"
    );
}
