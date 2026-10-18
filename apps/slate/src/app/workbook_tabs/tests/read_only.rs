use super::*;
use crate::app::UnsavedClose;
use atlas_shell::widgets::ConfirmChoice;

fn readonly_draft() -> Harness {
    let mut h = Harness::new("readonly-draft-transitions");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.new_tab();
    h.app.place_text_at(eframe::egui::pos2(10.0, 20.0));
    h.app.text_box_draft.as_mut().unwrap().buffer = "Preserve these pending words".into();
    h.app.tab_mut().dirty = false;
    h.app.tab_mut().read_only = true;
    h
}

#[test]
fn readonly_pending_text_blocks_all_document_replacements() {
    let mut h = readonly_draft();
    let index = h.app.active_tab;
    let id = h.app.tab().id;
    let count = h.app.tabs.len();
    let before = h.app.doc().clone();
    h.app.commit_text_box_draft();
    assert!(h.app.recovery_has_text_drafts());
    assert!(!h.app.finish_text_compose_on_click_away());
    h.app.new_tab();
    h.app.switch_tab(0);
    let other = h.base.join("other.slate");
    SlateDoc::new("Other").save_to(&other).unwrap();
    h.app.open_doc_at(other);
    h.app.install_recovered_doc(SlateDoc::new("Recovered"));
    assert_eq!(h.app.active_tab, index);
    assert_eq!(h.app.tab().id, id);
    assert_eq!(h.app.tabs.len(), count);
    assert_eq!(h.app.doc(), &before);
    assert!(!h.app.tab().dirty);
    assert_eq!(
        h.app.text_box_draft.as_ref().unwrap().buffer,
        "Preserve these pending words"
    );
}

#[test]
fn readonly_pending_text_requires_close_confirmation_and_only_explicit_discard_clears_it() {
    let mut h = readonly_draft();
    let index = h.app.active_tab;
    let count = h.app.tabs.len();
    h.app.close_tab(index);
    assert!(matches!(h.app.unsaved_close, Some(UnsavedClose::Tab(i)) if i == index));
    assert_eq!(h.app.tabs.len(), count);
    assert!(!h.app.tab_ready_to_close(index));
    assert_eq!(h.app.first_unsaved_tab(), Some(index));
    h.app.apply_unsaved_choice(&h.ctx, ConfirmChoice::Cancel);
    assert!(h.app.recovery_has_text_drafts());
    h.app.unsaved_close = Some(UnsavedClose::Exit);
    h.app.resume_unsaved_close_if_ready(&h.ctx);
    assert!(matches!(h.app.unsaved_close, Some(UnsavedClose::Exit)));
    h.app.apply_unsaved_choice(&h.ctx, ConfirmChoice::Cancel);
    h.app.close_tab(index);
    h.app.apply_unsaved_choice(&h.ctx, ConfirmChoice::Secondary);
    assert_eq!(h.app.tabs.len(), count - 1);
    assert!(!h.app.text_compose_active());
    assert!(!h.app.recovery_has_text_drafts());
    assert!(h.app.doc().scene.nodes.is_empty());
}
