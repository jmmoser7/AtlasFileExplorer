//! One result selection and action controller for the strip and generator history.
use super::*;
use atlas_shell::result_actions::{ResultAction, ResultInteraction, ResultSelection};
use slate_doc::image_paint::ImageResultId;
use slate_doc::scene::{ImageNode, Node};
use std::path::PathBuf;

mod save;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ResultAddress {
    tab: u64,
    host: NodeId,
    binding: String,
    outputs: Vec<String>,
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
    fn result_binding(&self, host: NodeId) -> Option<String> {
        let agent = self
            .doc()
            .scene
            .node(host)
            .and_then(slate_doc::agent_chat::agent)?;
        serde_json::to_string(&(
            &agent.provider,
            &agent.session,
            &agent.channel,
            &agent.bundle,
            agent.seed.as_ref().map(|seed| &seed.id),
        ))
        .ok()
    }

    pub(crate) fn result_selection(&self, host: NodeId) -> ResultSelection {
        self.result_binding(host)
            .and_then(|binding| {
                self.agents
                    .flow
                    .results
                    .selections
                    .get(&(self.tab().id, host, binding))
            })
            .cloned()
            .unwrap_or_default()
    }

    /// Shell adds the canonical strip gap; the host remains the hover origin.
    pub(crate) fn result_strip_placement(&self, host: NodeId, mut body: Rect) -> Rect {
        if let Some((_, _, bottom)) = self
            .shape_properties
            .result_chrome_bottom
            .filter(|(tab, owner, _)| *tab == self.tab().id && *owner == host)
        {
            body.max.y = body.bottom().max(bottom);
        }
        body
    }

    pub(crate) fn result_strip_state(
        &self,
        ctx: &egui::Context,
        host: NodeId,
        body: Rect,
        zoom: f32,
    ) -> Option<(bool, usize)> {
        let node = self.doc().scene.node(host)?;
        let agent = slate_doc::agent_chat::agent(node)?;
        if node.hidden || agent.view == PortalView::Text || self.agent_picture_draw_mode(host) {
            return None;
        }
        let selected = self.board_sel.len() == 1 && self.board_sel.contains(&host);
        let flowing = selected
            && self.image_wheel(host) == crate::app::board_agent::ImageWheel::Album
            && atlas_shell::home::album_browsing(ctx, Id::new(("agent-picture", host.0))) > 0.001;
        let focus = if flowing {
            self.agent_album_focus(host)
        } else {
            self.agent_shown_index(host)
        }
        .unwrap_or(0);
        let reveal = !flowing
            && !self.agents.flow.hud_hidden(host)
            && (ctx.pointer_latest_pos().is_some_and(|p| body.contains(p))
                || selected
                || self.flow_capsule_open(host)
                || self.result_strip_hovered(ctx, host, body, zoom));
        Some((reveal, focus))
    }

    /// Exact current thumbnail hits keep their editor open without taking input.
    pub(crate) fn result_strip_contains(
        &self,
        ctx: &egui::Context,
        host: NodeId,
        body: Rect,
        zoom: f32,
    ) -> bool {
        let Some(pointer) = ctx.pointer_latest_pos() else {
            return false;
        };
        if !self.canvas_rect.contains(pointer) {
            return false;
        }
        let images = self.agent_images(host);
        let runs: Vec<&str> = images.iter().map(|image| image.request.as_str()).collect();
        let Some((reveal, focus)) = self.result_strip_state(ctx, host, body, zoom) else {
            return false;
        };
        atlas_shell::home::album_result_layout(
            ctx,
            Id::new(("agent-picture", host.0)),
            self.result_strip_placement(host, body),
            &runs,
            images.len(),
            focus,
            reveal,
            zoom,
            2,
        )
        .is_some_and(|(_, squares)| squares.iter().any(|(_, square)| square.contains(pointer)))
    }

    pub(crate) fn remember_result_strip(
        &mut self,
        host: NodeId,
        body: Rect,
        rect: Rect,
        zoom: f32,
    ) {
        let key = (self.tab().id, host);
        if !rect.is_positive() || zoom <= 0.0 {
            self.agents.flow.results.strips.remove(&key);
            return;
        }
        let origin = body.center_bottom();
        let min = (rect.min - origin) / zoom;
        let max = (rect.max - origin) / zoom;
        self.agents.flow.results.strips.insert(
            key,
            Rect::from_min_max(Pos2::new(min.x, 0.0), Pos2::new(max.x, max.y)),
        );
    }

    /// Once revealed, the strip owns its row and the short gap from the image.
    /// Requiring recorded visibility prevents invisible thumbnails from taking
    /// input before the user has approached their host.
    pub(crate) fn result_strip_hovered(
        &self,
        ctx: &egui::Context,
        host: NodeId,
        body: Rect,
        zoom: f32,
    ) -> bool {
        if !self.doc().scene.node(host).is_some_and(|node| !node.hidden) {
            return false;
        }
        let Some(corridor) = self.agents.flow.results.strips.get(&(self.tab().id, host)) else {
            return false;
        };
        let origin = body.center_bottom();
        let lane = Rect::from_min_max(
            origin + corridor.min.to_vec2() * zoom,
            origin + corridor.max.to_vec2() * zoom,
        );
        ctx.pointer_latest_pos()
            .is_some_and(|pointer| lane.contains(pointer))
    }

    pub(crate) fn handle_result_interaction(
        &mut self,
        ui: &egui::Ui,
        host: NodeId,
        out: ResultInteraction,
        palette: Rect,
    ) {
        if out.menu.is_none() && out.clicked.is_none() && out.drag_started.is_none() {
            return;
        }
        let images = self.agent_images(host);
        if let Some((index, at)) = out.menu {
            let Some(binding) = self.result_binding(host) else {
                return;
            };
            let ordered: Vec<String> = images.iter().map(|image| image.id.clone()).collect();
            let tab = self.tab().id;
            let selection = self
                .agents
                .flow
                .results
                .selections
                .entry((tab, host, binding.clone()))
                .or_default();
            selection.select(&ordered, index, out.shift, out.clicked.is_none());
            let outputs = selection
                .identities(&ordered)
                .into_iter()
                .map(str::to_owned)
                .collect();
            self.agents.flow.results.menu = Some(ResultMenu {
                address: ResultAddress {
                    tab,
                    host,
                    binding,
                    outputs,
                },
                at,
                opener: ui.layer_id(),
            });
            self.board_sel = std::iter::once(host).collect();
            self.agents.flow.set_hud_hidden(host, false);
        }
        if let Some(index) = out.clicked {
            self.pick_agent_result(host, index);
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
        let images = self.agent_images(address.host);
        let available: std::collections::HashSet<&str> =
            images.iter().map(|image| image.id.as_str()).collect();
        address
            .outputs
            .iter()
            .all(|id| available.contains(id.as_str()))
    }

    /// Runs before board gestures: result chrome never clears the host selection.
    pub(crate) fn result_controls_input(&mut self, ui: &egui::Ui, xf: &BoardXf) -> bool {
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
        let mut action = None;
        let popup_id = Id::new(("image-result-menu", menu.address.tab, menu.address.host.0));
        let response = atlas_shell::selection_tools::popup_area(ui.ctx(), popup_id, menu.opener)
            .fixed_pos(menu.at)
            .show(ui.ctx(), |ui| {
                atlas_shell::menu::frame(self.settings.dark_mode).show(ui, |ui| {
                    action = atlas_shell::result_actions::menu(
                        ui,
                        menu.address.outputs.len(),
                        self.settings.dark_mode,
                    );
                });
            })
            .response;
        self.agents.flow.results.popup_rect = Some(response.rect);
        let over = pointer.is_some_and(|p| response.rect.contains(p));
        let dismiss =
            ui.input(|i| i.key_pressed(egui::Key::Escape) || (i.pointer.any_pressed() && !over));
        if let Some(action) = action {
            if action == ResultAction::SaveAs {
                self.agents.flow.results.save.opener = Some(menu.opener);
            }
            let command = match action {
                ResultAction::Copy => "portal.agent.result.copy",
                ResultAction::SaveAs => "portal.agent.result.save_as",
                ResultAction::CopyToCanvas => "portal.agent.result.copy_to_canvas",
            };
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId(command),
                serde_json::to_string(&menu.address).ok(),
            );
        } else if !dismiss {
            self.agents.flow.results.menu = Some(menu);
        }
        saving || strip_capture || over
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
                let ordered: Vec<String> = self
                    .agent_images(host)
                    .iter()
                    .map(|image| image.id.clone())
                    .collect();
                let mut outputs: Vec<String> = self
                    .result_selection(host)
                    .identities(&ordered)
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                if outputs.is_empty() {
                    outputs.push(ordered.get(self.agent_shown_index(host)?)?.clone());
                }
                Some(ResultAddress {
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
                let mut resolved: ImageNode =
                    self.image_for_result_id(address.host, &ImageResultId::Output(output.clone()))?;
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
