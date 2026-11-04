//! One result selection and action controller for the strip and generator history.
use super::*;
use atlas_shell::result_actions::{ProductResultAction, ResultInteraction, ResultSelection};
use slate_doc::image_paint::ImageResultId;
use slate_doc::scene::{ImageNode, Node};
use std::path::PathBuf;

mod export;
mod mutations;
mod outputs;
mod save;
mod strips;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ResultAddress {
    tab: u64,
    host: NodeId,
    binding: String,
    outputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    references: Vec<slate_doc::scene::HiddenOutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    witness: Option<Box<Node>>,
}

struct ResultMenu {
    address: ResultAddress,
    at: Pos2,
    opener: egui::LayerId,
}

#[derive(Default)]
pub(super) struct ResultController {
    selections: HashMap<(u64, NodeId, String), ResultSelection>,
    // Exact painted corridors, in board units relative to the host's bottom center.
    strips: HashMap<(u64, NodeId), Rect>,
    menu: Option<ResultMenu>,
    popup_rect: Option<Rect>,
    save: save::SaveController,
}

impl SlateApp {
    pub(crate) fn image_action_command(command: &str) -> bool {
        command == "board.image.save_as" || command.starts_with("portal.agent.result.")
    }

    pub(crate) fn result_popup_rect(&self) -> Option<Rect> {
        self.agents.flow.results.popup_rect
    }
    pub(crate) fn select_generator_result(
        &mut self,
        host: NodeId,
        ordered: &[String],
        index: usize,
        shift: bool,
    ) {
        let Some(binding) = self.result_binding(host) else {
            return;
        };
        let tab = self.tab().id;
        self.agents
            .flow
            .results
            .selections
            .entry((tab, host, binding))
            .or_default()
            .select(ordered, index, shift, false);
    }

    pub(crate) fn handle_result_interaction(
        &mut self,
        ui: &egui::Ui,
        host: NodeId,
        out: ResultInteraction,
        palette: Rect,
        square: bool,
    ) {
        if out.menu.is_none() && out.clicked.is_none() && out.drag_started.is_none() {
            return;
        }
        self.open_result_context(ui, host, out);
        if self.is_artifact_node(host) {
            if let Some(index) = out.clicked {
                let revisions = self
                    .result_outputs(host)
                    .into_iter()
                    .map(|output| output.id)
                    .collect::<Vec<_>>();
                self.select_generator_result(host, &revisions, index, out.shift);
                if let Some(revision) = revisions.get(index) {
                    self.dispatch(
                        ui.ctx(),
                        atlas_commands::CommandId("portal.artifact.revision"),
                        Some(serde_json::json!({"node":host,"revision":revision}).to_string()),
                    );
                }
                if square {
                    self.select_square_result(host);
                }
            }
            return;
        }
        let images = self.agent_images(host);
        if let Some(index) = out.clicked {
            let ordered = images
                .iter()
                .map(|image| image.id.clone())
                .collect::<Vec<_>>();
            self.select_generator_result(host, &ordered, index, out.shift);
            self.pick_agent_result(host, index);
            if square {
                self.select_square_result(host);
            }
        }
        if let Some(image) = out.drag_started.and_then(|index| images.get(index)) {
            self.agents.flow.results.menu = None;
            self.agents.flow.image_result_drag = Some(image_editor::ImageResultDrag::new(
                host,
                image.id.clone(),
                palette,
                self.tab().id,
            ));
        }
        if out.menu.is_some() {
            ui.ctx().request_repaint();
        }
    }

    fn result_address_valid(&self, address: &ResultAddress) -> bool {
        if self.tab().id != address.tab
            || self.result_binding(address.host).as_ref() != Some(&address.binding)
            || address.outputs.is_empty()
        {
            return false;
        }
        if address
            .witness
            .as_deref()
            .is_some_and(|node| self.doc().scene.node(address.host) != Some(node))
        {
            return false;
        }
        if !address.references.is_empty() {
            let available = self.result_outputs(address.host);
            return address.references.len() == address.outputs.len()
                && address
                    .outputs
                    .iter()
                    .zip(&address.references)
                    .all(|(id, reference)| {
                        available
                            .iter()
                            .any(|output| &output.id == id && &output.reference == reference)
                    });
        }
        let images = self.agent_images(address.host);
        let available: std::collections::HashSet<&str> =
            images.iter().map(|image| image.id.as_str()).collect();
        address
            .outputs
            .iter()
            .all(|id| available.contains(id.as_str()))
    }

    /// Runs before board gestures: result chrome never clears the host selection.
    pub(crate) fn result_controls_input(&mut self, ui: &mut egui::Ui, xf: &BoardXf) -> bool {
        self.agents.flow.results.popup_rect = None;
        let saving = self.result_save_input(ui.ctx());
        if saving {
            // The naming modal and its backdrop own every pointer press. Keep
            // the attached editor's raw click-away path inside that ownership.
            self.agents.flow.results.popup_rect = Some(ui.ctx().screen_rect());
            return true;
        }
        let pointer = ui.ctx().pointer_latest_pos();
        let tab = self.tab().id;
        let strip_capture = self
            .agents
            .flow
            .results
            .strips
            .iter()
            .any(|(&(owner, host), rect)| {
                owner == tab
                    && rect.is_positive()
                    && !(self.shape_properties.ids == [host]
                        && self.shape_properties.panel.is_some())
                    && self.doc().scene.node(host).is_some_and(|node| {
                        // Transform retained paint geometry; never rebuild history on input.
                        self.result_strip_hovered(ui.ctx(), host, xf.rect_w2s(node.rect), xf.z)
                    })
            });
        let Some(menu) = self.agents.flow.results.menu.take() else {
            return saving || strip_capture;
        };
        // Deep identity validation belongs to dispatch, not the frame loop.
        if self.tab().id != menu.address.tab
            || !self
                .doc()
                .scene
                .node(menu.address.host)
                .is_some_and(|node| slate_doc::agent_chat::agent(node).is_some())
        {
            return saving || strip_capture;
        }
        let popup_id = Id::new(("image-result-menu", menu.address.tab, menu.address.host.0));
        let mut open = true;
        let reason = self.remove_result_reason(&menu.address);
        let mut child = ui.new_child(egui::UiBuilder::new().layer_id(menu.opener));
        let response = atlas_shell::result_actions::product_menu(
            &mut child,
            popup_id,
            menu.at,
            &mut open,
            reason,
            self.palette(),
        );
        self.agents.flow.results.popup_rect = response.popup;
        let over = pointer.is_some_and(|p| response.popup.is_some_and(|rect| rect.contains(p)));
        let action = response.action;
        let dismiss = !open;
        if let Some(action) = action {
            if action == ProductResultAction::SaveAs {
                self.agents.flow.results.save.opener = Some(menu.opener);
            }
            let command = match action {
                ProductResultAction::Use => "portal.agent.result.use",
                ProductResultAction::SaveAs => "portal.agent.result.save_as",
                ProductResultAction::DuplicateActive => "portal.agent.result.duplicate",
                ProductResultAction::Remove => "portal.agent.result.remove",
            };
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId(command),
                serde_json::to_string(&menu.address).ok(),
            );
        } else if !dismiss {
            self.agents.flow.results.menu = Some(menu);
        }
        saving || strip_capture || over || response.dismissed_outside
    }

    pub(crate) fn dispatch_image_result_action(
        &mut self,
        ctx: &egui::Context,
        command: &str,
        detail: Option<&str>,
    ) -> bool {
        if command == "board.image.save_as" {
            return self.dispatch_board_image_save(ctx, detail);
        }
        let address = detail
            .and_then(|s| serde_json::from_str::<ResultAddress>(s).ok())
            .or_else(|| {
                let host = *self.board_sel.iter().next()?;
                let available = self.result_outputs(host);
                let ordered = available
                    .iter()
                    .map(|output| output.id.clone())
                    .collect::<Vec<_>>();
                let active = self.active_result_reference(host);
                let target = available
                    .iter()
                    .find(|output| Some(&output.reference) == active.as_ref())
                    .or_else(|| available.last())?
                    .id
                    .clone();
                let mut outputs = self
                    .result_selection(host)
                    .identities(&ordered)
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if outputs.is_empty() {
                    outputs.push(target.clone());
                }
                let references = available
                    .into_iter()
                    .filter(|output| outputs.contains(&output.id))
                    .map(|output| output.reference)
                    .collect();
                Some(ResultAddress {
                    references,
                    target: Some(target),
                    witness: self.doc().scene.node(host).cloned().map(Box::new),
                    tab: self.tab().id,
                    host,
                    binding: self.result_binding(host)?,
                    outputs,
                })
            });
        let Some(address) = address.filter(|a| self.result_address_valid(a)) else {
            return false;
        };
        match command {
            "portal.agent.result.use" => self.apply_result_output_action(address, false),
            "portal.agent.result.remove" => self.apply_result_output_action(address, true),
            "portal.agent.result.duplicate" => self.duplicate_active_result(&address),
            "portal.agent.result.copy" => {
                let (doc, payload) = self.result_payload(&address);
                self.copy_image_result_payload(ctx, &payload, &doc) > 0
            }
            "portal.agent.result.save_as" => self.begin_result_save(ctx, address),
            "portal.agent.result.copy_to_canvas" => {
                let Some(rect) = self.doc().scene.node(address.host).map(|n| n.rect) else {
                    return false;
                };
                let placements: Vec<_> = address
                    .outputs
                    .iter()
                    .enumerate()
                    .map(|(index, output)| {
                        (
                            output.clone(),
                            Pos2::new(
                                rect.x + rect.w + 24.0 + index as f32 * (rect.w + 24.0),
                                rect.y,
                            ),
                        )
                    })
                    .collect();
                !self
                    .copy_agent_results_to_canvas(address.host, &placements)
                    .is_empty()
            }
            _ => false,
        }
    }

    /// Snapshots retain each selected output's own layers without changing the pick.
    fn result_payload(&mut self, address: &ResultAddress) -> (slate_doc::SlateDoc, Vec<Node>) {
        if address.references.first().is_some_and(|reference| {
            !matches!(reference, slate_doc::scene::HiddenOutputRef::Image { .. })
        }) {
            return self.non_image_result_payload(address);
        }
        let Some(host) = self.doc().scene.node(address.host).cloned() else {
            return (slate_doc::SlateDoc::new("results"), Vec::new());
        };
        let images = self.agent_images(address.host);
        let payload = address
            .outputs
            .iter()
            .enumerate()
            .filter_map(|(index, output)| {
                let image = images.iter().find(|image| &image.id == output)?;
                let path =
                    slate_doc::scene::resolve_source(self.tab().path.as_deref(), &image.source);
                let item = self.item_for_ready_path(&path, 0, 0);
                let mut resolved: ImageNode = self.image_for_result_id(address.host, &ImageResultId::Output(output.clone()))
                    .or_else(|| matches!(&host.kind, NodeKind::Portal(portal) if portal.kind == slate_doc::scene::PortalKind::Agent)
                        .then(|| ImageNode::new(item)))?;
                resolved.item = item;
                resolved.agent = None;
                resolved.result_layers.clear();
                let mut node = host.clone();
                node.id = NodeId(u64::MAX - index as u64);
                node.kind = NodeKind::Image(resolved);
                node.rect = node
                    .rect
                    .translated(index as f32 * (host.rect.w + 24.0), 0.0);
                Some(node)
            })
            .collect();
        let mut snapshot = slate_doc::SlateDoc::new("image results");
        snapshot.items = self.doc().items.clone();
        (snapshot, payload)
    }
}

#[cfg(test)]
mod tests;
