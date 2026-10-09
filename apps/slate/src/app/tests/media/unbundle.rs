//! Media menu, picker, and PDF or PowerPoint unbundle.

use super::*;

#[test]
fn a_drop_while_the_picker_is_open_lands_and_dismisses_the_dialog() {
    let mut h = Harness::new("drop_dismisses_picker");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.frame();
    let dropped = h.base.join("dragged-from-dialog.png");
    let picked = h.base.join("picked-in-dialog.png");
    for path in [&dropped, &picked] {
        image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]))
            .save(path)
            .unwrap();
    }
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.picker.adopt(rx);
    let mut raw = egui::RawInput::default();
    raw.events.push(egui::Event::PointerButton {
        pos: Pos2::new(400.0, 400.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    assert!(h.app.dialogs.gate_input(&mut raw), "the window is gated");
    assert_eq!(raw.events, vec![egui::Event::PointerGone]);

    h.app
        .external_drop
        .push_test(super::super::super::external_drop::DropEvent {
            payload: super::super::super::external_drop::Payload::Files(vec![dropped.clone()]),
            at: h.app.canvas_rect.center(),
            alt: false,
        });
    h.frame();
    let linked = |h: &Harness| -> Vec<PathBuf> {
        h.app.doc().items.iter().map(|i| i.path.clone()).collect()
    };
    assert_eq!(linked(&h), vec![dropped.clone()], "drop is a normal drop");
    assert!(h.app.picker.is_open(), "gated until the dialog reports");

    // The cancelled dialog (or a pick that raced the cancel) is discarded.
    tx.send(PickerMsg::AddFiles(Some(vec![picked]))).unwrap();
    h.frame();
    assert!(!h.app.picker.is_open());
    assert_eq!(linked(&h), vec![dropped]);
}

#[test]
fn media_menu_has_four_registered_families() {
    let mut h = Harness::new("media_menu");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let items = ui::tools::palette_strip_items(&h.app, "tool.media", &[]);
    assert_eq!(
        items.iter().map(|i| i.label).collect::<Vec<_>>(),
        vec!["Image", "3D", "Video", "Text"]
    );
    for id in [
        "board.media.image",
        "board.media.model",
        "board.media.video",
        "board.media.text",
        "board.media.page",
        "board.media.unbundle",
    ] {
        assert!(h
            .app
            .registry
            .by_id(atlas_commands::CommandId(id))
            .is_some());
    }
    assert_eq!(
        slate_doc::media_kind(std::path::Path::new("model.3dm")),
        slate_doc::MediaKind::Model
    );
    assert_eq!(
        slate_doc::media_kind(std::path::Path::new("clip.mp4")),
        slate_doc::MediaKind::Video
    );
    // Keep a picker pending so this routing test never opens a native dialog.
    let (_tx, rx) = crossbeam_channel::unbounded();
    h.app.picker.adopt(rx);
    for (icon, command) in [
        ("media.image", "board.media.image"),
        ("media.model", "board.media.model"),
        ("media.video", "board.media.video"),
        ("media.text", "board.media.text"),
    ] {
        ui::tools::activate_flyout_id(&mut h.app, &h.ctx, icon);
        assert_eq!(h.app.cmd_history.iter().last().unwrap().id.0, command);
    }
}

#[test]
fn link_health_follows_relink_removal_and_tab_switches() {
    let mut h = Harness::new("link_health");
    h.app.ensure_work_tab();
    h.app.leave_home();
    let exists = h.base.join("exists.txt");
    let absent = h.base.join("absent.txt");
    std::fs::write(&exists, "local test fixture").unwrap();
    let id = h
        .app
        .doc_mut()
        .add_item(exists.clone(), "exists.txt", 0, 0, "");
    let wait_for = |app: &mut SlateApp, path: &Path, expected| {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            app.link_health_frame(&h.ctx);
            if app.tab().link_health.status(path) == expected {
                break;
            }
            assert!(Instant::now() < deadline, "link health did not settle");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait_for(&mut h.app, &exists, slate_doc::LinkStatus::Ok);
    h.app.doc_mut().relink(id, absent.clone());
    wait_for(&mut h.app, &absent, slate_doc::LinkStatus::Missing);
    assert_eq!(h.app.tab().link_health.counts().missing, 1);
    assert_eq!(
        h.app.tab().link_health.status(&exists),
        slate_doc::LinkStatus::Unknown
    );
    let first = h.app.active_tab;
    h.app.new_tab();
    h.app
        .doc_mut()
        .add_item(exists.clone(), "exists.txt", 0, 0, "");
    wait_for(&mut h.app, &exists, slate_doc::LinkStatus::Ok);
    assert_eq!(h.app.tab().link_health.counts().missing, 0);
    h.app.active_tab = first;
    assert_eq!(h.app.tab().link_health.counts().missing, 1);
    h.app.doc_mut().remove_item(id);
    h.app.link_health_frame(&h.ctx);
    assert_eq!(h.app.tab().link_health.counts().missing, 0);
    assert_eq!(
        h.app.tab().link_health.status(&absent),
        slate_doc::LinkStatus::Unknown
    );
}

#[test]
fn media_picker_places_one_undo_group_and_ignores_late_or_cancelled_results() {
    let mut h = Harness::new("media_picker");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let tab_id = h.app.tab().id;
    let path = h.base.join("picture.png");
    image::RgbaImage::from_pixel(16, 16, image::Rgba([80, 150, 220, 255]))
        .save(&path)
        .unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.picker.adopt(rx);
    tx.send(PickerMsg::AddMedia {
        tab_id,
        at: Pos2::new(80.0, 100.0),
        paths: Some(vec![path.clone()]),
    })
    .unwrap();
    h.app.drain_pickers(&h.ctx);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
    h.app.new_tab();
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.picker.adopt(rx);
    tx.send(PickerMsg::AddMedia {
        tab_id,
        at: Pos2::ZERO,
        paths: Some(vec![path]),
    })
    .unwrap();
    h.app.drain_pickers(&h.ctx);
    assert!(h.app.doc().items.is_empty());
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.picker.adopt(rx);
    tx.send(PickerMsg::AddMedia {
        tab_id: h.app.tab().id,
        at: Pos2::ZERO,
        paths: None,
    })
    .unwrap();
    h.app.drain_pickers(&h.ctx);
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn media_powerpoint_page_choice_is_undoable_and_keeps_the_source_link() {
    let mut h = Harness::new("media_page");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let source = h.base.join("deck.pptx");
    std::fs::write(&source, b"source remains linked").unwrap();
    let ids = h.app.add_paths(std::slice::from_ref(&source));
    h.app.place_items_on_board(&ids, Pos2::new(120.0, 100.0));
    h.app.documents.seed(
        source.clone(),
        pdf::documents::DocumentPreview {
            path: h.base.join("preview.pdf"),
            revision: "test-deck".into(),
            pages: 3,
            bytes: 120,
        },
    );
    let node = h.app.doc().scene.nodes[0].id;
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.page"),
        Some(format!("{}:2", ids[0].0))
    ));
    let page_item = match &h.app.doc().scene.node(node).unwrap().kind {
        slate_doc::NodeKind::Image(i) => i.item,
        _ => panic!(),
    };
    assert_ne!(page_item, ids[0]);
    assert_eq!(h.app.doc().item(page_item).unwrap().pdf_page, 2);
    assert_eq!(h.app.doc().item(page_item).unwrap().path, source);
    h.app.board_undo();
    assert!(
        matches!(&h.app.doc().scene.node(node).unwrap().kind,slate_doc::NodeKind::Image(i) if i.item==ids[0])
    );
    h.app.board_redo();
    assert!(
        matches!(&h.app.doc().scene.node(node).unwrap().kind,slate_doc::NodeKind::Image(i) if i.item==page_item)
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"source remains linked");
    h.app.tab_mut().read_only = true;
    h.app.set_pdf_poster_page(page_item, 0);
    assert!(
        matches!(&h.app.doc().scene.node(node).unwrap().kind,slate_doc::NodeKind::Image(i) if i.item==page_item)
    );
}

#[test]
fn media_unbundle_places_a_selected_page_grid_and_undoes() {
    let mut h = Harness::new("media_unbundle");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let source = h.base.join("deck.pdf");
    std::fs::write(&source, b"source remains linked").unwrap();
    let ids = h.app.add_paths(std::slice::from_ref(&source));
    h.app.place_items_on_board(&ids, Pos2::new(120.0, 100.0));
    h.app.documents.seed(
        source.clone(),
        pdf::documents::DocumentPreview {
            path: h.base.join("preview.pdf"),
            revision: "unbundle-deck".into(),
            pages: 3,
            bytes: 120,
        },
    );
    let node = h.app.doc().scene.nodes[0].id;
    let before = h.app.doc().scene.node(node).unwrap().rect;
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(format!("{}:1", node.0))
    ));
    assert_eq!(h.app.doc().scene.nodes.len(), 3);
    assert_eq!(h.app.board_sel.len(), 3);
    assert!(h.app.board_sel.contains(&node));
    let page_item = match &h.app.doc().scene.node(node).unwrap().kind {
        slate_doc::NodeKind::Image(i) => i.item,
        _ => panic!(),
    };
    assert_eq!(h.app.doc().item(page_item).unwrap().pdf_page, 1);
    let xs: Vec<f32> = h.app.doc().scene.nodes.iter().map(|n| n.rect.x).collect();
    assert!(
        xs.iter().any(|x| (*x - xs[0]).abs() > 1.0),
        "unbundled pages must spread into a grid, not stack"
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"source remains linked");
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.doc().scene.nodes[0].id, node);
    assert_eq!(h.app.doc().scene.node(node).unwrap().rect, before);
    h.app.documents.seed(
        source,
        pdf::documents::DocumentPreview {
            path: h.base.join("preview.pdf"),
            revision: "unbundle-deck".into(),
            pages: 1,
            bytes: 120,
        },
    );
    assert!(!h.app.unbundle_paged_media(node, None));
    h.app.tab_mut().read_only = true;
    assert!(!h.app.unbundle_paged_media(node, Some(0)));
}

#[test]
fn unbundle_pdf_places_pages_through_dispatch_and_undoes_in_one_step() {
    let mut h = Harness::new("unbundle_pdf_e2e");
    install_office(&mut h, render_must_not_run, render_must_not_run);
    let (source, node) = place_linked(&mut h, "notes.pdf", &fixture_pdf(2));
    let before = h.app.doc().scene.node(node).unwrap().rect;
    let original = match &h.app.doc().scene.node(node).unwrap().kind {
        slate_doc::NodeKind::Image(image) => image.item,
        _ => panic!("placed file"),
    };
    let depth = h.app.tab().journal.undo_depth();
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(format!("{node}:1", node = node.0))
    ));
    assert!(
        pump_until(&mut h, |app| app.doc().scene.nodes.len() == 2),
        "pdf unbundle did not finish: {}",
        toast_text(&h.app)
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(h.app.board_sel.len(), 2);
    let page_item = match &h.app.doc().scene.node(node).unwrap().kind {
        slate_doc::NodeKind::Image(image) => image.item,
        _ => panic!("focus page"),
    };
    assert_eq!(h.app.doc().item(page_item).unwrap().pdf_page, 1);
    assert_eq!(h.app.doc().item(page_item).unwrap().path, source);
    let xs: Vec<f32> = h.app.doc().scene.nodes.iter().map(|n| n.rect.x).collect();
    assert!(xs.iter().any(|x| (*x - xs[0]).abs() > 1.0));
    assert_eq!(std::fs::read(&source).unwrap(), fixture_pdf(2));
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.doc().scene.nodes[0].id, node);
    assert_eq!(h.app.doc().scene.node(node).unwrap().rect, before);
    assert!(matches!(
        &h.app.doc().scene.node(node).unwrap().kind,
        slate_doc::NodeKind::Image(image) if image.item == original
    ));
}

#[test]
fn unbundle_while_the_preview_is_pending_finishes_instead_of_failing() {
    let mut h = Harness::new("unbundle_pending");
    install_office(&mut h, render_office_slow, render_must_not_run);
    let (source, node) = place_linked(&mut h, "deck.pptx", b"source remains linked");
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(node.0.to_string())
    ));
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "pending unbundle must not fail closed"
    );
    assert!(
        !toast_text(&h.app).contains("still loading"),
        "{}",
        toast_text(&h.app)
    );
    assert!(
        pump_until(&mut h, |app| app.doc().scene.nodes.len() == 3),
        "{}",
        toast_text(&h.app)
    );
    assert_eq!(h.app.board_sel.len(), 3);
    assert_eq!(std::fs::read(&source).unwrap(), b"source remains linked");
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

#[test]
fn unbundle_pptx_and_legacy_ppt_when_office_can_render() {
    for name in ["deck.pptx", "legacy.ppt"] {
        let mut h = Harness::new(&format!("unbundle_{name}"));
        install_office(&mut h, render_three_page_pdf, render_must_not_run);
        let (source, node) = place_linked(&mut h, name, b"source remains linked");
        let depth = h.app.tab().journal.undo_depth();
        assert!(h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId("board.media.unbundle"),
            Some(format!("{node}:0", node = node.0))
        ));
        assert!(
            pump_until(&mut h, |app| app.doc().scene.nodes.len() == 3),
            "{name}: {}",
            toast_text(&h.app)
        );
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        let pages: Vec<u16> = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .map(|node| match &node.kind {
                slate_doc::NodeKind::Image(image) => h.app.doc().item(image.item).unwrap().pdf_page,
                _ => panic!("page image"),
            })
            .collect();
        assert!(pages.contains(&0) && pages.contains(&1) && pages.contains(&2));
        assert_eq!(std::fs::read(&source).unwrap(), b"source remains linked");
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.nodes.len(), 1);
    }
}

#[test]
fn unbundle_reports_when_office_is_not_installed() {
    let mut h = Harness::new("unbundle_no_office");
    install_office(&mut h, render_office_missing, render_office_missing);
    let (source, node) = place_linked(&mut h, "deck.pptx", b"source remains linked");
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(node.0.to_string())
    ));
    assert!(
        pump_until(&mut h, |app| toast_text(app).contains("not installed")),
        "{}",
        toast_text(&h.app)
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(std::fs::read(&source).unwrap(), b"source remains linked");
}

#[test]
fn unbundle_refuses_a_dehydrated_document_without_rendering_it() {
    let mut h = Harness::new("unbundle_cloud");
    install_office(&mut h, render_must_not_run, render_must_not_run);
    let (source, node) = place_linked(&mut h, "cloud.pptx", b"source remains linked");
    if !cfg!(windows) {
        return;
    }
    assert!(
        atlas_core::cloud::mark_offline(&source),
        "Windows should be able to mark a temp file offline"
    );
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(node.0.to_string())
    ));
    assert!(
        pump_until(&mut h, |app| toast_text(app).contains("cloud-only")),
        "{}",
        toast_text(&h.app)
    );
    assert!(
        !toast_text(&h.app).contains("RENDERED"),
        "{}",
        toast_text(&h.app)
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(std::fs::read(&source).unwrap(), b"source remains linked");
    let _ = atlas_core::cloud::clear_offline(&source);
}

#[test]
fn unbundle_of_a_single_page_does_not_place_a_grid() {
    let mut h = Harness::new("unbundle_one_page");
    install_office(&mut h, render_must_not_run, render_must_not_run);
    let (_, node) = place_linked(&mut h, "one.pdf", &fixture_pdf(1));
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(node.0.to_string())
    ));
    assert!(
        pump_until(&mut h, |app| toast_text(app).contains("only one page")),
        "{}",
        toast_text(&h.app)
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}
