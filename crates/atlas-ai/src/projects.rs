//! Project folders an agent portal can reopen: the ones Slate's own agent
//! conversations used, merged with each provider's recent projects.
//!
//! I/O only — never call from the frame loop (Constitution Art. II).

use std::cmp::Reverse;
use std::path::{Path, PathBuf};

use atlas_shell::recent::RecentEntry;

use crate::config::LINK_DIR;

/// Newest link folders a scan inspects. An AI workspace can hold thousands.
const LINK_SCAN_LIMIT: usize = 400;
/// Longest list the picker offers.
const MAX_PROJECTS: usize = 60;

/// Project folders earlier agent conversations wrote outputs into, newest
/// first. Each link folder's `output.json` records
/// `<project>/slate-outputs/<board>/<run>`; its newest session file dates the
/// conversation. A folder's own listed time updates lazily on NTFS, so it only
/// picks the candidates. Files still in the cloud are skipped, never downloaded.
pub fn session_projects(ai_workspace: &Path) -> Vec<RecentEntry> {
    let Ok(read) = std::fs::read_dir(ai_workspace.join(LINK_DIR).join("agent")) else {
        return Vec::new();
    };
    let mut links: Vec<(u64, PathBuf)> = read
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            meta.is_dir()
                .then(|| (unix_secs(meta.modified().ok()), entry.path()))
        })
        .collect();
    links.sort_by_key(|l| Reverse(l.0));
    links.truncate(LINK_SCAN_LIMIT);
    let mut out = Vec::new();
    for (listed, link) in links {
        let record = link.join("output.json");
        if atlas_core::cloud::is_dehydrated(&record) {
            continue;
        }
        let Some(project) =
            crate::agent::recorded_output_dir(&link).and_then(|d| project_of_output(&d))
        else {
            continue;
        };
        let at = [record, link.join("session.json")]
            .iter()
            .filter_map(|f| std::fs::metadata(f).ok())
            .map(|m| unix_secs(m.modified().ok()))
            .max()
            .unwrap_or(listed);
        out.push(entry(project, at));
    }
    out.sort_by_key(|e| Reverse(e.opened_at));
    out
}

/// `<project>/slate-outputs/<board>/<run>` → `<project>`.
fn project_of_output(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|a| a.file_name().is_some_and(|n| n == "slate-outputs"))
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

/// A picker entry titled by the folder name.
pub fn entry(path: PathBuf, opened_at: u64) -> RecentEntry {
    let title = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Project")
        .to_string();
    RecentEntry {
        path,
        title,
        opened_at,
        cover: None,
    }
}

/// One entry per existing folder, compared by canonical path, newest first.
/// Equal times keep list order, so earlier lists win ties and keep their
/// titles; undated provider entries follow everything Slate dated itself.
pub fn merge(lists: impl IntoIterator<Item = Vec<RecentEntry>>) -> Vec<RecentEntry> {
    let mut out: Vec<(PathBuf, RecentEntry)> = Vec::new();
    for entry in lists.into_iter().flatten() {
        if !entry.path.is_dir() {
            continue;
        }
        let key = folder_key(&entry.path);
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some((_, kept)) => kept.opened_at = kept.opened_at.max(entry.opened_at),
            None => out.push((key, entry)),
        }
    }
    out.sort_by_key(|(_, e)| Reverse(e.opened_at));
    out.truncate(MAX_PROJECTS);
    out.into_iter().map(|(_, e)| e).collect()
}

fn folder_key(path: &Path) -> PathBuf {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if cfg!(windows) {
        PathBuf::from(canon.to_string_lossy().to_lowercase())
    } else {
        canon
    }
}

fn unix_secs(t: Option<std::time::SystemTime>) -> u64 {
    t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas_projects_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn link(ws: &Path, session: &str, project: &Path, at: u64) {
        let dir = crate::agent::agent_dir(ws, session);
        std::fs::create_dir_all(&dir).unwrap();
        let out = project.join("slate-outputs").join("board").join("run");
        let record = dir.join("output.json");
        crate::agent::atomic_write_json(&record, &serde_json::json!({ "dir": out })).unwrap();
        std::fs::File::options()
            .write(true)
            .open(record)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(at))
            .unwrap();
    }

    #[test]
    fn an_output_folder_names_its_project() {
        let project = Path::new("/work/climate");
        let out = project.join("slate-outputs").join("board").join("2026-run");
        assert_eq!(project_of_output(&out).as_deref(), Some(project));
        assert_eq!(project_of_output(Path::new("/work/elsewhere/run")), None);
    }

    #[test]
    fn link_folders_name_the_projects_they_wrote_into_newest_first() {
        let root = temp("links");
        let ws = root.join("ws");
        let (a, b) = (root.join("a"), root.join("b"));
        link(&ws, "two", &b, 2_000_000_000);
        link(&ws, "one", &a, 1_000_000_000);
        std::fs::create_dir_all(crate::agent::agent_dir(&ws, "no-outputs")).unwrap();
        let found: Vec<PathBuf> = session_projects(&ws).into_iter().map(|e| e.path).collect();
        assert_eq!(found, vec![b, a]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn merge_keeps_slate_projects_beside_the_provider_list() {
        let root = temp("merge");
        let (mru, used, provider, gone) = (
            root.join("mru"),
            root.join("used"),
            root.join("provider"),
            root.join("gone"),
        );
        for d in [&mru, &used, &provider] {
            std::fs::create_dir_all(d).unwrap();
        }
        let merged = merge([
            vec![entry(mru.clone(), 20)],
            vec![
                entry(used.clone(), 30),
                entry(gone, 40),
                entry(mru.clone(), 10),
            ],
            vec![entry(provider.clone(), 0), entry(used.clone(), 0)],
        ]);
        let paths: Vec<PathBuf> = merged.iter().map(|e| e.path.clone()).collect();
        assert_eq!(paths, vec![used, mru, provider]);
        let _ = std::fs::remove_dir_all(root);
    }
}
