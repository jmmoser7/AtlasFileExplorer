use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Stable, portable locator. A detected root requires its relative form.
/// Resolving paths and checking filesystem access remain adapter responsibilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceLocator {
    #[serde(default)]
    pub root_id: Option<String>,
    #[serde(default)]
    pub relative: Option<String>,
    #[serde(default)]
    pub absolute: Option<String>,
}

impl SourceLocator {
    pub fn preferred(&self) -> Option<&str> {
        self.relative.as_deref().or(self.absolute.as_deref())
    }
}

/// Revisions are opaque equality tokens, never assumed ordered or content hashes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceRef {
    pub source_id: String,
    pub locator: SourceLocator,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceScope {
    pub source_id: String,
    pub revision: String,
}

impl SourceRef {
    pub fn scope(&self) -> SourceScope {
        SourceScope {
            source_id: self.source_id.clone(),
            revision: self.revision.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    Linked,
    IsolatedDraft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceBinding {
    pub source: SourceRef,
    /// Ephemeral native document identity; never serialize into a workbook.
    pub document_id: String,
    pub mode: SourceMode,
    pub dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceMode {
    NativeFullUi,
    NativeViewport,
    CompanionWindow,
    Headless,
}

pub const MAX_SESSION_SURFACES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SurfaceGeometry {
    /// Parent client coordinates in physical pixels (not logical canvas units).
    pub x_px: i32,
    pub y_px: i32,
    pub width_px: u32,
    pub height_px: u32,
    pub dpi: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProcessOwnership {
    AttachedExisting,
    LaunchedDedicated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SurfaceDescriptor {
    pub surface_id: String,
    pub mode: SurfaceMode,
    pub process_ownership: ProcessOwnership,
    /// Decimal OS window handle for native modes; session-local, never durable.
    #[serde(default)]
    pub native_handle: Option<String>,
    pub can_detach: bool,
}

impl SurfaceDescriptor {
    /// Pure descriptor validation. Native runtimes must additionally verify the
    /// handle belongs to the authenticated peer before hosting or focusing it.
    pub fn validate(&self, offered_modes: &[SurfaceMode]) -> Result<(), crate::ProtocolError> {
        crate::validation::validate_surface_descriptor(self, offered_modes)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SurfaceAction {
    Attach { surface: SurfaceDescriptor },
    Place { geometry: SurfaceGeometry },
    Show,
    Hide,
    Focus,
    Blur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DetachPolicy {
    /// Return the surface to its owner; unsaved material stays open.
    Preserve,
    /// A close request is refused while dirty. No implicit save/discard exists.
    CloseIfClean,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallerAuthority {
    Human {
        /// Trusted host-issued action receipt, scoped to this request ID.
        action_id: String,
        /// Required for a source/draft mutation, naming the specific target.
        #[serde(default)]
        target_source: Option<String>,
    },
    Agent {
        agent_id: String,
        /// v0.2 draft edits additionally require a host-local scoped grant.
        /// This field alone never grants mutation authority.
        #[serde(default)]
        workspace_autonomy: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommandEffect {
    Inspect,
    EphemeralView,
    SlateProposal,
    DraftMutation,
    SourceWriteBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UndoOwner {
    None,
    NativeApplication,
    SlateJournal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommandDescriptor {
    /// Application-namespaced, closed operation identifier, never script text.
    pub id: String,
    pub title: String,
    pub effect: CommandEffect,
    pub undo: UndoOwner,
    pub cancellable: bool,
    /// Offer only; host policy and scoped authority independently admit calls.
    pub agent_allowed: bool,
    /// Bounded JSON Schema dialect enforced by validate_parameters.
    pub parameters: serde_json::Value,
}
