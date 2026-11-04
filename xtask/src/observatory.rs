//! Read-only, local snapshot for the Slate architecture dashboard.
//!
//! The metrics collector remains the owner of metric definitions; this module
//! selects Slate's declared dependency closure and serializes it for a browser.

use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

use crate::collect::{git, line_counts, read_manifest, rust_sources, workspace_members, Member};
use crate::model::{CrateMetrics, Snapshot};
use crate::{map, rust_parse, MetricsError};

#[derive(Serialize)]
pub struct Observatory {
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    pub commit: String,
    pub dirty: bool,
    pub scope: &'static str,
    pub crates: Vec<Package>,
    pub edges: Vec<Edge>,
    pub totals: ScopedTotals,
    pub history: Vec<History>,
    pub commands: crate::CommandCounts,
    pub model: crate::ModelMetrics,
    pub deviations: crate::DeviationCounts,
    #[serde(rename = "workspaceTotals")]
    pub workspace_totals: crate::Totals,
    #[serde(rename = "excludedCrates")]
    pub excluded_crates: Vec<String>,
    pub definitions: Definitions,
    #[serde(rename = "gitActivity")]
    pub git_activity: crate::observatory_git::GitActivity,
    #[serde(rename = "changeCost")]
    pub change_cost: crate::observatory_cost::ChangeCost,
}

#[derive(Serialize)]
pub struct Package {
    #[serde(flatten)]
    pub metrics: CrateMetrics,
    pub path: String,
    pub description: String,
    pub files: Vec<SourceFile>,
}

#[derive(Serialize)]
pub struct SourceFile {
    pub path: String,
    pub lines: u32,
    pub lines_code: u32,
    pub purpose: String,
    pub tests: u32,
    pub unsafe_blocks: u32,
    pub bytes: u64,
    pub tracked: Option<bool>,
}

#[derive(Serialize)]
pub struct Edge {
    pub source: String,
    pub target: String,
    pub kind: &'static str,
}

#[derive(Serialize)]
pub struct ScopedTotals {
    pub crates: usize,
    pub files: usize,
    pub lines_total: u32,
    pub lines_code: u32,
    pub pure_lines_code: u32,
    pub renderer_lines_code: u32,
    pub pure_ratio: f64,
    pub tests: u32,
    pub unsafe_blocks: u32,
}

#[derive(Serialize)]
pub struct History {
    pub date: String,
    pub commit: String,
    pub totals: ScopedTotals,
    pub crates: Vec<CrateMetrics>,
    pub commands: crate::CommandCounts,
    pub model: crate::ModelMetrics,
    pub deviations: crate::DeviationCounts,
}

#[derive(Serialize)]
pub struct Definitions {
    pub scope: &'static str,
    pub snapshot: &'static str,
    pub edges: &'static str,
    pub lines_code: &'static str,
    pub tests: &'static str,
    pub unsafe_blocks: &'static str,
    pub renderer: &'static str,
    pub history: &'static str,
    pub files: &'static str,
    pub model: &'static str,
    pub deviations: &'static str,
}

/// Collects without writing metrics history, changing a workbook or launching
/// either app. Manifest edges include declarations for every target and feature.
pub fn collect(root: &Path) -> Result<Observatory, MetricsError> {
    let snapshot = crate::collect(root)?;
    let members = workspace_members(root)?;
    let names: BTreeSet<_> = members.iter().map(|m| m.name.clone()).collect();
    let mut edges = declared_edges(&members);
    let included = dependency_closure("slate", &edges);
    if !names.contains("slate") {
        return Err(MetricsError::manifest(
            root.join("Cargo.toml"),
            "no slate member",
        ));
    }
    edges.retain(|edge| included.contains(&edge.source) && included.contains(&edge.target));
    let tracked_paths = crate::observatory_cost::tracked_paths(root);
    let mut crates = Vec::new();
    for metrics in &snapshot.crates {
        if !included.contains(&metrics.name) {
            continue;
        }
        let member = members
            .iter()
            .find(|m| m.name == metrics.name)
            .expect("member metrics");
        let manifest = read_manifest(&member.dir.join("Cargo.toml"))?;
        let description = manifest
            .get("package")
            .and_then(|p| p.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut files = Vec::new();
        for (_, path) in rust_sources(&member.dir)? {
            let text = std::fs::read_to_string(&path).map_err(|e| MetricsError::io(&path, e))?;
            let (lines, lines_code) = line_counts(&text);
            let counts = rust_parse::file_counts(&rust_parse::parse_file(&path)?);
            let relative_path = relative(&path, root);
            files.push(SourceFile {
                tracked: tracked_paths
                    .as_ref()
                    .map(|paths| paths.contains(&relative_path)),
                path: relative_path,
                lines,
                lines_code,
                purpose: map::file_purpose(&text),
                tests: counts.tests,
                unsafe_blocks: counts.unsafe_blocks,
                bytes: text.len() as u64,
            });
        }
        crates.push(Package {
            metrics: metrics.clone(),
            path: relative(&member.dir, root),
            description,
            files,
        });
    }
    let totals = totals(
        crates.iter().map(|p| &p.metrics),
        crates.iter().map(|p| p.files.len()).sum(),
    );
    let history: Vec<_> = crate::load_history(&root.join("docs/metrics"))?
        .into_iter()
        .map(|s| scoped_history(s, &included))
        .collect();
    let git_activity = crate::observatory_git::collect(
        root,
        &snapshot.date,
        &crates,
        history.iter().map(|s| s.date.as_str()).min(),
    );
    let excluded_crates = names.difference(&included).cloned().collect();
    let dirty = git(root, &["status", "--porcelain"]).is_some();
    let change_cost =
        crate::observatory_cost::collect(root, &snapshot.date, &snapshot.commit, dirty, &crates);
    Ok(Observatory {
        generated_at: snapshot.date,
        commit: snapshot.commit,
        dirty,
        scope: "slate-dependency-closure",
        crates, edges, totals, history,
        commands: snapshot.commands, model: snapshot.model, deviations: snapshot.deviations,
        workspace_totals: snapshot.totals, excluded_crates, git_activity, change_cost,
        definitions: Definitions {
            scope: "Slate plus the transitive closure of local Cargo manifest declarations, including File Atlas and shared crates. Excludes unrelated workspace members and third-party sources.",
            snapshot: "Static source snapshot from the working tree, including uncommitted files. Not runtime telemetry. Refresh with cargo xtask observatory.",
            edges: "Unique directed local dependency declarations: source depends on target. Includes normal, dev, build, optional and target-specific tables; not a resolved build or call graph.",
            lines_code: "Canonical xtask metric: nonempty Rust lines excluding lines whose trimmed text starts with //. Includes block comments and inline tests; not executable statements.",
            tests: "syn-parsed #[test] attributes under each crate's src/ and tests/. Counts declarations, not executed or passing tests, coverage, or macro-expanded tests.",
            unsafe_blocks: "syn-parsed unsafe { ... } expressions. Not a count of unsafe functions, declarations, vulnerabilities, or all unsafe operations.",
            renderer: "Canonical xtask heuristic: a crate declares egui/eframe, or declares a workspace crate that directly does. One level, not transitive purity proof; independent crates are the complement.",
            history: "Committed docs/metrics/YYYY-MM-DD.json snapshots, filtered to names in today's Slate closure. Closure can change over time. Historical source-file counts are unavailable (0). Current snapshot is separate from committed history.",
            files: "Rust files under src/ and tests/ only, including inline and fixture Rust. Excludes examples, build.rs, target, vendor, generated binaries and non-Rust assets. File lines use the metrics collector definition.",
            model: "syn-parsed slate-doc format constant and model enums plus each app's registered SPECS table. Declaration counts, not behavior or reachability.",
            deviations: "Workspace-wide counts from docs/audit/deviations.md. Not restricted to Slate; open does not automatically mean a product defect.",
        },
    })
}

pub(crate) fn declared_edges(members: &[Member]) -> Vec<Edge> {
    let names: BTreeSet<_> = members.iter().map(|m| m.name.as_str()).collect();
    members
        .iter()
        .flat_map(|member| {
            member
                .dependencies
                .iter()
                .filter(|d| names.contains(d.as_str()))
                .map(|dependency| Edge {
                    source: member.name.clone(),
                    target: dependency.clone(),
                    kind: "declared",
                })
        })
        .collect()
}

pub(crate) fn dependency_closure(root: &str, edges: &[Edge]) -> BTreeSet<String> {
    let mut included = BTreeSet::from([root.to_string()]);
    loop {
        let previous = included.len();
        for edge in edges {
            if included.contains(&edge.source) {
                included.insert(edge.target.clone());
            }
        }
        if included.len() == previous {
            return included;
        }
    }
}

fn totals<'a>(crates: impl Iterator<Item = &'a CrateMetrics>, files: usize) -> ScopedTotals {
    let crates: Vec<_> = crates.collect();
    let lines_code = crates.iter().map(|c| c.lines_code).sum();
    let renderer_lines_code = crates
        .iter()
        .filter(|c| c.renderer)
        .map(|c| c.lines_code)
        .sum();
    let pure_lines_code = lines_code - renderer_lines_code;
    ScopedTotals {
        crates: crates.len(),
        files,
        lines_total: crates.iter().map(|c| c.lines_total).sum(),
        lines_code,
        renderer_lines_code,
        pure_lines_code,
        pure_ratio: if lines_code == 0 {
            0.0
        } else {
            pure_lines_code as f64 / lines_code as f64
        },
        tests: crates.iter().map(|c| c.tests).sum(),
        unsafe_blocks: crates.iter().map(|c| c.unsafe_blocks).sum(),
    }
}

fn scoped_history(mut snapshot: Snapshot, included: &BTreeSet<String>) -> History {
    snapshot.crates.retain(|c| included.contains(&c.name));
    History {
        date: snapshot.date,
        commit: snapshot.commit,
        totals: totals(snapshot.crates.iter(), 0),
        crates: snapshot.crates,
        commands: snapshot.commands,
        model: snapshot.model,
        deviations: snapshot.deviations,
    }
}

fn relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Reuses every string color from Slate's canonical theme, never a second palette.
pub fn theme_css(root: &Path) -> Result<String, MetricsError> {
    let path = root.join("crates/atlas-shell/ui-tokens.toml");
    let tokens = read_manifest(&path)?;
    let mut css = String::from(
        "/* Generated from atlas-shell/ui-tokens.toml by cargo xtask observatory. */\n",
    );
    for (mode, selector) in [
        ("dark", ":root, [data-theme=\"dark\"]"),
        ("light", "[data-theme=\"light\"]"),
    ] {
        let colors = tokens
            .get("theme")
            .and_then(|t| t.get(mode))
            .and_then(|t| t.as_table())
            .ok_or_else(|| MetricsError::manifest(&path, format!("no theme.{mode}")))?;
        css.push_str(&format!("{selector} {{\n  color-scheme: {mode};\n"));
        for (key, value) in colors {
            if let Some(color) = value.as_str() {
                css.push_str(&format!("  --{}: {color};\n", key.replace('_', "-")));
            }
        }
        css.push_str("}\n");
    }
    Ok(css)
}

pub fn write(root: &Path) -> Result<(), MetricsError> {
    let data = collect(root)?;
    let json = serde_json::to_string(&data).expect("observatory serializes");
    let dir = root.join("tools/slate-observatory");
    std::fs::create_dir_all(&dir).map_err(|e| MetricsError::io(&dir, e))?;
    let path = dir.join("data.js");
    let script = format!("// Generated by cargo xtask observatory; static source snapshot.\nwindow.SLATE_OBSERVATORY = {json};\n");
    std::fs::write(&path, script).map_err(|e| MetricsError::io(&path, e))?;
    let path = dir.join("theme.css");
    std::fs::write(&path, theme_css(root)?).map_err(|e| MetricsError::io(&path, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closure_handles_cycles_and_excludes_reverse_dependencies() {
        let edges = [
            ("slate", "shared"),
            ("shared", "core"),
            ("core", "shared"),
            ("other", "shared"),
        ]
        .map(|(source, target)| Edge {
            source: source.into(),
            target: target.into(),
            kind: "declared",
        });
        assert_eq!(
            dependency_closure("slate", &edges),
            BTreeSet::from(["slate".into(), "shared".into(), "core".into()])
        );
    }

    #[test]
    fn source_rollups_and_edges_match_the_canonical_snapshot() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let data = collect(root).expect("observatory collection");
        let names: BTreeSet<_> = data
            .crates
            .iter()
            .map(|p| p.metrics.name.as_str())
            .collect();
        assert!(names.contains("slate") && names.contains("native-file-atlas"));
        assert!(!names.contains("xtask"));
        for package in &data.crates {
            assert_eq!(
                package.files.iter().map(|f| f.lines).sum::<u32>(),
                package.metrics.lines_total
            );
            assert_eq!(
                package.files.iter().map(|f| f.lines_code).sum::<u32>(),
                package.metrics.lines_code
            );
            assert_eq!(
                package.files.iter().map(|f| f.tests).sum::<u32>(),
                package.metrics.tests
            );
            assert_eq!(
                package.files.iter().map(|f| f.unsafe_blocks).sum::<u32>(),
                package.metrics.unsafe_blocks
            );
            assert!(package
                .files
                .iter()
                .all(|f| !f.path.contains(':') && !f.path.starts_with('/')));
        }
        assert!(data
            .edges
            .iter()
            .all(|e| names.contains(e.source.as_str()) && names.contains(e.target.as_str())));
        assert_eq!(
            data.totals.lines_code,
            data.crates
                .iter()
                .map(|p| p.metrics.lines_code)
                .sum::<u32>()
        );
        assert_eq!(
            data.history.len(),
            crate::load_history(&root.join("docs/metrics"))
                .unwrap()
                .len()
        );
    }
}
