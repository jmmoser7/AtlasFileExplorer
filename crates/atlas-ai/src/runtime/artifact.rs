use atlas_agent::AgentRequest;

/// Submit only prepared, owned artifact drafts to installed coding harnesses.
/// The history owner performs filesystem validation; call from a worker before
/// either an in-process Codex submission or a Cursor request.json write.
pub fn validate_artifact_submission(request: &AgentRequest, provider: &str) -> Result<(), String> {
    request.validate_artifact_request()?;
    let Some(run) = &request.artifact else {
        return Ok(());
    };
    if !matches!(provider, "codex" | "cursor") {
        return Err("Artifact generation requires an installed Cursor or Codex harness.".into());
    }
    crate::versions::validate_prepared_artifact_run(run)
        .map_err(|e| format!("Artifact staging is unavailable: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact_request() -> AgentRequest {
        let root = std::env::temp_dir().join("artifact-submission-test");
        let run = atlas_agent::ArtifactRun {
            family_id: "family".into(),
            run_id: "run".into(),
            source_root: root.clone(),
            parent: None,
            staging_output: root.join(".drafts/run/output"),
        };
        AgentRequest {
            id: run.run_id.clone(),
            output_dir: Some(run.staging_output.to_string_lossy().into_owned()),
            artifact: Some(run),
            ..Default::default()
        }
    }

    #[test]
    fn ordinary_chat_retains_every_existing_provider_path() {
        for provider in ["cursor", "codex", "ollama", "openai-text", "external"] {
            assert!(validate_artifact_submission(&AgentRequest::default(), provider).is_ok());
        }
    }

    #[test]
    fn artifact_mode_requires_a_coding_harness_and_an_owned_draft() {
        let request = artifact_request();
        assert!(validate_artifact_submission(&request, "ollama")
            .unwrap_err()
            .contains("installed Cursor or Codex"));
        assert!(validate_artifact_submission(&request, "codex")
            .unwrap_err()
            .contains("Artifact staging is unavailable"));
        let mut request = request;
        request.output_dir = Some(std::env::temp_dir().to_string_lossy().into_owned());
        assert_eq!(
            validate_artifact_submission(&request, "cursor").unwrap_err(),
            "Artifact output must be its owned staging directory."
        );
    }
}
