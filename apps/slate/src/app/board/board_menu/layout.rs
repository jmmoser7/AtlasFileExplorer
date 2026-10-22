//! Shipped menu recipes, frozen from the user's studio checkpoint.
use super::*;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(serde::Deserialize)]
pub(crate) struct Layout {
    pub metrics: menu::Metrics,
    menus: Vec<Recipe>,
    actions: BTreeMap<String, Action>,
}

#[derive(serde::Deserialize)]
struct Recipe {
    id: String,
    entries: Vec<String>,
}

#[derive(serde::Deserialize)]
pub(super) struct Action {
    pub label: String,
    pub icon: String,
    #[serde(default)]
    pub shortcut: String,
    #[serde(default)]
    pub danger: bool,
}

pub(crate) fn layout() -> &'static Layout {
    static LAYOUT: OnceLock<Layout> = OnceLock::new();
    LAYOUT.get_or_init(|| {
        serde_json::from_str(include_str!("../../../../assets/context-menus.json"))
            .expect("validated, shipped context-menu recipes")
    })
}

pub(crate) fn entries(context: &str) -> impl Iterator<Item = &'static str> {
    layout()
        .menus
        .iter()
        .find(|m| m.id == context)
        .into_iter()
        .flat_map(|m| m.entries.iter().map(String::as_str))
}

pub(super) fn action(id: &str) -> Option<&'static Action> {
    layout().actions.get(id)
}

impl SlateApp {
    pub(super) fn board_menu_context(&self, id: NodeId, targets: &[NodeId]) -> &'static str {
        if targets.iter().any(|id| {
            self.doc()
                .scene
                .node(*id)
                .is_some_and(|n| n.group.is_some())
        }) {
            return "grouped";
        }
        if targets.len() > 1 {
            return "multi";
        }
        let Some(node) = self.doc().scene.node(id) else {
            return "canvas";
        };
        match &node.kind {
            NodeKind::Image(img) => {
                if img
                    .agent
                    .as_ref()
                    .is_some_and(|agent| agent.view == atlas_ai::agent::PortalView::Text)
                {
                    return "text-file";
                }
                let Some(path) = self.image_item_path(id) else {
                    return "image";
                };
                if slate_doc::media::is_table_source(&path) {
                    return "spreadsheet";
                }
                if slate_doc::media::has_pages(&path) {
                    return "pdf";
                }
                match slate_doc::media_kind(&path) {
                    slate_doc::MediaKind::Image => {
                        if self.image_selection_clip(id).is_some() {
                            "image-selection"
                        } else {
                            "image"
                        }
                    }
                    slate_doc::MediaKind::Model => "model",
                    slate_doc::MediaKind::Video => "video",
                    slate_doc::MediaKind::Pdf => "pdf",
                    slate_doc::MediaKind::Text if !self.croppable_image(id) => "text-file",
                    _ => "document",
                }
            }
            NodeKind::Shape(s) => match s.shape {
                ShapeKind::Rect => "rect",
                ShapeKind::Ellipse => "ellipse",
                ShapeKind::RegularPolygon => "polygon",
                ShapeKind::Line => "line",
                ShapeKind::Path => "path",
            },
            NodeKind::Text(_) => "text",
            NodeKind::Frame(_) => "frame",
            NodeKind::DockStrip(_) => "dock",
            NodeKind::Connector(_) => "wire",
            NodeKind::Portal(p) => match (p.kind, p.source.is_some()) {
                (PortalKind::Web, _) => "web",
                (PortalKind::FileAtlas, true) => "atlas",
                (PortalKind::FileAtlas, false) => "atlas-empty",
                (PortalKind::Slate, true) => "slate",
                (PortalKind::Slate, false) => "slate-empty",
                (PortalKind::Agent, true) => "agent",
                (PortalKind::Agent, false) => "agent-empty",
            },
        }
    }

    pub(super) fn board_menu_entry_available(
        &self,
        entry: &str,
        id: NodeId,
        targets: &[NodeId],
    ) -> bool {
        let Some(node) = self.doc().scene.node(id) else {
            return false;
        };
        match entry {
            "board.image.save_as" => self.board_image_save_available(id),
            "board.open_location" => self.board_node_local_path(id).is_some(),
            "board.shape.edit" => self.object_properties_available(targets),
            "board.crop" => self.croppable_image(id),
            "menu.layers" => Self::supports_image_paint(id, self),
            "board.open_file" => {
                matches!(&node.kind, NodeKind::Image(img) if self.doc().item(img.item).is_some())
            }
            "menu.tags" | "menu.tag_rows" | "heading:tags" => {
                !self.menu_image_items(targets).is_empty()
            }
            "board.media.unbundle" => targets.iter().any(|id| self.node_has_pages(*id)),
            "board.group" => targets.len() >= 2,
            "board.ungroup" => targets.iter().any(|id| {
                self.doc()
                    .scene
                    .node(*id)
                    .is_some_and(|n| n.group.is_some())
            }),
            "board.sheet.edit" => self.sheet_node(id),
            "board.model_display" | "board.model_screenshot_canvas" => self.model_has_viewport(id),
            "portal.chrome.toggle" => {
                matches!(&node.kind, NodeKind::Portal(p) if super::super::super::board_portal_chrome::uses_identity_tab(p.kind))
            }
            "portal.agent.fork" => self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| {
                    a.view == atlas_ai::agent::PortalView::Chat
                        && !a.chat.draft
                        && a.chat.bundled.is_empty()
                        && !self.agent_is_running(id)
                }),
            "portal.agent.fit" => slate_doc::agent_chat::agent(node).is_some(),
            "board.image.selection.layer" | "menu.clear_selection" => {
                self.image_selection_clip(id).is_some()
            }
            _ => true,
        }
    }

    pub(super) fn menu_image_items(&self, targets: &[NodeId]) -> Vec<ItemId> {
        targets
            .iter()
            .filter_map(|id| match &self.doc().scene.node(*id)?.kind {
                NodeKind::Image(img) if self.doc().item(img.item).is_some() => Some(img.item),
                _ => None,
            })
            .collect()
    }
}
