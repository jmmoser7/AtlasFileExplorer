use super::*;
use crate::scene::{ImageNode, NodeKind, PortalNode, SourceUri, WorldRect};
use crate::{PaintLayer, PaintLayerId};

struct TestStore(RecoveryStore);

impl TestStore {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        Self(RecoveryStore::new(std::env::temp_dir().join(format!(
            "slate-recovery-tests-{}-{}-{}",
            std::process::id(),
            unix_millis(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ))))
    }
}

impl Drop for TestStore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0.root);
    }
}

fn add_node(scene: &mut crate::Scene, rect: WorldRect, kind: NodeKind) {
    let node = scene.build_node(rect, kind);
    assert!(scene.apply(&crate::SceneCmd::Add {
        index: scene.nodes.len(),
        node
    }));
}

#[test]
fn never_saved_workbook_round_trips_full_authored_content() {
    let store = TestStore::new();
    let session = store.0.begin_session().unwrap();
    let mut doc = SlateDoc::new("Never saved");
    let group = doc.add_group("Materials");
    let _ = doc.add_tag(group, "Red", [255, 0, 0]);
    add_node(
        &mut doc.scene,
        WorldRect::new(17.0, 24.0, 800.0, 600.0),
        NodeKind::Portal(PortalNode::bound_web("Reference", "https://example.com/a")),
    );
    session.write(7, None, &doc, 3).unwrap();
    drop(session);
    let scan = store.0.scan().unwrap();
    assert!(scan.issues.is_empty());
    let entry = &scan.entries[0];
    assert_eq!(entry.original_path, None);
    assert_eq!(entry.name, "Never saved");
    assert_eq!(store.0.load(entry).unwrap(), doc);
}

#[test]
fn equal_names_in_distinct_tabs_and_runs_are_independent() {
    let store = TestStore::new();
    let first = store.0.begin_session().unwrap();
    let second = store.0.begin_session().unwrap();
    let doc = SlateDoc::new("Untitled");
    first.write(1, None, &doc, 3).unwrap();
    first.write(2, None, &doc, 3).unwrap();
    second.write(1, None, &doc, 3).unwrap();
    assert!(store.0.scan().unwrap().entries.is_empty());
    drop(first);
    assert_eq!(store.0.scan().unwrap().entries.len(), 2);
    drop(second);
    let scan = store.0.scan().unwrap();
    assert_eq!(scan.entries.len(), 3);
    store.0.discard(&scan.entries[0]).unwrap();
    assert_eq!(store.0.scan().unwrap().entries.len(), 2);
}

#[test]
fn save_base_and_recursive_sources_survive_without_opening_sources() {
    let store = TestStore::new();
    let session = store.0.begin_session().unwrap();
    // No directory or linked source is created: locator rebasing cannot need it.
    let original = std::env::temp_dir()
        .join("missing-cloud-source")
        .join("Board.slate");
    let base = original.parent().unwrap().to_path_buf();
    let mut doc = SlateDoc::new("Linked");
    let item = doc.add_item("assets/cloud-only.png".into(), "cloud-only.png", 0, 0, "x");
    let network = PathBuf::from("//unreachable-slate-recovery/share/placeholder.png");
    doc.add_item(network.clone(), "placeholder.png", 0, 0, "network");
    doc.lens_root = Some("code".into());
    let mut agent = *PortalNode::unbound_agent("Prompt", "local").agent.unwrap();
    agent.bundle = Some(SourceUri {
        locator: "history/conversation".into(),
    });
    let mut child_scene = crate::Scene::default();
    add_node(
        &mut child_scene,
        WorldRect::new(0.0, 0.0, 1.0, 1.0),
        NodeKind::Image(ImageNode::generated(agent)),
    );
    let mut image = ImageNode::new(item);
    let mut layer = PaintLayer::new(PaintLayerId(1));
    layer.nodes = child_scene.nodes;
    image.paint_layers.push(layer);
    add_node(
        &mut doc.scene,
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
        NodeKind::Image(image),
    );
    add_node(
        &mut doc.scene,
        WorldRect::new(1.0, 1.0, 20.0, 20.0),
        NodeKind::Portal(PortalNode::bound_slate("Sibling", "nested/Other.slate")),
    );
    add_node(
        &mut doc.scene,
        WorldRect::new(1.0, 1.0, 20.0, 20.0),
        NodeKind::Portal(PortalNode::bound_web("Remote", "https://example.com/cloud")),
    );
    session.write(1, Some(original.clone()), &doc, 3).unwrap();
    drop(session);
    let entry = store.0.scan().unwrap().entries.pop().unwrap();
    assert_eq!(entry.original_path, Some(original));
    let restored = store.0.load(&entry).unwrap();
    assert_eq!(restored.items[0].path, base.join("assets/cloud-only.png"));
    assert_eq!(restored.items[1].path, network);
    assert_eq!(restored.lens_root, Some(base.join("code")));
    let NodeKind::Image(image) = &restored.scene.nodes[0].kind else {
        panic!()
    };
    let NodeKind::Image(child) = &image.paint_layers[0].nodes[0].kind else {
        panic!()
    };
    assert_eq!(
        Path::new(
            &child
                .agent
                .as_ref()
                .unwrap()
                .bundle
                .as_ref()
                .unwrap()
                .locator
        ),
        base.join("history/conversation")
    );
    let NodeKind::Portal(sibling) = &restored.scene.nodes[1].kind else {
        panic!()
    };
    assert_eq!(
        Path::new(&sibling.source.as_ref().unwrap().locator),
        base.join("nested/Other.slate")
    );
    let NodeKind::Portal(remote) = &restored.scene.nodes[2].kind else {
        panic!()
    };
    assert_eq!(
        remote.source.as_ref().unwrap().locator,
        "https://example.com/cloud"
    );
    assert_eq!(
        doc.items[0].path,
        Path::new("assets/cloud-only.png"),
        "original remains authored"
    );
}

#[test]
fn corrupt_newest_falls_back_and_does_not_destroy_evidence() {
    let store = TestStore::new();
    let session = store.0.begin_session().unwrap();
    session.write(9, None, &SlateDoc::new("Older"), 3).unwrap();
    let newest = session.write(9, None, &SlateDoc::new("Newest"), 3).unwrap();
    let path = session.directory.join(&newest.metadata.generation);
    fs::write(&path, b"{interrupted").unwrap();
    fs::write(
        session.directory.join("9-99999999999999999999.tmp"),
        b"partial",
    )
    .unwrap();
    drop(session);
    let scan = store.0.scan().unwrap();
    assert_eq!(scan.entries.len(), 1);
    assert_eq!(store.0.load(&scan.entries[0]).unwrap().name, "Older");
    assert!(scan.entries[0].warning.is_some());
    assert_eq!(scan.issues.len(), 1);
    assert!(path.exists());
}

#[test]
fn prune_keeps_requested_valid_versions_and_all_corrupt_evidence() {
    let store = TestStore::new();
    let session = store.0.begin_session().unwrap();
    let corrupt = session.write(1, None, &SlateDoc::new("Bad"), 3).unwrap();
    let corrupt_path = session.directory.join(corrupt.metadata.generation);
    fs::write(&corrupt_path, b"bad").unwrap();
    for i in 0..6 {
        session
            .write(1, None, &SlateDoc::new(format!("Version {i}")), 3)
            .unwrap();
    }
    assert_eq!(tab_paths(&session.directory, 1).unwrap().len(), 4);
    assert!(corrupt_path.exists());
    session.discard(1).unwrap();
    assert!(tab_paths(&session.directory, 1).unwrap().is_empty());
}

#[test]
fn failed_commit_preserves_previous_generation() {
    let store = TestStore::new();
    let session = store.0.begin_session().unwrap();
    session.write(1, None, &SlateDoc::new("Safe"), 3).unwrap();
    let bad_target = session.directory.join("1-00000000000000000999.json");
    fs::create_dir(&bad_target).unwrap();
    assert!(persist_generation(&bad_target, b"cannot commit").is_err());
    drop(session);
    let scan = store.0.scan().unwrap();
    assert_eq!(scan.entries.len(), 1);
    assert_eq!(store.0.load(&scan.entries[0]).unwrap().name, "Safe");
}

#[test]
fn live_session_cannot_be_loaded_or_dismissed_even_with_an_entry() {
    let store = TestStore::new();
    let session = store.0.begin_session().unwrap();
    let entry = session.write(1, None, &SlateDoc::new("Live"), 3).unwrap();
    assert!(store.0.scan().unwrap().entries.is_empty());
    assert_eq!(
        store.0.load(&entry).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        store.0.discard(&entry).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    drop(session);
    assert_eq!(store.0.load(&entry).unwrap().name, "Live");
}

#[test]
fn future_workbook_version_is_rejected_through_canonical_decoder() {
    let store = TestStore::new();
    let session = store.0.begin_session().unwrap();
    session
        .write(1, None, &SlateDoc::new("Supported"), 3)
        .unwrap();
    let newest = session.write(1, None, &SlateDoc::new("Future"), 3).unwrap();
    let path = session.directory.join(newest.metadata.generation);
    let mut snapshot: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    snapshot["document"]["format_version"] = (SlateDoc::CURRENT + 1).into();
    fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    drop(session);
    let scan = store.0.scan().unwrap();
    assert_eq!(scan.entries[0].name, "Supported");
    assert!(scan.issues[0].contains("unsupported slate format"));
}

#[test]
fn malformed_metadata_cannot_redirect_load_or_discard_outside_store() {
    let store = TestStore::new();
    let session = store.0.begin_session().unwrap();
    let mut entry = session.write(1, None, &SlateDoc::new("Safe"), 3).unwrap();
    drop(session);
    entry.metadata.session = "../outside".into();
    assert_eq!(
        store.0.load(&entry).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        store.0.discard(&entry).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let mut entry = store.0.scan().unwrap().entries.pop().unwrap();
    entry.metadata.generation = "../../outside.json".into();
    assert_eq!(
        store.0.load(&entry).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(store.0.scan().unwrap().entries.len(), 1);
}

#[test]
fn canonical_load_hygiene_also_applies_to_recovery() {
    let store = TestStore::new();
    let session = store.0.begin_session().unwrap();
    let mut doc = SlateDoc::new("Paint hygiene");
    let mut layer = PaintLayer::new(PaintLayerId(1));
    let mut nested = crate::Scene::default();
    add_node(
        &mut nested,
        WorldRect::default(),
        NodeKind::Portal(PortalNode::unbound_web("Invalid child")),
    );
    layer.nodes = nested.nodes;
    let mut image = ImageNode::new(crate::ItemId::NONE);
    image.paint_layers.push(layer);
    add_node(&mut doc.scene, WorldRect::default(), NodeKind::Image(image));
    session.write(1, None, &doc, 3).unwrap();
    drop(session);
    let entry = store.0.scan().unwrap().entries.pop().unwrap();
    let recovered = store.0.load(&entry).unwrap();
    let NodeKind::Image(image) = &recovered.scene.nodes[0].kind else {
        panic!()
    };
    assert!(image.paint_layers[0].nodes.is_empty());
}

#[test]
fn unsupported_schema_is_reported_and_explicit_group_dismissal_is_available() {
    let store = TestStore::new();
    let session = store.0.begin_session().unwrap();
    let entry = session
        .write(17, None, &SlateDoc::new("Future recovery"), 3)
        .unwrap();
    let path = session.directory.join(&entry.metadata.generation);
    let mut snapshot: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    snapshot["schema"] = (SCHEMA + 1).into();
    fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(
        store
            .0
            .discard_group(&entry.session, entry.tab)
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    drop(session);
    let scan = store.0.scan().unwrap();
    assert!(scan.entries.is_empty());
    assert_eq!(scan.issues.len(), 1);
    assert!(path.exists());
    store.0.discard_group(&entry.session, entry.tab).unwrap();
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn symbolic_session_paths_cannot_redirect_recovery_deletion() {
    let store = TestStore::new();
    let external = TestStore::new();
    let session = external.0.begin_session().unwrap();
    let entry = session
        .write(1, None, &SlateDoc::new("External"), 3)
        .unwrap();
    let target = session.directory.clone();
    drop(session);
    fs::create_dir_all(&store.0.root).unwrap();
    std::os::unix::fs::symlink(&target, store.0.root.join(&entry.session)).unwrap();
    assert_eq!(
        store.0.discard(&entry).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(external.0.scan().unwrap().entries.len(), 1);
}
