//! Eraser validation image and a fast segment along its length.

use super::*;

#[test]
#[ignore]
fn eraser_validation_image() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/brush-validate");
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = Harness::new("eraser_validate");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.board_colors.fg.0 = [150, 255, 170, 255];
    h.app.brush_width = 44.0;
    h.app.brush_softness = 0.3;
    for y in [100.0, 220.0, 340.0] {
        let pts: Vec<Pos2> = (0..=40)
            .map(|i| Pos2::new(40.0 + i as f32 * 13.0, y + (i as f32 * 0.4).sin() * 20.0))
            .collect();
        h.app.finish_freehand_brush(pts);
    }
    h.app.set_board_tool(board::BoardTool::Eraser);
    let pass = |app: &mut SlateApp, pts: &[Pos2], shift: bool| {
        app.board_drag = Some(app.begin_erase(pts[0], shift));
        for p in &pts[1..] {
            app.update_erase(*p);
        }
        let Some(board::BoardDrag::Erase {
            touched,
            points,
            spot,
            ..
        }) = app.board_drag.take()
        else {
            panic!("erase drag");
        };
        app.finish_erase(touched, points, spot);
    };
    // Hard full-strength dabs.
    h.app.eraser_width = 36.0;
    h.app.eraser_softness = 0.0;
    h.app.eraser_opacity = 1.0;
    pass(&mut h.app, &[Pos2::new(120.0, 100.0)], false);
    pass(&mut h.app, &[Pos2::new(200.0, 100.0)], false);
    // Soft, half-strength freehand that crosses itself over all three.
    h.app.eraser_width = 50.0;
    h.app.eraser_softness = 0.8;
    h.app.eraser_opacity = 0.5;
    let zig: Vec<Pos2> = (0..=60)
        .map(|i| {
            let t = i as f32 / 60.0;
            Pos2::new(330.0 + (t * 18.0).sin() * 50.0, 60.0 + t * 320.0)
        })
        .collect();
    pass(&mut h.app, &zig, false);
    // Hard straight Shift pass, from the last pass's end.
    h.app.eraser_width = 16.0;
    h.app.eraser_softness = 0.0;
    h.app.eraser_opacity = 1.0;
    h.app.eraser_anchor = Some((h.app.tab().id, Pos2::new(470.0, 60.0)));
    pass(
        &mut h.app,
        &[Pos2::new(470.0, 60.0), Pos2::new(520.0, 380.0)],
        true,
    );

    let mut img = image::RgbaImage::from_pixel(620, 440, image::Rgba([16, 17, 20, 255]));
    for node in &h.app.doc().scene.nodes {
        let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
            continue;
        };
        let Some(path) = shape.path.as_ref() else {
            continue;
        };
        let contours = board_path::stamped_contours(node, shape, path, 0.25);
        let mut stamp = vector_ink::stamp_tipped(&contours, 1.0).unwrap();
        vector_ink::apply_erase(
            &mut stamp,
            &board_path::stamped_erase_marks(node, shape, path),
        );
        for y in 0..stamp.height {
            for x in 0..stamp.width {
                let i = ((y * stamp.width + x) * 4) as usize;
                let a = stamp.rgba[i + 3] as f32 / 255.0;
                let wx = stamp.origin[0] + x as f32 + 0.5;
                let wy = stamp.origin[1] + y as f32 + 0.5;
                if a <= 0.0 || wx < 0.0 || wy < 0.0 || wx >= 620.0 || wy >= 440.0 {
                    continue;
                }
                let p = img.get_pixel_mut(wx as u32, wy as u32);
                for c in 0..3 {
                    p.0[c] =
                        (stamp.rgba[i + c] as f32 * a + p.0[c] as f32 * (1.0 - a)).round() as u8;
                }
            }
        }
    }
    img.save(dir.join("eraser.png")).unwrap();
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        3,
        "strokes survive spot erasing"
    );
}

/// One fast pointer move is one long segment. Everything under that segment
/// is erased, not only what sits under its two ends.
#[test]
fn a_fast_eraser_segment_erases_along_its_whole_length() {
    let mut h = web_board("eraser_fast_segment");
    let lines: Vec<NodeId> = (1..=7)
        .map(|i| crossing_line(&mut h.app, i as f32 * 50.0))
        .collect();
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 10.0;
    let dots: Vec<NodeId> = (1..=6)
        .map(|i| {
            let p = Pos2::new(i as f32 * 50.0 + 25.0, i as f32 * 50.0 + 25.0);
            h.app.finish_freehand_brush(vec![p]);
            h.app.doc().scene.nodes.last().unwrap().id
        })
        .collect();
    {
        let cam = &mut h.app.tab_mut().cam;
        cam.z = 1.0;
        cam.offset = EVec2::new(200.0, 200.0);
    }
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 30.0;
    h.app.eraser_softness = 0.0;
    h.app.eraser_opacity = 1.0;
    h.frame();
    let xf = h.app.board_xf();
    let (a, b) = (xf.w2s(Pos2::new(0.0, 0.0)), xf.w2s(Pos2::new(400.0, 400.0)));
    assert!(h.app.canvas_rect.contains(a) && h.app.canvas_rect.contains(b));
    h.frame_with(pointer_to(a, false));
    h.frame_with(primary_button(a, true, false));
    h.frame_with(pointer_to(b, false));
    h.frame_with(primary_button(b, false, false));
    let left: Vec<NodeId> = lines
        .iter()
        .chain(&dots)
        .copied()
        .filter(|id| h.app.doc().scene.node(*id).is_some())
        .collect();
    assert!(
        left.is_empty(),
        "{} of {} marks under the segment survived",
        left.len(),
        lines.len() + dots.len()
    );
}
