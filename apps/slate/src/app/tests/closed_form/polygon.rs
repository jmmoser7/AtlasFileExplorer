//! Polygon dimensions, vertex rotation, and stroke properties through real input.

use super::*;
use slate_doc::scene::ShapeKind;

#[test]
fn hovering_an_unselected_polygon_vertex_does_not_rotate_the_selected_rect_hint() {
    let (mut h, polygon, _) = closed_form_board("polygon_hover_owner", ShapeKind::RegularPolygon);
    let rectangle = add_rect(&mut h.app, 350.0, 50.0);
    h.app.board_sel = std::iter::once(rectangle).collect();
    h.frame();
    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(polygon).unwrap();
    let center = Pos2::from(n.rect.center());
    let vertex = h.app.closed_form_points(polygon).unwrap()[4];
    let outward = (vertex - center).normalized();
    let at = xf.w2s(vertex + outward * 12.0 + outward.rot90() * 18.0);
    crop_pointer(&mut h, at, None);
    assert_eq!(h.app.board_hover_node, Some(polygon));
    assert_eq!(
        h.app.board_hover_hit,
        Some(board_handles::BoardHitTarget::Rotate(4))
    );
    h.frame();
    assert_eq!(h.app.board_sel, std::iter::once(rectangle).collect());
}

#[test]
fn polygon_rotation_at_every_vertex_yields_to_side_glyphs_and_point_picks() {
    let (mut h, id, _) = closed_form_board("polygon_rotate_zones", ShapeKind::RegularPolygon);
    for sides in [3, 6, 12] {
        h.app.patch_nodes(&[id], |n| {
            board_properties::Property::RegularSides(sides).apply(n, None)
        });
        for zoom in [0.75, 1.0, 2.0] {
            h.app.tab_mut().cam.z = zoom;
            h.app.patch_nodes(&[id], |n| n.rotation_deg = 27.0);
            let center_w = Pos2::from(h.app.doc().scene.node(id).unwrap().rect.center());
            h.app.tab_mut().cam.offset = center_w.to_vec2();
            let xf = h.app.board_xf();
            let center = xf.w2s(Pos2::from(
                h.app.doc().scene.node(id).unwrap().rect.center(),
            ));
            let vertices = h.app.closed_form_points(id).unwrap();
            for (i, vertex) in vertices.iter().enumerate() {
                let v = xf.w2s(*vertex);
                let outward = (v - center).normalized();
                // Approach beside the plus, on the exterior side of the vertex.
                let at = v
                    + outward * (12.0 * zoom).max(10.0)
                    + outward.rot90() * (18.0 * zoom).max(10.0);
                assert!(
                    matches!(h.app.transform_hit_at(at),
                Some((Some(n), board_handles::BoardHitTarget::Rotate(k))) if n == id && k as usize == i),
                    "vertex {i} at zoom {zoom}: {:?}",
                    h.app.transform_hit_at(at)
                );
                crop_pointer(&mut h, at, None);
                assert_eq!(
                    h.app.board_hover_hit,
                    Some(board_handles::BoardHitTarget::Rotate(i as u8))
                );
                // Rendering the last vertices must never index a four-corner box.
                h.frame();
                assert_eq!(h.app.hovered_vertex(v), Some((id, i)));
                assert!(!matches!(
                    h.app.transform_hit_at(v),
                    Some((_, board_handles::BoardHitTarget::Rotate(_)))
                ));
                crop_pointer(&mut h, v, None);
                let glyphs = h.app.polygon_sides_glyphs(&xf);
                assert_eq!(
                    glyphs.len(),
                    if sides == 6 { 2 } else { 1 },
                    "both small controls remain visible at zoom {zoom}"
                );
                for glyph in glyphs {
                    assert!((glyph.radius - 3.0 * zoom).abs() < 1e-5);
                    assert!(!matches!(
                        h.app.transform_hit_at(glyph.center),
                        Some((_, board_handles::BoardHitTarget::Rotate(_)))
                    ));
                    crop_pointer(&mut h, glyph.center, None);
                    assert_ne!(
                        h.app.board_hover_hit,
                        Some(board_handles::BoardHitTarget::Rotate(i as u8))
                    );
                }
            }
        }
    }
}

#[test]
fn polygon_vertex_rotation_drag_is_one_undo_step_and_escape_cancels() {
    let (mut h, id, _) = closed_form_board("polygon_rotate_drag", ShapeKind::RegularPolygon);
    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let center = Pos2::from(n.rect.center());
    // Vertex 4 is deliberately beyond the four box-corner indices.
    let vertex = h.app.closed_form_points(id).unwrap()[4];
    let outward = (vertex - center).normalized();
    let start = vertex + outward * 12.0 + outward.rot90() * 18.0;
    let end = center + EVec2::new(-(start - center).y, (start - center).x);
    let depth = h.app.tab().journal.undo_depth();
    grip_drag(&mut h, start, end, egui::Modifiers::NONE);
    let after = h.app.doc().scene.node(id).unwrap().clone();
    assert!(
        (after.rotation_deg - 90.0).abs() < 0.1,
        "{}",
        after.rotation_deg
    );
    assert_eq!(after.rect, n.rect);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rotation_deg,
        n.rotation_deg
    );
    press_key_with(&mut h, egui::Key::Y, egui::Modifiers::CTRL);
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rotation_deg,
        after.rotation_deg
    );

    // Start another rotate through the same gesture owner, then cancel it.
    h.app.board_drag = h.app.begin_transform_drag(xf.w2s(end), end);
    assert!(matches!(
        h.app.board_drag,
        Some(board::BoardDrag::Rotate { .. })
    ));
    h.app.update_gesture_for_test(start, egui::Modifiers::NONE);
    press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rotation_deg,
        after.rotation_deg
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
}

#[test]
fn polygon_measured_size_tracks_sides_phase_style_and_numeric_resize() {
    use board_properties::{DimensionKind, DimensionRequest, Property};
    let (mut h, id, _) = closed_form_board("polygon_measured_size", ShapeKind::RegularPolygon);
    h.app.patch_nodes(&[id], |n| {
        Property::RegularSides(3).apply(n, None);
        n.rotation_deg = 37.0;
    });
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let measured = board_properties::measured_bounds(&before);
    assert!((measured.w - before.rect.w * 3.0_f32.sqrt() / 2.0).abs() < 0.001);
    assert!((measured.h - before.rect.h * 0.75).abs() < 0.001);
    h.frame();
    let vertex = h
        .app
        .board_xf()
        .w2s(h.app.closed_form_points(id).unwrap()[1]);
    press_primary(&mut h, vertex, egui::Modifiers::NONE);
    h.app.preview_shape_property(Property::StrokeWidth(12.0));
    h.app.apply_shape_preview(&h.ctx, true);
    let styled = h.app.doc().scene.node(id).unwrap().clone();
    assert_eq!(
        board_properties::measured_bounds(&styled),
        measured,
        "style metadata is not geometry"
    );
    let depth = h.app.tab().journal.undo_depth();
    let req = DimensionRequest {
        ids: vec![id],
        kind: DimensionKind::Height,
        value: 210.0,
    };
    assert!(h
        .app
        .shape_dimension_command(Some(&serde_json::to_string(&req).unwrap())));
    let resized = h.app.doc().scene.node(id).unwrap();
    let actual = board_properties::measured_bounds(resized);
    assert!(
        (actual.h - 210.0).abs() < 0.001,
        "typed size measures the triangle: {actual:?}"
    );
    assert!((actual.w - measured.w).abs() < 0.001);
    let world_center = |n: &slate_doc::scene::Node| {
        let r = board_properties::measured_bounds(n);
        n.rect
            .rotate_point([r.center().0, r.center().1], n.rotation_deg)
    };
    let a = world_center(&styled);
    let b = world_center(resized);
    assert!((a[0] - b[0]).abs() < 0.001 && (a[1] - b[1]).abs() < 0.001);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        board_properties::measured_bounds(h.app.doc().scene.node(id).unwrap()),
        measured
    );
}

#[test]
fn polygon_stroke_width_color_and_opacity_edit_picks_then_the_entire_loop() {
    use board_properties::Property;
    let (mut h, id, v) = closed_form_board("polygon_stroke_edit", ShapeKind::RegularPolygon);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(v[2]), egui::Modifiers::NONE);
    press_primary(&mut h, xf.w2s(v[4]), egui::Modifiers::SHIFT);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![2, 4])));
    let before = closed_tips(&h, id, 6);
    let depth = h.app.tab().journal.undo_depth();
    for edit in [
        Property::StrokeWidth(15.0),
        Property::StrokeRgb([12, 210, 45]),
        Property::StrokeAlpha(90),
    ] {
        h.app.preview_shape_property(edit);
    }
    h.app.apply_shape_preview(&h.ctx, true);
    let tips = closed_tips(&h, id, 6);
    for k in 0..6 {
        if [2, 4].contains(&k) {
            assert_eq!(tips[k], (15.0, [12, 210, 45, 90]));
        } else {
            assert_eq!(tips[k], before[k]);
        }
    }
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(closed_tips(&h, id, 6), before);
    press_key_with(&mut h, egui::Key::Y, egui::Modifiers::CTRL);
    assert_eq!(closed_tips(&h, id, 6), tips);

    // Click an edge away from the grips to return to whole-stroke editing.
    press_primary(
        &mut h,
        xf.w2s(v[0] + (v[1] - v[0]) * 0.5),
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.picked_vertices(), None);
    for edit in [
        Property::StrokeWidth(30.0),
        Property::StrokeRgb([200, 25, 75]),
        Property::StrokeAlpha(180),
    ] {
        h.app.preview_shape_property(edit);
    }
    h.app.apply_shape_preview(&h.ctx, true);
    let whole = closed_tips(&h, id, 6);
    for k in 0..6 {
        assert!((whole[k].0 - tips[k].0 * 2.0).abs() < 0.001);
        assert_eq!(whole[k].1, [200, 25, 75, 180]);
    }
    assert_eq!(shape_kind_of(&h, id), ShapeKind::RegularPolygon);
}
