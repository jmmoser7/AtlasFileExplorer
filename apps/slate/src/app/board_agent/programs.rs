//! Program grid and binding.

use super::*;

impl SlateApp {
    /// Pins the installed-program list, so tests never probe the machine.
    #[cfg(test)]
    pub(crate) fn set_agent_programs_for_test(&mut self, ids: &[&str]) {
        self.agents.programs_started = true;
        self.ai.packs.hold = true;
        self.agents.programs = ids
            .iter()
            .map(|id| atlas_ai::agent::provider_by_id(id))
            .collect();
    }
    /// Pins one pack's health now and for every later probe, so a test does
    /// not depend on what this machine has installed.
    #[cfg(test)]
    pub(crate) fn pin_pack_for_test(&mut self, id: &str, health: atlas_ai::packs::PackHealth) {
        self.ai.packs.set_health_override(id, health);
        self.ai.packs.record(id, health);
    }
    /// The unbound agent portal's size (width, height) around its program
    /// grid, once the installed programs are known.
    pub(crate) fn agent_program_grid_size(&self) -> Option<(f32, f32)> {
        let count = self.agents.programs.len();
        (count > 0).then(|| {
            let grid = atlas_ai::ui::program_grid_size(count);
            (grid.x + 48.0, grid.y + 72.0)
        })
    }
    pub(crate) fn ensure_agent_programs(&mut self) {
        if !self.agents.programs_started {
            self.agents.programs_started = true;
            self.ai
                .packs
                .ensure_started(self.ai.config.workspace_dir.clone());
        }
        if let Some(programs) = self.ai.packs.poll_programs() {
            if !self.ai.packs.hold {
                self.agents.programs = programs;
            }
        }
    }
    pub(crate) fn pack_caption(&self, provider: &str) -> Option<String> {
        if provider.is_empty() {
            return None;
        }
        let health = self.ai.packs.health(provider);
        if health == atlas_ai::packs::PackHealth::Ok {
            return None;
        }
        if health == atlas_ai::packs::PackHealth::Unknown
            && self.ai.packs.detail(provider).is_empty()
        {
            return None;
        }
        let title = self.ai.packs.title(provider)?;
        Some(format!("{title} · {}", health.as_str()))
    }
    /// Names the pack a card needs when it did not answer Ok this session
    /// (P1.portal.health). The card keeps painting its last poster.
    pub(crate) fn paint_pack_caption(
        &self,
        painter: &egui::Painter,
        node: &Node,
        body: Rect,
        z: f32,
    ) {
        let Some(caption) =
            slate_doc::agent_chat::agent(node).and_then(|a| self.pack_caption(&a.provider))
        else {
            return;
        };
        atlas_shell::canvas_text::text(
            painter,
            egui::pos2(
                body.center().x,
                body.bottom() - atlas_shell::canvas_scale::px(14.0, z),
            ),
            egui::Align2::CENTER_BOTTOM,
            &caption,
            atlas_shell::canvas_scale::font(12.0, z),
            self.palette().sub,
        );
    }
    /// The shared OpenAI key window. A stored key opens the OpenAI catalogs.
    pub(super) fn paint_pack_key_entry(&mut self, ctx: &egui::Context) {
        match atlas_ai::ui::openai_key_window(&mut self.ai, ctx) {
            Some(Ok(())) => {
                self.agents.refresh_catalog("openai-image");
                self.agents.refresh_catalog("openai-text");
                self.toast("OpenAI API key saved for this user.");
            }
            Some(Err(error)) => self.toast(error),
            None => {}
        }
    }
    /// The program chooser is on screen. Probe again so an install that
    /// landed while Slate was open shows up without a restart.
    pub(crate) fn note_program_chooser(&mut self) {
        self.ai
            .packs
            .note_chooser_open(self.ai.config.workspace_dir.clone());
    }
    pub(crate) fn set_agent_program(&mut self, id: NodeId, provider: &str) {
        if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.chat.parent.is_some() || a.chat.end.is_some())
        {
            self.toast(
                "Create a new agent portal to change programs; this card is a history checkpoint.",
            );
            return;
        }
        if self.agent_is_running(id) {
            self.toast("Stop the current response before changing programs.");
            return;
        }
        if let Some((old, _)) = self.agent_session_for(id) {
            self.agents.codex.remove(&old);
        }

        let program = self
            .agents
            .programs
            .iter()
            .find(|p| p.id == provider)
            .cloned()
            .unwrap_or_else(|| atlas_ai::agent::provider_by_id(provider));
        let binding = self.program_binding(Some(id));
        self.patch_nodes(&[id], |node| bind_program(node, &program, &binding));

        self.agents.project_picker = atlas_ai::runtime::linear_provider(provider).then_some(id);
        if self.agents.project_picker.is_some() && self.agents.recents_rx.is_none() {
            // Rediscover for this workbook; the last list shows meanwhile.
            self.agents.recents_started = false;
        }
        self.agents.sessions.remove(&id);
        self.agents.local_turns.remove(&id);
        self.agents.awaiting.remove(&id);
        self.agent_focus(id);
    }
    /// A fresh session and the locators a newly bound program writes through.
    pub(crate) fn program_binding(&self, id: Option<NodeId>) -> ProgramBinding {
        let source = id
            .and_then(|id| self.agent_folder_for(id))
            .or_else(|| self.ai.config.workspace_dir.clone())
            .or_else(|| {
                self.tab()
                    .path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_path_buf())
            });
        let locator = source.as_deref().map(|path| {
            super::super::board_portal::source_locator(self.tab().path.as_deref(), path)
        });
        let session = slate_doc::scene::new_agent_session_id();
        let bundle = self
            .ai
            .config
            .workspace_dir
            .as_ref()
            .map(|ws| slate_doc::SourceUri {
                locator: super::super::board_portal::source_locator(
                    self.tab().path.as_deref(),
                    &atlas_ai::agent::agent_dir(ws, &session).join("session.json"),
                ),
            });
        ProgramBinding {
            session,
            bundle,
            locator,
        }
    }
    pub(crate) fn agent_images(
        &self,
        id: NodeId,
    ) -> std::sync::Arc<Vec<atlas_ai::agent::ImageOutput>> {
        let key = (self.scene_gen, self.agents.output_epoch);
        if let Some((_, images)) = self
            .agents
            .image_cache
            .borrow()
            .get(&id)
            .filter(|(k, _)| *k == key)
        {
            return images.clone();
        }
        let mut images = Vec::new();
        if let Some(seed) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|a| a.seed.clone())
        {
            images.push(seed);
        }
        if let Some(session) = self.agents.sessions.get(&id) {
            for image in &session.bundle.images {
                if !image.id.is_empty()
                    && !image.source.is_empty()
                    && !images.iter().any(|v| v.id == image.id)
                {
                    images.push(image.clone());
                }
            }
        }
        let images = std::sync::Arc::new(images);
        self.agents
            .image_cache
            .borrow_mut()
            .insert(id, (key, images.clone()));
        images
    }
    pub(crate) fn agent_active_output(&self, id: NodeId) -> Option<String> {
        let images = self.agent_images(id);
        images
            .get(self.agent_shown_index(id)?)
            .map(|i| i.id.clone())
    }
    /// Which result an agent's card shows. A picture shows the file a person
    /// picked, else the newest result; a chat card's album shows its focus.
    pub(crate) fn agent_shown_index(&self, id: NodeId) -> Option<usize> {
        let images = self.agent_images(id);
        if images.is_empty() {
            return None;
        }
        if let Some(NodeKind::Image(img)) = self.doc().scene.node(id).map(|n| &n.kind) {
            let base = self.tab().path.as_deref();
            if let Some(item) = self.doc().item(img.item) {
                if let Some(i) = images
                    .iter()
                    .position(|o| resolve_source(base, &o.source) == item.path)
                {
                    return Some(i);
                }
            }
            let newest = slate_doc::agent_inputs::newest_image(&images)?;
            return images.iter().position(|o| o.id == newest.id);
        }
        Some(
            self.agents
                .cover_focus
                .get(&id)
                .copied()
                .unwrap_or(0)
                .min(images.len() - 1),
        )
    }
    pub(crate) fn toggle_agent_wire_output(&mut self) -> bool {
        let Some(id)=self.board_sel.iter().copied().find(|id|matches!(self.doc().scene.node(*id).map(|n|&n.kind),Some(NodeKind::Connector(c)) if c.binding.as_ref().is_some_and(|b|b.kind==slate_doc::agent_inputs::InputKind::Images))) else{return false;};
        let NodeKind::Connector(c) = &self.doc().scene.node(id).unwrap().kind else {
            return false;
        };
        let binding = c.binding.as_ref().unwrap();
        let all = !binding.all_images;
        // TWIN: slate_doc::agent_inputs::WireBinding::source 2026-09-25
        let source = if binding.input_b { &c.a } else { &c.b };
        let pin = if all {
            None
        } else {
            slate_doc::agent_inputs::endpoint_node(source)
                .and_then(|id| self.agent_active_output(id))
        };
        self.patch_nodes(&[id], |n| {
            if let NodeKind::Connector(c) = &mut n.kind {
                if let Some(b) = &mut c.binding {
                    b.all_images = all;
                    b.output = pin.clone();
                }
            }
        });
        true
    }
}
