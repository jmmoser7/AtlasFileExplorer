use super::*;

fn wheel(point: Pos2) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(point),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, -80.0),
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn row_y(output: &egui::FullOutput, label: &str, list: Rect) -> f32 {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if text.galley.text() == label
                    && list_row_clip(shape.clip_rect, list)
                    && shape.clip_rect.contains_rect(text.visual_bounding_rect()) =>
            {
                Some(text.pos.y)
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            let matches: Vec<_> = output.shapes.iter().filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label =>
                    Some((text.pos, shape.clip_rect)),
                _ => None,
            }).collect();
            panic!("the visible typeface row is painted: label={label:?}, list={list:?}, matching_rows={matches:?}")
        })
}

fn list_row_clip(clip: Rect, list: Rect) -> bool {
    // The vertical ScrollArea clips rows to the list's height. Parent editor
    // labels share its location, but their canvas-height clip is unrelated.
    clip.intersects(list) && clip.top() >= list.top() && clip.bottom() <= list.bottom()
}

/// Wheel ownership follows the actual bounded popup, including its scrollbar.
#[test]
fn wheel_over_the_typeface_list_does_not_zoom_the_board() {
    let mut h = board();
    let id = text_node(&mut h, WorldRect::new(-120.0, -30.0, 240.0, 60.0));
    let mut time = h.ctx.input(|i| i.time);
    open_text_editor(&mut h, &mut time);
    timed_frame(&mut h, &mut time, vec![]);
    let editor = *h.app.shape_properties.chrome_hits.last().unwrap();
    let z = h.app.board_xf().z;
    let family = editor.min + Vec2::new(24.0, chrome::TEXT_ROW_HEIGHT * 0.5) * z;
    timed_frame(&mut h, &mut time, vec![egui::Event::PointerMoved(family)]);
    timed_frame(&mut h, &mut time, vec![primary(family, true)]);
    timed_frame(&mut h, &mut time, vec![primary(family, false)]);
    assert!(
        h.app.shape_properties.text_family_open,
        "typeface list open"
    );
    for _ in 0..3 {
        timed_frame(&mut h, &mut time, vec![]);
    }
    let strip_len = h
        .app
        .shape_properties
        .last_chrome
        .as_ref()
        .unwrap()
        .items
        .len();
    let reported = h.app.shape_properties.chrome_hits[strip_len];
    let layer = h.ctx.layer_id_at(reported.center()).expect("typeface Area");
    let list = h
        .ctx
        .memory(|m| m.area_rect(layer.id))
        .expect("measured Area");
    assert!(list.top() > editor.top() && list.height() > 100.0 * z);
    let visible = list.intersect(h.app.canvas_rect);
    let over = Pos2::new(visible.right() - 12.0 * z, visible.center().y);
    assert!(
        visible.contains(over) && list.contains(over),
        "exercise the actual visible list"
    );
    assert!(h.app.canvas_rect.contains(over), "wheel is over the board");
    let output = timed_frame(&mut h, &mut time, vec![egui::Event::PointerMoved(over)]);
    let label = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if visible.contains(text.pos)
                    && text.pos.y > visible.center().y
                    && list_row_clip(shape.clip_rect, list) =>
            {
                Some(text.galley.text().to_owned())
            }
            _ => None,
        })
        .expect("a visible middle typeface row");
    assert!(
        scene::Typeface::ALL
            .iter()
            .any(|face| face.label() == label),
        "the scrolling probe belongs to the actual typeface options: {label:?}"
    );
    let before_row = row_y(&output, &label, list);
    let before = (h.app.tab().cam.z, h.app.tab().cam.offset);
    timed_frame(&mut h, &mut time, wheel(over));
    assert!(
        atlas_shell::menu_wheel::wheel_owned(&h.ctx),
        "the typeface menu owns its wheel input"
    );
    assert_eq!(
        (h.app.tab().cam.z, h.app.tab().cam.offset),
        before,
        "the board must neither zoom nor pan"
    );
    for _ in 0..12 {
        timed_frame(&mut h, &mut time, vec![]);
        assert_eq!(
            (h.app.tab().cam.z, h.app.tab().cam.offset),
            before,
            "the smooth wheel tail remains owned"
        );
    }
    // Measure displacement while this middle row is still visible. Further
    // wheel input correctly clips it out of the ScrollArea's painted viewport.
    let output = timed_frame(&mut h, &mut time, vec![]);
    let after_row = row_y(&output, &label, list);
    assert!(
        after_row < before_row - 1.0,
        "the list actually scrolled: {label:?} y={before_row}->{after_row}, list={list:?}, pointer={over:?}"
    );
    for _ in 0..5 {
        timed_frame(&mut h, &mut time, wheel(over));
        assert!(
            atlas_shell::menu_wheel::wheel_owned(&h.ctx),
            "the typeface menu owns its wheel input"
        );
        assert_eq!(
            (h.app.tab().cam.z, h.app.tab().cam.offset),
            before,
            "the board must neither zoom nor pan"
        );
    }
    for _ in 0..12 {
        timed_frame(&mut h, &mut time, vec![]);
        assert_eq!(
            (h.app.tab().cam.z, h.app.tab().cam.offset),
            before,
            "the smooth wheel tail remains owned"
        );
    }
    assert!(h.app.shape_properties.text_family_open, "list stays open");
    assert!(h.app.board_sel.contains(&id));
    let canvas = h.app.canvas_rect;
    let outside = [
        canvas.right_bottom() - Vec2::splat(30.0),
        canvas.left_bottom() + Vec2::new(30.0, -30.0),
        canvas.left_top() + Vec2::splat(30.0),
    ]
    .into_iter()
    .find(|p| {
        !list.contains(*p)
            && h.ctx.layer_id_at(*p) == Some(egui::LayerId::background())
            && !h
                .app
                .shape_properties
                .chrome_hits
                .iter()
                .any(|r| r.contains(*p))
    })
    .expect("uncovered canvas outside the editor and list");
    timed_frame(&mut h, &mut time, vec![egui::Event::PointerMoved(outside)]);
    for _ in 0..6 {
        timed_frame(&mut h, &mut time, wheel(outside));
    }
    assert_ne!(
        h.app.tab().cam.z,
        before.0,
        "outside the list the board still zooms"
    );
}
