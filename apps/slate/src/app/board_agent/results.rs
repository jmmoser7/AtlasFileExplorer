//! Place results, stop, and the model menu.

use super::*;

impl SlateApp {
    pub(crate) fn unbundle_selected_agent(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        if self.agent_is_running(id) {
            self.toast("Wait for the current generation or stop it before unbundling.");
            return false;
        }
        let images = self.agent_images(id);
        if matches!(
            self.doc().scene.node(id).map(|n| &n.kind),
            Some(NodeKind::Image(_))
        ) {
            return self.place_agent_results(id, &images);
        }
        let active = self.agents.cover_focus.get(&id).copied().unwrap_or(0);
        match slate_doc::agent_inputs::unbundle(&self.doc().scene, id, &images, active) {
            Ok((commands, ids)) => {
                if self.commit_scene(commands) {
                    self.board_sel = ids.into_iter().collect();
                    self.agents.sessions.remove(&id);

                    self.agents.cover_focus.remove(&id);
                    true
                } else {
                    false
                }
            }
            Err(error) => {
                self.toast(error);
                false
            }
        }
    }
    /// Unbundle on a picture an agent makes: every other result becomes an
    /// ordinary placed picture in a row beside it, one undo step. The card
    /// keeps its agent and its shown result.
    fn place_agent_results(&mut self, id: NodeId, images: &[atlas_ai::agent::ImageOutput]) -> bool {
        let Some(rect) = self.doc().scene.node(id).map(|n| n.rect) else {
            return false;
        };
        let shown = self.agent_shown_index(id);
        let base = self.tab().path.clone();
        let mut nodes = Vec::new();
        for (i, image) in images.iter().enumerate() {
            if Some(i) == shown {
                continue;
            }
            let Some(item) = self.item_for_path(&resolve_source(base.as_deref(), &image.source))
            else {
                continue;
            };
            let at = rect.translated((nodes.len() as f32 + 1.0) * (rect.w + 24.0), 0.0);
            nodes.push(
                self.doc_mut()
                    .scene
                    .build_node(at, NodeKind::Image(slate_doc::scene::ImageNode::new(item))),
            );
        }
        if nodes.is_empty() {
            self.toast("This picture has no other results to place.");
            return false;
        }
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.into_iter().collect();
        true
    }
    pub(crate) fn stop_selected_agent(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some((session, _)) = self.agent_session_for(id) else {
            return false;
        };
        let generator = slate_doc::agent_inputs::is_flow_node(&self.doc().scene, id)
            || self
                .agent_session_for(id)
                .is_some_and(|(_, p)| atlas_ai::agent::local_image_engine(&p));
        if generator {
            self.agents.comfy_queue.remove(&id);
            self.agents.capture_wait.remove(&id);
            if self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.live)
            {
                self.set_generator_live(id, false);
            }
        }
        if let Some(link) = self.agents.codex.get(&session) {
            link.stop();
            return true;
        }
        if generator {
            return true;
        }
        let Some((_, provider)) = self.agent_session_for(id) else {
            return false;
        };
        if provider != "cursor" {
            self.toast("Stop this run in its source program.");
            return false;
        }
        let Some(request) = self.agents.requests.get(&id).cloned() else {
            self.toast("This Cursor response has not started yet.");
            return false;
        };
        let Some(ws) = self.ai.config.workspace_dir.clone() else {
            return false;
        };
        let Some(dir) = self.agent_link_dir(id, &ws) else {
            return false;
        };
        let published = dir == atlas_ai::agent::agent_dir(&ws, &session);
        if !published && !dir.is_dir() {
            self.toast("This run's folder has moved; stop it in Cursor.");
            return false;
        }
        std::thread::spawn(move || {
            // The workspace link folder is published by a worker; Stop may run
            // first. A bundle's folder is the user's, never recreated here.
            if published {
                let _ = std::fs::create_dir_all(&dir);
            }
            let _ = atlas_ai::agent::atomic_write_json(
                &dir.join("cancel.json"),
                &serde_json::json!({"id": request}),
            );
        });
        true
    }
    pub(super) fn detach_cursor_sidecar(&mut self, session: &str) {
        if let Some(mut child) = self.agents.sidecar_child.remove(session) {
            let _ = child.kill();
            let _ = child.try_wait();
        }
        self.agents.sidecar_spawned.remove(session);
        self.agents.sidecar_booting.remove(session);
    }
    /// A Cursor sidecar reads its permissions when it starts. Restart it now
    /// when idle; while a reply runs, before the next message instead, so the
    /// running reply is never killed and keeps the permissions it began with.
    pub(in super::super) fn restart_cursor_sidecar(&mut self, session: &str) {
        let running = self.doc().scene.nodes.iter().any(|n| {
            slate_doc::agent_chat::agent(n).is_some_and(|a| a.session == session)
                && self.agent_is_running(n.id)
        });
        if running {
            self.agents.sidecar_restart.insert(session.to_string());
        } else {
            self.detach_cursor_sidecar(session);
        }
    }
    /// Drop the portal's owned provider thread pointer. The provider keeps the
    /// conversation; the next send starts a new one.
    pub(super) fn forget_owned_thread(&mut self, portal: NodeId, file: &str) {
        if let Some(old) = self.agents.sessions.get(&portal).cloned() {
            let mut state = (*old).clone();
            state.conversation.clear();
            self.agents
                .sessions
                .insert(portal, std::sync::Arc::new(state));
        }
        let Some(ws) = self.ai.config.workspace_dir.clone() else {
            return;
        };
        let Some(dir) = self.agent_link_dir(portal, &ws) else {
            return;
        };
        let file = file.to_string();
        std::thread::spawn(move || {
            let _ = std::fs::remove_file(dir.join(&file));
            let session_path = dir.join("session.json");
            if let Ok(bytes) = std::fs::read(&session_path) {
                if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    value["conversation"] = serde_json::Value::String(String::new());
                    let _ = atlas_ai::agent::atomic_write_json(&session_path, &value);
                }
            }
        });
    }
    /// One rounded hover control over a generator image. Returns its click.
    pub(crate) fn generator_button(
        ui: &egui::Ui,
        painter: &egui::Painter,
        at: Pos2,
        label: &str,
        id: Id,
        ink: &OverlayInk,
        z: f32,
    ) -> (egui::Response, Rect) {
        let font = canvas_scale::font(GENERATOR_CHIP_PX, z);
        let text = canvas_text::layout_no_wrap(painter, label.to_string(), font, ink.text);
        let pad = canvas_scale::px(12.0, z);
        let rect = Rect::from_min_size(
            at,
            egui::vec2(text.size().x + pad * 2.0, canvas_scale::px(28.0, z)),
        );
        paint_overlay_pill(painter, rect, canvas_scale::px(8.0, z), ink.fill, ink, z);
        text.paint_anchored(painter, rect.center(), Align2::CENTER_CENTER, ink.text);
        (ui.interact(rect, id, Sense::click()), rect)
    }
    /// The generator's inputs as the run will read them. A click selects the source.
    pub(super) fn paint_generator_chips(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        id: NodeId,
        body: Rect,
        view: &GeneratorView,
        z: f32,
    ) -> f32 {
        let font = canvas_scale::font(GENERATOR_CHIP_PX, z);
        let mut y = body.top() + canvas_scale::px(12.0, z);
        if !canvas_text::legible(font.size) {
            return y;
        }
        let x = body.left() + canvas_scale::px(12.0, z);
        let pad = canvas_scale::px(6.0, z);
        let dot = canvas_scale::px(3.5, z);
        let ink = self.palette().overlay();
        let mut picked = None;
        let roles = self.palette().agent_roles();
        for (i, input) in view.inputs.iter().enumerate().take(6) {
            let (role, color) = input.role.look(&roles);
            let text = canvas_text::layout_no_wrap(
                painter,
                format!("{role} · {}", input.label),
                font.clone(),
                ink.text,
            );
            let size = text.size();
            let rect = Rect::from_min_size(
                Pos2::new(x, y),
                egui::vec2(size.x + pad * 3.0 + dot * 2.0, size.y + pad * 1.4),
            );
            paint_overlay_pill(painter, rect, canvas_scale::px(7.0, z), ink.fill, &ink, z);
            painter.circle_filled(
                Pos2::new(rect.left() + pad + dot, rect.center().y),
                dot,
                color,
            );
            text.paint_anchored(
                painter,
                Pos2::new(rect.left() + pad * 2.0 + dot * 2.0, rect.center().y),
                Align2::LEFT_CENTER,
                ink.text,
            );
            if ui
                .interact(rect, Id::new(("generator-input", id.0, i)), Sense::click())
                .clicked()
            {
                picked = Some(input.node);
            }
            y = rect.bottom() + canvas_scale::px(4.0, z);
        }
        if let Some(node) = picked {
            self.board_sel = std::iter::once(node).collect();
        }
        y
    }
    /// The model a portal will use next, as its menu button reads it.
    pub(super) fn model_menu_label(&self, agent: &slate_doc::scene::AgentPortalRef) -> String {
        let catalog = atlas_ai::agent::model_catalog(&agent.provider);
        let name = agent
            .model
            .as_ref()
            .and_then(|id| {
                self.agents
                    .models
                    .get(catalog)?
                    .iter()
                    .find(|m| &m.id == id)
            })
            .map(|m| m.name.as_str())
            .or(agent.model.as_deref())
            .or_else(|| agent.provider.strip_prefix("ollama/"))
            .unwrap_or(model_default(agent));
        atlas_ai::agent::model_label(name).to_string()
    }
    /// The one model menu: a chat title and a generator's checkpoint chip open it.
    /// Returns the choice; empty means the provider's default.
    pub(super) fn model_menu_items(
        &self,
        ui: &mut egui::Ui,
        id: NodeId,
        agent: &slate_doc::scene::AgentPortalRef,
        z: f32,
    ) -> Option<String> {
        atlas_shell::menu::prepare(ui, self.palette().dark_mode);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.style_mut().override_font_id =
            Some(FontId::proportional(canvas_text::authored_px(13.0, z)));
        let catalog = atlas_ai::agent::model_catalog(&agent.provider);
        let running = self.agent_is_running(id);
        let mut chosen = None;
        // The catalog is newest-first and longer than the window. An unscrolling
        // menu is shoved upward until its top (Grok 4.7, Opus 5, …) sits above
        // the screen, so only the older tail looks available.
        let max_h = (ui.ctx().screen_rect().height() - 48.0).max(160.0);
        // A popup lays out inside last frame's size. Its rows grow with the
        // zoom, so without room of its own the list clips and scrolls away.
        ui.set_max_height(max_h);
        egui::ScrollArea::vertical()
            .max_height(max_h)
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                // A local chat model is always an explicit choice; a text
                // block's Auto picks an installed model that reads pictures.
                if (catalog != "ollama" || agent.view == atlas_ai::agent::PortalView::Text)
                    && ui
                        .add_enabled(
                            !running,
                            egui::SelectableLabel::new(agent.model.is_none(), model_default(agent)),
                        )
                        .clicked()
                {
                    chosen = Some(String::new());
                    ui.close_menu();
                }
                for model in self.agents.models.get(catalog).into_iter().flatten() {
                    if ui
                        .add_enabled(
                            !running,
                            egui::SelectableLabel::new(
                                agent.model.as_ref() == Some(&model.id),
                                atlas_ai::agent::model_label(&model.name),
                            ),
                        )
                        .clicked()
                    {
                        chosen = Some(model.id.clone());
                        ui.close_menu();
                    }
                }
                if let Some(error) = self.agents.models_error.get(catalog) {
                    ui.label(error);
                }
                if self.agents.models_rx.is_some() {
                    ui.label("Loading models…");
                }
            });
        atlas_shell::menu_wheel::claim(ui.ctx(), ui.min_rect());
        chosen
    }
    /// A generator or text block switches engine or model. Its runtime link is
    /// dropped so the next run starts on the new engine, and the choice becomes
    /// the default for new agents of its kind.
    pub(crate) fn set_flow_model(&mut self, id: NodeId, image: bool, provider: &str, model: &str) {
        if self.agent_is_running(id) {
            self.toast("Stop this run before changing the model.");
            return;
        }
        if let Some((session, old)) = self.agent_session_for(id) {
            if old != provider {
                self.agents.codex.remove(&session);
            }
        }
        let chosen = (!model.is_empty()).then(|| model.to_string());
        self.patch_nodes(&[id], |n| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                a.provider = provider.to_string();
                a.model = chosen.clone();
            }
        });
        self.remember_model(image, provider, model);
    }
    /// Decode the newest sampling frame of each running generator once.
    pub(super) fn pump_generation_previews(&mut self, ctx: &egui::Context) {
        let running: Vec<(NodeId, String, String)> = self
            .agents
            .requests
            .iter()
            .filter(|(id, _)| self.agent_is_running(**id))
            .filter_map(|(id, request)| {
                let session = self.agents.bindings.get(id)?;
                Some((*id, request.clone(), session.clone()))
            })
            .collect();
        self.agents
            .previews
            .retain(|id, _| running.iter().any(|(r, _, _)| r == id));
        for (id, request, session) in running {
            let Some(frame) = self
                .agents
                .codex
                .get(&session)
                .and_then(|link| link.preview())
                .filter(|p| p.request == request)
            else {
                continue;
            };
            if self
                .agents
                .previews
                .get(&id)
                .is_some_and(|p| p.request == request && p.frame == frame.frame)
            {
                continue;
            }
            let Ok(decoded) = image::load_from_memory(&frame.image) else {
                continue;
            };
            let rgba = decoded.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let pixels = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            let texture = match self.agents.previews.remove(&id) {
                Some(mut p) => {
                    p.texture.set(pixels, egui::TextureOptions::LINEAR);
                    p.texture
                }
                None => ctx.load_texture(
                    format!("generation-preview-{}", id.0),
                    pixels,
                    egui::TextureOptions::LINEAR,
                ),
            };
            self.agents.previews.insert(
                id,
                LivePreview {
                    request,
                    frame: frame.frame,
                    step: frame.step,
                    steps: frame.steps,
                    texture,
                },
            );
            ctx.request_repaint();
        }
    }
    /// The image coming into focus while it samples, with a thin step bar.
    pub(super) fn paint_generation_preview(
        &self,
        painter: &egui::Painter,
        id: NodeId,
        body: Rect,
        z: f32,
    ) {
        let Some(preview) = self.agents.previews.get(&id) else {
            return;
        };
        if self.agents.requests.get(&id) != Some(&preview.request) {
            return;
        }
        let size = preview.texture.size_vec2();
        let scale = (body.width() / size.x).min(body.height() / size.y);
        let rect = Rect::from_center_size(body.center(), size * scale);
        painter.image(
            preview.texture.id(),
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if preview.steps > 0 {
            let t = (preview.step as f32 / preview.steps as f32).clamp(0.0, 1.0);
            let h = canvas_scale::px(3.0, z);
            let bar = Rect::from_min_size(
                egui::pos2(rect.left(), rect.bottom() - h),
                egui::vec2(rect.width() * t, h),
            );
            painter.rect_filled(bar, 0.0, self.palette().accent);
        }
    }
}
