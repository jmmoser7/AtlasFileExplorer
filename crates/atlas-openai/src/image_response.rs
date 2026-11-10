//! Persist the actual response before reporting an incomplete image batch.
use super::*;

impl Client {
    pub(super) fn accept_images(
        &self,
        request: &AgentRequest,
        session: &mut AgentSession,
        response: &Value,
        mut changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        let mut returned = 0;
        let mut failures = Vec::new();
        std::fs::create_dir_all(&self.output_dir).map_err(|e| e.to_string())?;
        let model = request
            .model
            .clone()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.into());
        for (index, item) in response["data"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let bytes = match decode_image(item) {
                Ok(bytes) if !bytes.is_empty() => bytes,
                Ok(_) => {
                    failures.push(format!("Image {} had empty data", index + 1));
                    continue;
                }
                Err(error) => {
                    failures.push(format!("Image {} could not be decoded: {error}", index + 1));
                    continue;
                }
            };
            let id = format!("{}-{index}", request.id);
            let dest = self.output_dir.join(format!("{id}.png"));
            std::fs::write(&dest, bytes).map_err(|e| e.to_string())?;
            session.bundle.images.push(ImageOutput {
                id,
                source: dest.to_string_lossy().into_owned(),
                request: request.id.clone(),
                prompt: request.prompt.clone(),
                model: model.clone(),
                task: if pictures(request).0.is_some() {
                    "vary"
                } else {
                    "generate"
                }
                .into(),
                live: String::new(),
            });
            returned += 1;
            changed(session);
        }
        let count_error = request
            .image
            .clone()
            .unwrap_or_default()
            .validate_output_count("OpenAI", returned)
            .err();
        if count_error.is_some() || !failures.is_empty() {
            let mut error = count_error.unwrap_or_else(|| {
                "OpenAI returned an invalid image response. Valid images were kept.".into()
            });
            if !failures.is_empty() {
                error.push_str(&format!(" {}.", failures.join(". ")));
            }
            session.status = AgentStatus::Error(error.clone());
            changed(session);
            return Err(error);
        }
        session.status = AgentStatus::Idle;
        changed(session);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_invalid_response_keeps_valid_images_before_and_after_the_bad_item() {
        let dir = std::env::temp_dir().join(atlas_agent::request_id());
        let client = Client::new("unused".into(), dir.clone());
        let request: AgentRequest = serde_json::from_value(json!({
            "id":"mixed", "at":0, "prompt":"pavilion", "image":{"count":3}
        }))
        .unwrap();
        for invalid in [
            json!({"b64_json":"not base64!"}),
            json!({}),
            json!({"b64_json":""}),
        ] {
            let response =
                json!({"data":[{"b64_json":"iVBORw0KGgo="}, invalid, {"b64_json":"iVBORw0KGgo="}]});
            let mut session = AgentSession {
                status: AgentStatus::Thinking,
                ..Default::default()
            };
            let error = client
                .accept_images(&request, &mut session, &response, |_| {})
                .unwrap_err();
            assert_eq!(session.bundle.images.len(), 2);
            assert_eq!(session.bundle.images[0].id, "mixed-0");
            assert_eq!(session.bundle.images[1].id, "mixed-2");
            assert!(session
                .bundle
                .images
                .iter()
                .all(|image| Path::new(&image.source).is_file()));
            assert!(error.contains("returned 2 images; 3 were requested"));
            assert!(error.contains("Image 2"));
            assert!(matches!(session.status, AgentStatus::Error(_)));
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn actual_response_keeps_every_image_and_only_completes_the_requested_count() {
        for returned in [0, 1, 3, 4] {
            let dir = std::env::temp_dir().join(atlas_agent::request_id());
            let client = Client::new("unused".into(), dir.clone());
            let request: AgentRequest = serde_json::from_value(json!({
                "id":"batch", "at":0, "prompt":"pavilion", "image":{"count":3}
            }))
            .unwrap();
            let response = json!({"data": (0..returned).map(|_| json!({"b64_json":"iVBORw0KGgo="})).collect::<Vec<_>>()});
            let mut session = AgentSession {
                status: AgentStatus::Thinking,
                ..Default::default()
            };
            let mut statuses = Vec::new();
            let result = client.accept_images(&request, &mut session, &response, |s| {
                statuses.push(s.status.clone())
            });
            assert_eq!(result.is_ok(), returned == 3);
            assert_eq!(session.bundle.images.len(), returned);
            for (index, image) in session.bundle.images.iter().enumerate() {
                assert_eq!(image.id, format!("batch-{index}"));
                assert_eq!(image.request, "batch");
                assert_eq!(std::fs::read(&image.source).unwrap(), b"\x89PNG\r\n\x1a\n");
            }
            if returned == 3 {
                assert_eq!(session.status, AgentStatus::Idle);
            } else {
                assert!(!statuses.contains(&AgentStatus::Idle));
                assert!(result
                    .unwrap_err()
                    .contains(&format!("returned {returned} images; 3 were requested")));
            }
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
