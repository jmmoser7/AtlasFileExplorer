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
                if text.galley.text() == label && shape.clip_rect.intersects(list) =>
            {
                Some(text.pos.y)
            }
            _ => None,
        })
        .expect("the typeface row is painted")
}

/// Wheel ownership includes the visible extension beyond the nominal width.
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
    let over = Pos2::new(list.right() - 12.0 * z, list.center().y);
    assert!(
        over.x > list.left() + 168.0 * z,
        "exercise the visible extension"
    );
    assert!(h.app.canvas_rect.contains(over), "wheel is over the board");
    let output = timed_frame(&mut h, &mut time, vec![egui::Event::PointerMoved(over)]);
    let label = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if list.contains(text.pos) && text.pos.y > list.center().y => {
                Some(text.galley.text().to_owned())
            }
            _ => None,
        })
        .expect("a visible middle typeface row");
    let before_row = row_y(&output, &label, list);
    let before = (h.app.tab().cam.z, h.app.tab().cam.offset);
    for _ in 0..6 {
        timed_frame(&mut h, &mut time, wheel(over));
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
    let output = timed_frame(&mut h, &mut time, vec![]);
    assert!(
        row_y(&output, &label, list) < before_row - 1.0,
        "the list actually scrolled"
    );
    assert!(h.app.shape_properties.text_family_open, "list stays open");
    assert!(h.app.board_sel.contains(&id));
    let outside = h.app.canvas_rect.left_top() + Vec2::splat(30.0);
    assert!(!list.contains(outside));
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
