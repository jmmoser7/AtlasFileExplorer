use super::*;
use crate::app::tests::Harness;
mod read_only;

fn saved_source(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.seed();
    let source = h.base.join("source.slate");
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, source);
    h.app.tab_mut().doc.items[0].path = PathBuf::from("assets/pasted/source.png");
    h.app.tab_mut().dirty = true;
    h
}

#[cfg(unix)]
#[test]
fn save_as_file_alias_preserves_the_link_source_and_original_authority() {
    let mut h = saved_source("save-as-symlink");
    let source = h.app.tab().path.clone().unwrap();
    let alias = h.base.join("alias.slate");
    std::os::unix::fs::symlink(&source, &alias).unwrap();
    let bytes = std::fs::read(&source).unwrap();
    let doc = h.app.doc().clone();
    let token = h.app.tab().lease.as_ref().unwrap().info().token.clone();
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, alias.clone());
    assert!(std::fs::symlink_metadata(&alias)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read(source.clone()).unwrap(), bytes);
    assert_eq!(h.app.doc(), &doc);
    assert_eq!(h.app.tab().path.as_ref(), Some(&source));
    assert_eq!(h.app.tab().lease.as_ref().unwrap().info().token, token);
    assert!(h.app.tab().dirty);
    assert!(!Lease::lock_path(&alias).exists());
}

#[test]
fn save_as_held_destination_preserves_bytes_model_history_jobs_and_old_lease() {
    let mut h = saved_source("save-as-held");
    let destination = h.base.join("held.slate");
    std::fs::write(&destination, b"destination must remain unchanged").unwrap();
    let LeaseState::Acquired(_other_holder) = Lease::acquire(&destination).unwrap() else {
        panic!("fixture holder");
    };
    let doc = h.app.doc().clone();
    let source = h.app.tab().path.clone();
    let token = h.app.tab().lease.as_ref().unwrap().info().token.clone();
    let edits = h.app.tab().edits.len();
    let journal = h.app.tab().journal.undo_depth();
    let jobs = h.app.asset_save_rx.len();
    let generation = h.app.asset_save_generation.clone();
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, destination.clone());
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"destination must remain unchanged"
    );
    assert_eq!(h.app.doc(), &doc);
    assert_eq!(h.app.tab().path, source);
    assert!(h.app.tab().dirty);
    assert_eq!(h.app.tab().edits.len(), edits);
    assert_eq!(h.app.tab().journal.undo_depth(), journal);
    assert_eq!(h.app.asset_save_rx.len(), jobs);
    assert_eq!(h.app.asset_save_generation, generation);
    assert_eq!(h.app.tab().lease.as_ref().unwrap().info().token, token);
    h.app
        .tab()
        .lease
        .as_ref()
        .unwrap()
        .validate_ownership()
        .unwrap();
}

#[test]
fn save_as_lease_io_error_does_not_prepare_the_document_or_schedule_copies() {
    let mut h = saved_source("save-as-lease-error");
    let destination = h.base.join("missing-parent").join("copy.slate");
    let doc = h.app.doc().clone();
    let source = h.app.tab().path.clone();
    let jobs = h.app.asset_save_rx.len();
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, destination.clone());
    assert!(!destination.exists());
    assert_eq!(h.app.doc(), &doc);
    assert_eq!(h.app.tab().path, source);
    assert_eq!(h.app.asset_save_rx.len(), jobs);
    assert!(h.app.tab().dirty);
}

#[test]
fn failed_destination_write_restores_name_view_locators_and_redo() {
    let mut h = saved_source("save-as-write-error");
    let destination = h.base.join("directory.slate");
    std::fs::create_dir(&destination).unwrap();
    h.app
        .tab_mut()
        .edit_redo
        .push(crate::app::board::BoardMark::Scene);
    let doc = h.app.doc().clone();
    let source = h.app.tab().path.clone();
    let edits = h.app.tab().edits.len();
    let jobs = h.app.asset_save_rx.len();
    let token = h.app.tab().lease.as_ref().unwrap().info().token.clone();
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, destination.clone());
    assert!(destination.is_dir());
    assert_eq!(h.app.doc(), &doc);
    assert_eq!(h.app.tab().path, source);
    assert_eq!(h.app.tab().edits.len(), edits);
    assert_eq!(h.app.tab().edit_redo.len(), 1);
    assert_eq!(h.app.asset_save_rx.len(), jobs);
    assert!(h.app.tab().dirty);
    assert_eq!(h.app.tab().lease.as_ref().unwrap().info().token, token);
}

#[test]
fn same_path_save_reuses_the_owned_lease() {
    let mut h = saved_source("save-same-lease");
    let path = h.app.tab().path.clone().unwrap();
    let token = h.app.tab().lease.as_ref().unwrap().info().token.clone();
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, path.clone());
    assert!(!h.app.tab().dirty);
    assert_eq!(h.app.tab().lease.as_ref().unwrap().info().token, token);
    assert_eq!(
        slate_doc::SlateDoc::load_from(&path).unwrap().name,
        "source"
    );
}

#[test]
fn save_as_same_path_read_only_and_another_open_tab_are_refused() {
    let mut h = saved_source("save-same-readonly");
    let source = h.app.tab().path.clone().unwrap();
    let bytes = std::fs::read(&source).unwrap();
    h.app.tab_mut().read_only = true;
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, source.clone());
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    h.app.new_tab();
    h.app.tab_mut().doc.name = "different workbook".into();
    h.app.tab_mut().dirty = true;
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, source);
    assert!(h.app.tab().path.is_none());
    assert!(h.app.tab().dirty);
}

#[test]
fn inactive_and_home_tabs_keep_their_owned_lease_and_observe_cached_loss() {
    let mut h = saved_source("save-home-lease");
    let source = h.app.tab().path.clone().unwrap();
    let old_tab = h.app.active_tab;
    h.app.new_tab();
    h.app.at_home = true;
    assert!(matches!(
        Lease::acquire(&source).unwrap(),
        LeaseState::Held(_)
    ));
    let lease = h.app.tabs[old_tab].lease.as_ref().unwrap();
    let mut replacement = lease.info().clone();
    replacement.token = "new holder".into();
    std::fs::write(
        Lease::lock_path(&source),
        serde_json::to_vec(&replacement).unwrap(),
    )
    .unwrap();
    assert!(lease.validate_ownership().is_err());
    h.app.poll_lease_failures(&h.ctx);
    assert!(h.app.tabs[old_tab].read_only);
    assert!(h.app.at_home);
    assert_eq!(h.app.active_tab, old_tab + 1);
}
