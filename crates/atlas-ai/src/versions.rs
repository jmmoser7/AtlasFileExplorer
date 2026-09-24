//! Per-message copies of the files an agent created or changed, so the board
//! can show how a deliverable evolved. Copies live in the link folder in the AI
//! workspace beside the conversation record, never inside a workbook (Art. IX):
//! `<link_dir>/versions/t{turn:03}/<file name>`, indexed by `versions.json`.

use std::collections::HashMap;
use std::path::Path;

use atlas_agent::{AgentSession, AgentStatus, ArtifactKind};
pub use atlas_agent::{Version, VersionedFile, Versions};

use crate::agent::atomic_write_json;

const MAX_COPY: u64 = 64 * 1024 * 1024;
const MAX_DIFF: u64 = 2 * 1024 * 1024;
const TEXT: &[&str] = &[
    "html", "htm", "css", "js", "ts", "json", "md", "txt", "csv", "tsv", "svg", "xml", "py", "rs",
    "toml", "yaml", "yml",
];

/// Copy each Created or Modified output of every completed message once per
/// (source, turn). A message is complete when the session is not running or a
/// later assistant message exists. Directories, missing and relative paths are
/// left for a later call; cloud placeholders, files over 64 MB, and files the
/// agent changed again before capture are recorded as skipped. Returns whether
/// `versions.json` changed. Blocking I/O: call from a worker.
pub fn capture(link_dir: &Path, session: &AgentSession) -> std::io::Result<bool> {
    let running = session.status == AgentStatus::Thinking;
    let complete = |turn: usize| {
        !running
            || session
                .turns
                .iter()
                .enumerate()
                .any(|(i, t)| i > turn && t.role == "assistant")
    };
    let mut due: Vec<(usize, &str, ArtifactKind)> = Vec::new();
    for a in &session.artifacts {
        if !matches!(a.kind, ArtifactKind::Created | ArtifactKind::Modified) || !complete(a.turn) {
            continue;
        }
        match due
            .iter_mut()
            .find(|(t, s, _)| *t == a.turn && *s == a.source)
        {
            Some(entry) if a.kind == ArtifactKind::Created => entry.2 = a.kind,
            Some(_) => {}
            None => due.push((a.turn, &a.source, a.kind)),
        }
    }
    if due.is_empty() {
        return Ok(false);
    }
    due.sort_by_key(|(turn, _, _)| *turn);
    let mut versions = read_versions(link_dir)?;
    let mut changed = false;
    for (turn, source, kind) in due {
        let captured = versions
            .files
            .iter()
            .any(|f| f.source == source && f.versions.iter().any(|v| v.turn == turn));
        if captured {
            continue;
        }
        let path = Path::new(source);
        if !path.is_absolute() {
            continue;
        }
        let Ok(meta) = std::fs::metadata(path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let later = session
            .artifacts
            .iter()
            .any(|a| a.source == source && a.kind.is_output() && a.turn > turn);
        let skipped = if later {
            Some("changed again before capture")
        } else if atlas_core::cloud::is_dehydrated(path) {
            Some("cloud file not on this machine")
        } else if meta.len() > MAX_COPY {
            Some("larger than 64 MB")
        } else {
            None
        };
        let version = match skipped {
            Some(reason) => Version {
                turn,
                path: String::new(),
                bytes: meta.len(),
                added: None,
                removed: None,
                skipped: Some(reason.into()),
            },
            None => {
                let name = unique_name(&versions, turn, path);
                let rel = format!("versions/t{turn:03}/{name}");
                let dest = link_dir.join(&rel);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let Ok(bytes) = std::fs::copy(path, &dest) else {
                    continue;
                };
                let previous = versions
                    .files
                    .iter()
                    .find(|f| f.source == source)
                    .and_then(|f| {
                        f.versions
                            .iter()
                            .rev()
                            .find(|v| v.turn < turn && v.skipped.is_none() && !v.path.is_empty())
                    })
                    .map(|v| link_dir.join(&v.path));
                let (added, removed) = if is_text(path) && bytes <= MAX_DIFF {
                    match (read_text(&dest), previous.as_deref().map(read_text)) {
                        (Some(new), Some(Some(old))) => {
                            let (a, r) = line_changes(&old, &new);
                            (Some(a), Some(r))
                        }
                        (Some(new), None) if kind == ArtifactKind::Created => {
                            (Some(new.lines().count() as u32), Some(0))
                        }
                        _ => (None, None),
                    }
                } else {
                    (None, None)
                };
                Version {
                    turn,
                    path: rel,
                    bytes,
                    added,
                    removed,
                    skipped: None,
                }
            }
        };
        let file = match versions.files.iter().position(|f| f.source == source) {
            Some(i) => &mut versions.files[i],
            None => {
                versions.files.push(VersionedFile {
                    source: source.to_string(),
                    versions: Vec::new(),
                });
                versions.files.last_mut().unwrap()
            }
        };
        file.versions.push(version);
        file.versions.sort_by_key(|v| v.turn);
        changed = true;
    }
    if changed {
        atomic_write_json(&link_dir.join("versions.json"), &versions)?;
    }
    Ok(changed)
}

/// Captured versions, oldest first per file. Empty when absent or unreadable.
pub fn load_versions(link_dir: &Path) -> Versions {
    read_versions(link_dir).unwrap_or_default()
}

fn read_versions(link_dir: &Path) -> std::io::Result<Versions> {
    match std::fs::read(link_dir.join("versions.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Versions::default()),
        Err(e) => Err(e),
    }
}

fn unique_name(versions: &Versions, turn: usize, source: &Path) -> String {
    let prefix = format!("versions/t{turn:03}/");
    let taken: Vec<&str> = versions
        .files
        .iter()
        .flat_map(|f| &f.versions)
        .filter_map(|v| v.path.strip_prefix(&prefix))
        .collect();
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    if !taken.contains(&name.as_str()) {
        return name;
    }
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let ext = source
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|n| format!("{stem}-{n}{ext}"))
        .find(|c| !taken.contains(&c.as_str()))
        .unwrap()
}

fn is_text(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| TEXT.contains(&e.to_ascii_lowercase().as_str()))
}

/// A copy in the link folder, which may itself sit in a synced AI workspace.
fn read_text(path: &Path) -> Option<String> {
    if atlas_core::cloud::is_dehydrated(path) || std::fs::metadata(path).ok()?.len() > MAX_DIFF {
        return None;
    }
    String::from_utf8(std::fs::read(path).ok()?).ok()
}

/// Lines added and removed, compared as multisets (order-insensitive).
fn line_changes(old: &str, new: &str) -> (u32, u32) {
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for line in new.lines() {
        *counts.entry(line).or_default() += 1;
    }
    for line in old.lines() {
        *counts.entry(line).or_default() -= 1;
    }
    let added = counts.values().filter(|&&n| n > 0).sum::<i64>();
    let removed = -counts.values().filter(|&&n| n < 0).sum::<i64>();
    (added as u32, removed as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_agent::{AgentArtifact, AgentTurn};
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas_ai_versions_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn artifact(turn: usize, kind: ArtifactKind, source: &Path) -> AgentArtifact {
        AgentArtifact {
            id: format!("{turn}:{}", source.display()),
            turn,
            kind,
            source: source.to_string_lossy().into_owned(),
            title: String::new(),
        }
    }

    fn session(roles: &[&str], status: AgentStatus, artifacts: Vec<AgentArtifact>) -> AgentSession {
        AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts,
            status,
            provider: "cursor".into(),
            turns: roles
                .iter()
                .map(|r| AgentTurn {
                    role: (*r).into(),
                    text: String::new(),
                    at: 0,
                })
                .collect(),
            updated_at: 0,
            bundle: Default::default(),
            request: String::new(),
        }
    }

    #[test]
    fn completed_messages_are_copied_once_with_line_counts() {
        let root = temp("copy");
        let link = root.join("link");
        let out = root.join("out");
        std::fs::create_dir_all(&link).unwrap();
        std::fs::create_dir_all(out.join("sub")).unwrap();
        let page = out.join("page.html");
        std::fs::write(&page, "a\nb\nc\n").unwrap();
        let mut arts = vec![artifact(1, ArtifactKind::Created, &page)];

        // Running and no later assistant turn: not complete yet.
        let s = session(&["user", "assistant"], AgentStatus::Thinking, arts.clone());
        assert!(!capture(&link, &s).unwrap());
        assert!(!link.join("versions.json").exists());

        let s = session(&["user", "assistant"], AgentStatus::Idle, arts.clone());
        assert!(capture(&link, &s).unwrap());
        assert!(!capture(&link, &s).unwrap(), "once per (source, turn)");
        let v = load_versions(&link);
        let first = &v.files[0].versions[0];
        assert_eq!(first.path, "versions/t001/page.html");
        assert_eq!((first.added, first.removed), (Some(3), Some(0)));
        assert_eq!(first.bytes, 6);

        std::fs::write(&page, "a\nc\nd\ne\n").unwrap();
        arts.push(artifact(3, ArtifactKind::Modified, &page));
        // A later assistant message completes turn 3 even while the session runs.
        let s = session(
            &[
                "user",
                "assistant",
                "user",
                "assistant",
                "user",
                "assistant",
            ],
            AgentStatus::Thinking,
            arts,
        );
        assert!(capture(&link, &s).unwrap());
        let v = load_versions(&link);
        let second = &v.files[0].versions[1];
        assert_eq!(second.turn, 3);
        assert_eq!((second.added, second.removed), (Some(2), Some(1)));
        assert_eq!(
            std::fs::read_to_string(link.join(&second.path)).unwrap(),
            "a\nc\nd\ne\n"
        );
        assert_eq!(
            std::fs::read_to_string(link.join(&v.files[0].versions[0].path)).unwrap(),
            "a\nb\nc\n"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn same_names_in_one_turn_do_not_collide() {
        let root = temp("names");
        let link = root.join("link");
        std::fs::create_dir_all(&link).unwrap();
        let a = root.join("a/index.html");
        let b = root.join("b/index.html");
        for p in [&a, &b] {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        let s = session(
            &["user", "assistant"],
            AgentStatus::Idle,
            vec![
                artifact(1, ArtifactKind::Modified, &a),
                artifact(1, ArtifactKind::Modified, &b),
            ],
        );
        capture(&link, &s).unwrap();
        let paths: Vec<_> = load_versions(&link)
            .files
            .iter()
            .map(|f| f.versions[0].path.clone())
            .collect();
        assert_eq!(
            paths,
            ["versions/t001/index.html", "versions/t001/index-2.html"]
        );
        let modified = &load_versions(&link).files[0].versions[0];
        assert_eq!(modified.added, None, "no baseline for a modified file");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn skip_rules() {
        let root = temp("skip");
        let link = root.join("link");
        std::fs::create_dir_all(&link).unwrap();
        let folder = root.join("folder");
        std::fs::create_dir_all(&folder).unwrap();
        let big = root.join("big.bin");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(MAX_COPY + 1)
            .unwrap();
        let redone = root.join("redone.md");
        std::fs::write(&redone, "new").unwrap();
        let deleted = root.join("deleted.txt");
        let s = session(
            &["user", "assistant", "user", "assistant"],
            AgentStatus::Idle,
            vec![
                artifact(1, ArtifactKind::Created, &folder),
                artifact(1, ArtifactKind::Created, &root.join("missing.txt")),
                artifact(1, ArtifactKind::Created, Path::new("relative.txt")),
                artifact(1, ArtifactKind::Created, &big),
                artifact(1, ArtifactKind::Created, &redone),
                artifact(3, ArtifactKind::Modified, &redone),
                artifact(3, ArtifactKind::Deleted, &deleted),
                artifact(3, ArtifactKind::Read, &redone),
            ],
        );
        assert!(capture(&link, &s).unwrap());
        let v = load_versions(&link);
        let find = |p: &Path| {
            v.files
                .iter()
                .find(|f| Path::new(&f.source) == p)
                .map(|f| f.versions.clone())
        };
        assert!(find(&folder).is_none());
        assert!(find(&root.join("missing.txt")).is_none());
        let big_v = find(&big).unwrap();
        assert_eq!(big_v[0].skipped.as_deref(), Some("larger than 64 MB"));
        assert!(big_v[0].path.is_empty());
        let redone_v = find(&redone).unwrap();
        assert_eq!(redone_v.len(), 2);
        assert_eq!(
            redone_v[0].skipped.as_deref(),
            Some("changed again before capture")
        );
        assert_eq!(redone_v[1].turn, 3);
        assert!(redone_v[1].skipped.is_none());
        assert_eq!(
            redone_v[1].added, None,
            "the only earlier version was skipped"
        );
        assert_eq!(v.files.len(), 2);
        assert!(!link.join("versions/t001/big.bin").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn line_changes_are_a_multiset_difference() {
        assert_eq!(line_changes("a\nb\nb\n", "b\na\nc\n"), (1, 1));
        assert_eq!(line_changes("", "x\ny"), (2, 0));
        assert_eq!(line_changes("x\ny", ""), (0, 2));
    }
}
