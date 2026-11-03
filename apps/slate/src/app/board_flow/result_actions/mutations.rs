//! Output choice and presentation removal reuse the existing prepared source owners.
use super::*;
use slate_doc::scene::{HiddenOutputRef, SceneCmd};

impl SlateApp {
    pub(crate) fn active_result_reference(&self, host: NodeId) -> Option<HiddenOutputRef> {
        let node = self.doc().scene.node(host)?;
        let agent = slate_doc::agent_chat::agent(node)?;
        if let Some(artifact) = slate_doc::scene::artifact(node) {
            return Some(HiddenOutputRef::Artifact {
                family: artifact.family.clone(),
                revision: artifact.revision.clone()?,
            });
        }
        if let Some(ticket) = &agent.generator.shown_model {
            return Some(HiddenOutputRef::Model {
                ticket: ticket.clone(),
            });
        }
        if agent.generator.text_response_only {
            let review = agent
                .generator
                .text_reviews
                .get(agent.generator.shown_review.unwrap_or(0))?;
            return Some(HiddenOutputRef::Text {
                review_id: Some(review.id.clone()),
                response: review.output.response.clone(),
            });
        }
        if let Some(output) = &agent.generator.shown_text {
            return Some(HiddenOutputRef::Text {
                review_id: output.review_id.clone(),
                response: output.response.clone(),
            });
        }
        if matches!(&node.kind, NodeKind::Text(_)) || agent.view == PortalView::Text {
            return self
                .result_outputs(host)
                .last()
                .map(|output| output.reference.clone());
        }
        let active = self.agent_active_output(host)?;
        let images = self.agent_images(host);
        let image = images.iter().find(|image| image.id == active)?;
        Some(HiddenOutputRef::Image {
            request: image.request.clone(),
            id: image.id.clone(),
        })
    }

    fn prepare_output_choice(
        &self,
        host: NodeId,
        output: &HiddenOutputRef,
    ) -> Result<SceneCmd, String> {
        match output {
            HiddenOutputRef::Image { id, .. } => self.prepare_image_output_choice(host, id),
            HiddenOutputRef::Model { ticket } => self.prepare_registered_model_output(host, ticket),
            HiddenOutputRef::Text {
                review_id,
                response,
            } => self.prepare_text_output_review(host, response.through, review_id.as_deref()),
            HiddenOutputRef::Artifact { .. } => {
                Err("Artifact output choice is prepared by the existing revision worker".into())
            }
        }
    }

    pub(crate) fn prepare_result_output_mutation(
        &self,
        host: NodeId,
        output: &HiddenOutputRef,
        remove: bool,
    ) -> Result<SceneCmd, String> {
        match output {
            HiddenOutputRef::Model { ticket } if !Self::valid_model_output_identity(ticket) => {
                return Err("An existing bounded model output ID is required".into())
            }
            HiddenOutputRef::Text { response, .. }
                if response.through == 0 || response.session.is_empty() =>
            {
                return Err(
                    "A positive saved text checkpoint and its real session are required".into(),
                )
            }
            HiddenOutputRef::Image { id, .. } if id.is_empty() => {
                return Err("An existing stable image output ID is required".into())
            }
            HiddenOutputRef::Artifact { family, revision }
                if family.is_empty() || revision.is_empty() =>
            {
                return Err("An existing artifact family and revision are required".into())
            }
            _ => {}
        }
        let before = self
            .doc()
            .scene
            .node(host)
            .ok_or("Output host is missing")?;
        if self.tab().read_only || before.locked {
            return Err("Output host is read-only or locked".into());
        }
        let available = self.result_outputs(host);
        if !available.iter().any(|entry| &entry.reference == output) {
            return Err("The saved output identity or binding has changed".into());
        }
        if !remove {
            return self.prepare_output_choice(host, output);
        }
        if matches!(&before.kind, NodeKind::Portal(portal) if portal.kind == slate_doc::scene::PortalKind::Agent)
            && matches!(output, HiddenOutputRef::Image { .. })
        {
            return Err("Legacy albums keep their runtime image pick; removal has no atomic saved-source fallback".into());
        }
        if matches!(output, HiddenOutputRef::Image {request,..} if request.is_empty()) {
            return Err(
                "This legacy output has no immutable request origin; its saved source is retained"
                    .into(),
            );
        }
        let mut after = before.clone();
        if self.active_result_reference(host).as_ref() == Some(output) {
            if let Some(fallback) = available
                .iter()
                .rev()
                .find(|entry| &entry.reference != output)
            {
                let SceneCmd::Patch {
                    after: selected, ..
                } = self.prepare_output_choice(host, &fallback.reference)?
                else {
                    return Err("Output choice did not prepare a host patch".into());
                };
                after = *selected;
            }
            // Native compatibility: the last removed result leaves its current
            // authored/displayed source in place. JS Remove itself is simulated.
        }
        let agent =
            slate_doc::agent_chat::agent_mut(&mut after).ok_or("Output host has no generator")?;
        if !agent.generator.hidden_outputs.contains(output) {
            agent.generator.hidden_outputs.push(output.clone());
        }
        Ok(SceneCmd::Patch {
            before: Box::new(before.clone()),
            after: Box::new(after),
        })
    }

    pub(super) fn result_target_reference(
        &self,
        address: &ResultAddress,
    ) -> Option<HiddenOutputRef> {
        let target = address
            .target
            .as_ref()
            .or_else(|| address.outputs.first())?;
        self.result_outputs(address.host)
            .into_iter()
            .find(|entry| &entry.id == target)
            .map(|entry| entry.reference)
    }

    pub(super) fn remove_result_reason(&self, address: &ResultAddress) -> Option<&'static str> {
        if self.is_artifact_node(address.host) && self.artifact_is_busy(address.host) {
            return Some("Wait for the saved output operation to finish before removing a result.");
        }
        if self.doc().scene.node(address.host).is_some_and(|node| {
            matches!(&node.kind, NodeKind::Portal(portal)
            if portal.kind == slate_doc::scene::PortalKind::Agent)
        }) {
            return Some("Legacy album removal has no atomic saved-source fallback. Choosing results remains available.");
        }
        match self.result_target_reference(address) {
            Some(HiddenOutputRef::Image { request, .. }) if request.is_empty() => Some(
                "This legacy output has no immutable request origin. The saved source is retained.",
            ),
            None => Some("The saved output is no longer available."),
            _ => None,
        }
    }

    pub(super) fn apply_result_output_action(
        &mut self,
        address: ResultAddress,
        remove: bool,
    ) -> bool {
        if !self.result_address_valid(&address) {
            return false;
        }
        if self.tab().read_only
            || self
                .doc()
                .scene
                .node(address.host)
                .is_none_or(|node| node.locked)
        {
            return false;
        }
        let Some(reference) = self.result_target_reference(&address) else {
            return false;
        };
        if !remove
            && matches!(reference, HiddenOutputRef::Image { .. })
            && self.doc().scene.node(address.host).is_some_and(|node| {
                matches!(&node.kind, NodeKind::Portal(portal)
                if portal.kind == slate_doc::scene::PortalKind::Agent)
            })
        {
            let id = address.target.as_ref().or_else(|| address.outputs.first());
            if let Some(index) = self
                .agent_images(address.host)
                .iter()
                .position(|image| Some(&image.id) == id)
            {
                self.pick_agent_result(address.host, index);
                return true;
            }
            return false;
        }
        if !remove && self.active_result_reference(address.host).as_ref() == Some(&reference) {
            return true;
        }
        if let HiddenOutputRef::Artifact { family, revision } = &reference {
            return if remove {
                self.remove_artifact_result(address.host, family, revision)
            } else {
                self.artifact_pick_revision(address.host, revision)
            };
        }
        // Immutable publication references use the existing ready-link owner;
        // neither direct input nor targeted preparation stats a network source.
        let mut links = vec![reference.clone()];
        if remove && self.active_result_reference(address.host).as_ref() == Some(&reference) {
            if let Some(fallback) = self
                .result_outputs(address.host)
                .into_iter()
                .rev()
                .find(|entry| entry.reference != reference)
            {
                links.push(fallback.reference);
            }
        }
        for link in links {
            match link {
                HiddenOutputRef::Image { id, .. } => {
                    if let Some(image) = self
                        .agent_images(address.host)
                        .iter()
                        .find(|image| image.id == id)
                        .cloned()
                    {
                        let path = slate_doc::scene::resolve_source(
                            self.tab().path.as_deref(),
                            &image.source,
                        );
                        self.item_for_ready_path(&path, 0, 0);
                    }
                }
                HiddenOutputRef::Model { ticket } => {
                    if let Some(output) = self
                        .doc()
                        .scene
                        .node(address.host)
                        .and_then(slate_doc::agent_chat::agent)
                        .and_then(|agent| {
                            agent
                                .generator
                                .model_outputs
                                .iter()
                                .find(|output| output.id == ticket)
                        })
                        .cloned()
                    {
                        let path = slate_doc::scene::resolve_source(
                            self.tab().path.as_deref(),
                            &output.source.locator,
                        );
                        self.item_for_ready_path(&path, output.size, output.mtime);
                    }
                }
                _ => {}
            }
        }
        match self.prepare_result_output_mutation(address.host, &reference, remove) {
            Ok(command) => {
                let applied = self.commit_scene(vec![command]);
                if applied && matches!(reference, HiddenOutputRef::Model { .. }) {
                    self.finish_model_output_selection(address.host);
                }
                applied
            }
            Err(error) => {
                self.toast(error);
                false
            }
        }
    }

    pub(super) fn duplicate_active_result(&mut self, address: &ResultAddress) -> bool {
        if !self.result_address_valid(address) || self.refuse_read_only_edit() {
            return false;
        }
        let Some(host) = self
            .doc()
            .scene
            .node(address.host)
            .cloned()
            .filter(|node| !node.locked)
        else {
            return false;
        };
        let Some(active) = self.active_result_reference(address.host) else {
            return false;
        };
        if let HiddenOutputRef::Image { id, .. } = active {
            return !self
                .copy_agent_results_to_canvas(
                    address.host,
                    &[(id, Pos2::new(host.rect.x + host.rect.w + 24.0, host.rect.y))],
                )
                .is_empty();
        }
        let mut plain = host.clone();
        match &mut plain.kind {
            NodeKind::Image(image) => {
                image.agent = None;
            }
            NodeKind::Text(text) => {
                text.agent = None;
            }
            NodeKind::Portal(portal) => {
                portal.agent = None;
                if let Some(web) = &mut portal.web {
                    web.artifact = None;
                }
            }
            _ => return false,
        }
        plain.group = None;
        let node = self
            .doc_mut()
            .scene
            .build_duplicate(&plain, host.rect.w + 24.0, 0.0);
        !self.add_nodes(vec![node]).is_empty()
    }
}
