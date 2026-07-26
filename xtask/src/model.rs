//! The snapshot schema.
//!
//! Field order here is field order in the JSON, and the JSON is read by humans
//! diffing two audits — so the declaration order below is part of the contract,
//! not a style choice.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub date: String,
    pub commit: String,
    pub totals: Totals,
    pub crates: Vec<CrateMetrics>,
    pub model: ModelMetrics,
    pub commands: CommandCounts,
    pub deviations: DeviationCounts,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Totals {
    pub lines_total: u32,
    pub lines_code: u32,
    pub pure_lines_code: u32,
    pub renderer_lines_code: u32,
    pub pure_ratio: f64,
    pub crates: u32,
    pub tests: u32,
    pub unsafe_blocks: u32,
    pub direct_dependencies: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrateMetrics {
    pub name: String,
    pub kind: CrateKind,
    /// Article I's line: the crate's manifest names `egui`/`eframe`, or names a
    /// workspace crate that does.
    pub renderer: bool,
    pub lines_total: u32,
    pub lines_code: u32,
    pub tests: u32,
    pub unsafe_blocks: u32,
    pub longest_file: LongestFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrateKind {
    Lib,
    Bin,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LongestFile {
    /// Relative to the crate root, always with `/` separators.
    pub path: String,
    pub lines: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelMetrics {
    pub format_version: u32,
    pub node_kinds: u32,
    pub node_kind_names: Vec<String>,
    pub edge_roles: u32,
    pub edge_role_names: Vec<String>,
    pub scene_cmd_variants: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandCounts {
    pub slate: u32,
    #[serde(rename = "file-atlas")]
    pub file_atlas: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviationCounts {
    pub open: u32,
    pub accepted: u32,
    pub closed: u32,
}
