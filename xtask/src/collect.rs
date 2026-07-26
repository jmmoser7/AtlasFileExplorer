//! Walks the workspace and assembles one [`Snapshot`].
//!
//! Nothing here writes: the collector is pure with respect to the tree so that
//! `metrics_snapshot_is_deterministic` can run it twice and compare bytes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::{
    CommandCounts, CrateKind, CrateMetrics, LongestFile, ModelMetrics, Snapshot, Totals,
};
use crate::{ledger, rust_parse, MetricsError};

/// Dependency tables a manifest may declare, including the `target.*` forms.
const DEPENDENCY_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

pub fn collect(root: &Path) -> Result<Snapshot, MetricsError> {
    crate::verify_workspace_root(root)?;

    let members = workspace_members(root)?;
    let member_names: BTreeSet<String> = members.iter().map(|m| m.name.clone()).collect();

    let mut crates = Vec::with_capacity(members.len());
    for member in &members {
        crates.push(measure_crate(member, &members)?);
    }
    crates.sort_by(|a, b| a.name.cmp(&b.name));

    let direct_dependencies: BTreeSet<&str> = members
        .iter()
        .flat_map(|m| m.dependencies.iter())
        .filter(|name| !member_names.contains(*name))
        .map(String::as_str)
        .collect();

    let lines_total: u32 = crates.iter().map(|c| c.lines_total).sum();
    let lines_code: u32 = crates.iter().map(|c| c.lines_code).sum();
    let renderer_lines_code: u32 = crates
        .iter()
        .filter(|c| c.renderer)
        .map(|c| c.lines_code)
        .sum();
    let pure_lines_code = lines_code - renderer_lines_code;

    let totals = Totals {
        lines_total,
        lines_code,
        pure_lines_code,
        renderer_lines_code,
        pure_ratio: ratio(pure_lines_code, lines_code),
        crates: crates.len() as u32,
        tests: crates.iter().map(|c| c.tests).sum(),
        unsafe_blocks: crates.iter().map(|c| c.unsafe_blocks).sum(),
        direct_dependencies: direct_dependencies.len() as u32,
    };

    Ok(Snapshot {
        date: today(root),
        commit: head_commit(root),
        totals,
        crates,
        model: model_metrics(root)?,
        commands: command_counts(root)?,
        deviations: ledger::parse_deviations(&ledger::ledger_path(root))?,
    })
}

/// One workspace member as its manifest describes it.
struct Member {
    name: String,
    dir: PathBuf,
    kind: CrateKind,
    /// Every dependency name the manifest mentions, in any table.
    dependencies: BTreeSet<String>,
}

impl Member {
    fn names_renderer(&self) -> bool {
        self.dependencies.contains("egui") || self.dependencies.contains("eframe")
    }
}

fn read_manifest(path: &Path) -> Result<toml::Value, MetricsError> {
    let text = std::fs::read_to_string(path).map_err(|e| MetricsError::io(path, e))?;
    toml::from_str(&text).map_err(|e| MetricsError::manifest(path, e.to_string()))
}

fn workspace_members(root: &Path) -> Result<Vec<Member>, MetricsError> {
    let manifest_path = root.join("Cargo.toml");
    let manifest = read_manifest(&manifest_path)?;
    let listed = manifest
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .ok_or_else(|| {
            MetricsError::manifest(&manifest_path, "[workspace] members is not an array")
        })?;

    let mut members = Vec::with_capacity(listed.len());
    for entry in listed {
        let rel = entry.as_str().ok_or_else(|| {
            MetricsError::manifest(&manifest_path, "a [workspace] member is not a string")
        })?;
        members.push(read_member(root.join(rel))?);
    }
    members.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(members)
}

fn read_member(dir: PathBuf) -> Result<Member, MetricsError> {
    let manifest_path = dir.join("Cargo.toml");
    let manifest = read_manifest(&manifest_path)?;
    let name = manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .ok_or_else(|| MetricsError::manifest(&manifest_path, "no [package] name"))?
        .to_string();

    let mut dependencies = BTreeSet::new();
    collect_dependencies(&manifest, &mut dependencies);
    if let Some(targets) = manifest.get("target").and_then(|t| t.as_table()) {
        for cfg in targets.values() {
            collect_dependencies(cfg, &mut dependencies);
        }
    }

    let kind = if dir.join("src").join("main.rs").is_file() || manifest.get("bin").is_some() {
        CrateKind::Bin
    } else {
        CrateKind::Lib
    };

    Ok(Member {
        name,
        dir,
        kind,
        dependencies,
    })
}

fn collect_dependencies(table: &toml::Value, out: &mut BTreeSet<String>) {
    for key in DEPENDENCY_TABLES {
        let Some(deps) = table.get(key).and_then(|d| d.as_table()) else {
            continue;
        };
        for (name, spec) in deps {
            let renamed = spec
                .as_table()
                .and_then(|t| t.get("package"))
                .and_then(|p| p.as_str());
            out.insert(renamed.unwrap_or(name).to_string());
        }
    }
}

fn measure_crate(member: &Member, all: &[Member]) -> Result<CrateMetrics, MetricsError> {
    let mut lines_total = 0u32;
    let mut lines_code = 0u32;
    let mut tests = 0u32;
    let mut unsafe_blocks = 0u32;
    let mut longest = LongestFile::default();

    for (rel, path) in rust_sources(&member.dir)? {
        let text = std::fs::read_to_string(&path).map_err(|e| MetricsError::io(&path, e))?;
        let total = text.lines().count() as u32;
        let code = text
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with("//")
            })
            .count() as u32;
        lines_total += total;
        lines_code += code;

        let counts = rust_parse::file_counts(&rust_parse::parse_file(&path)?);
        tests += counts.tests;
        unsafe_blocks += counts.unsafe_blocks;

        if total > longest.lines {
            longest = LongestFile {
                path: rel,
                lines: total,
            };
        }
    }

    Ok(CrateMetrics {
        name: member.name.clone(),
        kind: member.kind,
        renderer: is_renderer(member, all),
        lines_total,
        lines_code,
        tests,
        unsafe_blocks,
        longest_file: longest,
    })
}

/// Article I's dividing line, resolved one level deep: a crate is renderer-bound
/// if it names `egui`/`eframe` itself, or names a workspace crate that does.
fn is_renderer(member: &Member, all: &[Member]) -> bool {
    member.names_renderer()
        || all
            .iter()
            .any(|other| member.dependencies.contains(&other.name) && other.names_renderer())
}

/// Every `.rs` file under the crate's `src/` and `tests/`, as
/// `(path relative to the crate root, absolute path)`, sorted by the former so
/// the walk order never depends on the filesystem.
fn rust_sources(dir: &Path) -> Result<Vec<(String, PathBuf)>, MetricsError> {
    let mut out = Vec::new();
    for sub in ["src", "tests"] {
        push_rust_files(&dir.join(sub), dir, &mut out)?;
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn push_rust_files(
    dir: &Path,
    crate_root: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), MetricsError> {
    if !dir.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| MetricsError::io(dir, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| MetricsError::io(dir, e))?;
        let path = entry.path();
        if path.is_dir() {
            push_rust_files(&path, crate_root, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let rel = path
                .strip_prefix(crate_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
    Ok(())
}

fn model_metrics(root: &Path) -> Result<ModelMetrics, MetricsError> {
    let doc = root.join("crates").join("slate-doc").join("src");
    let doc_rs = doc.join("doc.rs");
    let format_version = rust_parse::const_u32(&rust_parse::parse_file(&doc_rs)?, "CURRENT")
        .ok_or_else(|| MetricsError::syntax(&doc_rs, "no `const CURRENT: u32 = N;`"))?;

    let mut node_kind_names = None;
    let mut edge_role_names = None;
    let mut scene_cmd_names = None;
    for (_, path) in rust_sources_in(&doc)? {
        let file = rust_parse::parse_file(&path)?;
        node_kind_names = node_kind_names.or_else(|| rust_parse::enum_variants(&file, "NodeKind"));
        edge_role_names = edge_role_names.or_else(|| rust_parse::enum_variants(&file, "EdgeRole"));
        scene_cmd_names = scene_cmd_names.or_else(|| rust_parse::enum_variants(&file, "SceneCmd"));
    }

    let node_kind_names = node_kind_names
        .ok_or_else(|| MetricsError::syntax(&doc, "no `enum NodeKind` in slate-doc/src"))?;
    let scene_cmd_names = scene_cmd_names
        .ok_or_else(|| MetricsError::syntax(&doc, "no `enum SceneCmd` in slate-doc/src"))?;
    let edge_role_names = edge_role_names.unwrap_or_default();

    Ok(ModelMetrics {
        format_version,
        node_kinds: node_kind_names.len() as u32,
        node_kind_names,
        edge_roles: edge_role_names.len() as u32,
        edge_role_names,
        scene_cmd_variants: scene_cmd_names.len() as u32,
    })
}

fn rust_sources_in(dir: &Path) -> Result<Vec<(String, PathBuf)>, MetricsError> {
    let mut out = Vec::new();
    push_rust_files(dir, dir, &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn command_counts(root: &Path) -> Result<CommandCounts, MetricsError> {
    Ok(CommandCounts {
        slate: specs_len(&root.join("apps/slate/src/app/commands.rs"))?,
        file_atlas: specs_len(&root.join("apps/file-atlas/src/app/commands.rs"))?,
    })
}

fn specs_len(path: &Path) -> Result<u32, MetricsError> {
    let file = rust_parse::parse_file(path)?;
    rust_parse::slice_literal_len(&file, "SPECS")
        .map(|len| len as u32)
        .ok_or_else(|| MetricsError::syntax(path, "no `SPECS` slice literal"))
}

fn ratio(numerator: u32, denominator: u32) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    ((numerator as f64 / denominator as f64) * 1000.0).round() / 1000.0
}

fn head_commit(root: &Path) -> String {
    git(root, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_string())
}

/// Today's date in the machine's local timezone.
///
/// `std` has no local clock and the card allows no date dependency, so the UTC
/// offset comes from git — which the collector already shells out to. Without
/// git the date falls back to UTC, which is wrong for at most a few hours a day
/// and never wrong twice in one run.
fn today(root: &Path) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default();
    let (year, month, day) = civil_from_unix(now + local_offset_secs(root));
    format!("{year:04}-{month:02}-{day:02}")
}

fn local_offset_secs(root: &Path) -> i64 {
    // `git var GIT_AUTHOR_IDENT` is "Name <mail> <unix secs> <±HHMM>".
    let Some(ident) = git(root, &["var", "GIT_AUTHOR_IDENT"]) else {
        return 0;
    };
    let Some(zone) = ident.split_whitespace().last() else {
        return 0;
    };
    let (sign, digits) = match zone.split_at(1) {
        ("+", rest) => (1, rest),
        ("-", rest) => (-1, rest),
        _ => return 0,
    };
    if digits.len() != 4 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return 0;
    }
    let hours: i64 = digits[..2].parse().unwrap_or(0);
    let minutes: i64 = digits[2..].parse().unwrap_or(0);
    sign * (hours * 3600 + minutes * 60)
}

/// Days-to-civil-date, Howard Hinnant's algorithm — the arithmetic `std` leaves
/// to the calendar crates this tool is not allowed to depend on.
fn civil_from_unix(secs: i64) -> (i64, i64, i64) {
    let days = secs.div_euclid(86_400) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_date_matches_known_instants() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1));
        assert_eq!(civil_from_unix(1_785_029_661), (2026, 7, 26));
        assert_eq!(civil_from_unix(1_785_029_661 - 4 * 3600), (2026, 7, 25));
    }

    #[test]
    fn ratio_rounds_to_three_decimals() {
        assert_eq!(ratio(1, 3), 0.333);
        assert_eq!(ratio(2, 3), 0.667);
        assert_eq!(ratio(0, 0), 0.0);
    }
}
