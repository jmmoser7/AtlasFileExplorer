use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::SourceScope;

pub const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_CAPTURE_DIMENSION: u32 = 16_384;
pub const MAX_VIEW_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewFidelity {
    ExactNative,
    SharedApproximation,
    PosterOnly,
}

/// Native state is opaque data scoped to a reverse-domain application namespace;
/// v0 deliberately defines no portable 3D camera until the camera owner exports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ViewSnapshot {
    pub source: SourceScope,
    pub schema: String,
    pub schema_version: u32,
    pub fidelity: ViewFidelity,
    pub state: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ViewSchema {
    pub namespace: String,
    pub version: u32,
}

/// Transfer IDs resolve only through the authenticated runtime's bounded store.
/// Arbitrary filesystem paths and remote URLs are deliberately not wire options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactTransfer {
    pub transfer_id: String,
    pub byte_length: u64,
    pub media_type: String,
    /// Lowercase SHA-256 hex of the transferred bytes, checked by the runtime.
    pub sha256: String,
}

impl ArtifactTransfer {
    pub fn validate(&self) -> Result<(), crate::ProtocolError> {
        crate::validation::validate_artifact(self)
    }
}

/// Terminal binary-pipe marker, sent after exactly `byte_length` payload bytes.
/// This strict transfer handshake deliberately rejects unknown fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCompletion {
    pub transfer_id: String,
    pub complete: bool,
}

impl ArtifactCompletion {
    pub fn new(descriptor: &ArtifactTransfer) -> Self {
        Self {
            transfer_id: descriptor.transfer_id.clone(),
            complete: true,
        }
    }
}

/// Receiver acknowledgement. Construct only after verifying complete length and
/// SHA-256; the sender compares it to the descriptor before reporting success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReceipt {
    pub transfer_id: String,
    pub sha256: String,
    pub verified: bool,
}

impl ArtifactReceipt {
    pub fn new(descriptor: &ArtifactTransfer) -> Self {
        Self {
            transfer_id: descriptor.transfer_id.clone(),
            sha256: descriptor.sha256.clone(),
            verified: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaptureConsistency {
    /// Adapter guarantees source, view and pixels describe the same revision.
    Atomic,
    /// Adapter cannot guarantee atomicity; publication must retain this fact.
    BestEffort,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CaptureMetadata {
    pub source: SourceScope,
    pub view: ViewSnapshot,
    /// RFC 3339 UTC time supplied by the adapter; no ordering assumption.
    pub captured_at: String,
    pub width_px: u32,
    pub height_px: u32,
    pub consistency: CaptureConsistency,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CaptureResult {
    pub metadata: CaptureMetadata,
    pub artifact: ArtifactTransfer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicationBehavior {
    PosterAndPointer,
    PortableArtifact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PublicationResult {
    pub source: SourceScope,
    pub behavior: PublicationBehavior,
    pub artifact: ArtifactTransfer,
}
