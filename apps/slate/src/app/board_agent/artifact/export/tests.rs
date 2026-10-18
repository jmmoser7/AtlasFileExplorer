use super::*;
use atlas_agent::{AgentTurn, ArtifactCheckpoint, ArtifactProvenance};
use slate_doc::scene::{ArtifactCopyPin, ArtifactGeneratorRef, SourceUri};

struct Fixture {
    root: PathBuf,
    doc: SlateDoc,
    node: NodeId,
    reference: atlas_agent::ArtifactRevisionRef,
    source: Vec<u8>,
}
impl Fixture {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "slate_artifact_export_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let source =
            b"<html><body><button>Exact pinned interactive chart</button></body></html>".to_vec();
        let run = atlas_ai::versions::prepare_artifact_run(&root, "chart", "r1", None).unwrap();
        std::fs::write(run.staging_output.join("index.html"), &source).unwrap();
        let checkpoint = ArtifactCheckpoint {
            conversation: "test-session".into(),
            through: 2,
            turns: vec![
                AgentTurn {
                    role: "user".into(),
                    text: "Create chart".into(),
                    at: 1,
                },
                AgentTurn {
                    role: "assistant".into(),
                    text: "Created chart".into(),
                    at: 2,
                },
            ],
        };
        let (reference, _) = atlas_ai::versions::publish_artifact_revision(
            &run,
            ArtifactProvenance {
                prompt: "Create chart".into(),
                provider: "test".into(),
                model: "test-model".into(),
                checkpoint,
                inputs: vec![],
            },
        )
        .unwrap();
        let mut portal =
            PortalNode::bound_web("Chart", reference.materialized_entry.to_string_lossy());
        portal.web.as_mut().unwrap().artifact = Some(Box::new(ArtifactGeneratorRef {
            family: "chart".into(),
            revision: Some("r1".into()),
            history_root: Some(SourceUri {
                locator: reference.source_root.to_string_lossy().into_owned(),
            }),
            ..Default::default()
        }));
        let mut doc = SlateDoc::new("Pinned chart");
        let node = doc.scene.build_node(
            slate_doc::WorldRect::new(0.0, 0.0, 600.0, 400.0),
            NodeKind::Portal(portal),
        );
        let id = node.id;
        doc.scene.nodes.push(node);
        Self {
            root,
            doc,
            node: id,
            reference,
            source,
        }
    }
    fn portal(&self) -> &PortalNode {
        let NodeKind::Portal(portal) = &self.doc.scene.node(self.node).unwrap().kind else {
            unreachable!()
        };
        portal
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn missing_materialization_is_reconstructed_before_export_copies_source() {
    let fixture = Fixture::new("repair");
    std::fs::remove_dir_all(fixture.reference.materialized_entry.parent().unwrap()).unwrap();
    let mut sources = BTreeMap::new();
    prepare_artifact_export_sources(&fixture.doc, None, &mut sources).unwrap();
    assert_eq!(
        std::fs::read(sources.get(&fixture.node).unwrap()).unwrap(),
        fixture.source
    );
    let out = fixture.root.join("export");
    let options = slate_artifact::ExportOptions {
        web_sources: sources,
        ..Default::default()
    };
    let report = slate_artifact::export_html(&fixture.doc, &out, &options).unwrap();
    assert_eq!(report.missing_assets, 0);
    assert_eq!(report.assets_copied, 1);
    let verified = materialize_artifact_source(None, fixture.portal()).unwrap();
    assert_eq!(verified.revision, "r1");
    assert_eq!(
        verified.source_hash,
        atlas_ai::versions::hash_artifact_input(&fixture.source)
    );
}

#[test]
fn tampered_materialization_refuses_export_instead_of_copying_different_bytes() {
    let fixture = Fixture::new("tampered");
    std::fs::write(
        &fixture.reference.materialized_entry,
        b"<html><body>Different data</body></html>",
    )
    .unwrap();
    let out = fixture.root.join("export");
    let mut sources =
        BTreeMap::from([(fixture.node, fixture.reference.materialized_entry.clone())]);
    let result = prepare_artifact_export_sources(&fixture.doc, None, &mut sources).and_then(|()| {
        slate_artifact::export_html(
            &fixture.doc,
            &out,
            &slate_artifact::ExportOptions {
                web_sources: sources,
                ..Default::default()
            },
        )
        .map_err(|error| error.to_string())
    });
    assert!(result.is_err());
    assert!(
        !out.exists(),
        "verification fails before the serializer creates output"
    );
}

#[test]
fn explicitly_packaged_pin_verifies_without_private_history_and_refuses_changed_html() {
    let mut fixture = Fixture::new("portable");
    let workbook = fixture.root.join("portable/chart.slate");
    let entry = workbook
        .parent()
        .unwrap()
        .join("assets/web/chart/index.html");
    std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
    std::fs::write(&entry, &fixture.source).unwrap();
    let NodeKind::Portal(portal) = &mut fixture.doc.scene.nodes[0].kind else {
        unreachable!()
    };
    portal.source = Some(SourceUri {
        locator: "assets/web/chart/index.html".into(),
    });
    portal
        .web
        .as_mut()
        .unwrap()
        .artifact
        .as_mut()
        .unwrap()
        .packaged = Some(ArtifactCopyPin {
        revision: "r1".into(),
        source_hash: atlas_ai::versions::hash_artifact_input(&fixture.source),
    });
    std::fs::remove_dir_all(&fixture.reference.source_root).unwrap();
    let mut sources = BTreeMap::new();
    prepare_artifact_export_sources(&fixture.doc, Some(&workbook), &mut sources).unwrap();
    assert_eq!(sources.get(&fixture.node), Some(&entry));
    std::fs::write(&entry, b"<html><body>Changed packaged data</body></html>").unwrap();
    assert!(prepare_artifact_export_sources(&fixture.doc, Some(&workbook), &mut sources).is_err());
}
