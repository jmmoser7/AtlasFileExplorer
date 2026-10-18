use super::*;
use crate::app::{ExportUi, TabState};
use crossbeam_channel::{unbounded, Sender};

fn started(app: &mut AtlasApp) -> Sender<ExportMsg> {
    let (tx, rx) = unbounded();
    app.export_ui = Some(ExportUi {
        rx,
        origin: app.cloud_context().unwrap(),
        completed: None,
        done: 0,
        total: 1,
        current: String::new(),
    });
    tx
}

fn completed() -> ExportMsg {
    ExportMsg::Done {
        manifest_path: "synthetic-export/file-atlas-manifest-test.json".into(),
        copied: vec!["synthetic-export/saved.bin".into()],
        created_dirs: vec!["synthetic-export/chosen".into()],
        errors: Vec::new(),
    }
}

#[test]
fn completion_appends_to_the_source_journal_without_requiring_its_scan_epoch() {
    let mut app = super::super::tests::app();
    let tx = started(&mut app);
    app.generation += 1;
    tx.send(completed()).unwrap();
    app.poll_export();
    assert!(app.export_ui.is_none());
    assert_eq!(app.journal.entries.len(), 1);
    assert!(matches!(
        app.journal.entries[0].action,
        Action::Export { .. }
    ));
}

#[test]
fn switching_tabs_keeps_export_undo_in_the_parked_source_journal() {
    let mut app = super::super::tests::app();
    let tx = started(&mut app);
    app.park_active_workspace();
    app.tabs.push(TabState::empty());
    app.active_tab = 1;
    app.root = Some(PathBuf::from("another-folder"));
    tx.send(completed()).unwrap();
    app.poll_export();
    assert!(app.export_ui.is_none());
    assert!(
        app.journal.entries.is_empty(),
        "the visible tab must not receive source undo"
    );
    let parked = app.tabs[0].parked.as_ref().unwrap();
    assert_eq!(parked.journal.entries.len(), 1);
    let Action::Export {
        copied,
        created_dirs,
        ..
    } = &parked.journal.entries[0].action
    else {
        panic!("export undo was lost");
    };
    assert_eq!(copied, &["synthetic-export/saved.bin"]);
    assert_eq!(created_dirs, &["synthetic-export/chosen"]);
}

#[test]
fn a_journal_still_loading_keeps_completion_until_it_can_append_safely() {
    let mut app = super::super::tests::app();
    let tx = started(&mut app);
    let (_load_tx, load_rx) = unbounded();
    app.pending_load = Some((app.root.clone().unwrap(), load_rx));
    tx.send(completed()).unwrap();
    app.poll_export();
    assert!(app.export_ui.as_ref().unwrap().completed.is_some());
    assert!(app.journal.entries.is_empty());
    app.pending_load = None;
    app.poll_export();
    app.poll_export();
    assert!(app.export_ui.is_none());
    assert_eq!(
        app.journal.entries.len(),
        1,
        "completion must append exactly once"
    );
}

#[test]
fn a_closed_source_reports_the_manifest_and_never_pollutes_another_journal() {
    let mut app = super::super::tests::app();
    let tx = started(&mut app);
    app.tabs = vec![TabState::empty()];
    app.root = Some(PathBuf::from("another-folder"));
    tx.send(completed()).unwrap();
    app.poll_export();
    assert!(app.export_ui.is_none());
    assert!(app.journal.entries.is_empty());
    assert!(app.toasts.iter().any(|(message, _)| {
        message.contains("undo was not attached")
            && message.contains("synthetic-cloud-source")
            && message.contains("file-atlas-manifest-test.json")
    }));
}

#[test]
fn a_same_tab_rebound_to_another_root_cannot_take_the_old_export_record() {
    let mut app = super::super::tests::app();
    let tx = started(&mut app);
    app.root = Some(PathBuf::from("another-folder"));
    tx.send(completed()).unwrap();
    app.poll_export();
    assert!(app.journal.entries.is_empty());
    assert!(app
        .toasts
        .iter()
        .any(|(message, _)| message.contains("undo was not attached")));
}

#[test]
fn progress_is_spread_across_frames_before_completion_is_applied() {
    let mut app = super::super::tests::app();
    let tx = started(&mut app);
    for done in 0..100 {
        tx.send(ExportMsg::Progress {
            done,
            total: 100,
            current: done.to_string(),
        })
        .unwrap();
    }
    tx.send(completed()).unwrap();
    app.poll_export();
    assert_eq!(app.export_ui.as_ref().unwrap().done, 63);
    assert!(app.journal.entries.is_empty());
    app.poll_export();
    assert!(app.export_ui.is_none());
    assert_eq!(app.journal.entries.len(), 1);
}

#[test]
fn a_dead_export_worker_reports_partial_output_risk_and_releases_busy_state() {
    let mut app = super::super::tests::app();
    drop(started(&mut app));
    app.poll_export();
    assert!(app.export_ui.is_none());
    assert!(app
        .toasts
        .iter()
        .any(|(message, _)| message.contains("partial copies")));
    assert!(app.journal.entries.is_empty());
}
