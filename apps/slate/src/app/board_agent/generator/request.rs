//! Captured image request; graph/history snapshots are fixed at Send.
use super::*;
impl SlateApp {
    pub(crate) fn generation_bundle_manifest(&self, dir: &std::path::Path) -> slate_doc::SourceUri {
        slate_doc::SourceUri {
            locator: super::super::super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &dir.join("session.json"),
            ),
        }
    }

    pub(in crate::app::board_agent) fn image_generation_binding(
        &self,
        id: NodeId,
    ) -> Result<(String, String), String> {
        self.agent_session_for(id)
            .filter(|(session, provider)| {
                !session.is_empty() && atlas_ai::agent::local_image_engine(provider)
            })
            .ok_or_else(|| "Choose an image provider before running this generator.".into())
    }

    pub(crate) fn generation_request_with_prompt(
        &mut self,
        id: NodeId,
        live: bool,
        prompt: Option<&str>,
    ) -> Result<AgentRequest, String> {
        self.image_generation_binding(id)?;
        if let Some(error) = self.generator_view(id).error.clone() {
            return Err(error);
        }
        let image_count = self
            .image_agent_count(id)
            .ok_or("Image generator is missing.")?;
        let image_history = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|a| a.generator.preserve_history);
        let inputs = self.generator_input_refs(id, !live)?;
        let dynamic = self.has_dynamic_prompt_inputs(&inputs);
        let prompt_inputs_projected = prompt.is_none();
        let pending_prompt_projection =
            (dynamic && prompt.is_none()).then(|| atlas_agent::PromptProjection {
                authored: self.authored_agent_prompt_text(id).to_owned(),
                separator: ", ".into(),
                static_instructions: self.image_non_prompt_instructions(&inputs),
            });
        let prompt = prompt
            .map(str::to_owned)
            .unwrap_or_else(|| self.generator_prompt(id));
        if prompt.is_empty() && !dynamic {
            return Err("Connect a note with a prompt to this generator.".into());
        }
        let (model, model_parameters, session, settings) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| {
                (
                    a.model.clone(),
                    a.model_parameters.clone(),
                    a.session.clone(),
                    {
                        let mut image = a.image;
                        image.count = image_count;
                        image
                    },
                )
            })
            .unwrap_or_default();
        let live_run = live.then(|| {
            self.agents
                .live_run
                .entry(id)
                .or_insert_with(atlas_ai::agent::request_id)
                .clone()
        });
        // Live repeats one seed per generator; otherwise a locked seed repeats.
        let image = Some(atlas_ai::agent::ImageParams {
            seed: if live {
                Some(settings.seed.unwrap_or_else(|| stable_seed(&session)))
            } else {
                settings.seed
            },
            live: live_run,
            aspect: settings.aspect,
            count: settings.count,
        });
        let mut request = AgentRequest {
            id: atlas_ai::agent::request_id(),
            prompt,
            prompt_inputs_projected,
            pending_prompt_projection,
            model,
            model_parameters,
            at: atlas_ai::context::now_secs(),
            inputs,
            history: vec![],
            image,
            output_dir: None,
            oneshot: false,
            ..Default::default()
        };
        self.capture_image_history_request(id, &mut request, image_history)?;
        Ok(request)
    }
}
