//! Word unbundle and workbook asset save.

use super::*;

#[test]
fn unbundle_word_writes_page_images_beside_a_saved_workbook() {
    let mut h = Harness::new("unbundle_word_saved");
    install_office(&mut h, render_must_not_run, render_two_page_pdf);
    let (source, node) = place_linked(&mut h, "Essay.docx", b"source remains linked");
    let original = match &h.app.doc().scene.node(node).unwrap().kind {
        slate_doc::NodeKind::Image(image) => image.item,
        _ => panic!("word card"),
    };
    assert_eq!(
        slate_doc::media_kind(&source),
        slate_doc::MediaKind::Text,
        "an unbundled Word file still classifies as text"
    );
    h.app.tab_mut().path = Some(h.base.join("Book.slate"));
    let depth = h.app.tab().journal.undo_depth();
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(node.0.to_string())
    ));
    assert!(
        pump_until(&mut h, |app| app.doc().scene.nodes.len() == 2),
        "{}",
        toast_text(&h.app)
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let focus = match &h.app.doc().scene.node(node).unwrap().kind {
        slate_doc::NodeKind::Image(image) => image.item,
        _ => panic!("page"),
    };
    let stored = h.app.doc().item(focus).unwrap().path.clone();
    let locator = stored.to_string_lossy().replace('\\', "/");
    assert!(
        locator.starts_with("assets/documents/Essay-") && locator.ends_with("/page-1.png"),
        "{locator}"
    );
    let absolute = h.base.join(&stored);
    let bytes = std::fs::read(&absolute).unwrap();
    assert!(bytes.starts_with(b"\x89PNG"), "page image was not a png");
    assert_eq!(
        slate_doc::scene::source_locator(h.app.tab().path.as_deref(), &absolute),
        stored.to_string_lossy().replace('\\', "/")
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"source remains linked");
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert!(matches!(
        &h.app.doc().scene.node(node).unwrap().kind,
        slate_doc::NodeKind::Image(image) if image.item == original
    ));
}

#[test]
fn unbundle_word_of_an_unsaved_workbook_writes_into_the_data_dir() {
    let mut h = Harness::new("unbundle_word_unsaved");
    install_office(&mut h, render_must_not_run, render_two_page_pdf);
    let (_, node) = place_linked(&mut h, "Letter.docx", b"source remains linked");
    assert!(h.app.tab().path.is_none());
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(node.0.to_string())
    ));
    assert!(
        pump_until(&mut h, |app| app.doc().scene.nodes.len() == 2),
        "{}",
        toast_text(&h.app)
    );
    let focus = match &h.app.doc().scene.node(node).unwrap().kind {
        slate_doc::NodeKind::Image(image) => image.item,
        _ => panic!("page"),
    };
    let stored = h.app.doc().item(focus).unwrap().path.clone();
    assert!(
        stored.starts_with(atlas_core::index::data_dir().join("document-pages")),
        "{}",
        stored.display()
    );
    assert!(stored.is_file());
    if let Some(dir) = stored.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn saving_files_data_dir_images_beside_the_workbook_and_undo_restores_locators() {
    let mut h = Harness::new("asset_save");
    h.app.ensure_work_tab();
    let data = atlas_core::index::data_dir().join("pasted");
    std::fs::create_dir_all(&data).unwrap();
    let src = data.join(format!("paste-asset-{}.png", now_nanos()));
    std::fs::write(&src, b"png-bytes").unwrap();
    let id = h
        .app
        .doc_mut()
        .add_item(src.clone(), "paste.png", 9, 1, "k");
    let user = h.base.join("holiday.png");
    std::fs::write(&user, b"user-bytes").unwrap();
    let user_id = h
        .app
        .doc_mut()
        .add_item(user.clone(), "holiday.png", 10, 1, "u");
    let dest = h.base.join("Board.slate");
    h.app.save_doc_to(h.app.tab().id, dest.clone());
    assert_eq!(
        h.app.doc().item(id).unwrap().path,
        src,
        "Save copies on a worker; the locator moves when the copy lands"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !h.app.asset_save_rx.is_empty() && std::time::Instant::now() < deadline {
        h.app.poll_asset_saves(&h.ctx);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        !h.app.tab().dirty,
        "the follow-up write leaves the tab clean"
    );
    let stored = h.app.doc().item(id).unwrap().path.clone();
    let locator = stored.to_string_lossy().replace('\\', "/");
    assert!(
        locator.starts_with("assets/pasted/paste-asset-"),
        "{locator}"
    );
    assert_eq!(std::fs::read(h.base.join(&stored)).unwrap(), b"png-bytes");
    assert_eq!(h.app.doc().item(user_id).unwrap().path, user);
    let on_disk = slate_doc::SlateDoc::load_from(&dest).unwrap();
    assert!(on_disk
        .items
        .iter()
        .any(|item| { item.path.to_string_lossy().replace('\\', "/") == locator }));
    let elsewhere = std::env::temp_dir().join(format!("slate-moved-{}", now_nanos()));
    copy_dir(&h.base, &elsewhere);
    let moved = slate_doc::SlateDoc::load_from(&elsewhere.join("Board.slate")).unwrap();
    let resolved = slate_doc::scene::resolve_source(
        Some(&elsewhere.join("Board.slate")),
        &moved
            .items
            .iter()
            .find(|item| item.file_name == "paste.png")
            .unwrap()
            .path
            .to_string_lossy(),
    );
    assert_eq!(std::fs::read(resolved).unwrap(), b"png-bytes");
    h.app.board_undo();
    assert_eq!(h.app.doc().item(id).unwrap().path, src);
    assert!(h.base.join(&locator).is_file());
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_dir_all(&elsewhere);
}

#[test]
fn collect_assets_rewrites_data_dir_images_and_refuses_user_files() {
    let mut h = Harness::new("asset_collect");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.assets.collect"),
        None
    ));
    assert!(
        toast_text(&h.app).contains("Save the workbook first"),
        "{}",
        toast_text(&h.app)
    );
    h.app.tab_mut().path = Some(h.base.join("Board.slate"));
    let gen_dir = atlas_core::index::data_dir()
        .join("openai-image")
        .join(format!("sess-{}", now_nanos()));
    std::fs::create_dir_all(&gen_dir).unwrap();
    let src = gen_dir.join("req-1.png");
    std::fs::write(&src, b"generated").unwrap();
    std::fs::write(
        src.with_extension("json"),
        br#"{"prompt":"barn","model":"gpt","api_key":"sk-no"}"#,
    )
    .unwrap();
    let user = h.base.join("holiday.png");
    std::fs::write(&user, b"user").unwrap();
    let gen_id = h
        .app
        .doc_mut()
        .add_item(src.clone(), "req-1.png", 9, 1, "g");
    let user_id = h
        .app
        .doc_mut()
        .add_item(user.clone(), "holiday.png", 4, 1, "u");
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.assets.collect"),
        None
    ));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        h.app.poll_collect_assets(&h.ctx);
        if h.app.collect_rx.is_none() && h.app.doc().item(gen_id).unwrap().path != src {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let locator = h
        .app
        .doc()
        .item(gen_id)
        .unwrap()
        .path
        .to_string_lossy()
        .replace('\\', "/");
    assert!(
        locator.starts_with("assets/generated/openai-image/") && locator.ends_with("/req-1.png"),
        "{locator}"
    );
    assert_eq!(std::fs::read(h.base.join(&locator)).unwrap(), b"generated");
    let sidecar = std::fs::read_to_string(h.base.join(&locator).with_extension("json")).unwrap();
    assert!(sidecar.contains("barn"));
    assert!(!sidecar.contains("sk-no"));
    assert_eq!(h.app.doc().item(user_id).unwrap().path, user);
    assert!(
        toast_text(&h.app).contains("outside the app data folder")
            || toast_text(&h.app).contains("Filed")
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().item(gen_id).unwrap().path, src);
    let _ = std::fs::remove_dir_all(&gen_dir);
}

#[test]
fn save_as_elsewhere_stays_resolvable_while_assets_copy() {
    let mut h = Harness::new("asset_save_as");
    h.app.ensure_work_tab();
    let first = h.base.join("A").join("Board.slate");
    std::fs::create_dir_all(first.parent().unwrap().join("assets").join("pasted")).unwrap();
    std::fs::write(
        first.parent().unwrap().join("assets/pasted/paste-1.png"),
        b"own-asset",
    )
    .unwrap();
    h.app.tab_mut().path = Some(first.clone());
    let id = h.app.doc_mut().add_item(
        std::path::PathBuf::from("assets/pasted/paste-1.png"),
        "paste-1.png",
        9,
        1,
        "p",
    );
    let dest = h.base.join("B").join("Board.slate");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    let undo_depth = h.app.tab().edits.len();
    h.app.save_doc_to(h.app.tab().id, dest.clone());
    let written = slate_doc::SlateDoc::load_from(&dest).unwrap();
    let interim = slate_doc::scene::resolve_source(
        Some(&dest),
        &written.item(id).unwrap().path.to_string_lossy(),
    );
    assert_eq!(
        std::fs::read(&interim).unwrap(),
        b"own-asset",
        "the file written before the copy must resolve"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !h.app.asset_save_rx.is_empty() && std::time::Instant::now() < deadline {
        h.app.poll_asset_saves(&h.ctx);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let on_disk = slate_doc::SlateDoc::load_from(&dest).unwrap();
    let locator = on_disk
        .item(id)
        .unwrap()
        .path
        .to_string_lossy()
        .replace('\\', "/");
    assert_eq!(locator, "assets/pasted/paste-1.png");
    assert_eq!(
        std::fs::read(dest.parent().unwrap().join(&locator)).unwrap(),
        b"own-asset"
    );
    assert!(
        h.app.tab().edits.len() <= undo_depth + 1,
        "one undo step at most"
    );
}
