//! Existing result-strip placement, painting and stable selection adapters.
use super::*;

impl SlateApp {
    pub(crate) fn result_binding(&self, host: NodeId) -> Option<String> {
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
        if self.agents.flow.results.selections.is_empty() {
            return ResultSelection::default();
        }
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
                || !self.result_selection(host).is_empty()
                || self.flow_capsule_open(host)
                || self.result_strip_hovered(ctx, host, body, zoom));
        Some((reveal, focus))
    }

    /// Open generator history owns result browsing until its panel closes.
    pub(crate) fn generator_history_open(&self, host: NodeId) -> bool {
        self.shape_properties.tab == self.tab().id
            && self.shape_properties.ids == [host]
            && self.shape_properties.panel
                == Some(super::super::super::board_properties::Panel::Agent)
    }

    /// Exact current thumbnail hits keep their editor open without taking input.
    pub(crate) fn result_strip_contains(
        &self,
        ctx: &egui::Context,
        host: NodeId,
        body: Rect,
        zoom: f32,
    ) -> bool {
        if self.generator_history_open(host) {
            return false;
        }
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

    pub(crate) fn paint_text_result_strip(
        &mut self,
        ui: &egui::Ui,
        host: NodeId,
        body: Rect,
        zoom: f32,
    ) {
        let body = self.text_review_visual_body(ui.ctx(), host, body, zoom);
        let outputs = self.result_text_entries(host);
        if outputs.is_empty() {
            return;
        }
        let identities = outputs
            .iter()
            .map(|(identity, _, _)| identity.clone())
            .collect::<Vec<_>>();
        let active = self.active_result_reference(host);
        let focus = self
            .result_outputs(host)
            .iter()
            .position(|output| Some(&output.reference) == active.as_ref())
            .unwrap_or(0);
        let circles = self.shape_properties.ids == [host]
            && self.shape_properties.panel
                == Some(super::super::super::board_properties::Panel::Agent);
        if circles {
            return;
        }
        let selection = self.result_selection(host);
        let shown = !selection.is_empty()
            || (self.board_sel.len() == 1 && self.board_sel.contains(&host))
            || ui
                .ctx()
                .pointer_latest_pos()
                .is_some_and(|point| body.contains(point))
            || self.result_strip_hovered(ui.ctx(), host, body, zoom);
        let images = vec![
            atlas_shell::home::AlbumImage {
                texture: None,
                size: egui::Vec2::splat(96.0),
                enabled: true
            };
            outputs.len()
        ];
        let selected = identities
            .iter()
            .enumerate()
            .filter_map(|(index, id)| selection.contains(id).then_some(index))
            .collect::<Vec<_>>();
        let theme = self.palette();
        let interaction = atlas_shell::home::album_result_strip_content(
            ui,
            Id::new(("text-results", host.0)),
            body,
            &images,
            &[],
            focus,
            shown,
            zoom,
            theme,
            1,
            None,
            &selected,
            |index, painter, rect| {
                atlas_shell::selection_tools::generator::paint_history_text_preview(
                    painter,
                    rect.shrink(3.0 * zoom),
                    &outputs[index].2,
                    3.0 * zoom,
                    theme,
                )
            },
        );
        if let Some(bounds) = interaction.painted_bounds {
            self.remember_result_strip(host, body, bounds, zoom);
        }
        if let Some(index) = interaction.clicked {
            self.select_generator_result(host, &identities, index, interaction.shift);
            self.choose_text_output_index(ui.ctx(), host, index);
            self.select_square_result(host);
        }
        self.open_result_context(ui, host, interaction);
    }

    pub(crate) fn paint_artifact_result_strip(
        &mut self,
        ui: &egui::Ui,
        host: NodeId,
        body: Rect,
        zoom: f32,
    ) {
        let Some(binding) = self
            .doc()
            .scene
            .node(host)
            .and_then(slate_doc::scene::artifact)
        else {
            return;
        };
        let revisions = self
            .result_outputs(host)
            .into_iter()
            .map(|output| output.id)
            .collect::<Vec<_>>();
        let focus = binding
            .revision
            .as_ref()
            .and_then(|revision| revisions.iter().position(|value| value == revision))
            .unwrap_or(0);
        let circles = self.shape_properties.ids == [host]
            && self.shape_properties.panel
                == Some(super::super::super::board_properties::Panel::Agent);
        if circles {
            return;
        }
        let selected = self.board_sel.len() == 1 && self.board_sel.contains(&host);
        let shown = selected
            || !self.result_selection(host).is_empty()
            || ui
                .ctx()
                .pointer_latest_pos()
                .is_some_and(|point| body.contains(point))
            || self.result_strip_hovered(ui.ctx(), host, body, zoom);
        let mut images = vec![
            atlas_shell::home::AlbumImage {
                texture: None,
                size: egui::Vec2::splat(96.0),
                enabled: true
            };
            revisions.len()
        ];
        for (index, _) in
            atlas_shell::home::generator_result_rects(body, revisions.len(), focus, zoom)
        {
            let source = self
                .artifact_reference(host, &revisions[index])
                .ok()
                .map(|reference| {
                    super::super::super::board_portal::source_locator(
                        self.tab().path.as_deref(),
                        &reference.materialized_entry,
                    )
                });
            images[index].texture =
                source.and_then(|source| self.web_history_thumbnail(host, &source));
        }
        let selection = self.result_selection(host);
        let selected = revisions
            .iter()
            .enumerate()
            .filter_map(|(index, revision)| selection.contains(revision).then_some(index))
            .collect::<Vec<_>>();
        let interaction = atlas_shell::home::album_result_strip(
            ui,
            Id::new(("artifact-results", host.0)),
            body,
            &images,
            &[],
            focus,
            shown,
            zoom,
            self.palette(),
            1,
            None,
            &selected,
        );
        if let Some(bounds) = interaction.painted_bounds {
            self.remember_result_strip(host, body, bounds, zoom);
        }
        self.handle_result_interaction(ui, host, interaction, body, true);
    }
}
