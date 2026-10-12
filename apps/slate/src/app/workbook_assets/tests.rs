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
