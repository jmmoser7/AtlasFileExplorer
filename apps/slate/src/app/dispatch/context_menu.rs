//! Existing selected-object commands used by the context-menu recipes.
use super::*;
use std::path::PathBuf;

impl SlateApp {
    /// Local locator behind the displayed media; never probes a remote source.
    pub(crate) fn board_node_local_path(&self, id: NodeId) -> Option<PathBuf> {
        self.image_item_path(id).or_else(|| {
            let NodeKind::Portal(portal) = &self.doc().scene.node(id)?.kind else {
                return None;
            };
            let source = portal.source.as_ref()?;
            (!source.locator.contains("://")).then(|| {
                crate::app::board_portal::resolve_source(
                    self.tab().path.as_deref(),
                    &source.locator,
                )
            })
        })
    }

    pub(super) fn open_selected_file_location(&mut self) -> bool {
        let Some(path) = self
            .board_sel
            .iter()
            .find_map(|id| self.board_node_local_path(*id))
        else {
            return false;
        };
        #[cfg(windows)]
        let result = std::process::Command::new("explorer.exe")
            .arg("/select,")
            .arg(&path)
            .spawn();
        #[cfg(not(windows))]
        let result = std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(&path))
            .spawn();
        if let Err(error) = result {
            self.toast(format!("Couldn't open the file location: {error}"));
            return false;
        }
        true
    }

    pub(super) fn dispatch_model_display(&mut self, detail: Option<&str>) -> bool {
        if detail == Some("menu") {
            return self.open_object_panel(crate::app::board_properties::Panel::ModelDisplay);
        }
        // "<node>:<mode>" from the strip; bare (palette) cycles the
        // single selected model to its next pass.
        let target = match detail {
            Some(detail) => detail.split_once(':').and_then(|(node, mode)| {
                node.parse::<u64>()
                    .ok()
                    .map(slate_doc::NodeId)
                    .zip(slate_doc::scene::ModelDisplay::from_key(mode))
            }),
            None => self.selected_model_viewport().and_then(|id| {
                let now = self.model_display_of(id)?;
                Some((id, now.next()))
            }),
        };
        target.is_some_and(|(id, mode)| self.set_model_display(id, mode))
    }

    pub(super) fn dispatch_model_materials(&mut self, detail: Option<&str>) -> bool {
        let target = match detail {
            Some(detail) => detail.split_once(':').and_then(|(node, state)| {
                let enabled = match state {
                    "on" | "true" => true,
                    "off" | "false" => false,
                    _ => return None,
                };
                node.parse::<u64>()
                    .ok()
                    .map(|id| (slate_doc::NodeId(id), enabled))
            }),
            None => self
                .selected_model_viewport()
                .and_then(|id| self.model_materials_of(id).map(|enabled| (id, !enabled))),
        };
        target.is_some_and(|(id, enabled)| self.set_model_materials(id, enabled))
    }

    pub(super) fn dispatch_model_measure(&mut self, detail: Option<&str>) -> bool {
        let node = match detail {
            Some(s) => s.parse::<u64>().ok().map(slate_doc::NodeId),
            None => self.selected_model_viewport(),
        };
        node.is_some_and(|id| self.toggle_model_measure(id))
    }

    pub(super) fn edit_selected_sheet(&mut self) -> bool {
        let Some(id) = self
            .board_sel
            .iter()
            .copied()
            .find(|id| self.sheet_node(*id))
        else {
            return false;
        };
        self.enter_sheet(id);
        self.sheet_open == Some(id)
    }

    pub(super) fn open_selected_image_file(&mut self) -> bool {
        let Some(path) = self.board_sel.iter().find_map(|id| {
            let node = self.doc().scene.node(*id)?;
            if let NodeKind::Image(img) = &node.kind {
                self.doc().item(img.item).map(|item| item.path.clone())
            } else {
                None
            }
        }) else {
            return false;
        };
        self.open_item_path(&path);
        true
    }

    pub(super) fn pick_image_layer_command(&mut self, detail: Option<&str>) -> bool {
        let image = detail
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| v.get("image").and_then(|v| v.as_u64()))
            .map(NodeId)
            .or_else(|| {
                self.board_sel
                    .iter()
                    .copied()
                    .find(|id| SlateApp::supports_image_paint(*id, self))
            });
        image.is_some_and(|id| self.pick_image_layer_for(id))
    }
}
