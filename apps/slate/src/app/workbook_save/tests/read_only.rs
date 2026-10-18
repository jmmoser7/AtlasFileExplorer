use super::*;
use slate_doc::scene::NodeKind;

fn pending_text() -> (Harness, PathBuf, Vec<u8>) {
    let mut h = Harness::new("readonly-copy-pending-text");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let source = h.base.join("source.slate");
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, source.clone());
    let bytes = std::fs::read(&source).unwrap();
    h.app.place_text_at(eframe::egui::pos2(10.0, 20.0));
    h.app.text_box_draft.as_mut().unwrap().buffer = "Words typed before lease loss".into();
    h.app.tab_mut().dirty = false;
    (h, source, bytes)
}

fn text(doc: &slate_doc::SlateDoc) -> &str {
    let NodeKind::Text(text) = &doc.scene.nodes[0].kind else {
        panic!()
    };
    &text.text
}

#[test]
fn readonly_save_copy_preserves_pending_words_without_touching_original() {
    let (mut h, source, bytes) = pending_text();
    h.app.tab_mut().read_only = true;
    let destination = h.base.join("copy.slate");
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, destination.clone());
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    assert_eq!(
        text(&slate_doc::SlateDoc::load_from(&destination).unwrap()),
        "Words typed before lease loss"
    );
    assert_eq!(h.app.tab().path.as_ref(), Some(&destination));
    assert!(!h.app.tab().read_only);
    assert!(!h.app.recovery_has_text_drafts());
}

#[test]
fn failed_readonly_save_copy_retains_dirty_words_and_original_write_refusal() {
    let (mut h, source, bytes) = pending_text();
    h.app.tab_mut().read_only = true;
    let destination = h.base.join("directory.slate");
    std::fs::create_dir(&destination).unwrap();
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, destination);
    assert_eq!(h.app.tab().path.as_ref(), Some(&source));
    assert!(h.app.tab().read_only && h.app.tab().dirty);
    assert_eq!(text(h.app.doc()), "Words typed before lease loss");
    h.app.save_doc_to(tab, source.clone());
    assert_eq!(std::fs::read(source).unwrap(), bytes);
}

#[test]
fn refused_copy_destination_keeps_pending_buffer_and_readonly_source_unchanged() {
    let (mut h, source, bytes) = pending_text();
    h.app.tab_mut().read_only = true;
    let destination = h.base.join("held.slate");
    let LeaseState::Acquired(_held) = Lease::acquire(&destination).unwrap() else {
        panic!()
    };
    let before = h.app.doc().clone();
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, destination.clone());
    assert_eq!(h.app.doc(), &before);
    assert!(h.app.tab().read_only && !h.app.tab().dirty);
    assert!(h.app.recovery_has_text_drafts());
    assert!(!destination.exists());
    assert_eq!(std::fs::read(source).unwrap(), bytes);
}

#[test]
fn actual_active_lease_failure_materializes_pending_words_before_readonly() {
    let (mut h, source, bytes) = pending_text();
    let lease = h.app.tab().lease.as_ref().unwrap();
    let mut replacement = lease.info().clone();
    replacement.token = "replaced lease".into();
    std::fs::write(
        Lease::lock_path(&source),
        serde_json::to_vec(&replacement).unwrap(),
    )
    .unwrap();
    assert!(lease.validate_ownership().is_err());
    h.app.poll_lease_failures(&h.ctx);
    assert!(h.app.tab().read_only && h.app.tab().dirty);
    assert_eq!(text(h.app.doc()), "Words typed before lease loss");
    assert!(!h.app.text_compose_active());
    assert_eq!(std::fs::read(source).unwrap(), bytes);
}
