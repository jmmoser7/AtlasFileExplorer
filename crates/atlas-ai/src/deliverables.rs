//! Deliverables an agent names in `return.json`, recorded per message in
//! `deliverables.json` beside `session.json`. The workbook only links to them
//! (Art. IX). Existence checks only: no deliverable's bytes are read here.

use std::path::{Component, Path, PathBuf};

use atlas_agent::{AgentSession, Deliverable, DeliverableSet, Deliverables, ReturnManifest};

use crate::agent::atomic_write_json;

/// Consume `<link_dir>/return.json` if present. Relative paths resolve against
/// `bases` in order (output folder, working folder, AI workspace); the first
/// base where the path exists wins. The set is appended to `deliverables.json`
/// (replacing a set with the same id), `return.result.json` reports the
/// outcome, and `return.json` is removed. An unreadable manifest is removed too
/// and reported in the result file, so the agent can write a corrected one.
/// Blocking I/O: call from a worker.
pub fn consume_return(
    link_dir: &Path,
    session: &AgentSession,
    bases: &[&Path],
) -> Result<Option<DeliverableSet>, String> {
    let request = link_dir.join("return.json");
    if !request.is_file() || atlas_core::cloud::is_dehydrated(&request) {
        return Ok(None);
    }
    let raw = std::fs::read(&request).map_err(|e| format!("Could not read return.json: {e}"))?;
    let manifest = match serde_json::from_slice::<ReturnManifest>(&raw) {
        Ok(m) if !m.items.is_empty() => m,
        Ok(_) => return Err(reject(link_dir, &request, "return.json names no items")),
        Err(e) => return Err(reject(link_dir, &request, &format!("return.json: {e}"))),
    };
    let mut missing = Vec::new();
    let mut items = Vec::new();
    for item in &manifest.items {
        let Some(path) = resolve(&item.path, bases) else {
            missing.push(item.path.clone());
            continue;
        };
        let feeds = item.feeds.as_deref().and_then(|f| {
            let found = resolve(f, bases);
            if found.is_none() {
                missing.push(f.to_string());
            }
            found
        });
        items.push(Deliverable {
            path: path.to_string_lossy().into_owned(),
            face: item.face,
            feeds: feeds.map(|f| f.to_string_lossy().into_owned()),
        });
    }
    // An agent writes return.json during its reply, before that reply is in
    // the history: after a person's message the set belongs to the slot the
    // reply will take, the same slot its artifacts record.
    let turn = match session.turns.last() {
        Some(t) if t.role == "user" => session.turns.len(),
        _ => session
            .turns
            .iter()
            .rposition(|t| t.role == "assistant")
            .or_else(|| session.artifacts.iter().map(|a| a.turn).max())
            .unwrap_or(0),
    };
    let id = if manifest.id.trim().is_empty() {
        format!("return-{}", crate::context::now_secs())
    } else {
        manifest.id.trim().to_string()
    };
    let set = DeliverableSet {
        id,
        turn,
        title: manifest.title.trim().to_string(),
        items,
        missing,
    };
    let record = link_dir.join("deliverables.json");
    let mut all = if record.is_file() {
        let bytes = std::fs::read(&record).map_err(|e| format!("deliverables.json: {e}"))?;
        serde_json::from_slice::<Deliverables>(&bytes)
            .map_err(|e| format!("deliverables.json is unreadable, left unchanged: {e}"))?
    } else {
        Deliverables::default()
    };
    all.sets.retain(|s| s.id != set.id);
    all.sets.push(set.clone());
    atomic_write_json(&record, &all).map_err(|e| format!("deliverables.json: {e}"))?;
    let _ = atomic_write_json(
        &link_dir.join("return.result.json"),
        &serde_json::json!({
            "id": set.id,
            "ok": true,
            "count": set.items.len(),
            "missing": set.missing,
        }),
    );
    let _ = std::fs::remove_file(&request);
    Ok(Some(set))
}

/// Every consumed deliverable set, oldest first. Empty when absent or unreadable.
pub fn load_deliverables(link_dir: &Path) -> Deliverables {
    std::fs::read(link_dir.join("deliverables.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn reject(link_dir: &Path, request: &Path, error: &str) -> String {
    let _ = atomic_write_json(
        &link_dir.join("return.result.json"),
        &serde_json::json!({ "ok": false, "error": error }),
    );
    let _ = std::fs::remove_file(request);
    error.to_string()
}

fn resolve(written: &str, bases: &[&Path]) -> Option<PathBuf> {
    let written = written.trim();
    if written.is_empty() {
        return None;
    }
    let path = Path::new(written);
    if path.is_absolute() {
        let path = normalize(path);
        return path.exists().then_some(path);
    }
    bases
        .iter()
        .map(|base| normalize(&base.join(path)))
        .find(|p| p.exists())
}

/// Lexical `.` and `..` removal, so a resolved path compares equal to the
/// artifact sources providers record. Never touches the filesystem.
pub(crate) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_agent::{AgentArtifact, AgentStatus, AgentTurn, ArtifactKind, Face};

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas_ai_deliverables_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn session(turns: &[&str], artifact_turns: &[usize]) -> AgentSession {
        AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: artifact_turns
                .iter()
                .map(|&turn| AgentArtifact {
                    id: format!("a{turn}"),
                    turn,
                    kind: ArtifactKind::Created,
                    source: "x".into(),
                    title: "x".into(),
                })
                .collect(),
            status: AgentStatus::Idle,
            provider: "cursor".into(),
            turns: turns
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
    fn items_resolve_through_bases_in_order_and_missing_are_named() {
        let root = temp("resolve");
        let link = root.join("link");
        let out = root.join("out");
        let work = root.join("work");
        for d in [&link, &out, &work, &out.join("assets")] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(out.join("dash.html"), "<html>").unwrap();
        std::fs::write(work.join("dash.html"), "<html>").unwrap();
        std::fs::write(work.join("data.csv"), "a,b").unwrap();
        let abs = root.join("abs.png");
        std::fs::write(&abs, "png").unwrap();
        std::fs::write(
            link.join("return.json"),
            serde_json::json!({"id":"q3","title":" Q3 ","items":[
                {"path":"./dash.html","as":"graphic"},
                {"path":"data.csv","feeds":"dash.html"},
                {"path":abs.to_string_lossy(),"as":"auto"},
                {"path":"gone.txt"},
                {"path":"assets","as":"folder"}
            ]})
            .to_string(),
        )
        .unwrap();
        let s = session(&["user", "assistant", "user", "assistant"], &[1]);
        let set = consume_return(&link, &s, &[&out, &work]).unwrap().unwrap();
        assert_eq!(set.turn, 3);
        assert_eq!(set.title, "Q3");
        assert_eq!(set.items.len(), 4);
        assert_eq!(Path::new(&set.items[0].path), out.join("dash.html"));
        assert_eq!(set.items[0].face, Face::Graphic);
        assert_eq!(Path::new(&set.items[1].path), work.join("data.csv"));
        assert_eq!(
            set.items[1].feeds.as_deref().map(Path::new),
            Some(out.join("dash.html").as_path())
        );
        assert_eq!(Path::new(&set.items[2].path), abs);
        assert_eq!(set.items[3].face, Face::Folder);
        assert_eq!(set.missing, vec!["gone.txt".to_string()]);
        assert!(!link.join("return.json").exists());
        let result: serde_json::Value =
            serde_json::from_slice(&std::fs::read(link.join("return.result.json")).unwrap())
                .unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["count"], 4);
        assert_eq!(result["missing"][0], "gone.txt");
        assert_eq!(load_deliverables(&link).sets, vec![set.clone()]);
        assert_eq!(consume_return(&link, &s, &[&out]).unwrap(), None);

        // The same id again replaces rather than duplicates; a new id appends.
        std::fs::write(
            link.join("return.json"),
            r#"{"id":"q3","items":[{"path":"dash.html"}]}"#,
        )
        .unwrap();
        consume_return(&link, &s, &[&out]).unwrap();
        std::fs::write(
            link.join("return.json"),
            r#"{"id":"q4","items":[{"path":"dash.html"}]}"#,
        )
        .unwrap();
        consume_return(&link, &s, &[&out]).unwrap();
        let all = load_deliverables(&link);
        assert_eq!(all.version, 1);
        assert_eq!(
            all.sets.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["q3", "q4"]
        );
        assert_eq!(all.sets[0].items.len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_image_return_becomes_one_images_item() {
        let root = temp("legacy");
        let link = root.join("link");
        let ws = root.join("ws");
        std::fs::create_dir_all(&link).unwrap();
        std::fs::create_dir_all(ws.join("penn-images")).unwrap();
        std::fs::write(
            link.join("return.json"),
            r#"{"id":"penn","kind":"images","title":"Penn Station","path":"penn-images"}"#,
        )
        .unwrap();
        // Written mid-reply: the history ends with the person's message.
        let set = consume_return(
            &link,
            &session(&["user", "assistant", "user"], &[3]),
            &[&ws],
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            set.turn, 3,
            "a set written during a reply belongs to that reply's slot, not the previous answer"
        );
        assert_eq!(set.items.len(), 1);
        assert_eq!(set.items[0].face, Face::Images);
        assert_eq!(Path::new(&set.items[0].path), ws.join("penn-images"));
        assert_eq!(set.title, "Penn Station");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unreadable_or_empty_manifests_are_reported_and_removed() {
        let root = temp("bad");
        std::fs::write(root.join("return.json"), "{not json").unwrap();
        let err = consume_return(&root, &session(&[], &[]), &[&root]).unwrap_err();
        assert!(err.starts_with("return.json"), "{err}");
        assert!(!root.join("return.json").exists());
        let result = std::fs::read_to_string(root.join("return.result.json")).unwrap();
        assert!(result.contains("\"ok\": false"), "{result}");
        std::fs::write(root.join("return.json"), r#"{"id":"x","items":[]}"#).unwrap();
        assert!(consume_return(&root, &session(&[], &[]), &[&root]).is_err());
        assert_eq!(load_deliverables(&root), Deliverables::default());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn normalize_is_lexical() {
        assert_eq!(normalize(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    }
}
