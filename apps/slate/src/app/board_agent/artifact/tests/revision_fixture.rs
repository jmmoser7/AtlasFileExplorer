//! Existing saved-revision fixture shared by artifact result component tests.
use super::*;

pub(in crate::app::board_agent::artifact) fn fixture_revision(
    h: &mut Harness,
    node: NodeId,
    run_id: &str,
    html: &[u8],
) -> ArtifactRevisionRef {
    let family = h
        .app
        .doc()
        .scene
        .node(node)
        .and_then(artifact)
        .unwrap()
        .family
        .clone();
    let link = h.app.agent_link_dir(node, &h.base).unwrap();
    let run = atlas_ai::versions::prepare_artifact_run(&link, &family, run_id, None).unwrap();
    std::fs::write(run.staging_output.join("index.html"), html).unwrap();
    let source = h
        .app
        .image_item_path(h.app.doc().scene.nodes[0].id)
        .unwrap();
    let provenance = ArtifactProvenance {
        prompt: "Make the supplied pie interactive.".into(),
        provider: "codex".into(),
        model: "fixture-model".into(),
        checkpoint: ArtifactCheckpoint {
            conversation: "fixture-conversation".into(),
            through: 2,
            turns: vec![
                AgentTurn {
                    role: "user".into(),
                    text: "Make the supplied pie interactive.".into(),
                    at: 1,
                },
                AgentTurn {
                    role: "assistant".into(),
                    text: "Created interactive pie.".into(),
                    at: 2,
                },
            ],
        },
        inputs: vec![atlas_agent::ArtifactInputProvenance {
            source: source.to_string_lossy().into_owned(),
            source_hash: atlas_ai::versions::hash_artifact_input(&std::fs::read(source).unwrap()),
        }],
    };
    let (reference, revision) =
        atlas_ai::versions::publish_artifact_revision(&run, provenance).unwrap();
    let linked = |path: &std::path::Path| SourceUri {
        locator: crate::app::board_portal::source_locator(h.app.tab().path.as_deref(), path),
    };
    let entry = linked(&reference.materialized_entry);
    let root = linked(&reference.source_root);
    let session = h.app.agent_session_for(node).unwrap().0;
    let work = linked(&reference.source_root.join("work").join(session));
    h.app.patch_nodes(&[node], |node| {
        if let NodeKind::Portal(portal) = &mut node.kind {
            portal.source = Some(entry.clone());
        }
        let binding = artifact_mut(node).unwrap();
        binding.history_root = Some(root.clone());
        binding.workspace = Some(work.clone());
        binding.revision = Some(reference.revision_id.clone());
        binding.revisions.push(reference.revision_id.clone());
        binding.checkpoint = revision.provenance.checkpoint.through;
    });
    reference
}
