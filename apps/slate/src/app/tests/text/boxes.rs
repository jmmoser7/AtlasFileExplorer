//! Text boxes, sticky notes, and quote-to-type.

use super::*;

/// Sticky Tab-spawn: the sibling lands one note-width + gap to the right,
/// keeps the fill preset, and takes the caret.
#[test]
fn sticky_tab_spawn_offsets_right() {
    use slate_doc::scene::NodeKind;
    let mut h = Harness::new("sticky");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;

    h.app.place_sticky_at(Pos2::new(0.0, 0.0));
    let first = *h.app.board_sel.iter().next().expect("sticky selected");
    assert!(h.app.text_edit.as_ref().is_some_and(|(id, _)| *id == first));
    let r0 = h.app.doc().scene.node(first).unwrap().rect;
    assert_eq!((r0.w, r0.h), (200.0, 200.0));

    h.app.spawn_adjacent_sticky(first, 1.0);
    let second = *h.app.board_sel.iter().next().expect("sibling selected");
    assert_ne!(second, first);
    let n = h.app.doc().scene.node(second).unwrap();
    assert_eq!(n.rect.x, r0.x + r0.w + 24.0);
    assert_eq!(n.rect.y, r0.y);
    match &n.kind {
        NodeKind::Text(t) => assert_eq!(t.fill, Some(board_color::STICKY_FILL)),
        _ => panic!("sticky is a text node"),
    }
    assert!(h
        .app
        .text_edit
        .as_ref()
        .is_some_and(|(id, _)| *id == second));
    h.frame();
}

/// Place is one-shot: Select returns, text is center-aligned, the caret is
/// open, and the text/color capsule stays closed.
#[test]
fn sticky_place_opens_centered_edit_without_the_color_capsule() {
    use slate_doc::scene::{NodeKind, TextAlign};
    let mut h = Harness::new("sticky-place");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(board::BoardTool::Sticky);

    h.app.place_sticky_at(Pos2::new(0.0, 0.0));
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    let id = *h.app.board_sel.iter().next().expect("sticky selected");
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Text(t) => {
            assert_eq!(t.align, TextAlign::Center);
            assert_eq!(t.fill, Some(board_color::STICKY_FILL));
            assert_eq!(board_color::STICKY_FILL.0, [255, 255, 255, 255]);
            assert!(t.text.is_empty());
        }
        _ => panic!("sticky is a text node"),
    }
    assert!(h
        .app
        .text_edit
        .as_ref()
        .is_some_and(|(edit, _)| *edit == id));
    h.app.sync_shape_properties();
    assert!(h.app.shape_properties.panel.is_none());

    h.app.commit_text_edit();
    assert!(h.app.text_edit.is_none());
    assert_eq!(h.app.board_tool, board::BoardTool::Select);

    h.app.board_double_click_for_test(Pos2::new(0.0, 0.0));
    assert!(h
        .app
        .text_edit
        .as_ref()
        .is_some_and(|(edit, _)| *edit == id));
    h.app.sync_shape_properties();
    assert!(h.app.shape_properties.panel.is_none());
    h.frame();
}

#[test]
fn text_box_click_starts_draft_without_journal() {
    let mut h = Harness::new("text-draft");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let before = h.app.tab().journal.undo_depth();
    h.app.place_text_at(Pos2::new(40.0, 40.0));
    assert!(h.app.text_box_draft.is_some());
    assert!(h.app.doc().scene.nodes.is_empty());
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    let draft = h.app.text_box_draft.as_ref().unwrap();
    assert!(draft.buffer.is_empty());
    assert_eq!(draft.color, board::to_rgba(h.app.palette().ink));
    h.frame();
}

#[test]
fn text_box_commit_on_click_away_is_one_undo_step() {
    use slate_doc::scene::NodeKind;
    let mut h = Harness::new("text-commit");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.place_text_at(Pos2::ZERO);
    h.app.text_box_draft.as_mut().unwrap().buffer = "Hello".into();
    let before = h.app.tab().journal.undo_depth();
    h.app.commit_text_box_draft();
    assert!(h.app.text_box_draft.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.tab().journal.undo_depth(), before + 1);
    match &h.app.doc().scene.nodes[0].kind {
        NodeKind::Text(t) => assert_eq!(t.text, "Hello"),
        _ => panic!("text node"),
    }
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
    h.frame();
}

#[test]
fn text_box_empty_click_away_discards_without_journal() {
    let mut h = Harness::new("text-discard");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.place_text_at(Pos2::ZERO);
    let before = h.app.tab().journal.undo_depth();
    h.app.cancel_text_box_draft();
    assert!(h.app.text_box_draft.is_none());
    assert!(h.app.doc().scene.nodes.is_empty());
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    h.frame();
}

#[test]
fn text_box_draft_uses_theme_ink_in_dark_mode() {
    let mut h = Harness::new("text-ink-dark");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.dark_mode = true;
    h.app.place_text_at(Pos2::ZERO);
    assert_eq!(
        h.app.text_box_draft.as_ref().unwrap().color,
        board::to_rgba(h.app.palette().ink)
    );
    h.app.dark_mode = false;
    h.app.cancel_text_box_draft();
    h.app.place_text_at(Pos2::ZERO);
    assert_eq!(
        h.app.text_box_draft.as_ref().unwrap().color,
        board::to_rgba(h.app.palette().ink)
    );
    h.frame();
}

/// Text D16: a new text box takes the last text color the person gave a
/// text box, unless that color would vanish on the canvas; then it takes
/// the theme's ink. Existing text keeps its color.
#[test]
fn new_text_takes_the_last_text_color_unless_it_vanishes_on_the_canvas() {
    for dark in [true, false] {
        let mut h = text_draft_board("text-color-memory");
        h.app.dark_mode = dark;
        let palette = h.app.palette();
        let ink = board::to_rgba(palette.ink);
        let canvas = board::to_rgba(palette.bg);
        let center = h.app.canvas_rect.center();
        compose_text(&mut h, center, "first");
        press_key(&mut h, egui::Key::Escape);
        let id = h.app.doc().scene.nodes[0].id;
        let world = h.app.board_xf().s2w(center + EVec2::new(0.0, 200.0));

        let red = Rgba::opaque(214, 48, 49);
        recolor_text(&mut h, id, red);
        assert_eq!(draft_color_at(&mut h, world), red, "dark={dark}");

        // The canvas color itself, and a near miss of it, fall back to ink.
        recolor_text(&mut h, id, canvas);
        assert_eq!(draft_color_at(&mut h, world), ink, "dark={dark}");
        let near = Rgba::opaque(
            canvas.0[0].saturating_add(6),
            canvas.0[1].saturating_add(6),
            canvas.0[2].saturating_sub(6),
        );
        recolor_text(&mut h, id, near);
        assert_eq!(draft_color_at(&mut h, world), ink, "dark={dark}");
        let existing = match &h.app.doc().scene.node(id).unwrap().kind {
            NodeKind::Text(t) => t.color,
            _ => unreachable!(),
        };
        assert_eq!(existing, near, "only new text is guarded");

        // White on a light canvas, black on a dark one.
        let invisible = if dark { Rgba::BLACK } else { Rgba::WHITE };
        recolor_text(&mut h, id, invisible);
        assert_eq!(draft_color_at(&mut h, world), ink, "dark={dark}");
        let legible = if dark { Rgba::WHITE } else { Rgba::BLACK };
        recolor_text(&mut h, id, legible);
        assert_eq!(draft_color_at(&mut h, world), legible, "dark={dark}");
    }
}

/// A sticky's ink is its own: editing it leaves the text box's color alone.
#[test]
fn sticky_ink_edits_do_not_become_the_text_box_color() {
    let mut h = text_draft_board("text-color-sticky");
    h.app.dark_mode = true;
    let center = h.app.canvas_rect.center();
    compose_text(&mut h, center, "first");
    press_key(&mut h, egui::Key::Escape);
    let text = h.app.doc().scene.nodes[0].id;
    let orange = Rgba::opaque(240, 140, 20);
    recolor_text(&mut h, text, orange);
    let world = h.app.board_xf().s2w(center + EVec2::new(0.0, 200.0));
    h.app.place_sticky_at(world);
    let sticky = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.commit_text_edit();
    recolor_text(&mut h, sticky, Rgba::opaque(20, 90, 200));
    assert_eq!(
        draft_color_at(&mut h, world + EVec2::new(0.0, 200.0)),
        orange
    );
}

/// Crash regression: typing into a click-created text box made its rect
/// infinitely wide and egui's hit test panicked on the next frame.
#[test]
fn text_box_click_draft_grows_to_typed_width_only() {
    let mut h = text_draft_board("text-autowidth");
    let world = h.app.board_xf().s2w(Pos2::new(600.0, 400.0));
    h.app.place_text_at(world);
    type_into_draft(&mut h, "hello world");
    assert_eq!(h.app.text_box_draft.as_ref().unwrap().buffer, "hello world");
    let r = assert_draft_rect_sane(&h);
    let z = h.app.board_xf().z;
    let text_w = h.ctx.fonts(|f| {
        f.layout_no_wrap(
            "hello world".into(),
            egui::FontId::proportional(24.0 * z),
            egui::Color32::WHITE,
        )
        .size()
        .x
    }) / z;
    assert!(
        (r.w - text_w).abs() <= 8.0,
        "auto width {} should fit the text ({text_w})",
        r.w
    );
}

#[test]
fn text_box_click_draft_stays_finite_for_long_text() {
    let mut h = text_draft_board("text-autowidth-long");
    let world = h.app.board_xf().s2w(Pos2::new(600.0, 400.0));
    h.app.place_text_at(world);
    type_into_draft(&mut h, &"x".repeat(500));
    assert_eq!(h.app.text_box_draft.as_ref().unwrap().buffer.len(), 500);
    assert_draft_rect_sane(&h);
}

#[test]
fn text_box_click_draft_stays_finite_at_extreme_zoom() {
    let mut size = None;
    for z in [0.05_f32, 1.0, 20.0] {
        let mut h = text_draft_board("text-autowidth-zoom");
        h.app.tab_mut().cam.z = z;
        let world = h.app.board_xf().s2w(Pos2::new(600.0, 400.0));
        h.app.place_text_at(world);
        type_into_draft(&mut h, "hello world");
        assert_eq!(h.app.board_xf().z, z, "typed at the requested zoom");
        assert_eq!(h.app.text_box_draft.as_ref().unwrap().buffer, "hello world");
        let rect = assert_draft_rect_sane(&h);
        match size {
            None => size = Some((rect.w, rect.h)),
            Some((w, hgt)) => {
                assert!(
                    (rect.w - w).abs() < 0.5 && (rect.h - hgt).abs() < 0.5,
                    "draft size moved with zoom {z}: {w}x{hgt} vs {}x{}",
                    rect.w,
                    rect.h
                );
            }
        }
    }
}

/// Grasshopper panel entry: `"` then Enter places one blank linked text
/// document at the pointer, as one undo step, with the caret in it.
#[test]
fn quote_then_enter_places_a_blank_text_document_ready_to_type() {
    use slate_doc::scene::NodeKind;
    let mut h = quote_board("quote-enter");
    let before = h.app.tab().journal.undo_depth();
    press_quote(&mut h, '"');
    assert!(
        h.app.palette_state.open,
        "the quote opens the canvas search"
    );
    assert_eq!(h.app.palette_state.query, "\"");
    assert_eq!(
        h.app
            .palette_items
            .iter()
            .map(|it| it.id.0)
            .collect::<Vec<_>>(),
        vec!["board.media.text_new"]
    );
    assert!(h.app.doc().scene.nodes.is_empty());
    h.frame();
    press_key(&mut h, egui::Key::Enter);

    assert!(!h.app.palette_state.open);
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "exactly one node");
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        before + 1,
        "one undo step"
    );
    let node = h.app.doc().scene.nodes[0].clone();
    let NodeKind::Image(img) = &node.kind else {
        panic!("a text media card is a linked image node");
    };
    let path = h.app.doc().item(img.item).unwrap().path.clone();
    assert_eq!(slate_doc::media_kind(&path), slate_doc::MediaKind::Text);
    assert_eq!(
        path.parent().unwrap(),
        h.base.join("slate-outputs").join("quote").join("text")
    );
    let pointer = h.app.board_xf().s2w(Pos2::new(700.0, 420.0));
    let center = Pos2::new(
        node.rect.x + node.rect.w / 2.0,
        node.rect.y + node.rect.h / 2.0,
    );
    assert!((center - pointer).length() < 1.0, "placed at the pointer");
    assert!(h
        .app
        .text_doc_edit
        .as_ref()
        .is_some_and(|e| e.node == node.id && e.buffer.is_empty()));

    h.frame();
    assert!(h.ctx.wants_keyboard_input(), "the caret holds the keyboard");
    h.frame_with(|input| input.events.push(egui::Event::Text("hi".into())));
    assert_eq!(h.app.text_doc_edit.as_ref().unwrap().buffer, "hi");
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "typing adds no nodes");
    press_key(&mut h, egui::Key::Escape);
    assert!(h.app.text_doc_edit.is_none());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::fs::read_to_string(&path).ok().as_deref() != Some("hi") {
        assert!(
            std::time::Instant::now() < deadline,
            "typed words reach the linked file"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn single_quote_then_escape_creates_nothing() {
    let mut h = quote_board("quote-escape");
    let before = h.app.tab().journal.undo_depth();
    press_quote(&mut h, '\'');
    assert!(h.app.palette_state.open);
    assert_eq!(h.app.palette_state.query, "'");
    h.frame();
    press_key(&mut h, egui::Key::Escape);
    assert!(!h.app.palette_state.open);
    assert!(h.app.doc().scene.nodes.is_empty());
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    assert!(h.app.text_doc_edit.is_none());
}

#[test]
fn text_box_drag_draft_keeps_its_wrap_width() {
    let mut h = text_draft_board("text-fixed-width");
    let world = h.app.board_xf().s2w(Pos2::new(600.0, 400.0));
    let rect = slate_doc::scene::WorldRect::new(world.x, world.y, 120.0, 48.0);
    h.app.begin_text_box_draft(world, rect, true);
    let long = "the quick brown fox jumps over the lazy dog";
    type_into_draft(&mut h, long);
    assert_eq!(h.app.text_box_draft.as_ref().unwrap().buffer, long);
    let r = assert_draft_rect_sane(&h);
    assert_eq!(r.w, 120.0, "drag-created width is the wrap width");
    h.app.commit_text_box_draft();
    let node = h.app.doc().scene.nodes.last().expect("committed");
    assert_eq!(node.rect.w, 120.0);
}

/// Text D12: clicking empty canvas after typing commits the words as one
/// journaled add. A later click-away must not rewrite them either.
#[test]
fn text_box_click_away_after_typing_commits_the_words() {
    let mut h = text_draft_board("text-click-away-commit");
    let before = h.app.tab().journal.undo_depth();
    let center = h.app.canvas_rect.center();
    compose_text(&mut h, center, "hello");
    click_board(&mut h, center + EVec2::new(260.0, 180.0));
    for _ in 0..4 {
        h.frame();
    }
    assert_text_committed_once(&h, before, "hello");
    pause(&mut h);
    click_board(&mut h, center + EVec2::new(-280.0, 200.0));
    for _ in 0..4 {
        h.frame();
    }
    assert_text_committed_once(&h, before, "hello");
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty(), "one undo removes it");
}

/// Text D12: Esc after typing commits exactly like a click-away and opens
/// no popup (no color editor, no menu, no search).
#[test]
fn text_box_escape_after_typing_commits_without_a_popup() {
    let mut h = text_draft_board("text-escape-commit");
    let before = h.app.tab().journal.undo_depth();
    let center = h.app.canvas_rect.center();
    compose_text(&mut h, center, "hello");
    press_key(&mut h, egui::Key::Escape);
    for _ in 0..4 {
        h.frame();
    }
    assert_text_committed_once(&h, before, "hello");
    pause(&mut h);
    click_board(&mut h, center + EVec2::new(-280.0, 200.0));
    for _ in 0..4 {
        h.frame();
    }
    assert_text_committed_once(&h, before, "hello");
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty(), "one undo removes it");
}
