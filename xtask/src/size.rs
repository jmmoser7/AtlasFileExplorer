//! Physical line count per Rust source file (500-line cap, ratcheting allowlist).
//!
//! Lines are counted as `text.split('\n').count()` — physical rows, not non-blank.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const LIMIT: usize = 500;

const ALLOWLIST: &str = include_str!("../size-allowlist.toml");

#[derive(Debug, Deserialize, Serialize)]
struct AllowlistFile {
    #[serde(default)]
    entry: Vec<AllowEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AllowEntry {
    file: String,
    ceiling: usize,
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SizeFindingKind {
    OverLimit,
    AboveCeiling,
    StaleAllowlist,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SizeFinding {
    pub kind: SizeFindingKind,
    pub file: PathBuf,
    pub lines: usize,
    pub ceiling: Option<usize>,
    pub detail: String,
}

#[derive(Debug)]
pub struct SizeAudit {
    pub findings: Vec<SizeFinding>,
    pub scanned: usize,
    pub oversized_allowlisted: usize,
}

pub fn count_physical_lines(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.split('\n').count()
    }
}

fn normalize_rel(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn skip_path(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    if s.contains("/target/") || s.contains("/vendor/") {
        return true;
    }
    if path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.contains("generated"))
    {
        return true;
    }
    false
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if skip_path(dir) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") && !skip_path(&path) {
            out.push(path);
        }
    }
}

pub fn scoped_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for sub in ["crates", "apps", "xtask"] {
        let dir = root.join(sub);
        if dir.is_dir() {
            collect_rs_files(&dir, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn line_count(path: &Path) -> Result<usize, std::io::Error> {
    let text = std::fs::read_to_string(path)?;
    Ok(count_physical_lines(&text))
}

pub fn audit(root: &Path) -> Result<SizeAudit, Box<dyn std::error::Error>> {
    audit_allowlist(root, ALLOWLIST)
}

pub fn audit_allowlist(
    root: &Path,
    allowlist_toml: &str,
) -> Result<SizeAudit, Box<dyn std::error::Error>> {
    let parsed: AllowlistFile = if allowlist_toml.trim().is_empty() {
        AllowlistFile { entry: vec![] }
    } else {
        toml::from_str(allowlist_toml)?
    };
    let allow: BTreeMap<String, &AllowEntry> = parsed
        .entry
        .iter()
        .map(|e| (e.file.replace('\\', "/"), e))
        .collect();

    let mut findings = Vec::new();
    let files = scoped_files(root);
    let mut seen_allow = BTreeSet::new();

    for path in &files {
        let rel = normalize_rel(path, root);
        let lines = line_count(path)?;
        if let Some(entry) = allow.get(&rel) {
            seen_allow.insert(rel.clone());
            if lines <= LIMIT {
                findings.push(SizeFinding {
                    kind: SizeFindingKind::StaleAllowlist,
                    file: path.clone(),
                    lines,
                    ceiling: Some(entry.ceiling),
                    detail: format!("stale allowlist row — file is now {lines} lines (≤ {LIMIT})"),
                });
                continue;
            }
            if lines > entry.ceiling {
                findings.push(SizeFinding {
                    kind: SizeFindingKind::AboveCeiling,
                    file: path.clone(),
                    lines,
                    ceiling: Some(entry.ceiling),
                    detail: format!(
                        "allowlisted ceiling {} — shrink or split before adding lines",
                        entry.ceiling
                    ),
                });
            }
            continue;
        }
        if lines > LIMIT {
            findings.push(SizeFinding {
                kind: SizeFindingKind::OverLimit,
                file: path.clone(),
                lines,
                ceiling: None,
                detail: format!("over {LIMIT}-line cap — split into a module or allowlist row"),
            });
        }
    }

    for entry in &parsed.entry {
        let rel = entry.file.replace('\\', "/");
        if seen_allow.contains(&rel) {
            continue;
        }
        let path = root.join(&rel);
        let lines = if path.is_file() {
            line_count(&path)?
        } else {
            0
        };
        if !path.is_file() || lines <= LIMIT {
            findings.push(SizeFinding {
                kind: SizeFindingKind::StaleAllowlist,
                file: PathBuf::from(&entry.file),
                lines,
                ceiling: Some(entry.ceiling),
                detail: if path.is_file() {
                    format!("stale allowlist row — file is now {lines} lines (≤ {LIMIT})")
                } else {
                    "stale allowlist row — file missing".into()
                },
            });
        }
    }

    let oversized_allowlisted = parsed.entry.iter().filter(|e| e.ceiling > LIMIT).count();

    Ok(SizeAudit {
        findings,
        scanned: files.len(),
        oversized_allowlisted,
    })
}

pub fn render(audit: &SizeAudit) -> String {
    if audit.findings.is_empty() {
        return format!(
            "size lint: ok — {} `.rs` file(s) scanned, {LIMIT}-line cap enforced, {} allowlisted oversized file(s)\n",
            audit.scanned, audit.oversized_allowlisted
        );
    }
    let mut out = String::from(
        "size lint findings (physical line count; split file or update xtask/size-allowlist.toml):\n",
    );
    for f in &audit.findings {
        let tag = match f.kind {
            SizeFindingKind::OverLimit => "over cap",
            SizeFindingKind::AboveCeiling => "above ceiling",
            SizeFindingKind::StaleAllowlist => "stale allowlist",
        };
        out.push_str(&format!(
            "\n[{tag}] {} ({} lines",
            f.file.display(),
            f.lines
        ));
        if let Some(c) = f.ceiling {
            out.push_str(&format!(", ceiling {c}"));
        }
        out.push_str(&format!(")\n  {}\n", f.detail));
    }
    out
}

/// Rewrites `xtask/size-allowlist.toml`: drops rows for files ≤ limit or missing;
/// lowers each remaining `ceiling` to the current line count (never raises).
pub fn update_ceilings(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join("xtask").join("size-allowlist.toml");
    let text = std::fs::read_to_string(&path)?;
    let mut parsed: AllowlistFile = toml::from_str(&text)?;
    let mut kept = Vec::new();
    for mut entry in parsed.entry.drain(..) {
        let file_path = root.join(entry.file.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !file_path.is_file() {
            continue;
        }
        let lines = line_count(&file_path)?;
        if lines <= LIMIT {
            continue;
        }
        entry.ceiling = entry.ceiling.min(lines);
        kept.push(entry);
    }
    kept.sort_by(|a, b| a.file.cmp(&b.file));
    parsed.entry = kept;
    let out = toml::to_string_pretty(&parsed)?;
    let header = "# Ratcheting allowlist: one row per `.rs` file still over 500 physical lines.\n\
# `ceiling` is the max line count allowed until the file is split below the cap.\n\
# Remove rows when a file shrinks to ≤500 lines. `cargo xtask size --update-ceilings` lowers\n\
# ceilings to current counts (never raises). Lines = physical rows (`split('\\n').count()`).\n\n";
    std::fs::write(&path, format!("{header}{out}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("xtask_size_test_{n}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_lines(dir: &Path, name: &str, n: usize) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        let body: String = (0..n)
            .map(|i| format!("// line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn physical_line_count_matches_split() {
        assert_eq!(count_physical_lines(""), 0);
        assert_eq!(count_physical_lines("a\nb"), 2);
        assert_eq!(count_physical_lines("a\nb\n"), 3);
    }

    #[test]
    fn unallowlisted_file_over_limit_fails() {
        let root = temp_root();
        write_lines(&root.join("crates/demo/src"), "big.rs", 501);
        let audit = audit_allowlist(&root, "").expect("scan");
        assert_eq!(audit.scanned, 1, "{:?}", audit.findings);
        assert!(audit
            .findings
            .iter()
            .any(|f| { f.kind == SizeFindingKind::OverLimit && f.lines == 501 }));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn allowlisted_file_above_ceiling_fails() {
        let root = temp_root();
        write_lines(&root.join("crates/demo/src"), "big.rs", 600);
        let allow = r#"
[[entry]]
file = "crates/demo/src/big.rs"
ceiling = 590
reason = "test"
"#;
        let audit = audit_allowlist(&root, allow).expect("scan");
        assert!(audit.findings.iter().any(|f| {
            f.kind == SizeFindingKind::AboveCeiling && f.lines == 600 && f.ceiling == Some(590)
        }));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_allowlist_row_when_file_shrunk_fails() {
        let root = temp_root();
        write_lines(&root.join("crates/demo/src"), "small.rs", 400);
        let allow = r#"
[[entry]]
file = "crates/demo/src/small.rs"
ceiling = 900
reason = "test"
"#;
        let audit = audit_allowlist(&root, allow).expect("scan");
        assert!(audit
            .findings
            .iter()
            .any(|f| f.kind == SizeFindingKind::StaleAllowlist));
        let _ = fs::remove_dir_all(&root);
    }
}
