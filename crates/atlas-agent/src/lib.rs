//! Renderer- and provider-independent contracts for agent sidecars.
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentContext {
    pub app: &'static str,
    pub session: String,
    pub provider: String,
    pub workbook: Option<PathBuf>,
    pub format_version: u32,
    pub scope: String,
    pub selection: Vec<String>,
    pub viewport: Option<Viewport>,
    pub board_summary: String,
    pub generated_at: u64,
}

impl AgentContext {
    pub fn fingerprint(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.app.hash(&mut h);
        self.session.hash(&mut h);
        self.provider.hash(&mut h);
        self.workbook.hash(&mut h);
        self.format_version.hash(&mut h);
        self.scope.hash(&mut h);
        self.selection.hash(&mut h);
        self.board_summary.hash(&mut h);
        if let Some(v) = &self.viewport {
            v.hash_into(&mut h);
        }
        h.finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub zoom: f32,
}

impl Viewport {
    fn hash_into(&self, h: &mut DefaultHasher) {
        self.x.to_bits().hash(h);
        self.y.to_bits().hash(h);
        self.w.to_bits().hash(h);
        self.h.to_bits().hash(h);
        self.zoom.to_bits().hash(h);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AgentRequest {
    pub id: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Immutable provider-advertised parameters captured at submission.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub model_parameters: std::collections::BTreeMap<String, String>,
    pub at: u64,
    /// Immutable inputs captured when the user presses Send / Generate.
    #[serde(default)]
    pub inputs: InputSnapshot,
    /// Explicit replay into a fresh provider session, never an implicit attach.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<AgentTurn>,
    /// Image-engine controls. Chat providers ignore them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageParams>,
    /// Owned staged artifact generation; absent for ordinary chat and images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactRun>,
    /// Absolute folder for new files the person did not place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<String>,
    /// A text block: no conversation history and no transport guides. The
    /// reply replaces the previous one.
    #[serde(default, skip_serializing_if = "is_false")]
    pub oneshot: bool,
    /// Enforced by the provider's own permissions, never by prompt text.
    #[serde(default, skip_serializing_if = "TurnPolicy::is_default")]
    pub policy: TurnPolicy,
    /// Label of the agent that wrote this message, e.g. "Codex · reviewer".
    /// Set only on relayed turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relayed_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<GoalTurn>,
    /// Explicit, revocable board collaborator binding; never ambient prompt context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collaborator: Option<collaborator::BoardLinkSession>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// The permissions a provider runs one turn under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnPolicy {
    /// The provider's own settings, plus the person's full-access grant.
    #[default]
    Default,
    /// No edits, commands, or subagents, whatever the grant.
    ReadOnly,
}

impl TurnPolicy {
    pub fn is_default(&self) -> bool {
        *self == Self::Default
    }
}

/// A shared goal two relayed agents work toward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalTurn {
    pub goal: String,
    /// true: this side claims the goal (the builder); false: it judges a claim (the reviewer).
    pub claims: bool,
    /// A read-only turn cannot write `return.json`, so it reports the verdict
    /// as the last line of its reply ([`goal_in_reply`]).
    #[serde(default, skip_serializing_if = "is_false")]
    pub in_reply: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Met,
    NotMet,
}

/// The `goal` an agent reports in `return.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalVerdict {
    pub status: GoalStatus,
    #[serde(default)]
    pub reason: String,
}

mod transport;
pub use transport::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortalView {
    #[default]
    Chat,
    Images,
    /// A one-shot local language model: an instruction plus wired text and
    /// pictures in, one reply out.
    Text,
}

/// Collision-free within a process, including multiple sends in one clock tick.
pub fn request_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tick = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "req-{}-{tick}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

mod versions;
pub use versions::*;

mod images;
pub use images::*;
mod deliverables;
pub use deliverables::*;
mod session;
pub use session::*;
mod inputs;
pub use inputs::*;
mod status;
pub use status::*;
mod models;
pub use models::*;
mod activity;
pub use activity::*;
mod activity_semantics;
pub use activity_semantics::*;
pub mod collaborator;
pub mod control;
#[cfg(test)]
mod manifest_tests;
pub mod presence;
#[cfg(test)]
mod prompt_tests;
#[cfg(test)]
mod tests;
