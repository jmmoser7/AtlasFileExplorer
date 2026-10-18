use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireFrame {
    Hello { hello: Hello },
    Welcome { welcome: Welcome },
    Request { request: RequestEnvelope },
    Reply { reply: ReplyEnvelope },
    Error { error: ProtocolError },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RequestEnvelope {
    pub session_id: String,
    pub generation: u64,
    pub request_id: String,
    pub caller: CallerAuthority,
    #[serde(default)]
    pub source: Option<SourceScope>,
    pub operation: Operation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Operation {
    BindSource {
        source: SourceRef,
        mode: SourceMode,
        selection_id: String,
    },
    UnbindSource,
    DescribeCommands,
    InvokeCommand {
        command_id: String,
        parameters: serde_json::Value,
    },
    Surface {
        action: SurfaceAction,
    },
    DescribeSurfaces,
    DescribeViewSchemas,
    SnapshotView {
        schema: ViewSchema,
    },
    RestoreView {
        view: ViewSnapshot,
    },
    Capture {
        schema: ViewSchema,
    },
    Publish,
    Detach {
        policy: DetachPolicy,
    },
    Cancel {
        request_id: String,
    },
    OperationStatus {
        request_id: String,
    },
}

impl Operation {
    pub fn capability(&self) -> CapabilityGroup {
        match self {
            Self::BindSource { .. } | Self::UnbindSource => CapabilityGroup::Sources,
            Self::DescribeCommands | Self::InvokeCommand { .. } => CapabilityGroup::Commands,
            Self::Surface { .. } | Self::DescribeSurfaces => CapabilityGroup::Surfaces,
            Self::DescribeViewSchemas | Self::SnapshotView { .. } | Self::RestoreView { .. } => {
                CapabilityGroup::Views
            }
            Self::Capture { .. } => CapabilityGroup::Capture,
            Self::Publish => CapabilityGroup::Publication,
            Self::Detach { .. } | Self::Cancel { .. } | Self::OperationStatus { .. } => {
                CapabilityGroup::Lifecycle
            }
        }
    }

    pub fn needs_source(&self) -> bool {
        matches!(
            self,
            Self::UnbindSource
                | Self::InvokeCommand { .. }
                | Self::Surface { .. }
                | Self::DescribeSurfaces
                | Self::SnapshotView { .. }
                | Self::RestoreView { .. }
                | Self::Capture { .. }
                | Self::Publish
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReplyEnvelope {
    pub session_id: String,
    pub generation: u64,
    pub request_id: String,
    #[serde(default)]
    pub source: Option<SourceScope>,
    pub outcome: ReplyBody,
}

impl ReplyEnvelope {
    pub fn failure(request: &RequestEnvelope, error: ProtocolError) -> Self {
        Self {
            session_id: request.session_id.clone(),
            generation: request.generation,
            request_id: request.request_id.clone(),
            source: request.source.clone(),
            outcome: ReplyBody::Failure { error },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReplyBody {
    Ack,
    SourceBound {
        binding: SourceBinding,
    },
    Commands {
        commands: Vec<CommandDescriptor>,
    },
    Surfaces {
        surfaces: Vec<SurfaceDescriptor>,
    },
    CommandResult {
        value: serde_json::Value,
        #[serde(default)]
        revision: Option<String>,
    },
    ViewSchemas {
        schemas: Vec<ViewSchema>,
    },
    View {
        view: ViewSnapshot,
    },
    Captured {
        capture: CaptureResult,
    },
    Published {
        publication: PublicationResult,
    },
    /// Stopping is requested only; committed effects are never claimed rolled back.
    CancellationRequested {
        target_request_id: String,
        rollback: bool,
    },
    OperationState {
        target_request_id: String,
        state: OperationState,
    },
    Detached {
        preserved_unsaved: bool,
        application_terminated: bool,
    },
    Failure {
        error: ProtocolError,
    },
    OutcomeUnknown {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Pending,
    CancellationRequested,
    Completed,
    OutcomeUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    IncompatibleProtocol,
    RequiredCapabilityMissing,
    InvalidDescriptor,
    InvalidRequest,
    UnsupportedOperation,
    UnsupportedSchema,
    SessionMismatch,
    StaleGeneration,
    SourceNotBound,
    SourceMismatch,
    StaleRevision,
    AuthorityDenied,
    RequestIdConflict,
    RequestPending,
    RequestCacheFull,
    RequestNotFound,
    OutcomeUnknown,
    CancellationUnsupported,
    DirtyDocument,
    Detached,
    InvalidResult,
    TransferTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProtocolError {
    pub code: ErrorCode,
    pub message: String,
}

impl ProtocolError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for ProtocolError {}
