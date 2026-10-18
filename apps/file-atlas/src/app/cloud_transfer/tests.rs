use super::*;
use crate::app::{Journal, TabState};
use atlas_core::index::Db;
use atlas_shell::file_picker::DialogGate;
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicU64, Ordering};

fn context() -> CloudContext {
    CloudContext {
        tab: 7,
        root: PathBuf::from("synthetic-cloud-source"),
        generation: 13,
    }
}

fn export() -> Transfer {
    Transfer::Export {
        dest: PathBuf::from("synthetic-export-destination"),
        items: vec![ExportItem {
            source: context().root.join("cloud.bin"),
            rel: "cloud.bin".into(),
            dest_rel: "Chosen".into(),
            new_name: Some("saved.bin".into()),
        }],
    }
}

fn copy() -> Transfer {
    Transfer::FsCopy {
        op: FsOp::Copy {
            sources: vec![context().root.join("cloud.bin")],
            dest_dir: PathBuf::from("synthetic-copy-destination"),
        },
        label: "Copy".into(),
    }
}

fn audit(transfer: Transfer, context: CloudContext) -> (Sender<CloudCost>, CloudAudit) {
    let (tx, rx) = unbounded();
    (
        tx,
        CloudAudit {
            rx,
            transfer,
            context,
            label: "Cloud audit test".into(),
            at: Pos2::ZERO,
        },
    )
}

pub(super) fn app() -> AtlasApp {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let database = std::env::temp_dir().join(format!(
        "atlas_cloud_audit_{}_{}.db",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut app = AtlasApp::with_db(
        &egui::Context::default(),
        Db::open_at(database),
        None,
        DialogGate::new(),
    );
    app.tabs = vec![TabState::empty()];
    app.active_tab = 0;
    app.root = Some(context().root);
    app.at_home = false;
    app.journal = Journal::default();
    app
}

fn inject(app: &mut AtlasApp, transfer: Transfer) -> Sender<CloudCost> {
    let (tx, audit) = audit(transfer, app.cloud_context().unwrap());
    app.cloud_audit = Some(audit);
    tx
}

fn assert_not_started(app: &AtlasApp) {
    assert!(app.fs_op.is_none(), "no copy worker may start");
    assert!(app.export_ui.is_none(), "no export worker may start");
    assert!(
        app.journal.entries.is_empty(),
        "a refused transfer is not journaled"
    );
}

#[test]
fn a_pending_audit_is_nonblocking_and_keeps_the_request() {
    let mut app = app();
    let _tx = inject(&mut app, export());
    app.poll_cloud_audit();
    assert!(app.cloud_audit.is_some());
    assert!(app.cloud_copy_plan.is_none());
    assert_not_started(&app);
}

#[test]
fn nonzero_export_cost_uses_the_existing_confirmation_before_any_worker() {
    let mut app = app();
    let tx = inject(&mut app, export());
    tx.send(CloudCost {
        files: 3,
        bytes: 4096,
    })
    .unwrap();
    app.poll_cloud_audit();
    assert!(app.cloud_audit.is_none());
    let plan = app.cloud_copy_plan.as_ref().expect("download confirmation");
    assert_eq!(
        plan.cost,
        CloudCost {
            files: 3,
            bytes: 4096
        }
    );
    let Transfer::Export { dest, items } = &plan.transfer else {
        panic!("export snapshot lost")
    };
    assert_eq!(dest, &PathBuf::from("synthetic-export-destination"));
    assert_eq!(items[0].dest_rel, "Chosen");
    assert_eq!(items[0].new_name.as_deref(), Some("saved.bin"));
    assert_not_started(&app);
}

#[test]
fn cancelling_a_download_discards_the_export_without_starting_it() {
    let mut app = app();
    let tx = inject(&mut app, export());
    tx.send(CloudCost { files: 1, bytes: 1 }).unwrap();
    app.poll_cloud_audit();
    app.respond_cloud_copy(false);
    assert!(app.cloud_copy_plan.is_none());
    assert_not_started(&app);
}

#[test]
fn a_dead_audit_never_authorizes_a_copy_or_export() {
    for transfer in [copy(), export()] {
        let mut app = app();
        drop(inject(&mut app, transfer));
        app.poll_cloud_audit();
        assert!(app.cloud_audit.is_none());
        assert!(app.cloud_copy_plan.is_none());
        assert!(app.toasts.iter().any(|(text, _)| text == AUDIT_FAILED));
        assert_not_started(&app);
    }
}

#[test]
fn root_tab_and_epoch_changes_all_refuse_late_zero_cost_replies() {
    for change in 0..3 {
        let mut app = app();
        let tx = inject(&mut app, export());
        match change {
            0 => app.root = Some(PathBuf::from("another-folder")),
            1 => app.tabs[0].id += 1,
            _ => app.generation += 1,
        }
        tx.send(CloudCost::default()).unwrap();
        app.poll_cloud_audit();
        assert!(app.cloud_audit.is_none());
        assert!(app.cloud_copy_plan.is_none());
        assert_not_started(&app);
    }
}

#[test]
fn stale_confirmation_cannot_start_a_transfer_after_the_user_accepts() {
    let mut app = app();
    let tx = inject(&mut app, export());
    tx.send(CloudCost { files: 1, bytes: 1 }).unwrap();
    app.poll_cloud_audit();
    app.root = Some(PathBuf::from("another-folder"));
    app.respond_cloud_copy(true);
    assert!(app.cloud_copy_plan.is_none());
    assert_not_started(&app);
}

#[test]
fn a_second_export_does_not_replace_an_audit_or_confirmation() {
    let mut app = app();
    let tx = inject(&mut app, export());
    app.begin_export(PathBuf::from("another-destination"));
    assert!(app.cloud_audit.is_some());
    tx.send(CloudCost { files: 1, bytes: 1 }).unwrap();
    app.poll_cloud_audit();
    app.begin_export(PathBuf::from("another-destination"));
    assert!(app.cloud_copy_plan.is_some());
    assert_not_started(&app);
}

#[test]
fn edit_mode_is_rechecked_after_cloud_confirmation_but_export_stays_explicit() {
    let origin = context();
    assert_eq!(
        copy().authorize(&origin, Some(&origin), EditMode::View, false),
        Err("Switch to Edit mode before changing files")
    );
    assert_eq!(
        copy().authorize(&origin, Some(&origin), EditMode::Edit, false),
        Ok(())
    );
    assert_eq!(
        export().authorize(&origin, Some(&origin), EditMode::View, false),
        Ok(())
    );
    assert_eq!(
        export().authorize(&origin, Some(&origin), EditMode::View, true),
        Err(BUSY)
    );
}

#[test]
fn switching_back_to_view_mode_refuses_an_earlier_copy_confirmation() {
    let mut app = app();
    app.edit_mode = EditMode::Edit;
    let tx = inject(&mut app, copy());
    tx.send(CloudCost { files: 1, bytes: 1 }).unwrap();
    app.poll_cloud_audit();
    app.set_edit_mode(EditMode::View);
    app.respond_cloud_copy(true);
    assert!(app.cloud_copy_plan.is_none());
    assert_not_started(&app);
}

#[test]
fn returning_home_cancels_both_pending_audit_and_confirmation() {
    let mut app = app();
    let tx = inject(&mut app, export());
    app.go_home();
    assert!(app.cloud_audit.is_none());
    assert!(tx.send(CloudCost::default()).is_err());
    assert_not_started(&app);

    app.at_home = false;
    app.root = Some(context().root);
    let tx = inject(&mut app, export());
    tx.send(CloudCost { files: 1, bytes: 1 }).unwrap();
    app.poll_cloud_audit();
    app.go_home();
    assert!(app.cloud_copy_plan.is_none());
    assert_not_started(&app);
}

#[test]
fn leaving_a_tab_drops_an_audit_even_if_the_other_tab_maps_the_same_root() {
    let mut app = app();
    let tx = inject(&mut app, export());
    app.park_active_workspace();
    assert!(app.cloud_audit.is_none());
    assert!(tx.send(CloudCost::default()).is_err());
    assert_not_started(&app);
}
