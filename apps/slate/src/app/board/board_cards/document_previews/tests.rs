use super::*;
use crate::app::tests::Harness;

impl SlateApp {
    pub(crate) fn complete_document_preview_for_test(&mut self, item: ItemId, text: &str) {
        self.prepare_document_card(item);
        let source = Source::for_item(self.tab(), item).unwrap();
        self.document_previews.borrow_mut().inject(source, text);
        self.pump_document_previews(&egui::Context::default());
    }
}

#[test]
fn cache_identity_is_tab_source_signature_and_workbook_relative_base() {
    let mut h = Harness::new("document-cache-identity");
    let item = h
        .app
        .doc_mut()
        .add_item("note.txt".into(), "note.txt", 12, 1, "txt");
    h.app.cache_snippet(item, Some("first tab".into()));
    let first = h.app.active_tab;
    h.app.new_tab();
    let other = h
        .app
        .doc_mut()
        .add_item("note.txt".into(), "note.txt", 12, 1, "txt");
    assert_eq!(item, other);
    assert!(h.app.cached_snippet(other).is_none());
    h.app.cache_snippet(other, Some("second tab".into()));
    h.app.switch_tab(first);
    assert_eq!(h.app.cached_snippet(item), Some("first tab"));
    h.app.tab_mut().path = Some(h.base.join("moved/Workbook.slate"));
    assert!(
        h.app.cached_snippet(item).is_none(),
        "Save As rebases relative locators"
    );
    h.app.cache_snippet(item, Some("new base".into()));
    h.app
        .doc_mut()
        .items
        .iter_mut()
        .find(|file| file.id == item)
        .unwrap()
        .mtime += 1;
    assert!(h.app.cached_snippet(item).is_none());
}

#[test]
fn stale_completion_cannot_overwrite_an_authored_update_or_rebound_source() {
    let mut h = Harness::new("document-cache-stale");
    let item = h
        .app
        .doc_mut()
        .add_item("one.txt".into(), "one.txt", 12, 1, "txt");
    assert!(!h.app.prepare_document_card(item));
    let source = Source::for_item(h.app.tab(), item).unwrap();
    h.app
        .document_previews
        .borrow_mut()
        .inject(source, "old worker words");
    h.app.cache_snippet(item, Some("new authored words".into()));
    h.app.pump_document_previews(&h.ctx);
    assert_eq!(h.app.cached_snippet(item), Some("new authored words"));
    h.app.invalidate_document_card(item);
    assert!(!h.app.prepare_document_card(item));
    let source = Source::for_item(h.app.tab(), item).unwrap();
    h.app
        .document_previews
        .borrow_mut()
        .inject(source, "previous locator");
    h.app
        .doc_mut()
        .items
        .iter_mut()
        .find(|file| file.id == item)
        .unwrap()
        .path = "two.txt".into();
    h.app.pump_document_previews(&h.ctx);
    assert!(h.app.cached_snippet(item).is_none());
}

#[test]
fn sheet_paints_share_storage_and_editing_uses_copy_on_write() {
    let mut h = Harness::new("document-cache-sheet");
    let item = h
        .app
        .doc_mut()
        .add_item("rows.csv".into(), "rows.csv", 12, 1, "csv");
    h.app.cache_sheet(
        item,
        Some(vec![vec![atlas_core::office::SheetCell {
            text: "before".into(),
            fill: None,
        }]]),
    );
    let first = h.app.cached_sheet(item).unwrap();
    let second = h.app.cached_sheet(item).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    h.app.cached_sheet_mut(item).unwrap()[0][0].text = "after".into();
    assert_eq!(first[0][0].text, "before");
    assert_eq!(h.app.cached_sheet(item).unwrap()[0][0].text, "after");
}

#[test]
fn committed_sheet_edits_refresh_footprint_and_close_rechecks_eviction() {
    let mut h = Harness::new("document-cache-edit-size");
    let item = h
        .app
        .doc_mut()
        .add_item("rows.csv".into(), "rows.csv", 12, 1, "csv");
    h.app.cache_sheet(
        item,
        Some(vec![vec![atlas_core::office::SheetCell {
            text: "before".into(),
            fill: None,
        }]]),
    );
    let before = h.app.tab().document_cards.entries[&item].data.bytes;
    h.app.sheet_edit = Some(crate::app::board::SheetEdit {
        node: slate_doc::NodeId(1),
        item,
        row: 0,
        col: 0,
        buf: "expanded cell".repeat(1024),
        origin: "before".into(),
        fresh: false,
        screen: egui::Rect::NOTHING,
        font_px: 12.0,
    });
    h.app.commit_sheet_edit();
    assert!(h.app.tab().document_cards.entries[&item].data.bytes > before);
    h.app.document_previews.borrow_mut().changed();
    h.app.close_sheet();
    assert!(
        h.app.document_previews.borrow_mut().changed(),
        "releasing a pinned sheet must recheck its byte budget"
    );
}

#[test]
fn inactive_originating_tab_receives_its_own_result() {
    let mut h = Harness::new("document-cache-origin");
    let item = h
        .app
        .doc_mut()
        .add_item("one.txt".into(), "one.txt", 12, 1, "txt");
    let first = h.app.active_tab;
    h.app.prepare_document_card(item);
    let source = Source::for_item(h.app.tab(), item).unwrap();
    h.app
        .document_previews
        .borrow_mut()
        .inject(source, "origin words");
    h.app.new_tab();
    let other = h
        .app
        .doc_mut()
        .add_item("one.txt".into(), "one.txt", 12, 1, "txt");
    h.app.pump_document_previews(&h.ctx);
    assert!(h.app.cached_snippet(other).is_none());
    h.app.switch_tab(first);
    assert_eq!(h.app.cached_snippet(item), Some("origin words"));
}

#[test]
fn completed_file_writes_invalidate_matching_sources_across_tabs_only() {
    let mut h = Harness::new("document-source-invalidation");
    let source = h.base.join("shared.txt");
    let item = h
        .app
        .doc_mut()
        .add_item(source.clone(), "shared.txt", 12, 1, "txt");
    h.app.prepare_document_card(item);
    let first = h.app.active_tab;
    h.app.new_tab();
    let other =
        h.app
            .doc_mut()
            .add_item(h.base.join("unrelated.txt"), "unrelated.txt", 12, 1, "txt");
    assert_eq!(item, other);
    h.app.cache_snippet(other, Some("unrelated words".into()));
    let same_source = h
        .app
        .doc_mut()
        .add_item(source.clone(), "shared.txt", 12, 1, "txt");
    h.app
        .cache_snippet(same_source, Some("old shared words".into()));
    h.app.invalidate_document_source(item, &source);
    assert_eq!(h.app.cached_snippet(other), Some("unrelated words"));
    assert!(h.app.cached_snippet(same_source).is_none());
    h.app.switch_tab(first);
    assert!(
        !h.app.document_previews.borrow().busy(),
        "originating pending request was invalidated too"
    );
}

#[test]
fn tiny_document_cards_do_not_admit_preview_reads_below_text_lod() {
    let mut h = Harness::new("document-preview-lod");
    h.app.leave_home();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    for (index, name) in ["note.txt", "rows.csv", "letter.docx"].iter().enumerate() {
        let item = h
            .app
            .doc_mut()
            .add_item(h.base.join(name), *name, 12, 1, "document");
        let node = h.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(index as f32 * 160.0, 0.0, 140.0, 100.0),
            slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
        );
        h.app.add_nodes(vec![node]);
    }
    h.app.tab_mut().cam.z = 0.05;
    h.frame();
    assert!(!h.app.document_previews.borrow().busy());
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    assert!(
        h.app.document_previews.borrow().busy(),
        "legible cards admit asynchronous preparation"
    );
}
