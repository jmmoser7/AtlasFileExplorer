//! Captured image request; graph/history snapshots are fixed at Send.
use super::*;
impl SlateApp {
    pub(crate) fn generation_request_with_prompt(
        &mut self,
        id: NodeId,
        live: bool,
        prompt: Option<&str>,
    ) -> Result<AgentRequest, String> {
        if let Some(error) = self.generator_view(id).error.clone() {
            return Err(error);
        }
        let image_history = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|a| a.generator.preserve_history);
        let prompt = prompt.map(str::to_owned).unwrap_or_else(|| {
            if image_history.is_some() {
                let authored = self
                    .doc()
                    .scene
                    .node(id)
                    .and_then(slate_doc::agent_chat::agent)
                    .map(|a| a.instruction.trim().to_owned())
                    .unwrap_or_default();
                if authored.is_empty() {
                    self.agents.prompt_mut(id).trim().to_owned()
                } else {
                    authored
                }
            } else {
                self.generator_prompt(id)
            }
        });
        if prompt.is_empty() {
            return Err("Connect a note with a prompt to this generator.".into());
        }
        let inputs = self.generator_input_refs(id, !live)?;
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
                        image.count = a.generator.count.unwrap_or(image.count).max(1);
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
