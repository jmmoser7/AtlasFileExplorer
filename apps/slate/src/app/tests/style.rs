//! Color, opacity, and shared style-memory tests.

use super::*;

#[test]
fn accepted_colors_from_object_text_vertex_and_brush_editors_share_history() {
    use board_properties::{Property, PropertyRequest};
    let mut h = Harness::new("shared_color_history");
    h.app
        .place_default_at(board::BoardTool::RectShape, Pos2::ZERO);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    for n in 0..8 {
        h.app.remember_document_colors(vec![[n, 30, 50]]);
    }
    for (edit, points, rgb) in [
        (Property::FillRgb([90, 20, 30]), vec![], [90, 20, 30]),
        (Property::StrokeRgb([40, 150, 60]), vec![], [40, 150, 60]),
        (Property::TextRgb([70, 80, 190]), vec![], [70, 80, 190]),
        (Property::StrokeRgb([200, 40, 50]), vec![1], [200, 40, 50]),
    ] {
        let detail = serde_json::to_string(&PropertyRequest {
            ids: vec![id],
            edits: vec![edit],
            points,
        })
        .unwrap();
        assert!(h.app.shape_property_command(Some(&detail)));
        assert_eq!(h.app.doc().view.recent_color_swatches().as_slice()[0], rgb);
        assert!(h
            .app
            .doc()
            .view
            .recent_colors
            .as_ref()
            .unwrap()
            .contains(&rgb));
    }
    h.app.adopt_sampled_brush_color([10, 210, 120]);
    assert_eq!(
        h.app.doc().view.recent_color_swatches().as_slice()[0],
        [10, 210, 120]
    );
    h.app.adopt_board_color_rgb([220, 110, 10]);
    assert_eq!(
        h.app.doc().view.recent_color_swatches().as_slice()[0],
        [220, 110, 10]
    );
    assert_eq!(h.app.doc().view.recent_color_swatches().as_slice().len(), 6);
}

#[test]
fn repeating_a_color_already_in_slot_zero_persists_its_new_age() {
    let mut h = Harness::new("color_reuse_age");
    h.app
        .remember_document_colors(vec![[10, 20, 30], [40, 50, 60]]);
    h.app.remember_document_colors(vec![[10, 20, 30]]);
    let wheel = h.app.doc().view.recent_colors.clone();
    let ages = h.app.doc().view.recent_color_used.clone();
    h.app.tab_mut().dirty = false;
    h.app.remember_document_colors(vec![[10, 20, 30]]);
    assert_eq!(h.app.doc().view.recent_colors, wheel);
    assert_ne!(h.app.doc().view.recent_color_used, ages);
    assert!(
        h.app.tab().dirty,
        "refreshed usage ages must survive save/reopen"
    );
}

#[test]
fn text_ink_commits_enter_history_while_alpha_and_geometry_do_not() {
    use board_properties::{Property, PropertyRequest};
    let mut h = Harness::new("text_color_history");
    h.app.place_sticky_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    for edit in [Property::TextRgb([50, 160, 70]), Property::TextAlpha(128)] {
        let before = h.app.doc().view.recent_color_clock;
        let rgb_edit = matches!(edit, Property::TextRgb(_));
        let detail = serde_json::to_string(&PropertyRequest {
            ids: vec![id],
            edits: vec![edit],
            points: vec![],
        })
        .unwrap();
        assert!(h.app.shape_property_command(Some(&detail)));
        assert_eq!(h.app.doc().view.recent_color_clock > before, rgb_edit);
        assert_eq!(
            h.app.doc().view.recent_color_swatches().as_slice()[0],
            [50, 160, 70]
        );
    }
    let before = h.app.doc().view.recent_color_clock;
    h.app.patch_nodes(&[id], |node| node.rect.x += 20.0);
    assert_eq!(h.app.doc().view.recent_color_clock, before);
}

#[test]
fn a_color_dot_moves_the_pointer_and_leaves_the_wheel() {
    let mut h = Harness::new("brush_dot");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.tab_mut().doc.view.recent_colors = Some(vec![[255, 0, 0]]);
    h.app.ctrl_down = true;
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(400.0, 300.0)), true, true));
    let center = match h.app.brush_hud {
        Some(board_color::BrushHud::Wheel { center, .. }) => center,
        other => panic!("wheel opened, got {other:?}"),
    };
    let slot = board_color::wheel_slot_offset(0, 24, board_color::WHEEL_SLOT_RADIUS);
    let dot = center + egui::vec2(slot[0], slot[1]);
    let approach = dot + egui::vec2(7.0, 0.0);
    assert!(h.app.drive_brush_hud(Some(approach), true, false));
    match h.app.brush_hud {
        Some(board_color::BrushHud::Wheel { center: now, .. }) => {
            assert_eq!(now, center);
        }
        other => panic!("wheel stayed open, got {other:?}"),
    }
    assert_eq!(h.app.board_colors.fg.0[0], 255);
    assert_eq!(h.app.brush_cursor_warp, Some((approach, dot)));
}

/// Stated intent: per-tool memory is saved with the workbook.
#[test]
fn per_tool_style_memory_survives_save_and_reopen() {
    let mut h = line_board("tool_style_save");
    let line = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(50.0, 0.0))
        .unwrap();
    restyle(&mut h, line, 6.0, [40, 50, 60]);
    draw_pen(&mut h, 100.0);
    let (pen, _) = last_stroke(&h);
    restyle(&mut h, pen, 3.0, [70, 80, 90]);
    let path = h.base.join("styles.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());
    drop(h);
    // App teardown queues lease cleanup on the shared maintenance worker. Model
    // process exit before reopening so the next app cannot race that release.
    slate_doc::Lease::flush_pending_releases(std::time::Duration::from_secs(2))
        .expect("saved workbook lease release completes before relaunch");

    let mut h2 = Harness::new("tool_style_reopen");
    h2.app.open_doc_at(path);
    h2.frame();
    assert!(!h2.app.tab().read_only, "the reopened workbook is editable");
    h2.app.doc_mut().view.active_view = ViewKind::Board;
    h2.app.set_board_tool(board::BoardTool::Line);
    h2.app
        .commit_line(Pos2::new(0.0, 200.0), Pos2::new(50.0, 200.0))
        .unwrap();
    let (_, line_stroke) = last_stroke(&h2);
    assert_eq!(line_stroke.width, 6.0);
    assert_eq!(line_stroke.color, Rgba([40, 50, 60, 255]));
    draw_pen(&mut h2, 300.0);
    let (_, pen_stroke) = last_stroke(&h2);
    assert_eq!(pen_stroke.width, 3.0);
    assert_eq!(pen_stroke.color, Rgba([70, 80, 90, 255]));
}

/// Closed shapes keep one shared memory, and curve edits stay out of it.
#[test]
fn closed_shapes_still_share_style_memory() {
    let mut h = line_board("closed_style_shared");
    h.app
        .place_default_at(board::BoardTool::RectShape, Pos2::new(0.0, 0.0));
    let (rect, _) = last_stroke(&h);
    h.app.patch_nodes(&[rect], |n| {
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke.width = 4.0;
            s.stroke.color = Rgba([200, 100, 0, 255]);
            s.fill = Some(Rgba([0, 90, 180, 255]));
        }
    });
    let line = h
        .app
        .commit_line(Pos2::new(0.0, 300.0), Pos2::new(50.0, 300.0))
        .unwrap();
    restyle(&mut h, line, 11.0, [9, 9, 9]);
    h.app
        .place_default_at(board::BoardTool::Ellipse, Pos2::new(300.0, 0.0));
    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(s) = &node.kind else {
        panic!("ellipse");
    };
    assert_eq!(s.stroke.width, 4.0);
    assert_eq!(s.stroke.color, Rgba([200, 100, 0, 255]));
    assert_eq!(s.fill, Some(Rgba([0, 90, 180, 255])));
}

#[test]
fn the_wheel_gap_keeps_the_color_and_only_outside_samples() {
    use board_color::{
        sample_wheel, WheelHit, WHEEL_BACKDROP_RADIUS, WHEEL_HUE_INNER, WHEEL_SV_RADIUS,
    };
    let hsv = [0.3, 0.5, 0.5];
    // Between the saturation/value disc and the hue ring: a dead strip that
    // keeps the value and never reaches for the eyedropper.
    for r in [
        WHEEL_SV_RADIUS + 0.5,
        WHEEL_SV_RADIUS + 4.0,
        WHEEL_HUE_INNER - 0.5,
    ] {
        for a in [0.0_f32, 1.3, 2.9, 4.4] {
            assert!(matches!(
                sample_wheel([a.cos() * r, a.sin() * r], hsv, &[]),
                WheelHit::Keep
            ));
        }
    }
    assert!(matches!(
        sample_wheel([WHEEL_HUE_INNER + 1.0, 0.0], hsv, &[]),
        WheelHit::Field(..)
    ));
    // Between the ring and the swatches: keep the value.
    assert!(matches!(
        sample_wheel([0.0, -(WHEEL_BACKDROP_RADIUS - 4.0)], hsv, &[]),
        WheelHit::Keep
    ));
    assert!(matches!(
        sample_wheel([0.0, -(WHEEL_BACKDROP_RADIUS + 4.0)], hsv, &[]),
        WheelHit::Outside
    ));
}

/// The Alt+right-drag size circle stays on the press point and grows about
/// that center. The pointer's travel only changes the diameter.
#[test]
fn the_size_hud_circle_stays_on_the_press_point() {
    let mut h = line_board("size_hud_center");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.tab_mut().cam.z = 1.0;
    h.app.brush_width = 20.0;
    h.frame();
    let c = h.app.canvas_rect.center();
    h.frame_with(pointer_to(c, true));
    h.frame_with(right_button(c, true, true));
    let to = c + EVec2::new(40.0, -30.0);
    let out = h.frame_output(|input| {
        input.modifiers.alt = true;
        input.events.push(egui::Event::PointerMoved(to));
    });
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Size { .. })
    ));
    assert!(
        (h.app.brush_width - (20.0 + board_color::SIZE_DRAG_GAIN * 40.0)).abs() < 0.5,
        "40 px right from a 20 px tip gains 2 px of diameter per px, got {}",
        h.app.brush_width
    );
    let r = 10.0 + 40.0;
    assert!((h.app.brush_width * 0.5 - r).abs() < 0.5);
    let mut rings = Vec::new();
    fn walk(shape: &egui::Shape, r: f32, acc: &mut Vec<Pos2>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, r, acc)),
            egui::Shape::Circle(c) if (c.radius - r).abs() < 0.5 && c.stroke.width > 0.0 => {
                acc.push(c.center)
            }
            _ => {}
        }
    }
    for clipped in &out.shapes {
        walk(&clipped.shape, r, &mut rings);
    }
    assert!(!rings.is_empty(), "the size HUD painted its width ring");
    for center in rings {
        assert!(
            center.distance(c) < 0.5,
            "the size ring sits at {center:?}, the press was {c:?} (pointer moved to {to:?})"
        );
    }
    h.frame_with(right_button(to, false, false));
}
