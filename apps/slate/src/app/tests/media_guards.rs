//! Workbook-in-workbook guards, video trim, and export cards.

use super::*;

// ----- media kinds & workbook-in-workbook guards ---------------------------------

#[test]
fn slate_files_never_become_items() {
    let mut h = Harness::new("wb_guard");
    // A real workbook file on disk plus a plain image.
    let wb_path = h.base.join("other.slate");
    SlateDoc::new("Other").save_to(&wb_path).unwrap();
    let img_path = h.base.join("pic.png");
    std::fs::write(&img_path, b"png-ish").unwrap();

    let ids = h.app.add_paths(&[wb_path.clone(), img_path]);
    // Only the image became an item; the workbook was queued to open.
    assert_eq!(ids.len(), 1);
    assert_eq!(h.app.doc().items.len(), 1);
    assert_eq!(h.app.pending_workbooks, vec![wb_path.clone()]);

    // The frame pump opens it as a tab.
    h.frame();
    assert!(h.app.pending_workbooks.is_empty());
    assert_eq!(h.app.tabs.len(), 2);
    assert_eq!(h.app.tab().doc.name, "Other");
    assert_eq!(h.app.tab().path.as_deref(), Some(wb_path.as_path()));
}

#[test]
fn a_blank_board_opens_a_dropped_workbook_without_asking() {
    let mut h = Harness::new("wb_blank_drop");
    let wb_path = h.base.join("other.slate");
    SlateDoc::new("Other").save_to(&wb_path).unwrap();
    h.app
        .accept_workbook_drop(wb_path.clone(), Pos2::new(40.0, 40.0));
    assert_eq!(h.app.pending_workbook_drops(), 0);
    assert_eq!(h.app.pending_workbooks, vec![wb_path.clone()]);
    h.frame();
    assert!(h.app.pending_workbooks.is_empty());
    assert_eq!(h.app.tabs.len(), 1);
    assert_eq!(h.app.tab().doc.name, "Other");
    assert!(h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .all(|n| !matches!(&n.kind, slate_doc::scene::NodeKind::Portal(_))));
}

#[test]
fn an_occupied_board_can_insert_a_dropped_workbook() {
    let mut h = Harness::new("wb_insert");
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 40.0),
        slate_doc::scene::NodeKind::Text(slate_doc::scene::TextNode {
            text: "keep".into(),
            family: slate_doc::scene::Typeface::Sans,
            size: 24.0,
            color: slate_doc::scene::Rgba::opaque(20, 20, 20),
            align: slate_doc::scene::TextAlign::Left,
            fill: None,
            stroke: Default::default(),
            agent: None,
        }),
    );
    h.app.add_nodes(vec![node]);
    let wb_path = h.base.join("child.slate");
    let mut child = SlateDoc::new("Child");
    let mark = child.scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 120.0, 40.0),
        slate_doc::scene::NodeKind::Text(slate_doc::scene::TextNode {
            text: "NESTED-MARK".into(),
            family: slate_doc::scene::Typeface::Sans,
            size: 24.0,
            color: slate_doc::scene::Rgba::opaque(20, 20, 20),
            align: slate_doc::scene::TextAlign::Left,
            fill: None,
            stroke: Default::default(),
            agent: None,
        }),
    );
    child.scene.nodes.push(mark);
    child.save_to(&wb_path).unwrap();
    h.app.tab_mut().path = Some(h.base.join("parent.slate"));

    h.app
        .accept_workbook_drop(wb_path.clone(), Pos2::new(200.0, 200.0));
    assert!(h.app.pending_workbooks.is_empty());
    assert_eq!(h.app.pending_workbook_drops(), 1);
    let (path, at) = h.app.pop_workbook_drop().unwrap();
    h.app
        .apply_workbook_drop(&h.ctx, board_slate::WorkbookDropChoice::Insert, path, at);
    let portal = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            slate_doc::scene::NodeKind::Portal(p)
                if p.kind == slate_doc::scene::PortalKind::Slate =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("inserted portal");
    assert_eq!(portal.class, slate_doc::scene::PortalClass::Document);
    assert_eq!(
        portal.source.as_ref().map(|s| s.locator.as_str()),
        Some("child.slate")
    );

    h.app.do_export(h.base.join("export"));
    h.wait_for_export();
    let html = std::fs::read_to_string(
        h.base
            .join("export")
            .join("Untitled-slides")
            .join("index.html"),
    )
    .unwrap();
    assert!(html.contains("NESTED-MARK"), "{html}");
}

#[test]
fn opening_same_workbook_twice_focuses_existing_tab() {
    let mut h = Harness::new("wb_dedupe");
    let path = h.base.join("one.slate");
    SlateDoc::new("One").save_to(&path).unwrap();

    h.app.open_doc_at(path.clone());
    assert_eq!(h.app.tabs.len(), 1); // blank tab was reused
    h.app.new_tab();
    assert_eq!(h.app.active_tab, 1);

    // Re-opening switches back to the existing tab instead of loading twice.
    h.app.open_doc_at(path);
    assert_eq!(h.app.tabs.len(), 2);
    assert_eq!(h.app.active_tab, 0);
    h.frame();
}

#[test]
fn workbook_cannot_load_into_itself() {
    let mut h = Harness::new("wb_self");
    h.seed();
    let path = h.base.join("self.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());
    let items_before = h.app.doc().items.len();

    // "Add" the workbook's own file to itself (drop / add-files flow).
    let ids = h.app.add_paths(&[path]);
    h.frame();
    // No self-item, no second tab — dedupe lands on the same tab.
    assert!(ids.is_empty());
    assert_eq!(h.app.doc().items.len(), items_before);
    assert_eq!(h.app.tabs.len(), 1);
}

#[test]
fn video_trim_settings_survive_save_and_reload() {
    use slate_doc::scene::VideoOpts;

    let mut h = Harness::new("video_trim");
    let clip = h.base.join("clip.mp4");
    std::fs::write(&clip, b"not really mp4").unwrap();
    let ids = h.app.add_paths(&[clip]);
    h.app
        .place_items_on_board(&ids, eframe::egui::Pos2::new(100.0, 100.0));
    let node_id = h.app.doc().scene.nodes[0].id;
    h.app.patch_nodes(&[node_id], |n| {
        if let NodeKind::Image(i) = &mut n.kind {
            i.video = VideoOpts {
                start: 3.0,
                end: Some(11.0),
                controls: true,
                ..VideoOpts::default()
            };
        }
    });

    let path = h.base.join("video.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());

    let mut h2 = Harness::new("video_trim2");
    h2.app.open_doc_at(path);
    let NodeKind::Image(img) = &h2.app.doc().scene.nodes[0].kind else {
        panic!("expected image node");
    };
    assert_eq!(img.video.start, 3.0);
    assert_eq!(img.video.end, Some(11.0));
    assert!(img.video.controls);
    h2.frame();
}

#[test]
fn video_pan_scrubs_the_full_trim_and_does_not_journal() {
    use slate_doc::scene::VideoOpts;

    let mut h = Harness::new("video_pan");
    let clip = h.base.join("clip.mp4");
    std::fs::write(&clip, b"not really mp4").unwrap();
    let ids = h.app.add_paths(&[clip]);
    h.app
        .place_items_on_board(&ids, eframe::egui::Pos2::new(400.0, 300.0));
    let node_id = h.app.doc().scene.nodes[0].id;
    h.app.patch_nodes(&[node_id], |n| {
        if let NodeKind::Image(i) = &mut n.kind {
            i.video = VideoOpts {
                start: 3.0,
                end: Some(11.0),
                ..VideoOpts::default()
            };
        }
    });
    h.app.video_probe_for_test(node_id, 20.0);
    let before = h.app.doc().scene.nodes.clone();
    assert!(h.app.video_shown_for_test(node_id).is_none());

    let rect = h.app.doc().scene.node(node_id).unwrap().rect;
    h.app
        .video_pointer(Pos2::new(rect.x + 0.5, rect.y + rect.h * 0.5));
    let left = h.app.video_shown_for_test(node_id).expect("in-point");
    assert!((left - 3.0).abs() < 0.05, "left pan {left}");

    h.app
        .video_pointer(Pos2::new(rect.x + rect.w - 0.5, rect.y + rect.h * 0.5));
    let right = h.app.video_shown_for_test(node_id).expect("out-point");
    assert!((right - 11.0).abs() < 0.05, "right pan {right}");

    h.app.video_clear_hover();
    let held = h.app.video_shown_for_test(node_id).expect("held frame");
    assert!(
        (held - right).abs() < 0.001,
        "pan should leave the playhead {held}"
    );

    h.app.video_click(node_id);
    assert!(h.app.video_playing_for_test(node_id));
    assert_eq!(
        before,
        h.app.doc().scene.nodes,
        "scrub and play are derived, not journaled"
    );
    h.app.video_click(node_id);
    assert!(!h.app.video_playing_for_test(node_id));
    let paused = h.app.video_shown_for_test(node_id).expect("paused");
    assert!(
        (paused - right).abs() < 0.2,
        "pause keeps the panned frame {paused}"
    );
    assert_eq!(before, h.app.doc().scene.nodes);
}

#[test]
fn export_renders_kind_specific_cards() {
    let mut h = Harness::new("kind_cards");
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.seed_frame(None);
    let notes = h.base.join("notes.md");
    std::fs::write(&notes, "# Title\nbody text").unwrap();
    let clip = h.base.join("clip.mp4");
    std::fs::write(&clip, b"fake").unwrap();
    let report = h.base.join("report.pdf");
    std::fs::write(&report, b"%PDF fake").unwrap();

    let ids = h.app.add_paths(&[notes, clip, report]);
    assert_eq!(ids.len(), 3);
    // Drop at the frame center: the multi-item grid is centered on the drop
    // point, so this keeps all three cards inside the exported frame.
    h.app
        .place_items_on_board(&ids, eframe::egui::Pos2::new(400.0, 225.0));
    for _ in 0..3 {
        h.frame(); // board paints snippet cards / badges without panicking
    }

    let out = h.base.join("export");
    h.app.do_export(out.clone());
    h.wait_for_export();
    let html = std::fs::read_to_string(out.join("Untitled-slides").join("index.html")).unwrap();
    assert!(html.contains("class=\"textcard\""), "text snippet card");
    assert!(html.contains("# Title"), "snippet content");
    assert!(html.contains("<video"), "web-safe video element");
    assert!(
        html.contains("<span class=\"badge\">PDF</span>"),
        "pdf card badge"
    );
    h.frame();
}

/// An untitled tab whose only content is on its board is not blank:
/// opening a workbook takes a new tab instead of replacing the drawing.
#[test]
fn opening_a_workbook_keeps_an_untitled_board_only_tab() {
    let mut h = brush_board("open_keeps_board_only_tab");
    let freehand = [
        Pos2::new(40.0, 40.0),
        Pos2::new(80.0, 60.0),
        Pos2::new(120.0, 40.0),
    ];
    press_drag_release_frames(&mut h, &freehand, egui::Modifiers::NONE, |_| {});
    let mark = h.app.doc().scene.nodes[0].clone();
    assert!(h.app.tab().path.is_none());
    assert!(h.app.doc().items.is_empty());
    assert!(!h.app.tab().is_blank(), "a board stroke is content");
    let (drawn_tab, tabs) = (h.app.tab().id, h.app.tabs.len());
    let path = h.base.join("other.slate");
    SlateDoc::new("Other").save_to(&path).unwrap();
    h.app.open_doc_at(path.clone());
    assert_eq!(h.app.tabs.len(), tabs + 1, "the workbook took a new tab");
    assert_eq!(h.app.tab().path.as_deref(), Some(path.as_path()));
    let kept = h.app.tabs.iter().find(|t| t.id == drawn_tab).unwrap();
    assert_eq!(kept.doc.scene.nodes, vec![mark]);
}

/// Review r12 note N7: an untitled tab whose board was emptied still holds
/// its undo history, which may be the only copy of the drawing. Opening a
/// workbook takes a new tab, and one undo there brings the stroke back.
#[test]
fn opening_a_workbook_keeps_an_untitled_tab_with_undo_history() {
    let mut h = brush_board("open_keeps_undo_history");
    let freehand = [
        Pos2::new(40.0, 40.0),
        Pos2::new(80.0, 60.0),
        Pos2::new(120.0, 40.0),
    ];
    press_drag_release_frames(&mut h, &freehand, egui::Modifiers::NONE, |_| {});
    let mark = h.app.doc().scene.nodes[0].clone();
    h.app.delete_board_nodes(&[mark.id]);
    assert!(h.app.tab().path.is_none());
    assert!(h.app.doc().scene.nodes.is_empty());
    assert!(!h.app.tab().is_blank(), "undo history is content");
    let (drawn_tab, tabs) = (h.app.tab().id, h.app.tabs.len());
    let depth = h.app.tab().journal.undo_depth();
    let path = h.base.join("other.slate");
    SlateDoc::new("Other").save_to(&path).unwrap();
    h.app.open_doc_at(path.clone());
    assert_eq!(h.app.tabs.len(), tabs + 1, "the workbook took a new tab");
    assert_eq!(h.app.tab().path.as_deref(), Some(path.as_path()));
    let at = h.app.tabs.iter().position(|t| t.id == drawn_tab).unwrap();
    assert_eq!(h.app.tabs[at].journal.undo_depth(), depth, "history kept");
    h.app.switch_tab(at);
    h.frame();
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes, vec![mark]);
}

/// Review r10 finding 1, same tab: opening a workbook reuses a blank tab
/// under the same tab id, and the loaded document numbers its nodes
/// afresh. The stroke is deleted and the tab's history dropped, so the tab
/// is blank again while the chain still names it. A Shift drag there starts
/// at the press and leaves the loaded node with the mark's number.
#[test]
fn brush_shift_after_opening_a_workbook_over_the_tab_starts_at_the_press() {
    let mut h = brush_board("brush_shift_open_over");
    let freehand = [
        Pos2::new(40.0, 40.0),
        Pos2::new(80.0, 60.0),
        Pos2::new(120.0, 40.0),
    ];
    press_drag_release_frames(&mut h, &freehand, egui::Modifiers::NONE, |_| {});
    let mark = h.app.doc().scene.nodes[0].clone();
    let tab_id = h.app.tab().id;
    h.app.delete_board_nodes(&[mark.id]);
    let tab = h.app.tab_mut();
    tab.journal = slate_doc::scene::SceneJournal::default();
    tab.edits.clear();
    tab.edit_redo.clear();
    assert!(h.app.tab().is_blank());
    let mut other = SlateDoc::new("Other");
    other.view.active_view = ViewKind::Board;
    while other.scene.node(mark.id).is_none() {
        let node = other
            .scene
            .build_node(mark.rect.translated(0.0, 200.0), mark.kind.clone());
        other.scene.nodes.push(node);
    }
    let path = h.base.join("other.slate");
    other.save_to(&path).unwrap();
    h.app.open_doc_at(path);
    assert_eq!(h.app.tab().id, tab_id, "the blank tab was reused");
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = egui::Vec2::ZERO;
    h.frame();
    assert!(h.app.doc().scene.node(mark.id).is_some());
    assert_shift_starts_at_the_press(&mut h, Pos2::new(300.0, 320.0));
}
