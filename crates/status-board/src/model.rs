//! Snapshot schema — the authored status instrument, not a live probe.
//!
//! Extra JSON fields are ignored so a richer dashboard file still loads.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Journaled knobs that shape which bands the layout emits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusQuery {
    pub show_overview: bool,
    pub show_phases: bool,
    pub show_waves: bool,
    pub show_deviations: bool,
    pub show_next: bool,
}

impl Default for StatusQuery {
    fn default() -> Self {
        Self {
            show_overview: true,
            show_phases: true,
            show_waves: true,
            show_deviations: true,
            show_next: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub meta: Meta,
    #[serde(default)]
    pub thesis: Option<Thesis>,
    pub progress_scores: ProgressScores,
    pub deviations: Deviations,
    #[serde(default)]
    pub roadmap_phases: Vec<Phase>,
    #[serde(default)]
    pub workplan: Workplan,
    #[serde(default)]
    pub capabilities_inventory: Inventory,
}

impl Snapshot {
    /// Stable key for cache invalidation — excludes wall-clock-only notes.
    pub fn fingerprint_key(&self) -> String {
        format!(
            "{}|{}|{:.4}|{:.4}|{}|{}",
            self.meta.head_commit,
            self.meta.generated,
            self.progress_scores.overall_roadmap,
            self.progress_scores.overall_workplan_through_w2,
            self.deviations.open,
            self.deviations.closed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub generated: String,
    pub head_commit: String,
    #[serde(default)]
    pub head_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thesis {
    #[serde(default)]
    pub one_liner: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressScores {
    pub overall_roadmap: f64,
    pub overall_workplan_through_w2: f64,
    #[serde(default)]
    pub phase_scores: BTreeMap<String, f64>,
    #[serde(default)]
    pub wave_scores: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deviations {
    pub open: u32,
    pub closed: u32,
    #[serde(default)]
    pub accepted: u32,
    #[serde(default)]
    pub rows: Vec<DeviationRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviationRow {
    pub id: String,
    #[serde(default)]
    pub article: String,
    pub status: String,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Phase {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub progress: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Workplan {
    #[serde(default)]
    pub waves: Vec<Wave>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wave {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub progress: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Inventory {
    #[serde(default)]
    pub next_critical: Vec<NextAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextAction {
    #[serde(default)]
    pub priority: u32,
    pub item: String,
    #[serde(default)]
    pub why: String,
}
