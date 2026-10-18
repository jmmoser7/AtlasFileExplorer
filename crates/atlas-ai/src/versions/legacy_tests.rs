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
        activity: Vec::new(),
        usage: None,
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
