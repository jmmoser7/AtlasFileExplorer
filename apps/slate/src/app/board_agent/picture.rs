//! Generation preview and the agent picture.

use super::*;

impl SlateApp {
    /// What an agent adds to the picture it makes: the prompt docked along the
    /// bottom, progress, the run button, and squares under the card for every
    /// result. A click on a square picks that result; the newest shows until
    /// then. Model, count, aspect, seed and Live live on the Agent squircle.
    /// An agent's media card, then the caption naming a pack it needs that
    /// did not answer this session.
    pub(crate) fn paint_agent_media(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        body: Rect,
    ) {
        if slate_doc::agent_chat::agent(node)
            .is_some_and(|a| a.view == atlas_ai::agent::PortalView::Text)
        {
            self.paint_agent_text_window(ui, painter, xf, node, body);
        } else {
            self.paint_agent_picture(ui, painter, xf, node, body);
        }
        self.paint_pack_caption(painter, node, body, xf.z);
    }
    pub(crate) fn paint_agent_picture(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        body: Rect,
    ) {
        let Some(agent) = slate_doc::agent_chat::agent(node).cloned() else {
            return;
        };
        if self.agent_picture_draw_mode(node.id) {
            return;
        }
        let id = node.id;
        let z = xf.z;
        let images = self.agent_images(id);
        let shown = self.agent_shown_index(id);
        let view = self.generator_view(id);
        let picked = matches!(&node.kind, NodeKind::Image(i) if !i.item.is_none());
        let hovered = ui
            .ctx()
            .pointer_latest_pos()
            .is_some_and(|p| body.contains(p));
        let selected = self.board_sel.len() == 1 && self.board_sel.contains(&id);
        let base = self.tab().path.clone();
        // A selected picture with several results cover-flows under the wheel.
        // Browsing hides its labels; where it settles becomes the pick.
        let album_id = Id::new(("agent-picture", id.0));
        let mut flowing = false;
        if !selected {
            self.agents.flow.set_hud_hidden(id, false);
        } else if self.image_wheel(id) == ImageWheel::Album {
            let was_browsing = atlas_shell::home::album_browsing(ui.ctx(), album_id) > 0.001;
            let focus = if was_browsing {
                self.agents.cover_focus.get(&id).copied()
            } else {
                shown
            }
            .unwrap_or(0)
            .min(images.len() - 1);
            if was_browsing {
                painter.rect_filled(body, 0.0, self.palette().thumb_bg);
            }
            let covers: Vec<AlbumImage> = images
                .iter()
                .enumerate()
                .map(|(i, image)| {
                    let texture = (i.abs_diff(focus) <= 3)
                        .then(|| {
                            self.linked_image_texture(
                                resolve_source(base.as_deref(), &image.source),
                                &image.id,
                                body.width().max(body.height()),
                            )
                        })
                        .flatten();
                    AlbumImage {
                        texture: texture.as_ref().map(|t| t.id()),
                        size: texture
                            .as_ref()
                            .map(|t| t.size_vec2())
                            .unwrap_or(egui::vec2(1.0, 1.0)),
                        enabled: true,
                    }
                })
                .collect();
            let next = image_album(
                ui,
                album_id,
                body,
                &covers,
                focus,
                atlas_shell::home::AlbumInput {
                    drag: false,
                    wheel: true,
                    host_paints_rest: true,
                },
            );
            if next != focus {
                self.agents.flow.set_hud_hidden(id, true);
            }
            self.agents.cover_focus.insert(id, next);
            flowing = atlas_shell::home::album_browsing(ui.ctx(), album_id) > 0.001;
            if !flowing && next == focus && Some(next) != shown {
                self.pick_agent_result(id, next);
            }
        }
        // Clicking the picture again brings its labels back.
        if !flowing
            && self.agents.flow.hud_hidden(id)
            && hovered
            && ui.input(|i| i.pointer.primary_clicked())
        {
            self.agents.flow.set_hud_hidden(id, false);
        }
        let hud = !flowing && !self.agents.flow.hud_hidden(id);
        let reveal = hud && (hovered || selected || self.flow_capsule_open(id));
        // A new result reshapes the card to its aspect, once.
        if let (Some(i), false) = (shown, picked) {
            let image = &images[i];
            let path = resolve_source(base.as_deref(), &image.source);
            if let Some(tex) =
                self.linked_image_texture(path, "shown", body.width().max(body.height()))
            {
                self.fit_generator_frame(id, &image.id, tex.size_vec2());
            }
        }
        if self.image_wheel(id) == ImageWheel::Model {
            self.steer_generator_model(ui, id, body);
        }
        let running = self.agent_is_running(id);
        let waiting = self.generations_waiting(id);
        if running {
            self.paint_generation_preview(painter, id, body, z);
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let legible = canvas_text::legible(canvas_scale::px(GENERATOR_CHIP_PX, z));
        if agent.live && !reveal {
            painter.circle_filled(
                body.left_bottom()
                    + egui::vec2(canvas_scale::px(14.0, z), canvas_scale::px(-14.0, z)),
                canvas_scale::px(4.5, z),
                self.palette().accent,
            );
        }
        // Before its first result the picture is a prompt: type, or wire a note.
        if images.is_empty() && legible && !(running || waiting > 0) {
            let wired = view.inputs.iter().any(|i| i.role == InputRole::Prompt);
            let hint = if wired {
                "Add to the wired prompt, or press Generate"
            } else {
                "Wire a note to Prompt, or type here"
            };
            let width = (body.width() - canvas_scale::px(48.0, z)).min(canvas_scale::px(520.0, z));
            let field =
                Rect::from_center_size(body.center(), egui::vec2(width, canvas_scale::px(64.0, z)));
            let ink = self.palette().overlay();
            paint_overlay_pill(
                painter,
                field.expand(canvas_scale::px(10.0, z)),
                canvas_scale::px(10.0, z),
                ink.scrim,
                &ink,
                z,
            );
            self.paint_generator_prompt(ui, id, field, z, hint);
        }
        let previews = agent.provider == "comfy";
        if hud {
            self.paint_run_progress(ui, painter, id, body, z, view.task().working(), previews);
        }
        // Squares under the card: every result, grouped by run. While it
        // flows they follow the flow.
        if images.len() > 1 {
            let focus = if flowing {
                self.agents.cover_focus.get(&id).copied()
            } else {
                shown
            }
            .unwrap_or(0);
            let squares: Vec<AlbumImage> = images
                .iter()
                .enumerate()
                .map(|(i, image)| {
                    let near = i.abs_diff(focus) <= 12;
                    let texture = near
                        .then(|| {
                            self.linked_image_texture(
                                resolve_source(base.as_deref(), &image.source),
                                &image.id,
                                canvas_scale::px(44.0, z),
                            )
                        })
                        .flatten();
                    AlbumImage {
                        texture: texture.as_ref().map(|t| t.id()),
                        size: texture
                            .as_ref()
                            .map(|t| t.size_vec2())
                            .unwrap_or(egui::vec2(1.0, 1.0)),
                        enabled: true,
                    }
                })
                .collect();
            let runs: Vec<&str> = images.iter().map(|i| i.request.as_str()).collect();
            if let Some(pick) = atlas_shell::home::album_index_strip(
                ui,
                Id::new(("agent-picture", id.0)),
                body,
                &squares,
                &runs,
                focus,
                reveal,
                z,
                self.palette(),
                2,
                None,
            ) {
                self.pick_agent_result(id, pick);
            }
        }
        if !reveal || !legible {
            return;
        }
        let text_top = self.paint_generator_chips(ui, painter, id, body, &view, z);
        let ink = self.palette().overlay();
        let failure = match self.agents.awaiting.get(&id) {
            Some(AgentAwait::Failed { reason, .. }) => Some(reason.clone()),
            _ => view.error.clone(),
        };
        if let Some(reason) = failure {
            let text = canvas_text::world_wrapped(
                painter.ctx(),
                &reason,
                egui::FontId::proportional(GENERATOR_CHIP_PX),
                body.width() / z.max(0.01) - 24.0,
                usize::MAX,
                z,
            );
            let at = Pos2::new(body.left() + canvas_scale::px(12.0, z), text_top);
            let back = Rect::from_min_size(at, text.size()).expand(canvas_scale::px(5.0, z));
            paint_overlay_pill(painter, back, canvas_scale::px(6.0, z), ink.fill, &ink, z);
            text.paint(painter, at, ink.warn);
        }
        // The dock: run, then the prompt that made the picture across the rest.
        let action = if agent.live {
            "Keep"
        } else if running {
            "Stop"
        } else {
            view.task().action()
        };
        let at =
            body.left_bottom() + egui::vec2(canvas_scale::px(12.0, z), canvas_scale::px(-40.0, z));
        let (primary, rect) = Self::generator_button(
            ui,
            painter,
            at,
            action,
            Id::new(("agent-generate", id.0)),
            &ink,
            z,
        );
        let primary = match action {
            "Keep" => primary.on_hover_text("Keep this frame. Live continues."),
            "Stop" => primary.on_hover_text("Stop this run and clear the waiting ones."),
            _ => primary.on_hover_text(view.task().explain()),
        };
        if primary.clicked() {
            self.board_sel = std::iter::once(id).collect();
            let command = match action {
                "Stop" => "portal.agent.stop",
                "Keep" => "portal.agent.keep",
                _ => "portal.agent.send",
            };
            self.dispatch(ui.ctx(), atlas_commands::CommandId(command), None);
        }
        if !images.is_empty() {
            let capsule = Rect::from_min_max(
                Pos2::new(rect.right() + canvas_scale::px(6.0, z), rect.top()),
                Pos2::new(body.right() - canvas_scale::px(12.0, z), rect.bottom()),
            );
            self.paint_prompt_capsule(ui, painter, id, capsule, body, z);
        }
    }
    /// Picking a result makes it the picture's file (journaled, undoable).
    pub(crate) fn pick_agent_result(&mut self, id: NodeId, index: usize) {
        let Some(image) = self.agent_images(id).get(index).cloned() else {
            return;
        };
        if self.tab().read_only {
            return;
        }
        let path = resolve_source(self.tab().path.as_deref(), &image.source);
        let Some(item) = self.item_for_path(&path) else {
            self.toast("That result is no longer on disk.");
            return;
        };
        self.patch_nodes(&[id], |n| {
            if let NodeKind::Image(i) = &mut n.kind {
                i.item = item;
            }
        });
    }
    /// The file an agent picture shows: the picked result, else the newest.
    pub(crate) fn agent_shown_path(&self, id: NodeId) -> Option<PathBuf> {
        let images = self.agent_images(id);
        let image = images.get(self.agent_shown_index(id)?)?;
        Some(resolve_source(self.tab().path.as_deref(), &image.source))
    }
    pub(super) fn agent_background_pending(&self) -> bool {
        self.ai.packs.in_flight()
            || self.agents.recents_rx.is_some()
            || self.agents.ide_inflight
            || self.agents.chats_rx.is_some()
            || self.agents.connection_rx.is_some()
            || self.agents.connection_pending.is_some()
            || self.agents.models_rx.is_some()
            || !self.agents.live_run.is_empty()
            || !self.agents.comfy_queue.is_empty()
            || self.agents.awaiting.values().any(|state| {
                matches!(
                    state,
                    AgentAwait::Sent { .. }
                        | AgentAwait::Thinking { .. }
                        | AgentAwait::Responding { .. }
                )
            })
            || self
                .agents
                .sessions
                .values()
                .any(|session| session.status == AgentStatus::Thinking)
    }
}
