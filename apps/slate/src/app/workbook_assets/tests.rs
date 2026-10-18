use super::*;

#[test]
fn a_disconnected_old_save_cannot_clear_the_current_save_and_current_failure_is_retryable() {
    let mut h = crate::app::tests::Harness::new("asset_save_disconnect");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let tab = h.app.tab().id;
    h.app.tab_mut().dirty = false;
    h.app.asset_save_generation.insert(tab, 2);
    h.app.asset_saves_pending.insert(tab);
    let (old_tx, old_rx) = crossbeam_channel::bounded(1);
    let (current_tx, current_rx) = crossbeam_channel::bounded(1);
    h.app.asset_save_rx.push(SaveAssetsWatch {
        tab,
        generation: 1,
        rx: old_rx,
    });
    h.app.asset_save_rx.push(SaveAssetsWatch {
        tab,
        generation: 2,
        rx: current_rx,
    });
    drop(old_tx);
    h.app.poll_asset_saves(&h.ctx);
    assert!(h.app.asset_saves_pending.contains(&tab));
    assert!(!h.app.tab().dirty);
    drop(current_tx);
    h.app.poll_asset_saves(&h.ctx);
    assert!(!h.app.asset_saves_pending.contains(&tab));
    assert!(h.app.asset_save_rx.is_empty());
    assert!(
        h.app.tab().dirty,
        "the unsaved-close prompt stays safe after failure"
    );
}

#[test]
fn asset_save_completion_cannot_write_after_the_lease_marker_is_replaced() {
    let mut h = crate::app::tests::Harness::new("asset-save-lost-lease");
    h.seed();
    let destination = h.base.join("source.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, destination.clone());
    let before = std::fs::read(&destination).unwrap();
    let original = h.app.tab().doc.items[0].path.clone();
    let lease = h.app.tab().lease.as_ref().unwrap();
    let mut replacement = lease.info().clone();
    replacement.token = "replacement holder".into();
    let replacement = serde_json::to_vec(&replacement).unwrap();
    std::fs::write(slate_doc::Lease::lock_path(&destination), &replacement).unwrap();
    let generation = h.app.asset_save_generation[&tab_id];
    h.app.finish_asset_save(SaveAssetsJob {
        tab: tab_id,
        generation,
        dest: destination.clone(),
        interim: None,
        outcome: CollectOutcome {
            collected: vec![CollectedAsset {
                from: original,
                to: h.base.join("assets/pasted/copied.png"),
                locator: "assets/pasted/copied.png".into(),
            }],
            refused: Vec::new(),
        },
    });
    assert_eq!(std::fs::read(&destination).unwrap(), before);
    assert!(h.app.tab().dirty);
    assert_eq!(
        h.app.tab().doc.items[0].path,
        PathBuf::from("assets/pasted/copied.png")
    );
    assert_eq!(
        std::fs::read(slate_doc::Lease::lock_path(&destination)).unwrap(),
        replacement
    );
    assert!(!h.app.asset_saves_pending.contains(&tab_id));
}
