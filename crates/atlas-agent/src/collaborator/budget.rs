//! A request budget is selected only after the host validates model parameters.
use crate::AgentRequest;

pub const DEFAULT_TOOL_TASK_SECONDS: u64 = 600;
pub const MAX_EFFORT_TOOL_TASK_SECONDS: u64 = 1800;

/// The caller must validate the immutable model/parameter choice against its
/// provider catalog before admission. This does not create or extend a grant.
pub fn tool_task_budget_seconds(request: &AgentRequest) -> u64 {
    let model = request
        .collaborator
        .as_ref()
        .is_some_and(|route| route.is_model_task() && route.validate().is_ok())
        && request.artifact.is_none();
    let artifact = request.artifact.is_some()
        && request.collaborator.is_none()
        && request.validate_artifact_request().is_ok();
    if (model || artifact)
        && request.policy.is_default()
        && request.image.is_none()
        && request.goal.is_none()
        && request.model_parameters.get("effort").map(String::as_str) == Some("max")
    {
        MAX_EFFORT_TOOL_TASK_SECONDS
    } else {
        DEFAULT_TOOL_TASK_SECONDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collaborator::BoardLinkSession;
    #[test]
    fn only_an_explicit_default_model_task_with_max_effort_gets_thirty_minutes() {
        let mut request = AgentRequest::default();
        request
            .model_parameters
            .insert("effort".into(), "max".into());
        assert_eq!(tool_task_budget_seconds(&request), 600);
        request.collaborator = Some(BoardLinkSession {
            session_id: "model-task-budget".into(),
            grant_id: "grant".into(),
            workspace_id: "workspace".into(),
        });
        assert_eq!(tool_task_budget_seconds(&request), 1800);
        request
            .model_parameters
            .insert("effort".into(), "high".into());
        assert_eq!(tool_task_budget_seconds(&request), 600);
        request
            .model_parameters
            .insert("effort".into(), "MAX".into());
        assert_eq!(tool_task_budget_seconds(&request), 600);
        request
            .model_parameters
            .insert("effort".into(), "max".into());
        request.policy = crate::TurnPolicy::ReadOnly;
        assert_eq!(tool_task_budget_seconds(&request), 600);
    }
    #[test]
    fn artifact_uses_the_same_explicit_max_policy_and_invalid_shapes_do_not_extend() {
        let root = std::env::temp_dir();
        let mut request = AgentRequest {
            id: "artifact-budget".into(),
            artifact: Some(crate::ArtifactRun {
                family_id: "family".into(),
                run_id: "artifact-budget".into(),
                source_root: root.clone(),
                parent: None,
                staging_output: root.join("output"),
            }),
            output_dir: Some(root.join("output").to_string_lossy().into_owned()),
            ..Default::default()
        };
        assert_eq!(tool_task_budget_seconds(&request), 600);
        request
            .model_parameters
            .insert("effort".into(), "max".into());
        assert_eq!(tool_task_budget_seconds(&request), 1800);
        request.oneshot = true;
        assert_eq!(tool_task_budget_seconds(&request), 600);
        request.oneshot = false;
        request.policy = crate::TurnPolicy::ReadOnly;
        assert_eq!(tool_task_budget_seconds(&request), 600);
        request.policy = crate::TurnPolicy::Default;
        request.output_dir = None;
        assert_eq!(tool_task_budget_seconds(&request), 600);
    }
}
