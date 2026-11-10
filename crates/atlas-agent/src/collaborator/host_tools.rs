//! Ephemeral host launch data. Routing identities never confer native authority.
use super::BoardLinkSession;
use crate::AgentRequest;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const HOST_TOOLS_FLAG: &str = "--slate-host-tools";

#[derive(Clone, Debug)]
pub struct HostToolsLaunch {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub image_workspace: PathBuf,
    pub session: BoardLinkSession,
    pub request_id: String,
    /// Advisory copy of the host's once-captured deadline; never authority.
    pub deadline_unix_ms: u64,
}

impl HostToolsLaunch {
    pub fn model_task(
        executable: PathBuf,
        root: &Path,
        image_workspace: &Path,
        request: &AgentRequest,
    ) -> Result<Self, String> {
        let deadline = deadline_after(super::model_task_budget_seconds(request))?;
        Self::model_task_until(executable, root, image_workspace, request, deadline)
    }

    pub fn model_task_until(
        executable: PathBuf,
        root: &Path,
        image_workspace: &Path,
        request: &AgentRequest,
        deadline_unix_ms: u64,
    ) -> Result<Self, String> {
        let session = request
            .collaborator
            .clone()
            .ok_or("No explicit model task")?;
        if !session.is_model_task()
            || !root.is_absolute()
            || !image_workspace.is_absolute()
            || !request.policy.is_default()
            || request.artifact.is_some()
            || request.image.is_some()
            || request.goal.is_some()
        {
            return Err(
                "Host tools require an explicit default-policy model task and absolute link".into(),
            );
        }
        let launch = Self {
            executable,
            arguments: vec![
                HOST_TOOLS_FLAG.into(),
                root.to_string_lossy().into_owned(),
                serde_json::to_string(&session).map_err(|e| e.to_string())?,
                request.id.clone(),
                image_workspace.to_string_lossy().into_owned(),
                deadline_unix_ms.to_string(),
            ],
            image_workspace: image_workspace.to_path_buf(),
            session,
            request_id: request.id.clone(),
            deadline_unix_ms,
        };
        launch.validate_for(request)?;
        Ok(launch)
    }

    pub fn validate_for(&self, request: &AgentRequest) -> Result<(), String> {
        self.session.validate().map_err(str::to_owned)?;
        if !self.executable.is_absolute()
            || self.arguments.len() != 6
            || self.arguments[0] != HOST_TOOLS_FLAG
            || !Path::new(&self.arguments[1]).is_absolute()
            || serde_json::from_str::<BoardLinkSession>(&self.arguments[2])
                .ok()
                .as_ref()
                != Some(&self.session)
            || self.arguments[3] != self.request_id
            || !self.image_workspace.is_absolute()
            || Path::new(&self.arguments[4]) != self.image_workspace.as_path()
            || self.arguments[5] != self.deadline_unix_ms.to_string()
            || request.collaborator.as_ref() != Some(&self.session)
            || self.request_id != request.id
            || !super::valid_id(&self.request_id)
            || !self.session.is_model_task()
            || !request.policy.is_default()
            || request.artifact.is_some()
            || request.image.is_some()
            || request.goal.is_some()
        {
            return Err("Host tools do not match this exact model task request".into());
        }
        Ok(())
    }

    pub fn remaining_time(&self) -> Result<Duration, String> {
        remaining_time(self.deadline_unix_ms)
    }
}

pub fn deadline_after(seconds: u64) -> Result<u64, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    u64::try_from(now.as_millis())
        .ok()
        .and_then(|ms| ms.checked_add(seconds.checked_mul(1000)?))
        .ok_or_else(|| "Task deadline overflow".into())
}

pub fn remaining_time(deadline_unix_ms: u64) -> Result<Duration, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let remaining = deadline_unix_ms
        .checked_sub(u64::try_from(now.as_millis()).map_err(|e| e.to_string())?)
        .filter(|ms| *ms > 0)
        .ok_or("The host task deadline has expired")?;
    if remaining > 1_800_000 {
        return Err("The captured host deadline exceeds the 1800-second tool-task ceiling".into());
    }
    Ok(Duration::from_millis(remaining))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helper_descriptor_is_bound_to_the_exact_request_and_route() {
        let root = std::env::temp_dir();
        let mut request = AgentRequest {
            id: "request-test".into(),
            collaborator: Some(BoardLinkSession {
                session_id: "model-task-test".into(),
                grant_id: "grant-test".into(),
                workspace_id: "workspace-test".into(),
            }),
            ..Default::default()
        };
        let launch =
            HostToolsLaunch::model_task(root.join("slate.exe"), &root, &root, &request).unwrap();
        assert!(launch.validate_for(&request).is_ok());
        let deadline = deadline_after(42).unwrap();
        let captured = HostToolsLaunch::model_task_until(
            root.join("slate.exe"),
            &root,
            &root,
            &request,
            deadline,
        )
        .unwrap();
        assert_eq!(captured.deadline_unix_ms, deadline);
        assert_eq!(captured.arguments[5], deadline.to_string());
        let remaining = captured.remaining_time().unwrap();
        assert!(remaining <= Duration::from_secs(42));
        assert!(remaining > Duration::from_secs(40));
        let mut mismatched = captured.clone();
        mismatched.deadline_unix_ms += 1;
        assert!(mismatched.validate_for(&request).is_err());
        assert!(remaining_time(0).is_err());
        assert!(remaining_time(u64::MAX).is_err());
        request.id = "request-other".into();
        assert!(launch.validate_for(&request).is_err());
        request.id = launch.request_id.clone();
        request.collaborator.as_mut().unwrap().grant_id = "grant-other".into();
        assert!(launch.validate_for(&request).is_err());
        request.collaborator = Some(launch.session.clone());
        request.policy = crate::TurnPolicy::ReadOnly;
        assert!(launch.validate_for(&request).is_err());
    }
}
