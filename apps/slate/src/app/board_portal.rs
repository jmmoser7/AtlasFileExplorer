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
    if let Some(wb) = workbook.and_then(|p| p.parent()) {
        if let Ok(rel) = repo.strip_prefix(wb) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    repo.to_string_lossy().into_owned()
}

pub fn resolve_source(workbook: Option<&Path>, locator: &str) -> PathBuf {
    let p = PathBuf::from(locator);
    if p.is_absolute() {
        return p;
    }
    if let Some(wb) = workbook.and_then(|p| p.parent()) {
        return wb.join(p);
    }
    p
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
        self.contents_blur();
        self.portals.contents = Some(id);
        match kind {
            PortalKind::Agent => self.agent_enter_contents(id),
            PortalKind::Web => self.web_enter_contents(id),
            PortalKind::FileAtlas => self.atlas_enter_contents(id),
        }
        self.board_sel = std::iter::once(id).collect();
    }

    /// One contents-focus slot for every host portal (P1.portal.contents-focus).
    pub(crate) fn contents_focused(&self) -> Option<NodeId> {
        self.portals.contents
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
