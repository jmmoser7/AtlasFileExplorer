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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRequest {
    pub id: String,
    pub prompt: String,
    pub at: u64,
    /// Immutable inputs captured when the user presses Send / Generate.
    #[serde(default)]
    pub inputs: InputSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSession {
    #[serde(default)]
    pub status: AgentStatus,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub turns: Vec<AgentTurn>,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub bundle: ImageBundle,
    #[serde(default)]
    pub request: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InputSnapshot {
    pub revision: String,
    pub context: Vec<ContextItem>,
    pub wired: Vec<ContextItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextItem {
    pub node: u64,
    pub text: String,
    /// Linked asset locators; ordered, never copied into the workbook.
    pub images: Vec<String>,
    #[serde(default)]
    pub outputs: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub active: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImageBundle {
    pub images: Vec<ImageOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageOutput {
    /// Stable within the bundle, independent of its current ordering.
    pub id: String,
    pub source: String,
    pub request: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortalView {
    #[default]
    Chat,
    Images,
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

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    #[default]
    Idle,
    Thinking,
    Offline,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTurn {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rapid_requests_have_distinct_identity() {
        let ids: std::collections::BTreeSet<_> = (0..1000).map(|_| request_id()).collect();
        assert_eq!(ids.len(), 1000);
    }
    #[test]
    fn old_session_contract_remains_readable() {
        let value: AgentSession = serde_json::from_str(
            r#"{"status":"idle","provider":"cursor","turns":[],"updated_at":1}"#,
        )
        .unwrap();
        assert!(value.bundle.images.is_empty());
        assert!(value.request.is_empty());
    }
}
