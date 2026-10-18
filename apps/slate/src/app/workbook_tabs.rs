//! Workbook tab transitions and unsaved-close lifecycle.

use super::{Lease, LeaseInfo, LeaseState, SlateApp, SlateDoc, SlateTab, UnsavedClose};
use eframe::egui;
use std::path::PathBuf;

#[cfg(test)]
mod tests;

impl SlateApp {
    pub(super) fn open_doc_at(&mut self, path: PathBuf) {
        self.pending_lease_releases
            .retain(|(_, release)| !release.is_complete());
        if self
            .pending_lease_releases
            .iter()
            .any(|(closed, _)| closed == &path)
        {
            if !self.pending_workbooks.contains(&path) {
                self.pending_workbooks.push(path);
            }
            return;
        }
        // Same workbook already open (compare canonical paths): focus that
        // tab instead of opening a second copy that would race it on save.
        // This is also the "load a workbook into itself" guard — re-opening
        // the active workbook is a no-op with a toast.
        let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
        let target = canon(&path);
        if let Some((closed, _)) = self
            .pending_lease_releases
            .iter()
            .find(|(closed, _)| atlas_core::workbook_assets::same_path(&target, &canon(closed)))
        {
            // Retry with the stored key: subsequent frames need no filesystem probe.
            if !self.pending_workbooks.contains(closed) {
                self.pending_workbooks.push(closed.clone());
            }
            return;
        }
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.path.as_deref().map(canon) == Some(target.clone()))
        {
            self.switch_tab(i);
            self.leave_home();
            self.toast("Workbook is already open — switched to its tab");
            return;
        }
        match SlateDoc::load_from(&path) {
            Ok(doc) => {
                if !self.tabs.is_empty() && !self.finish_text_compose_on_click_away() {
                    return;
                }
                if self.tabs.is_empty() {
                    self.ensure_work_tab();
                } else if !self.tab().is_blank() {
                    self.new_tab();
                }
                // A reused blank tab keeps its id; its old board's runtime must not.
                self.release_agent_doc(self.active_tab);
                let tab_id = self.tab().id;
                self.recovery_forget_tab(tab_id);
                self.brush_chain.forget_tab(tab_id);
                self.pen_chain.forget_tab(tab_id);
                if self.eraser_anchor.is_some_and(|(t, _)| t == tab_id) {
                    self.eraser_anchor = None;
                }
                self.record_recent_workbook(&path, &doc);
                let (lease, read_only, holder, held_toast) = match Lease::acquire(&path) {
                    Ok(LeaseState::Acquired(lease)) => (Some(lease), false, None, None),
                    Ok(LeaseState::Held(info)) => {
                        let msg = format!(
                            "Opened read-only — {} has this workbook open",
                            info.describe()
                        );
                        (None, true, Some(info), Some(msg))
                    }
                    Err(e) => {
                        let msg = format!("Opened read-only — could not take write lease ({e})");
                        (None, true, Some(LeaseInfo::unknown()), Some(msg))
                    }
                };
                {
                    let tab = self.tab_mut();
                    // Replacing a blank tab: drop any prior lease (untitled has none).
                    if let Some(old) = tab.lease.take() {
                        old.release();
                    }
                    tab.doc = doc;
                    tab.recovered = false;
                    tab.link_health_revision = None;
                    tab.path = Some(path);
                    tab.dirty = false;
                    tab.lease = lease;
                    tab.read_only = read_only;
                    tab.lease_holder = holder;
                    let view = tab.doc.view.clone();
                    tab.cam.offset = egui::Vec2::new(view.cam_x, view.cam_y);
                    tab.cam.z = atlas_core::display::SLATE_CANVAS.clamp(view.zoom);
                }
                self.load_create_style_from_doc();
                self.selection.clear();
                self.note_scene_change();
                self.leave_home();
                self.publish_session_tags();
                if let Some(msg) = held_toast {
                    self.toast(msg);
                }
            }
            Err(e) => self.toast(format!("Could not open workbook: {e}")),
        }
    }

    /// Open queued `.slate` files as tabs (deduped inside `open_doc_at`).
    /// Runs after drop placement so item placement targets the tab the drop
    /// landed on, not a tab a workbook drop just switched to.
    pub(super) fn drain_pending_workbooks(&mut self, ctx: &egui::Context) {
        self.pending_lease_releases
            .retain(|(_, release)| !release.is_complete());
        for path in std::mem::take(&mut self.pending_workbooks) {
            self.open_doc_at(path);
        }
        if !self.pending_workbooks.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    pub(crate) fn tab_has_unsaved_changes(&self, index: usize) -> bool {
        self.tabs.get(index).is_some_and(|tab| {
            tab.dirty || (index == self.active_tab && self.recovery_has_text_drafts())
        })
    }
    pub(crate) fn first_unsaved_tab(&self) -> Option<usize> {
        (0..self.tabs.len()).find(|&i| self.tab_has_unsaved_changes(i))
    }

    pub(super) fn save_tab_for_close(&mut self, i: usize) -> bool {
        if i >= self.tabs.len() {
            return true;
        }
        let tab_id = self.tabs[i].id;
        if !self.tab_has_unsaved_changes(i) {
            return !self.asset_saves_pending.contains(&tab_id)
                && !self.deferred_workbook_actions.has_tab(tab_id)
                && !self.layer_bakes.pending_for(tab_id);
        }
        if self.tabs[i].read_only {
            let name = self.tabs[i].doc.name.clone();
            self.save_doc_as_dialog_for(tab_id, name);
            return false;
        }
        if let Some(path) = self.tabs[i].path.clone() {
            self.save_doc_to(tab_id, path);
        } else {
            let name = self.tabs[i].doc.name.clone();
            self.save_doc_as_dialog_for(tab_id, name);
            return false;
        }
        !self.tab_has_unsaved_changes(i)
            && !self.asset_saves_pending.contains(&tab_id)
            && !self.deferred_workbook_actions.has_tab(tab_id)
            && !self.layer_bakes.pending_for(tab_id)
    }

    pub(super) fn unsaved_close_tab_id(&self) -> Option<u64> {
        match self.unsaved_close {
            Some(super::UnsavedClose::Tab(index)) => self.tabs.get(index).map(|tab| tab.id),
            _ => None,
        }
    }

    pub(super) fn tab_ready_to_close(&self, index: usize) -> bool {
        !self.tab_has_unsaved_changes(index)
            && self.tabs.get(index).is_some_and(|tab| {
                !tab.dirty
                    && !self.asset_saves_pending.contains(&tab.id)
                    && !self.deferred_workbook_actions.has_tab(tab.id)
                    && !self.layer_bakes.pending_for(tab.id)
            })
    }

    // ----- tabs -------------------------------------------------------------

    pub fn new_tab(&mut self) {
        if !self.finish_text_compose_on_click_away() {
            return;
        }
        self.lock_all_models();
        self.sync_agent_doc();
        let mut tab = SlateTab::empty();
        tab.chrome = self.chrome().clone();
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.selection.clear();
        self.sync_agent_doc();
    }

    pub fn switch_tab(&mut self, i: usize) {
        self.finish_bumper_glide();
        if i < self.tabs.len() {
            if i != self.active_tab {
                if !self.finish_text_compose_on_click_away() {
                    return;
                }
                // Live 3D viewports are keyed by node id, which is per-document:
                // freeze them before another doc's ids can collide.
                self.lock_all_models();
                self.sync_agent_doc();
                self.flush_create_style_to_doc();
                self.active_tab = i;
                self.load_create_style_from_doc();
                self.sync_agent_doc();
                self.selection.clear();
                self.note_scene_change();
                self.publish_session_tags();
            }
            // Leaving the Cover Flow home for a workbook tab.
            self.leave_home();
        }
    }

    pub fn close_tab(&mut self, i: usize) {
        self.finish_bumper_glide();
        if i >= self.tabs.len() {
            return;
        }
        // Freezing can dirty an otherwise saved workbook. Check after it
        // journals the camera so closing cannot discard the latest view.
        if i == self.active_tab {
            self.finish_text_compose_on_click_away();
            self.lock_all_models();
        }
        if self.tab_has_unsaved_changes(i) {
            self.unsaved_close = Some(UnsavedClose::Tab(i));
            return;
        }
        self.force_close_tab(i);
    }

    pub(super) fn force_close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if i == self.active_tab && self.recovery_has_text_drafts() {
            self.unsaved_close = Some(UnsavedClose::Tab(i));
            return;
        }
        let tab_id = self.tabs[i].id;
        if self.layer_bakes.pending_for(tab_id) {
            self.deferred_workbook_actions.close_discard(tab_id);
            return;
        }
        self.deferred_workbook_actions.clear_tab(tab_id);
        self.release_agent_doc(i);
        if let Some(lease) = self.tabs[i].lease.take() {
            let path = self.tabs[i]
                .path
                .clone()
                .expect("leased workbook has a path");
            self.pending_lease_releases.push((path, lease.release()));
        }
        let closed = self.tabs.remove(i);
        self.recovery_forget_tab(closed.id);
        self.erase_settle
            .forget_tab(closed.id, &mut self.brush_tiles);
        self.brush_tiles.forget_tab(closed.id);
        self.brush_stamps.retain(|_, (_, g)| g.tab != closed.id);
        if self.tabs.is_empty() {
            self.at_home = true;
            self.active_tab = 0;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        } else if self.active_tab > i {
            self.active_tab -= 1;
        }
        self.selection.clear();
        self.publish_session_tags();
    }

    pub(super) fn discard_dirty_tabs(&mut self) {
        self.cancel_text_compose();
        for index in (0..self.tabs.len()).rev() {
            let tab_id = self.tabs[index].id;
            self.tabs[index].dirty = false;
            self.deferred_workbook_actions.cancel_io(tab_id);
            self.force_close_tab(index);
        }
    }

    pub(super) fn resume_unsaved_close_if_ready(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.unsaved_close else {
            return;
        };
        match kind {
            UnsavedClose::Tab(i) if self.tab_ready_to_close(i) => {
                self.continue_unsaved_close(ctx);
            }
            UnsavedClose::Exit
                if self.first_unsaved_tab().is_none()
                    && self.asset_saves_pending.is_empty()
                    && self.deferred_workbook_actions.is_empty()
                    && !self.layer_bakes.is_pending()
                    && self.export_rx.is_none()
                    && self.collect_rx.is_none() =>
            {
                self.continue_unsaved_close(ctx);
            }
            _ => {}
        }
    }

    pub(super) fn continue_unsaved_close(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.unsaved_close else {
            return;
        };
        match kind {
            UnsavedClose::Tab(i) => {
                if self.tab_ready_to_close(i) {
                    self.unsaved_close = None;
                    self.force_close_tab(i);
                }
            }
            UnsavedClose::Exit => {
                if let Some(i) = self.first_unsaved_tab() {
                    if !self.save_tab_for_close(i) {
                        return;
                    }
                    self.continue_unsaved_close(ctx);
                } else if self.asset_saves_pending.is_empty()
                    && self.deferred_workbook_actions.is_empty()
                    && !self.layer_bakes.is_pending()
                    && self.export_rx.is_none()
                    && self.collect_rx.is_none()
                {
                    self.unsaved_close = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    pub(crate) fn apply_unsaved_choice(
        &mut self,
        ctx: &egui::Context,
        choice: atlas_shell::widgets::ConfirmChoice,
    ) {
        use atlas_shell::widgets::ConfirmChoice;
        let Some(kind) = self.unsaved_close else {
            return;
        };
        match choice {
            ConfirmChoice::Cancel => self.unsaved_close = None,
            ConfirmChoice::Secondary => match kind {
                UnsavedClose::Tab(i) => {
                    self.unsaved_close = None;
                    if i < self.tabs.len() {
                        if i == self.active_tab {
                            self.cancel_text_compose();
                        }
                        let tab_id = self.tabs[i].id;
                        self.tabs[i].dirty = false;
                        self.deferred_workbook_actions.cancel_io(tab_id);
                        self.force_close_tab(i);
                    }
                }
                UnsavedClose::Exit => {
                    self.discard_dirty_tabs();
                    self.resume_unsaved_close_if_ready(ctx);
                }
            },
            ConfirmChoice::Primary => match kind {
                UnsavedClose::Tab(i) => {
                    if self.save_tab_for_close(i) {
                        self.continue_unsaved_close(ctx);
                    }
                }
                UnsavedClose::Exit => self.continue_unsaved_close(ctx),
            },
        }
    }

    pub(super) fn unsaved_prompt_frame(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.unsaved_close else {
            return;
        };
        let i = match kind {
            UnsavedClose::Tab(i) => i,
            UnsavedClose::Exit => match self.first_unsaved_tab() {
                Some(i) => i,
                None => {
                    self.resume_unsaved_close_if_ready(ctx);
                    return;
                }
            },
        };
        if i >= self.tabs.len() {
            self.unsaved_close = None;
            return;
        }
        let name = self.tabs[i].doc.name.clone();
        let many = matches!(kind, UnsavedClose::Exit)
            && (0..self.tabs.len())
                .filter(|&i| self.tab_has_unsaved_changes(i))
                .count()
                > 1;
        let body = if many {
            format!(
                "“{name}” and other workbooks have unsaved changes. Save this one, discard all, or cancel."
            )
        } else {
            format!("“{name}” has unsaved changes. Save it, or close without saving.")
        };
        if let Some(choice) = atlas_shell::widgets::confirm_window(
            ctx,
            "Unsaved changes",
            &body,
            "Save",
            "Don't save",
        ) {
            self.apply_unsaved_choice(ctx, choice);
        }
    }

    /// Drop every tab's write lease (app exit). Best-effort; `Lease::Drop` is
    /// the backstop if this is skipped.
    pub(super) fn release_all_leases(&mut self) {
        for tab in &mut self.tabs {
            if let Some(lease) = tab.lease.take() {
                lease.release();
            }
        }
    }

    /// Recovery is a new editable copy; the source workbook keeps its lease
    /// and bytes. Live generation requires a fresh, explicit human action.
    pub(super) fn install_recovered_doc(&mut self, mut doc: super::SlateDoc) {
        if !self.finish_text_compose_on_click_away() {
            return;
        }
        let commands = doc
            .scene
            .nodes
            .iter()
            .filter_map(|before| {
                let mut after = before.clone();
                let agent = slate_doc::agent_chat::agent_mut(&mut after)?;
                if !agent.live {
                    return None;
                }
                agent.live = false;
                Some(slate_doc::SceneCmd::Patch {
                    before: Box::new(before.clone()),
                    after: Box::new(after),
                })
            })
            .collect::<Vec<_>>();
        let mut journal = slate_doc::SceneJournal::default();
        if !commands.is_empty() {
            journal.commit_as(
                &mut doc.scene,
                commands,
                slate_doc::scene::CmdAuthor::Agent("Recovery".into()),
            );
        }
        doc.name.push_str(" (recovered)");
        self.new_tab();
        let tab = self.tab_mut();
        tab.cam.offset = egui::vec2(doc.view.cam_x, doc.view.cam_y);
        tab.cam.z = atlas_core::display::SLATE_CANVAS.clamp(doc.view.zoom);
        tab.doc = doc;
        // The recovered document is a load baseline. Undo must not undo the
        // safety pause and silently restart a previously live generator.
        tab.journal = slate_doc::SceneJournal::default();
        tab.path = None;
        tab.dirty = true;
        tab.recovered = true;
        self.board_sel.clear();
        self.load_create_style_from_doc();
        self.note_scene_change();
        self.leave_home();
        self.publish_session_tags();
        self.toast("Recovered a copy. Use Save as to keep it. Live agents are paused.");
    }
}
