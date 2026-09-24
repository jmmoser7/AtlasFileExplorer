//! Shared portal helpers: workbook-relative locators and contents focus.
//!
//! Authored fields live on [`slate_doc::PortalNode`]. Host portals (agent,
//! web, File Atlas) keep their own runtimes; this module only owns the one
//! contents-focus slot they share and the locator math they all call.

use super::SlateApp;
use eframe::egui::{self, Pos2};
use slate_doc::scene::{NodeId, NodeKind, PortalKind, SceneCmd, SourceUri};
use std::path::{Path, PathBuf};

/// Shared contents-focus slot for host portals. Derived view-state; never journaled.
#[derive(Debug, Default)]
pub struct PortalRuntime {
    pub contents: Option<NodeId>,
}

pub fn source_locator(workbook: Option<&Path>, repo: &Path) -> String {
    slate_doc::scene::source_locator(workbook, repo)
}

pub fn resolve_source(workbook: Option<&Path>, locator: &str) -> PathBuf {
    slate_doc::scene::resolve_source(workbook, locator)
}

/// One pending-drop window. Folder drops and workbook drops both call this;
/// each caller owns its queue and what a picked index means.
pub struct DropChoice {
    pub label: &'static str,
    pub hint: &'static str,
}

pub enum DropChooserResult {
    Picked(usize),
    Cancelled,
}

pub fn paint_drop_chooser(
    ctx: &egui::Context,
    title: &str,
    prompt: &str,
    options: &[DropChoice],
) -> Option<DropChooserResult> {
    let mut result = None;
    egui::Window::new(title)
        .id(egui::Id::new(("drop-chooser", title)))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(prompt);
            ui.add_space(8.0);
            for (index, opt) in options.iter().enumerate() {
                let btn = ui
                    .add(egui::Button::new(opt.label).min_size(egui::Vec2::new(280.0, 28.0)))
                    .on_hover_text(opt.hint);
                if btn.clicked() {
                    result = Some(DropChooserResult::Picked(index));
                }
            }
            ui.add_space(6.0);
            if ui.button("Cancel").clicked() {
                result = Some(DropChooserResult::Cancelled);
            }
        });
    result
}

impl SlateApp {
    pub(crate) fn bind_portal_source(&mut self, portal: NodeId, path: PathBuf) {
        if self.agent_is_running(portal) {
            self.toast("Stop this response before changing its source.");
            return;
        }
        let workbook = self.tab().path.clone();
        let locator = source_locator(workbook.as_deref(), &path);
        let Some(before) = self.doc().scene.node(portal).cloned() else {
            return;
        };
        let NodeKind::Portal(_) = &before.kind else {
            return;
        };
        let mut after = before.clone();
        let NodeKind::Portal(p) = &mut after.kind else {
            return;
        };
        p.source = Some(SourceUri { locator });
        let rename = match p.kind {
            PortalKind::Web => false,
            PortalKind::Agent => p.title == "Agent portal" || p.title.starts_with("Agent portal"),
            PortalKind::FileAtlas => p.title == "File Atlas" || p.title.starts_with("File Atlas"),
            PortalKind::Slate => p.title == "Board" || p.title.starts_with("Slate board"),
        };
        if rename {
            if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                p.title = name.to_string();
            }
        }
        let _ = self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]);
    }

    pub(crate) fn portal_clear_focus(&mut self) -> bool {
        self.portals.contents.take().is_some()
    }

    pub(crate) fn portal_enter_interactive(&mut self, id: NodeId) {
        let Some(kind) = self.doc().scene.node(id).and_then(|n| match &n.kind {
            NodeKind::Portal(p) => Some(p.kind),
            _ => None,
        }) else {
            return;
        };
        if kind == PortalKind::Slate {
            self.open_slate_portal(id);
            return;
        }
        self.contents_blur();
        self.portals.contents = Some(id);
        match kind {
            PortalKind::Agent => self.agent_enter_contents(id),
            PortalKind::Web => self.web_enter_contents(id),
            PortalKind::FileAtlas => self.atlas_enter_contents(id),
            PortalKind::Slate => {}
        }
        self.board_sel = std::iter::once(id).collect();
    }

    /// One contents-focus slot for every host portal (P1.portal.contents-focus).
    pub(crate) fn contents_focused(&self) -> Option<NodeId> {
        self.portals.contents
    }

    /// Nested editing keeps the frame selected, but the board's selection
    /// cast and outline stay off so the inner canvas is the active surface.
    pub(crate) fn portal_frame_chrome_suppressed(&self, id: NodeId) -> bool {
        self.contents_focused() == Some(id)
    }

    /// Same cast/outline suppression for every surface a double-click enters:
    /// host portals, text (including shape text), a sheet cell, crop, a live
    /// 3D viewport, and a shown Enscape walkthrough. Single-click selection
    /// still paints the cast. A Slate board portal opens a tab instead of
    /// entering the frame, so its cast stays.
    pub(crate) fn frame_chrome_suppressed(&self, id: NodeId) -> bool {
        self.portal_frame_chrome_suppressed(id)
            || self.text_edit.as_ref().is_some_and(|(edit, _)| *edit == id)
            || self.sheet_edit.as_ref().is_some_and(|edit| edit.node == id)
            || self.sheet_open == Some(id)
            || self.board_crop == Some(id)
            || self.model3d.live.contains_key(&id)
            || self.enscape_shown_node() == Some(id)
    }

    /// Dimension stringers belong to the frame selection. They stay hidden
    /// while a selected frame is the nested-edit target.
    pub(crate) fn selection_stringers_suppressed(&self) -> bool {
        self.board_sel
            .iter()
            .any(|id| self.frame_chrome_suppressed(*id))
    }

    pub(crate) fn contents_blur(&mut self) -> bool {
        let a = self.web_leave_contents();
        let b = self.agent_leave_contents();
        let c = self.atlas_leave_contents();
        let d = self.portal_clear_focus();
        a || b || c || d
    }

    /// Primary click outside the focused portal body (and not on its chrome)
    /// peels contents-focus. The click then belongs to the board.
    pub(crate) fn peel_contents_focus_if_clicked_outside(
        &mut self,
        ui: &egui::Ui,
        xf: &super::board::BoardXf,
        pointer: Option<Pos2>,
    ) {
        let Some(id) = self.contents_focused() else {
            return;
        };
        if self.portal_is_maximized(id) {
            return;
        }
        if !ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            return;
        }
        let Some(p) = pointer else {
            return;
        };
        let Some(node) = self.doc().scene.node(id) else {
            self.contents_blur();
            return;
        };
        let NodeKind::Portal(portal) = &node.kind else {
            self.contents_blur();
            return;
        };
        let srect = xf.rect_w2s(node.rect);
        let layout = super::board_portal_chrome::layout_for_portal(
            portal.kind,
            srect,
            self.portal_chrome_collapsed(id),
            self.portal_is_maximized(id),
            xf.z,
        );
        if layout.pointer_on_chrome(p) {
            return;
        }
        if layout.body.contains(p) {
            return;
        }
        self.contents_blur();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_locator_prefers_relative() {
        let wb = Path::new("C:/work/book.slate");
        let repo = Path::new("C:/work/repo");
        assert_eq!(source_locator(Some(wb), repo), "repo");
    }
}
