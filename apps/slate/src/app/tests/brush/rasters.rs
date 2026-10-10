//! Landed brush rasters, validation images, and nested portals.

use super::*;

/// Renders the committed brush pipeline to `target/brush-validate/*.png` so
/// joints, self-overlaps, tweens, and dabs can be inspected by eye.
#[test]
#[ignore]
fn brush_validation_images() {
    fn composite(img: &mut image::RgbaImage, stamp: &vector_ink::StampImage, offset: [f32; 2]) {
        for y in 0..stamp.height {
            for x in 0..stamp.width {
                let i = ((y * stamp.width + x) * 4) as usize;
                let a = stamp.rgba[i + 3] as f32 / 255.0;
                if a <= 0.0 {
                    continue;
                }
                let wx = stamp.origin[0] + (x as f32 + 0.5) * stamp.pixel - offset[0];
                let wy = stamp.origin[1] + (y as f32 + 0.5) * stamp.pixel - offset[1];
                if wx < 0.0 || wy < 0.0 || wx >= img.width() as f32 || wy >= img.height() as f32 {
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
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/brush-validate");
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = Harness::new("brush_validate");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.board_colors.fg.0 = [150, 255, 170, 255];

    // 1. Soft freehand wave, 60% opacity.
    h.app.brush_width = 40.0;
    h.app.brush_softness = 0.6;
    h.app.brush_opacity = 0.6;
    let wave: Vec<Pos2> = (0..=60)
        .map(|i| {
            let t = i as f32 / 60.0;
            Pos2::new(40.0 + t * 520.0, 120.0 + (t * 9.0).sin() * 60.0)
        })
        .collect();
    h.app.finish_freehand_brush(wave);

    // 2. Semi-transparent Shift zigzag with acute joints.
    h.app.brush_width = 36.0;
    h.app.brush_softness = 0.4;
    h.app.brush_opacity = 0.5;
    h.app.finish_freehand_brush(vec![Pos2::new(60.0, 300.0)]);
    for p in [
        Pos2::new(300.0, 300.0),
        Pos2::new(90.0, 340.0),
        Pos2::new(330.0, 380.0),
        Pos2::new(120.0, 430.0),
    ] {
        let a = h.app.brush_line_anchor().unwrap();
        h.app.commit_tween_line(a.pos, p, a.tip, a.node);
    }

    // 3. Tween chain: size and color change between Shift clicks.
    h.app.brush_softness = 0.3;
    h.app.brush_opacity = 1.0;
    h.app.brush_width = 8.0;
    h.app.finish_freehand_brush(vec![Pos2::new(380.0, 460.0)]);
    for (i, p) in [
        Pos2::new(470.0, 300.0),
        Pos2::new(560.0, 460.0),
        Pos2::new(560.0, 250.0),
    ]
    .into_iter()
    .enumerate()
    {
        h.app.brush_width = 8.0 + 22.0 * (i + 1) as f32;
        h.app.board_colors.fg.0 = [150, 255 - 60 * i as u8, 170 + 25 * i as u8, 255];
        let a = h.app.brush_line_anchor().unwrap();
        h.app.commit_tween_line(a.pos, p, a.tip, a.node);
    }

    // 4. Dabs, hard and soft.
    h.app.board_colors.fg.0 = [255, 120, 200, 255];
    h.app.brush_opacity = 0.7;
    h.app.brush_width = 50.0;
    h.app.brush_softness = 0.0;
    h.app.finish_freehand_brush(vec![Pos2::new(90.0, 520.0)]);
    h.app.brush_softness = 1.0;
    h.app.finish_freehand_brush(vec![Pos2::new(170.0, 520.0)]);

    // 5. Self-crossing loop at 50%.
    h.app.board_colors.fg.0 = [120, 200, 255, 255];
    h.app.brush_opacity = 0.5;
    h.app.brush_softness = 0.5;
    h.app.brush_width = 30.0;
    let loop_pts: Vec<Pos2> = (0..=80)
        .map(|i| {
            let t = i as f32 / 80.0 * std::f32::consts::TAU * 1.25;
            Pos2::new(300.0 + t.cos() * 60.0 + t * 12.0, 520.0 + t.sin() * 45.0)
        })
        .collect();
    h.app.finish_freehand_brush(loop_pts);

    let mut img = image::RgbaImage::from_pixel(620, 600, image::Rgba([16, 17, 20, 255]));
    let mut tops = Vec::new();
    for node in &h.app.doc().scene.nodes {
        let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
            continue;
        };
        let Some(path) = shape.path.as_ref() else {
            continue;
        };
        let contours = board_path::stamped_contours(node, shape, path, 0.25);
        let stamp = vector_ink::stamp_tipped(&contours, 1.0).unwrap();
        let top = stamp.rgba.iter().skip(3).step_by(4).copied().max().unwrap();
        let widest = path
            .paint_tips(&shape.stroke)
            .iter()
            .map(|t| t.color.0[3])
            .chain([shape.stroke.color.0[3]])
            .max()
            .unwrap();
        tops.push((node.id, top, widest));
        composite(&mut img, &stamp, [0.0, 0.0]);
    }
    img.save(dir.join("brush.png")).unwrap();
    for (id, top, cap) in tops {
        assert!(top <= cap, "node {id:?} peaks at {top}, opacity is {cap}");
    }
}

/// Review r16 R2: a nested portal deleted while its stroke's raster is on
/// the workers leaves no landed raster behind.
#[test]
fn a_deleted_nested_portal_frees_its_landed_rasters() {
    let (mut h, mut raster, id, _) = eraser_bar_board("eraser_nested_landed");
    let portal = nested_bar_raster_landed(&mut h, &mut raster, id);
    h.app.delete_board_nodes(&[portal]);
    assert!(
        h.app.doc().scene.node(portal).is_none(),
        "the portal is deleted"
    );
    h.app.brush_tiles.hold_rasters = false;
    for _ in 0..200 {
        shot(&mut h, &mut raster, |_| {});
    }
    let left = h.app.brush_tiles.stroke_landed_len(true);
    assert_eq!(left, 0, "{left} nested rasters stay after 200 frames");
}

/// Review r16 R3 (Art. II): a landed raster nobody takes, for a visible
/// stroke that is exact again, costs no cache-id hash per visible node
/// per frame.
#[test]
fn a_landed_raster_hashes_no_visible_node_per_frame() {
    let (mut h, mut raster, first, _) = eraser_bar_board("eraser_landed_hashes");
    h.app.delete_board_nodes(&[first]);
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let rects: Vec<slate_doc::Node> = (0..500)
        .map(|i| {
            use slate_doc::scene::{ShapeKind, ShapeNode};
            let (x, y) = (
                (i % 25) as f32 * 20.0 - 250.0,
                (i / 25) as f32 * 3.0 - 100.0,
            );
            let rect = slate_doc::scene::WorldRect::new(c.x + x, c.y + y, 20.0, 10.0);
            h.app.doc_mut().scene.build_node(
                rect,
                slate_doc::scene::NodeKind::Shape(ShapeNode {
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
            )
        })
        .collect();
    h.app.add_nodes(rects);
    // Last in paint order, so a search for it passes every other node.
    let id = big_brush_bar(&mut h);
    // Selected, the bar paints its own bitmap instead of tiles.
    h.app.board_sel = [id].into_iter().collect();
    let key = board_path::node_stamp_key(h.app.doc().scene.node(id).unwrap());
    let z = h.app.tab().cam.z;
    let want = board_path::stamp_pixel_for_zoom(z, h.ctx.pixels_per_point());
    let exact = move |app: &SlateApp| {
        stamp_of_app(app, id)
            .is_some_and(|(k, g)| g.exact && Some(*k) == key && g.wanted_pixel == want)
    };
    settle_captured(&mut h, &mut raster, "the selected bar", exact);
    h.app.brush_tiles.hold_rasters = true;
    h.app.tab_mut().cam.z = z * 0.5;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.brush_tiles.stroke_landed_len(false) == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the zoomed bar's raster never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        shot(&mut h, &mut raster, |_| {});
    }
    h.app.tab_mut().cam.z = z;
    h.app.brush_tiles.hold_rasters = false;
    for _ in 0..3 {
        shot(&mut h, &mut raster, |_| {});
    }
    assert!(exact(&h.app), "back at the old zoom the bar is exact");
    assert_eq!(
        h.app.brush_tiles.stroke_landed_len(false),
        1,
        "its landed raster waits"
    );
    let hashed = board_slate::salted_on_this_thread();
    for _ in 0..10 {
        shot(&mut h, &mut raster, |_| {});
    }
    let spent = board_slate::salted_on_this_thread() - hashed;
    let xf = h.app.board_xf();
    let in_view = |n: &&slate_doc::Node| {
        let (x, y) = n.rect.center();
        h.app.canvas_rect.contains(xf.w2s(Pos2::new(x, y)))
    };
    let seen = h.app.doc().scene.nodes.iter().filter(in_view).count();
    assert!(seen > 500, "{seen} nodes in view");
    assert!(
        spent < 500,
        "10 idle frames over {seen} visible nodes hashed {spent} cache ids"
    );
}
