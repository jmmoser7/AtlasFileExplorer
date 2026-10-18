//! Workbook tab transitions and unsaved-close lifecycle.

use super::{SlateApp, SlateTab, UnsavedClose};
use eframe::egui;

#[cfg(test)]
mod tests;

impl SlateApp {
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
            lease.release();
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
