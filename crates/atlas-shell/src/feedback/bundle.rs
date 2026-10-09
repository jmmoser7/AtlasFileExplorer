use super::recorder::RecordedStep;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReportJson {
    pub app: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_hash: Option<String>,
    pub os: String,
    pub kind: String,
    pub description: String,
    pub links: Vec<String>,
    pub steps: Vec<RecordedStep>,
    pub reproduce: bool,
    pub created_at: String,
    pub bundle_dir: String,
}

pub fn repo_root_from(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    for _ in 0..24 {
        if dir.join("CONSTITUTION.md").is_file() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

pub fn repo_root_from_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    repo_root_from(exe.parent()?)
}

pub fn default_reports_dir() -> PathBuf {
    repo_root_from_exe()
        .map(|r| r.join("feedback").join("client"))
        .unwrap_or_else(|| atlas_core::index::data_dir().join("feedback"))
}

pub fn git_hash_short() -> Option<String> {
    let root = repo_root_from_exe()?;
    let head = std::fs::read_to_string(root.join(".git/HEAD")).ok()?;
    let hash = if let Some(rest) = head.strip_prefix("ref: ") {
        let ref_path = rest.trim();
        std::fs::read_to_string(root.join(".git").join(ref_path))
            .ok()?
            .trim()
            .to_string()
    } else {
        head.trim().to_string()
    };
    Some(hash.chars().take(12).collect())
}

pub fn slug_from_description(desc: &str) -> String {
    let mut slug = String::new();
    for ch in desc.chars().take(48) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_whitespace() && !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_matches('-').chars().take(32).collect()
}

pub fn write_bundle(
    reports_dir: &Path,
    report: &ReportJson,
    attachments: &[(PathBuf, Vec<u8>)],
) -> Result<PathBuf, String> {
    let stamp = Local::now().format("%Y-%m-%d_%H%M%S");
    let slug = slug_from_description(&report.description);
    let slug = if slug.is_empty() {
        "report".into()
    } else {
        slug
    };
    let dir = reports_dir.join(format!("{stamp}-{}-{}", report.kind, slug));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json_path = dir.join("report.json");
    let mut report_on_disk = report.clone();
    report_on_disk.bundle_dir = dir.display().to_string();
    let json = serde_json::to_string_pretty(&report_on_disk).map_err(|e| e.to_string())?;
    std::fs::write(&json_path, json).map_err(|e| e.to_string())?;

    let mut steps_txt = String::new();
    for step in &report.steps {
        steps_txt.push_str(&format!("+{} ms\t{}\n", step.at_ms, step.label));
    }
    std::fs::write(dir.join("steps.txt"), steps_txt).map_err(|e| e.to_string())?;

    for (i, (name, bytes)) in attachments.iter().enumerate() {
        let file_name = name
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment.png");
        let dest = dir.join(format!("{i:02}_{file_name}"));
        std::fs::write(dest, bytes).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::recorder::RecordedStep;

    #[test]
    fn repo_root_finds_constitution() {
        let root = std::env::temp_dir().join(format!("atlas_repo_root_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let inner = root.join("apps").join("slate");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(root.join("CONSTITUTION.md"), "# law").unwrap();
        assert_eq!(repo_root_from(&inner), Some(root.clone()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bundle_writes_expected_files() {
        let tmp = std::env::temp_dir().join(format!("atlas_bundle_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let report = ReportJson {
            app: "test".into(),
            version: "0".into(),
            git_hash: None,
            os: "test".into(),
            kind: "bug".into(),
            description: "Something broke".into(),
            links: vec!["https://example.com".into()],
            steps: vec![RecordedStep {
                at_ms: 10,
                label: "app.undo".into(),
            }],
            reproduce: true,
            created_at: "now".into(),
            bundle_dir: String::new(),
        };
        let png = vec![0x89, 0x50, 0x4E, 0x47];
        let dir = write_bundle(&tmp, &report, &[(PathBuf::from("shot.png"), png)]).unwrap();
        assert!(dir.join("report.json").is_file());
        assert!(dir.join("steps.txt").is_file());
        assert!(dir.join("00_shot.png").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
