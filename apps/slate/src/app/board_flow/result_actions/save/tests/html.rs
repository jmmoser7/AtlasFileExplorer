//! Verified single offline HTML uses the existing saved-output worker unchanged.
use super::*;
use slate_doc::scene::HiddenOutputRef;
#[test]
fn html_save_preserves_embedded_bytes_and_rejects_corrupt_or_dependent_output() {
    let (mut h, host, _, reference) = SlateApp::saved_result_test_board("save-offline-html");
    let family = slate_doc::scene::artifact(h.app.doc().scene.node(host).unwrap())
        .unwrap()
        .family
        .clone();
    let address = ResultAddress {
        tab: h.app.tab().id,
        host,
        binding: h.app.result_binding(host).unwrap(),
        outputs: vec![reference.revision_id.clone()],
        target: Some(reference.revision_id.clone()),
        witness: None,
        references: vec![HiddenOutputRef::Artifact {
            family,
            revision: reference.revision_id.clone(),
        }],
    };
    let (doc, nodes) = h.app.result_payload(&address);
    assert_eq!(nodes.len(), 1);
    let snapshot = SaveSnapshot {
        target: SaveTarget::Results(address),
        doc,
        nodes,
        workbook: None,
        extensions: vec!["html".into()],
    };
    let expected = std::fs::read(&reference.materialized_entry).unwrap();
    assert!(std::str::from_utf8(&expected).unwrap().contains("<script>"));
    let destination = h.base.join("saved.html");
    worker::save(&snapshot, std::slice::from_ref(&destination)).unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), expected);
    std::fs::write(
        &reference.materialized_entry,
        b"<html>tampered cache</html>",
    )
    .unwrap();
    let corrupt = h.base.join("corrupt.html");
    assert!(worker::save(&snapshot, std::slice::from_ref(&corrupt)).is_err());
    assert!(!corrupt.exists());
    // Even matching hashes do not admit a manifest with external dependencies.
    let dependency = b"<html><script src='missing.js'></script></html>";
    let revision_dir = reference
        .source_root
        .join("revisions")
        .join(&reference.revision_id);
    let manifest_path = revision_dir.join("revision.json");
    let mut manifest: atlas_agent::ArtifactRevision =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest.source_hash = atlas_ai::versions::hash_artifact_input(dependency);
    manifest.bytes = dependency.len() as u64;
    std::fs::write(revision_dir.join("source.html"), dependency).unwrap();
    std::fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    std::fs::write(&reference.materialized_entry, dependency).unwrap();
    let rejected = h.base.join("dependent.html");
    assert!(worker::save(&snapshot, std::slice::from_ref(&rejected)).is_err());
    assert!(!rejected.exists());
}
