//! Guard against hardcoded [`Color32`] literals in board and panel UI paths.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const ALLOWLIST: &str = include_str!("../theme-allowlist.toml");

#[derive(Debug, Deserialize)]
struct AllowlistFile {
    #[serde(default)]
    entry: Vec<AllowEntry>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct AllowEntry {
    file: String,
    line: u32,
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ThemeFinding {
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
}

#[derive(Debug)]
pub struct ThemeAudit {
    pub findings: Vec<ThemeFinding>,
    pub scanned: usize,
}

fn normalize_rel(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn scoped_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for app in ["slate", "file-atlas"] {
        let app_root = root.join("apps").join(app).join("src").join("app");
        if !app_root.is_dir() {
            continue;
        }
        if app == "file-atlas" {
            let main = app_root.join("mod.rs");
            if main.is_file() {
                out.push(main);
            }
        }
        if let Ok(entries) = std::fs::read_dir(&app_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if path.is_file() && name.starts_with("board_") && name.ends_with(".rs") {
                    out.push(path);
                }
            }
        }
        let ui = app_root.join("ui");
        if ui.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&ui) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "rs") {
                        out.push(path);
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

const PATTERNS: &[&str] = &[
    "Color32::from_rgb",
    "Color32::from_rgba_unmultiplied",
    "Color32::from_rgba_premultiplied",
    "Color32::from_gray",
    "egui::Color32::from_rgb",
    "egui::Color32::from_rgba_unmultiplied",
    "egui::Color32::from_rgba_premultiplied",
    "egui::Color32::from_gray",
];

fn line_is_ignored(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("//") || trimmed.contains("#[cfg(test)]")
}

pub fn audit(root: &Path) -> Result<ThemeAudit, Box<dyn std::error::Error>> {
    let allow: BTreeSet<(String, u32)> = {
        let parsed: AllowlistFile = toml::from_str(ALLOWLIST)?;
        parsed
            .entry
            .into_iter()
            .map(|e| (e.file.replace('\\', "/"), e.line))
            .collect()
    };

    let mut findings = Vec::new();
    let files = scoped_files(root);
    for path in &files {
        let rel = normalize_rel(path, root);
        let text = std::fs::read_to_string(path)?;
        let mut in_tests = false;
        let mut test_depth = 0i32;
        for (i, line) in text.lines().enumerate() {
            if !in_tests {
                let trimmed = line.trim_start();
                if trimmed.starts_with("mod tests") && trimmed.contains('{') {
                    in_tests = true;
                    test_depth = 1;
                    continue;
                }
            } else {
                test_depth += line.matches('{').count() as i32;
                test_depth -= line.matches('}').count() as i32;
                if test_depth <= 0 {
                    in_tests = false;
                }
                continue;
            }
            if line_is_ignored(line) {
                continue;
            }
            let line_no = (i + 1) as u32;
            if allow.contains(&(rel.clone(), line_no)) {
                continue;
            }
            if PATTERNS.iter().any(|p| line.contains(p)) {
                findings.push(ThemeFinding {
                    file: path.clone(),
                    line: line_no,
                    snippet: line.trim().to_string(),
                });
            }
        }
    }

    Ok(ThemeAudit {
        findings,
        scanned: files.len(),
    })
}

pub fn render(audit: &ThemeAudit) -> String {
    if audit.findings.is_empty() {
        return format!(
            "theme lint: ok — {} file(s) scanned, no unallowlisted Color32 constructors\n",
            audit.scanned
        );
    }
    let mut by_file: BTreeMap<&Path, Vec<&ThemeFinding>> = BTreeMap::new();
    for f in &audit.findings {
        by_file.entry(&f.file).or_default().push(f);
    }
    let mut out =
        String::from("theme lint findings (add xtask/theme-allowlist.toml or use Palette):\n");
    for (file, items) in by_file {
        out.push_str(&format!("\n{}\n", file.display()));
        for item in items {
            out.push_str(&format!("  L{}: {}\n", item.line, item.snippet));
        }
    }
    out
}
