//! Ephemeral executable routing. Neither descriptor grants host authority.
use crate::{
    artifact_tools::ArtifactIdentity,
    collaborator::{
        host_tools::{deadline_after, remaining_time, HOST_TOOLS_FLAG},
        HostToolsLaunch,
    },
    AgentRequest,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct ArtifactToolsLaunch {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub link_dir: PathBuf,
    pub identity: ArtifactIdentity,
    pub deadline_unix_ms: u64,
    /// Parent-held monotonic end, captured once at admission, never serialized.
    pub expires: Instant,
}
impl ArtifactToolsLaunch {
    pub fn artifact(
        executable: PathBuf,
        link_dir: &Path,
        request: &AgentRequest,
    ) -> Result<Self, String> {
        let seconds = crate::collaborator::tool_task_budget_seconds(request);
        let admitted = Instant::now();
        Self::captured(
            executable,
            link_dir,
            request,
            deadline_after(seconds)?,
            admitted
                .checked_add(Duration::from_secs(seconds))
                .ok_or("Artifact deadline overflow")?,
        )
    }
    pub fn artifact_until(
        executable: PathBuf,
        link_dir: &Path,
        request: &AgentRequest,
        deadline_unix_ms: u64,
    ) -> Result<Self, String> {
        let expires = Instant::now()
            .checked_add(remaining_time(deadline_unix_ms)?)
            .ok_or("Artifact deadline overflow")?;
        Self::captured(executable, link_dir, request, deadline_unix_ms, expires)
    }
    fn captured(
        executable: PathBuf,
        link_dir: &Path,
        request: &AgentRequest,
        deadline_unix_ms: u64,
        expires: Instant,
    ) -> Result<Self, String> {
        let identity = ArtifactIdentity::from_request(request)?;
        let launch = Self {
            arguments: vec![
                HOST_TOOLS_FLAG.into(),
                Self::transport_dir_for(link_dir, &identity)
                    .to_string_lossy()
                    .into_owned(),
                serde_json::to_string(&identity).map_err(|e| e.to_string())?,
                request.id.clone(),
                "artifact".into(),
                deadline_unix_ms.to_string(),
            ],
            executable,
            link_dir: link_dir.to_path_buf(),
            identity,
            deadline_unix_ms,
            expires,
        };
        launch.validate_for(request)?;
        Ok(launch)
    }
    pub fn transport_dir(&self) -> PathBuf {
        Self::transport_dir_for(&self.link_dir, &self.identity)
    }
    fn transport_dir_for(link: &Path, identity: &ArtifactIdentity) -> PathBuf {
        link.join("artifact-tasks").join(&identity.run_id)
    }
    pub fn remaining_time(&self) -> Result<Duration, String> {
        let wall = remaining_time(self.deadline_unix_ms)?;
        let monotonic = self
            .expires
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or("The captured Artifact task time budget has expired")?;
        let ceiling = Duration::from_secs(self.identity.budget_seconds);
        if wall > ceiling || monotonic > ceiling {
            return Err("The captured Artifact deadline exceeds its validated task budget".into());
        }
        Ok(wall.min(monotonic))
    }
    pub fn validate_for(&self, request: &AgentRequest) -> Result<(), String> {
        let identity = ArtifactIdentity::from_request(request)?;
        if identity != self.identity
            || !self.executable.is_absolute()
            || !self.link_dir.is_absolute()
            || !request.policy.is_default()
            || request.collaborator.is_some()
            || request.goal.is_some()
            || request.image.is_some()
            || self.arguments
                != [
                    HOST_TOOLS_FLAG.to_owned(),
                    self.transport_dir().to_string_lossy().into_owned(),
                    serde_json::to_string(&identity).map_err(|e| e.to_string())?,
                    request.id.clone(),
                    "artifact".into(),
                    self.deadline_unix_ms.to_string(),
                ]
        {
            return Err("Artifact helper does not match this exact request/run".into());
        }
        self.remaining_time()?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum HelperLaunch {
    Model(HostToolsLaunch),
    Artifact(ArtifactToolsLaunch),
}
impl HelperLaunch {
    pub fn validate_for(&self, request: &AgentRequest) -> Result<(), String> {
        match self {
            Self::Model(model) => model.validate_for(request),
            Self::Artifact(artifact) => artifact.validate_for(request),
        }
    }
    pub fn executable(&self) -> &Path {
        match self {
            Self::Model(model) => &model.executable,
            Self::Artifact(artifact) => &artifact.executable,
        }
    }
    pub fn arguments(&self) -> &[String] {
        match self {
            Self::Model(model) => &model.arguments,
            Self::Artifact(artifact) => &artifact.arguments,
        }
    }
    pub fn request_id(&self) -> &str {
        match self {
            Self::Model(model) => &model.request_id,
            Self::Artifact(artifact) => &artifact.identity.request_id,
        }
    }
    pub fn remaining_time(&self) -> Result<Duration, String> {
        match self {
            Self::Model(model) => model.remaining_time(),
            Self::Artifact(artifact) => artifact.remaining_time(),
        }
    }
}
#[cfg(test)]
mod tests;
