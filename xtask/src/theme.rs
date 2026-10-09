//! Guard against hardcoded [`Color32`] literals in board and panel UI paths.
//!
//! Comments, string and char literals, and `#[cfg(test)]` items are not
//! code that paints chrome, so they are blanked before matching. Allowlist
//! rows name a code fragment rather than a line number, so an edit above a
//! row does not move it; a row that matches nothing is itself a finding.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const ALLOWLIST: &str = include_str!("../theme-allowlist.toml");

#[derive(Debug, Deserialize)]
struct AllowlistFile {
    #[serde(default)]
    entry: Vec<AllowEntry>,
}

#[derive(Debug, Deserialize)]
struct AllowEntry {
    file: String,
    /// A fragment of the code line, after comments and strings are blanked.
    contains: String,
    #[allow(dead_code)]
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

fn rs_files_in(dir: &Path, keep: impl Fn(&str) -> bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_file() && name.ends_with(".rs") && keep(name) {
            out.push(path);
        }
    }
}

fn scoped_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for app in ["slate", "file-atlas"] {
        let app_root = root.join("apps").join(app).join("src").join("app");
        if !app_root.is_dir() {
            continue;
        }
        rs_files_in(
            &app_root,
            |name| {
                name.starts_with("board_")
                    || name == "present.rs"
                    || (name == "mod.rs" && app == "file-atlas")
            },
            &mut out,
        );
        rs_files_in(&app_root.join("ui"), |_| true, &mut out);
        if let Ok(entries) = std::fs::read_dir(&app_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if path.is_dir() && name.starts_with("board_") {
                    rs_files_in(&path, |_| true, &mut out);
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
];

/// `text` with comments and the insides of string / char literals replaced
/// by spaces. Newlines are kept so line numbers still line up.
pub(crate) fn blank_non_code(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(chars[i]));
                    i += 1;
                }
            }
        } else if c == 'r' && matches!(next, Some('"') | Some('#')) && !ident_before(&chars, i) {
            let mut j = i + 1;
            let mut hashes = 0;
            while chars.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if chars.get(j) != Some(&'"') {
                out.push(c);
                i += 1;
                continue;
            }
            for _ in i..=j {
                out.push(' ');
            }
            i = j + 1;
            while i < chars.len() {
                if chars[i] == '"' && (1..=hashes).all(|k| chars.get(i + k) == Some(&'#')) {
                    for _ in 0..=hashes {
                        out.push(' ');
                    }
                    i += hashes + 1;
                    break;
                }
                out.push(blank(chars[i]));
                i += 1;
            }
        } else if c == '"' {
            out.push(' ');
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' {
                    out.push(' ');
                    i += 1;
                }
                if i < chars.len() {
                    out.push(blank(chars[i]));
                    i += 1;
                }
            }
            out.push(' ');
            i += 1;
        } else if c == '\'' {
            // A char literal is `'x'` or `'\…'`; a lifetime has no closing quote.
            let close = if next == Some('\\') {
                (i + 2..chars.len().min(i + 12)).find(|&k| chars[k] == '\'')
            } else if chars.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            match close {
                Some(end) => {
                    for _ in i..=end {
                        out.push(' ');
                    }
                    i = end + 1;
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn ident_before(chars: &[char], i: usize) -> bool {
    i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')
}

/// Per line of blanked `code`: is it inside a `#[cfg(test)]` item?
pub(crate) fn test_lines(code: &str) -> Vec<bool> {
    let mut out = Vec::new();
    // Saw `#[cfg(test)]`; the next item (after any further attributes) is skipped.
    let mut pending = false;
    let mut skipping = false;
    let mut depth = 0i32;
    let mut opened = false;
    for line in code.lines() {
        let trimmed = line.trim();
        let compact: String = trimmed.split_whitespace().collect();
        let mut item = trimmed;
        if !skipping && compact.starts_with("#[cfg(test)]") {
            pending = true;
            item = trimmed[trimmed.find(']').map_or(trimmed.len(), |k| k + 1)..].trim();
        }
        if !skipping && pending && !item.is_empty() && !item.starts_with("#[") {
            pending = false;
            skipping = true;
            depth = 0;
            opened = false;
        }
        if !skipping {
            out.push(pending);
            continue;
        }
        out.push(true);
        for c in item.chars() {
            match c {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        if (opened && depth <= 0) || (!opened && item.ends_with(';')) {
            skipping = false;
        }
    }
    out
}

pub fn audit(root: &Path) -> Result<ThemeAudit, Box<dyn std::error::Error>> {
    let parsed: AllowlistFile = toml::from_str(ALLOWLIST)?;
    let mut used = vec![false; parsed.entry.len()];

    let mut findings = Vec::new();
    let files = scoped_files(root);
    for path in &files {
        let rel = normalize_rel(path, root);
        let text = std::fs::read_to_string(path)?;
        let code = blank_non_code(&text);
        let tests = test_lines(&code);
        for (i, (line, original)) in code.lines().zip(text.lines()).enumerate() {
            if tests.get(i).copied().unwrap_or(false) {
                continue;
            }
            if !PATTERNS.iter().any(|p| line.contains(p)) {
                continue;
            }
            let allowed = parsed.entry.iter().enumerate().find(|(_, e)| {
                e.file.replace('\\', "/") == rel && line.contains(e.contains.as_str())
            });
            if let Some((k, _)) = allowed {
                used[k] = true;
                continue;
            }
            findings.push(ThemeFinding {
                file: path.clone(),
                line: (i + 1) as u32,
                snippet: original.trim().to_string(),
            });
        }
    }
    for (entry, used) in parsed.entry.iter().zip(&used) {
        if !used {
            findings.push(ThemeFinding {
                file: PathBuf::from(&entry.file),
                line: 0,
                snippet: format!("stale allowlist row: `{}`", entry.contains),
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_strings_are_not_code() {
        let src = "let a = 1; // Color32::from_rgb(1, 2, 3)\n\
                   let s = \"Color32::from_gray(9)\";\n\
                   /* Color32::from_gray(1) /* nested */ still */ let c = '{';\n\
                   let r = r#\"Color32::from_rgb\"#;\n\
                   let real = Color32::from_gray(4);\n";
        let code = blank_non_code(src);
        let hits: Vec<usize> = code
            .lines()
            .enumerate()
            .filter(|(_, l)| PATTERNS.iter().any(|p| l.contains(p)))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(hits, vec![4]);
        assert_eq!(code.lines().count(), src.lines().count());
        assert!(!code.lines().nth(2).unwrap().contains('{'));
    }

    #[test]
    fn any_cfg_test_item_is_skipped_including_nested_braces() {
        let src = "fn ui() {}\n\
                   #[cfg(test)]\n\
                   mod shape_text_layout {\n\
                   \x20   fn a() { if x { y } }\n\
                   \x20   mod inner { fn b() {} }\n\
                   }\n\
                   fn after() {}\n\
                   #[cfg(test)]\n\
                   use foo::bar;\n\
                   fn last() {}\n";
        let flags = test_lines(&blank_non_code(src));
        assert_eq!(
            flags,
            vec![false, true, true, true, true, true, false, true, true, false]
        );
    }
}
