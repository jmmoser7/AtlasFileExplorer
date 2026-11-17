use super::*;
use crate::app::ui::tools::{palette_strip_items, DOCK_ID};
use atlas_shell::dock::{dock_pointer_nav, flyout_icon_size, DockSide};

#[test]
fn hosted_image_brush_keeps_a_dock_release_and_the_dock_still_clicks() {
    let mut h = Harness::new("image-brush-dock-release");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    h.app.dock_side = DockSide::BottomCenter;
    h.app.dock_bar_collapsed = false;
    h.app.dock_pins = vec!["tool.shapes".into()];
    h.app.dock_strip_hidden = vec![(
        "tool.shapes".into(),
        palette_strip_items(&h.app, "tool.shapes", &[])
            .into_iter()
            .filter(|item| item.id != "shape.pen")
            .map(|item| item.id.to_owned())
            .collect(),
    )];
    let image = crate::app::board_flow::tests::place(&mut h, "photo.png", 0.0);
    h.app.patch_nodes(&[image], |node| {
        node.rect = WorldRect::new(-184.0, -115.0, 368.0, 230.0);
    });
    h.app.tab_mut().cam.offset = Vec2::ZERO;
    h.app.tab_mut().cam.z = 0.72;
    h.app.board_sel = std::iter::once(image).collect();

    let mut time = 0.0;
    for _ in 0..24 {
        step(&mut h, &mut time, vec![]);
    }
    fallback::click_external_image_panel(&mut h, &mut time, Panel::Brush);
    for _ in 0..14 {
        step(&mut h, &mut time, vec![]);
    }
    assert_eq!(h.app.paint_layer_host(), Some(image));
    assert_eq!(h.app.board_tool, BoardTool::Brush);

    let primary = h
        .ctx
        .read_response(egui::Id::new(("floating_dock", DOCK_ID)).with(("primary", "tool.shapes")))
        .expect("painted Shapes dock")
        .rect;
    let mut tokens = atlas_shell::tokens::current().dock.clone();
    tokens.normalize();
    let gap = (tokens.icon_gap * tokens.flyout_icon_scale).max(4.0);
    let pen = primary.right_center() + Vec2::new(gap + flyout_icon_size(&tokens) * 0.5, 0.0);
    let start = h.app.board_xf().w2s(Pos2::new(-40.0, -30.0));
    assert!(h
        .app
        .point_in_image_paint_window(image, h.app.board_xf().s2w(start)));

    let before = h.app.tab().journal.undo_depth();
    for pass in [false, true] {
        let mut events = vec![
            egui::Event::PointerMoved(start),
            primary_button(start, true),
        ];
        if pass {
            events.push(egui::Event::PointerMoved(pen));
        }
        events.push(primary_button(pen, false));
        step(&mut h, &mut time, events);
        assert!(
            dock_pointer_nav(&h.ctx).over_chrome,
            "release reached the dock"
        );
        assert_eq!(h.app.board_tool, BoardTool::Brush);
        assert_eq!(h.app.paint_layer_host(), Some(image));
        assert_eq!(
            h.app.tab().journal.undo_depth(),
            before + 1 + usize::from(pass)
        );
    }
    let node = h.app.doc().scene.node(image).unwrap();
    let NodeKind::Image(host) = &node.kind else {
        panic!("image host must remain");
    };
    let result = h.app.image_result_id(image).unwrap();
    let layers = slate_doc::image_paint::layers_for_result(host, &result);
    assert_eq!(layers[0].nodes.len(), 2);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);

    step(
        &mut h,
        &mut time,
        vec![primary_button(pen, true), primary_button(pen, false)],
    );
    assert_eq!(h.app.board_tool, BoardTool::Pen);
    assert_eq!(h.app.tab().journal.undo_depth(), before + 2);
}

fn primary_button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn step(h: &mut Harness, time: &mut f64, events: Vec<egui::Event>) {
    *time += 1.0 / 60.0;
    let mut input = egui::RawInput {
        time: Some(*time),
        events,
        ..Default::default()
    };
    eframe::App::raw_input_hook(&mut h.app, &h.ctx, &mut input);
    h.frame_with(|frame| {
        frame.time = input.time;
        frame.events = input.events;
    });
}
