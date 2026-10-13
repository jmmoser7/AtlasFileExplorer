//! Existing selected-object commands used by the context-menu recipes.
use super::*;

impl SlateApp {
    pub(super) fn dispatch_model_display(&mut self, detail: Option<&str>) -> bool {
        if detail == Some("menu") {
            return self.open_object_panel(crate::app::board_properties::Panel::ModelDisplay);
        }
        // "<node>:<mode>" from the strip; bare (palette) cycles the
        // single selected model to its next pass.
        let target = match detail.and_then(|s| s.split_once(':')) {
            Some((node, mode)) => node
                .parse::<u64>()
                .ok()
                .map(slate_doc::NodeId)
                .zip(slate_doc::scene::ModelDisplay::from_key(mode)),
            None => self.selected_model_viewport().and_then(|id| {
                let now = self.model_display_of(id)?;
                let next = match now {
                    slate_doc::scene::ModelDisplay::Shaded => {
                        slate_doc::scene::ModelDisplay::Arctic
                    }
                    slate_doc::scene::ModelDisplay::Arctic => {
                        slate_doc::scene::ModelDisplay::Material
                    }
                    slate_doc::scene::ModelDisplay::Material => {
                        slate_doc::scene::ModelDisplay::Depth
                    }
                    slate_doc::scene::ModelDisplay::Depth => slate_doc::scene::ModelDisplay::Shaded,
                };
                Some((id, next))
            }),
        };
        target.is_some_and(|(id, mode)| self.set_model_display(id, mode))
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
