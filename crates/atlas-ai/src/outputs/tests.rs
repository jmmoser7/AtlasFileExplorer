use super::*;
use atlas_agent::{
    AgentArtifact, AgentStatus, AgentTurn, ArtifactKind, Deliverable, DeliverableSet, Version,
};

fn session(artifacts: Vec<(usize, ArtifactKind, &str)>) -> AgentSession {
    AgentSession {
        activity: Vec::new(),
        usage: None,
        approval: None,
        conversation: String::new(),
        artifacts: artifacts
            .into_iter()
            .enumerate()
            .map(|(i, (turn, kind, source))| AgentArtifact {
                id: format!("a{i}"),
                turn,
                kind,
                source: source.into(),
                title: String::new(),
            })
            .collect(),
        status: AgentStatus::Idle,
        provider: "codex".into(),
        turns: ["user", "assistant", "user", "assistant"]
            .iter()
            .enumerate()
            .map(|(i, r)| AgentTurn {
                role: (*r).into(),
                text: format!("message {i}"),
                at: 0,
            })
            .collect(),
        updated_at: 0,
        bundle: Default::default(),
        request: String::new(),
    }
}

fn set(id: &str, turn: usize, items: &[(&str, Face)]) -> DeliverableSet {
    DeliverableSet {
        id: id.into(),
        turn,
        title: "Q3 sales".into(),
        items: items
            .iter()
            .map(|(p, f)| Deliverable {
                path: (*p).into(),
                face: *f,
                feeds: None,
            })
            .collect(),
        missing: vec!["gone.md".into()],
        goal: None,
    }
}

#[test]
fn manifest_items_are_requested_and_other_edits_sort_below() {
    let s = session(vec![
        (1, ArtifactKind::Created, "/p/out/dash.html"),
        (1, ArtifactKind::Modified, "/p/src/main.rs"),
        (1, ArtifactKind::Deleted, "/p/old.txt"),
        (1, ArtifactKind::Read, "/p/readme.md"),
        (3, ArtifactKind::Modified, "/p/later.rs"),
    ]);
    let mut outputs = LinkOutputs::default();
    outputs.deliverables.sets = vec![set(
        "q3",
        1,
        &[
            ("/p/out/dash.html", Face::Graphic),
            ("/p/out/a.png", Face::Images),
            ("/p/out/b.png", Face::Images),
            ("/p/out/c.svg", Face::Images),
            ("/p/out/data.csv", Face::Auto),
        ],
    )];
    let m = partition(&s, &outputs, 0..2);
    let names: Vec<_> = m.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["dash", "Q3 sales", "data", "main", "old"]);
    assert_eq!(m.requested, 3);
    assert_eq!(m.items[1].images.len(), 2, "svg is not part of a photo set");
    assert!(m.items[..3].iter().all(|i| i.requested));
    assert!(!m.items[3].requested && !m.items[3].deleted);
    assert!(m.items[4].deleted);
    assert_eq!(m.spawnable().count(), 4);
    assert_eq!(m.missing, ["gone.md"]);
    assert_eq!(m.title, "Q3 sales");
    assert!(partition(&s, &outputs, 2..usize::MAX).items[0].name == "later");
}

#[test]
fn each_message_names_and_shows_its_own_version() {
    let s = session(vec![
        (1, ArtifactKind::Created, "C:\\p\\dash.html"),
        (3, ArtifactKind::Modified, "C:\\p\\dash.html"),
    ]);
    let link = PathBuf::from("C:/ws/.atlas-ai/agent/s1");
    let mut outputs = LinkOutputs {
        link_dir: link.clone(),
        ..Default::default()
    };
    let version = |turn: usize, path: &str| Version {
        turn,
        path: path.into(),
        bytes: 1,
        added: Some(1),
        removed: Some(0),
        skipped: None,
    };
    outputs.versions.files = vec![VersionedFile {
        source: "C:/p/dash.html".into(),
        versions: vec![
            version(1, "versions/t001/dash.html"),
            version(3, "versions/t003/dash.html"),
        ],
    }];
    let first = &partition(&s, &outputs, 0..2).items[0];
    assert_eq!((first.versions, first.version), (2, 1));
    assert_eq!(first.shown_path(), link.join("versions/t001/dash.html"));
    let second = &partition(&s, &outputs, 2..4).items[0];
    assert_eq!((second.versions, second.version), (2, 2));
    assert_eq!(second.snapshot, None, "the newest version is the live file");
    assert_eq!(second.shown_path(), second.path);
}

#[test]
fn link_folder_writes_are_not_outputs() {
    let (link_file, report) = if cfg!(windows) {
        (
            "C:\\ws\\.atlas-ai\\agent\\s1\\return.json",
            "C:\\p\\report.md",
        )
    } else {
        ("/ws/.atlas-ai/agent/s1/return.json", "/p/report.md")
    };
    let s = session(vec![
        (1, ArtifactKind::Created, link_file),
        (1, ArtifactKind::Created, report),
    ]);
    let m = partition(&s, &LinkOutputs::default(), 0..usize::MAX);
    assert_eq!(
        m.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        ["report"]
    );
}

#[test]
fn without_a_manifest_new_files_in_the_output_folder_are_requested() {
    let s = session(vec![
        (1, ArtifactKind::Modified, "/p/src/lib.rs"),
        (
            1,
            ArtifactKind::Created,
            "/p/slate-outputs/b/run/report.html",
        ),
        (
            1,
            ArtifactKind::Deleted,
            "/p/slate-outputs/b/run/scratch/tmp.txt",
        ),
    ]);
    let outputs = LinkOutputs {
        output_dir: Some(PathBuf::from("/p/slate-outputs/b/run")),
        ..Default::default()
    };
    let m = partition(&s, &outputs, 0..usize::MAX);
    assert_eq!(m.requested, 1);
    assert_eq!(m.items[0].name, "report");
    assert_eq!(
        m.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        ["report", "lib", "tmp"]
    );
    assert!(
        m.items[2].deleted,
        "deleted files stay incidental and disabled"
    );
}

#[test]
fn versions_and_prompts_are_found_by_path() {
    let mut s = session(vec![(3, ArtifactKind::Modified, "C:\\p\\dash.html")]);
    s.turns[2].text = format!(
        "make it blue{}{}",
        atlas_agent::ARTIFACT_MARKER,
        atlas_agent::artifact_guide(None)
    );
    let mut outputs = LinkOutputs::default();
    outputs.versions.files = vec![VersionedFile {
        source: "C:/p/dash.html".into(),
        versions: vec![
            Version {
                turn: 1,
                path: "versions/t001/dash.html".into(),
                bytes: 1,
                added: Some(3),
                removed: Some(0),
                skipped: None,
            },
            Version {
                turn: 3,
                path: "versions/t003/dash.html".into(),
                bytes: 1,
                added: Some(1),
                removed: Some(1),
                skipped: None,
            },
        ],
    }];
    let m = partition(&s, &outputs, 0..usize::MAX);
    assert_eq!(m.items[0].versions, 2);
    assert_eq!(prompt_before(&s, 3), Some("make it blue"));
    assert_eq!(prompt_before(&s, 1), Some("message 0"));
    assert_eq!(prompt_before(&s, 0), None);
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "atlas_ai_outputs_{name}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn sources_consume_a_return_only_once_the_roots_arrive() {
    let root = temp("sources");
    let link = root.join("link");
    let ws = root.join("ws");
    std::fs::create_dir_all(&link).unwrap();
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::write(ws.join("a.md"), "# a").unwrap();
    std::fs::write(
        link.join("return.json"),
        r#"{"id":"x","items":[{"path":"a.md"}]}"#,
    )
    .unwrap();
    let mut sources = crate::agent::AgentSources::default();
    let _ = sources.poll(&link, None);
    std::thread::sleep(std::time::Duration::from_millis(200));
    let _ = sources.poll(&link, None);
    assert!(sources.outputs(&link).is_none());
    assert!(
        link.join("return.json").exists(),
        "no roots, nothing consumed"
    );
    sources.set_roots(&link, vec![ws.clone()]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while sources.outputs(&link).is_none() {
        assert!(std::time::Instant::now() < deadline, "no snapshot");
        std::thread::sleep(std::time::Duration::from_millis(20));
        let _ = sources.poll(&link, None);
    }
    let outputs = sources.outputs(&link).unwrap();
    assert_eq!(outputs.deliverables.sets.len(), 1);
    assert_eq!(
        Path::new(&outputs.deliverables.sets[0].items[0].path),
        ws.join("a.md")
    );
    assert!(!link.join("return.json").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_watch_consumes_returns_against_its_roots_and_lists_image_folders() {
    let root = temp("watch");
    let link = root.join("link");
    let ws = root.join("ws");
    std::fs::create_dir_all(&link).unwrap();
    std::fs::create_dir_all(ws.join("shots")).unwrap();
    for name in ["b.png", "a.jpg", "skip.svg"] {
        std::fs::write(ws.join("shots").join(name), "x").unwrap();
    }
    let s = session(vec![]);
    let mut watch = OutputWatch::default();
    let first = watch
        .tick(&link, &s, std::slice::from_ref(&ws), false)
        .unwrap();
    assert!(first.deliverables.sets.is_empty());
    assert!(watch
        .tick(&link, &s, std::slice::from_ref(&ws), false)
        .is_none());
    std::fs::write(
        link.join("return.json"),
        r#"{"id":"shots","kind":"images","title":"Shots","path":"shots"}"#,
    )
    .unwrap();
    let next = watch
        .tick(&link, &s, std::slice::from_ref(&ws), false)
        .unwrap();
    let folder = ws.join("shots").to_string_lossy().into_owned();
    assert!(next.folders.contains(&folder));
    let pictures = &next.image_sets[&folder];
    assert_eq!(
        pictures
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["a.jpg", "b.png"]
    );
    let m = partition(&s, &next, 0..usize::MAX);
    assert_eq!(m.items[0].name, "Shots");
    assert_eq!(m.items[0].images.len(), 2);
    assert!(watch
        .tick(&link, &s, std::slice::from_ref(&ws), true)
        .is_some());
    std::fs::write(link.join("return.json"), "{broken").unwrap();
    assert!(watch.tick(&link, &s, &[ws], false).unwrap().error.is_some());
    let _ = std::fs::remove_dir_all(root);
}
