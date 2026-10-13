//! Evening feedback regressions: reentry, caret priority, and linked text edits.

use super::*;

#[test]
fn no_system_fonts_render_preferred_faces_and_emphasis_with_nonempty_aliases() {
    use slate_doc::scene::{TextStyle, Typeface};
    let mut h = text_draft_board("text-without-system-fonts");
    let center = h.app.canvas_rect.center();
    compose_text(&mut h, center, "Fallback words");
    press_key(&mut h, egui::Key::Escape);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.font_rx = None;
    SlateApp::install_loaded_fonts(&h.ctx, &[]);
    h.frame();
    h.frame();
    for face in [Typeface::Replica, Typeface::Arial, Typeface::Courier] {
        for style in [
            TextStyle::default(),
            TextStyle {
                bold: true,
                italic: true,
                underline: true,
            },
        ] {
            h.app.patch_nodes(&[id], |node| {
                let NodeKind::Text(text) = &mut node.kind else {
                    unreachable!()
                };
                text.family = face;
                text.style = style;
            });
            let output = h.frame_output(|_| {});
            assert!(
                painted_block(&output, "Fallback words", h.app.board_xf().z).is_some(),
                "{face:?} {style:?} renders without a system font"
            );
            h.app.board_double_click_for_test(Pos2::new(
                h.app.doc().scene.node(id).unwrap().rect.x + 2.0,
                h.app.doc().scene.node(id).unwrap().rect.y + 2.0,
            ));
            h.frame();
            assert!(
                h.app.text_edit.is_some(),
                "fallback family also supports the caret"
            );
            press_key(&mut h, egui::Key::Escape);
        }
    }
}

#[test]
fn text_emphasis_is_journaled_and_survives_inline_reentry() {
    let mut h = text_draft_board("text-emphasis-reentry");
    let center = h.app.canvas_rect.center();
    compose_text(&mut h, center, "Styled words");
    press_key(&mut h, egui::Key::Escape);
    let original = h.app.doc().scene.nodes[0].clone();
    let id = original.id;
    let before = h.app.tab().journal.undo_depth();
    assert!(h.app.dispatch(&h.ctx, atlas_commands::CommandId("board.shape.edit"), Some(
        serde_json::json!({"ids":[id], "edits":[{"TextBold":true},{"TextItalic":true},{"TextUnderline":true}], "points":[]}).to_string()
    )));
    assert_eq!(h.app.tab().journal.undo_depth(), before + 1);
    let NodeKind::Text(text) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert_eq!(
        text.style,
        slate_doc::scene::TextStyle {
            bold: true,
            italic: true,
            underline: true
        }
    );
    let center = Pos2::new(
        original.rect.x + original.rect.w * 0.5,
        original.rect.y + original.rect.h * 0.5,
    );
    h.app.board_double_click_for_test(center);
    h.frame();
    press_key(&mut h, egui::Key::End);
    h.frame_with(|i| i.events.push(egui::Event::Text("!".into())));
    press_key(&mut h, egui::Key::Escape);
    let NodeKind::Text(text) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert!(text.style.bold && text.style.italic && text.style.underline);
    assert!(text.text.ends_with('!'));
    h.app.board_undo();
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &original);
}

#[test]
fn reentered_text_grows_down_and_commits_text_with_height_in_one_undo() {
    let mut h = text_draft_board("text-reentry-height");
    let center = h.app.canvas_rect.center();
    compose_text(&mut h, center, "hello");
    press_key(&mut h, egui::Key::Escape);
    let id = h.app.doc().scene.nodes[0].id;
    let original = h.app.doc().scene.node(id).unwrap().clone();
    let before = h.app.tab().journal.undo_depth();
    let world = Pos2::new(
        original.rect.x + original.rect.w * 0.5,
        original.rect.y + original.rect.h * 0.5,
    );
    h.app.board_double_click_for_test(world);
    h.frame();
    press_key(&mut h, egui::Key::End);
    press_key(&mut h, egui::Key::Enter);
    h.frame_with(|i| i.events.push(egui::Event::Text("second\nthird".into())));
    h.frame();
    assert!(h.app.text_edit.as_ref().unwrap().1.contains('\n'));
    let (_, live) = h.app.text_edit_rect.expect("growing editor bounds");
    assert_eq!(live.w, original.rect.w, "the wrap width stays authored");
    assert!(live.h > original.rect.h, "every new row fits the editor");
    assert_eq!(
        h.app.doc().scene.node(id).unwrap(),
        &original,
        "typing is transient until finish"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    press_key(&mut h, egui::Key::Escape);
    let committed = h.app.doc().scene.node(id).unwrap();
    assert_eq!(committed.rect.h, live.h);
    assert_eq!(h.app.tab().journal.undo_depth(), before + 1);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &original);
}

#[test]
fn double_click_text_opens_only_the_caret_and_inactive_text_keeps_canvas_zoom() {
    let mut h = text_draft_board("text-double-caret");
    let center = h.app.canvas_rect.center();
    compose_text(&mut h, center, "hello world");
    press_key(&mut h, egui::Key::Escape);
    pause(&mut h);
    let node = h.app.doc().scene.nodes[0].clone();
    let point = h.app.board_xf().rect_w2s(node.rect).center();
    click_board(&mut h, point);
    click_board(&mut h, point);
    h.frame();
    assert!(h
        .app
        .text_edit
        .as_ref()
        .is_some_and(|(id, _)| *id == node.id));
    assert_eq!(
        h.app.shape_properties.panel, None,
        "double-click does not open color controls"
    );
    assert!(h.ctx.wants_keyboard_input());
    press_key(&mut h, egui::Key::Escape);
    h.frame();
    h.frame();
    let zoom = h.app.board_xf().z;
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(point));
        i.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: EVec2::new(0.0, 80.0),
            modifiers: egui::Modifiers::NONE,
        });
    });
    assert_ne!(
        h.app.board_xf().z,
        zoom,
        "inactive text must not capture the wheel"
    );
}

#[test]
fn linked_plain_text_double_click_edits_authored_words_without_writing_source() {
    let mut h = text_draft_board("linked-text-edit");
    let (path, id) = place_linked(&mut h, "linked.txt", b"original words");
    h.frame();
    let original = h.app.doc().scene.node(id).unwrap().clone();
    let before = h.app.tab().journal.undo_depth();
    let point = Pos2::new(
        original.rect.x + original.rect.w * 0.5,
        original.rect.y + original.rect.h * 0.5,
    );
    h.app.board_double_click_for_test(point);
    h.frame();
    assert!(
        h.app.frame_chrome_suppressed(id),
        "the caret owns the text card"
    );
    assert_eq!(
        h.app.text_doc_edit.as_ref().unwrap().buffer,
        "original words"
    );
    press_key(&mut h, egui::Key::End);
    h.frame_with(|i| i.events.push(egui::Event::Text(" edited".into())));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original words");
    press_key(&mut h, egui::Key::Escape);
    let NodeKind::Image(image) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    let words = image.text_override.clone().expect("authored override");
    assert_eq!(words, "original words edited");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original words");
    assert_eq!(h.app.tab().journal.undo_depth(), before + 1);
    let out = h.frame_output(|_| {});
    assert!(
        painted_block(&out, &words, h.app.board_xf().z).is_some(),
        "board paints authored words"
    );
    assert_eq!(
        crate::app::clipboard::clipboard_text_fallback(
            &[h.app.doc().scene.node(id).unwrap().clone()],
            |_| { false }
        ),
        Some(words.clone())
    );
    h.app.board_double_click_for_test(point);
    assert_eq!(h.app.text_doc_edit.as_ref().unwrap().buffer, words);
    h.app.commit_text_doc_edit();
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &original);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original words");
}
