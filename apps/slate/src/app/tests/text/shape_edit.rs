//! Double-click a closed shape to edit its text.

use super::*;

#[test]
fn empty_shape_text_keeps_each_authored_emphasis_through_exit_and_reentry() {
    use slate_doc::scene::{ShapeKind, ShapeNode, TextStyle, WorldRect};
    for style in [
        TextStyle {
            bold: true,
            ..Default::default()
        },
        TextStyle {
            italic: true,
            ..Default::default()
        },
        TextStyle {
            underline: true,
            ..Default::default()
        },
    ] {
        let mut h = Harness::new("empty-shape-text-emphasis");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = ViewKind::Board;
        let node = h.app.doc_mut().scene.build_node(
            WorldRect::new(10.0, 20.0, 180.0, 90.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: None,
                stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
                corner: Default::default(),
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: None,
                text: None,
            }),
        );
        let id = h.app.add_nodes(vec![node])[0];
        assert!(h.app.dispatch(&h.ctx, atlas_commands::CommandId("board.shape.edit"), Some(
            serde_json::json!({"ids":[id], "edits":[{"TextBold":style.bold},{"TextItalic":style.italic},{"TextUnderline":style.underline}], "points":[]}).to_string()
        )));
        h.app.board_double_click_for_test(egui::pos2(40.0, 50.0));
        h.frame();
        press_key(&mut h, egui::Key::Escape);
        let NodeKind::Shape(shape) = &h.app.doc().scene.node(id).unwrap().kind else {
            unreachable!()
        };
        let text = shape
            .text
            .as_ref()
            .expect("empty authored style survives finishing");
        assert_eq!(text.style, style);
        assert!(text.body.is_empty());
        h.app.board_double_click_for_test(egui::pos2(40.0, 50.0));
        h.frame();
        h.frame_with(|i| i.events.push(egui::Event::Text("Retyped".into())));
        press_key(&mut h, egui::Key::Escape);
        let NodeKind::Shape(shape) = &h.app.doc().scene.node(id).unwrap().kind else {
            unreachable!()
        };
        let text = shape.text.as_ref().unwrap();
        assert_eq!(text.style, style);
        assert_eq!(text.body, "Retyped");
        h.app.board_undo();
        let NodeKind::Shape(shape) = &h.app.doc().scene.node(id).unwrap().kind else {
            unreachable!()
        };
        assert_eq!(shape.text.as_ref().unwrap().style, style);
        assert!(shape.text.as_ref().unwrap().body.is_empty());
    }
}

/// Double-click anywhere on a closed shape opens center-justified text editing
/// with its configuration closed until explicitly opened. A line does not.
#[test]
fn double_click_closed_shape_opens_text_editor() {
    use slate_doc::scene::{NodeKind, ShapeKind, ShapeNode, TextAlign};
    let mut h = Harness::new("shape-text");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let rect = slate_doc::scene::WorldRect::new(10.0, 20.0, 180.0, 90.0);
    let node = h.app.doc_mut().scene.build_node(
        rect,
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_double_click_for_test(egui::pos2(40.0, 50.0));
    assert!(h
        .app
        .text_edit
        .as_ref()
        .is_some_and(|(edit_id, body)| *edit_id == id && body.is_empty()));
    assert_eq!(h.app.shape_properties.panel, None);
    h.app.text_edit = Some((id, "Hello".into()));
    h.app.commit_text_edit();
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Shape(s) => {
            let text = s.text.as_ref().expect("hosted text");
            assert_eq!(text.body, "Hello");
            assert_eq!(text.align, TextAlign::Center);
        }
        _ => panic!("rectangle"),
    }
    h.app.board_undo();
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Shape(s) => assert!(s.text.is_none()),
        _ => panic!("rectangle"),
    }

    let line = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(300.0, 20.0, 80.0, 40.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Line,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let line_id = line.id;
    h.app.add_nodes(vec![line]);
    h.app.board_double_click_for_test(egui::pos2(340.0, 40.0));
    assert!(h
        .app
        .text_edit
        .as_ref()
        .is_none_or(|(edit_id, _)| *edit_id != line_id));
    h.frame();
}
