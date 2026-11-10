//! Typed Artifact file operations. Routing is never filesystem authority.
use crate::{collaborator::valid_id, AgentRequest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub const MAX_WRITE_BYTES: usize = 32 * 1024;
pub mod transport;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdentity {
    pub request_id: String,
    pub run_id: String,
    pub family_id: String,
    /// Exact prepared request correlation; the live parent scope is authority.
    pub request_fingerprint: String,
    pub budget_seconds: u64,
}
impl ArtifactIdentity {
    pub fn from_request(request: &AgentRequest) -> Result<Self, String> {
        request.validate_artifact_request()?;
        let run = request
            .artifact
            .as_ref()
            .ok_or("No explicit Artifact run")?;
        let identity = Self {
            request_id: request.id.clone(),
            run_id: run.run_id.clone(),
            family_id: run.family_id.clone(),
            budget_seconds: crate::collaborator::tool_task_budget_seconds(request),
            request_fingerprint: {
                let mut hash = Sha256::new();
                hash.update(b"slate-prepared-artifact-request-v1\0");
                // AgentRequest contains ordered vectors and BTreeMaps. Hash the
                // exact immutable submission, including model/history/inputs.
                hash.update(serde_json::to_vec(request).map_err(|error| error.to_string())?);
                format!("{:x}", hash.finalize())
            },
        };
        identity.validate()?;
        Ok(identity)
    }
    pub fn validate(&self) -> Result<(), String> {
        if [&self.request_id, &self.run_id, &self.family_id]
            .iter()
            .all(|id| valid_id(id))
            && matches!(self.budget_seconds, 600 | 1800)
            && self.request_fingerprint.len() == 64
            && self
                .request_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            Ok(())
        } else {
            Err("Invalid Artifact request identity".into())
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactOperation {
    ReadContext,
    ReadFile { path: String },
    WriteFile { path: String, content: String },
}
impl ArtifactOperation {
    pub fn validate(&self) -> Result<(), String> {
        let path = match self {
            Self::ReadContext => return Ok(()),
            Self::ReadFile { path } => path,
            Self::WriteFile { path, content } => {
                if content.len() > MAX_WRITE_BYTES {
                    return Err("Artifact write exceeds the 32 KiB UTF-8 limit".into());
                }
                path
            }
        };
        if path.is_empty() || path.len() > 4096 || path.contains('\0') {
            return Err("Invalid Artifact file path".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCall {
    pub id: String,
    pub identity: ArtifactIdentity,
    pub sequence: u64,
    pub operation: ArtifactOperation,
}
impl ArtifactCall {
    pub fn validate(&self) -> Result<(), String> {
        self.identity.validate()?;
        if !valid_id(&self.id) {
            return Err("Invalid Artifact call identity".into());
        }
        self.operation.validate()?;
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() as u64
            > crate::collaborator::MAX_BOARD_MESSAGE_BYTES
        {
            return Err("Encoded Artifact call exceeds the 64 KiB message limit".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactImage {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFileReply {
    pub result: Value,
    pub image: Option<ArtifactImage>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReply {
    pub id: String,
    pub identity: ArtifactIdentity,
    pub success: bool,
    pub result: ArtifactFileReply,
}
impl ArtifactReply {
    pub fn matches(&self, call: &ArtifactCall) -> bool {
        self.id == call.id && self.identity == call.identity
    }
    pub fn error(call: &ArtifactCall, error: &str) -> Self {
        Self {
            id: call.id.clone(),
            identity: call.identity.clone(),
            success: false,
            result: ArtifactFileReply {
                result: serde_json::json!({"error":error}),
                image: None,
            },
        }
    }
}
