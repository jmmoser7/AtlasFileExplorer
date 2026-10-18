//! App-level authored artifact flow. The pie image and provider output are
//! synthetic fixtures; these checks do not claim live visual fidelity.
use super::*;
use crate::app::tests::Harness;
use slate_doc::agent_inputs::{self, InputKind};
use slate_doc::scene::{ConnectorEnd, Side};

#[path = "tests/approval.rs"]
mod approval;
#[path = "tests/feedback.rs"]
mod feedback;
#[path = "tests/lineage.rs"]
mod lineage;

fn board(tag: &str) -> (Harness, NodeId) {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.set_agent_programs_for_test(&["codex", "cursor"]);
    h.app
        .pin_pack_for_test("codex", atlas_ai::packs::PackHealth::Ok);
    h.app
        .pin_pack_for_test("cursor", atlas_ai::packs::PackHealth::Ok);
    let path = h.base.join("synthetic-pie.png");
    image::RgbImage::from_fn(180, 120, |x, y| {
        let dx = x as f32 - 90.0;
        let dy = y as f32 - 60.0;
        image::Rgb(if dx * dx + dy * dy > 2500.0 {
            [248, 246, 240]
        } else if dy.atan2(dx) < 0.63 {
            [18, 62, 106]
        } else {
            [226, 171, 64]
        })
    })
    .save(&path)
    .unwrap();
    let item = h
        .app
        .doc_mut()
        .add_item(path, "synthetic-pie.png", 100, 0, "synthetic-pie");
    let image = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0.0, 0.0, 360.0, 240.0),
        NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image = h.app.add_nodes(vec![image])[0];
    (h, image)
}

fn spawn(h: &mut Harness, source: NodeId) -> NodeId {
    h.app.board_sel = [source].into_iter().collect();
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("portal.agent.spawn"),
        Some(
            serde_json::json!({"source":source.0,"kind":"artifact","provider":"codex"}).to_string()
        )
    ));
    *h.app.board_sel.iter().next().unwrap()
}

fn fixture_revision(
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

fn content_wires(h: &Harness, target: NodeId) -> Vec<NodeId> {
    h.app
        .doc()
        .scene
        .nodes
        .iter()
        .filter_map(|node| {
            let NodeKind::Connector(conn) = &node.kind else {
                return None;
            };
            let binding = conn.binding.as_ref()?;
            (binding.kind != InputKind::Memory
                && agent_inputs::endpoint_node(binding.target_end(conn)) == Some(target))
            .then_some(node.id)
        })
        .collect()
}

fn prepare_fixture_run(h: &mut Harness, node: NodeId) -> Prepared {
    *h.app.agents.prompt_mut(node) = "Make the supplied pie interactive.".into();
    assert!(h.app.queue_artifact_generation(node));
    let mut pending = h.app.agents.artifact.preparing.remove(&node).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !h
        .app
        .poll_agent_input_preparation(&mut pending.inputs)
        .unwrap()
    {
        assert!(Instant::now() < until, "input preparation timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    pending.plan.request.inputs = pending.inputs.inputs;
    super::generation::prepare(pending.plan).unwrap()
}

#[test]
fn fourth_spawn_choice_creates_one_web_host_and_content_wire_without_running() {
    let (mut h, image) = board("artifact-fourth-choice");
    assert_eq!(
        crate::app::board_flow::SPAWN_CHOICES
            .iter()
            .map(|c| c.id)
            .collect::<Vec<_>>(),
        ["text", "image", "agent", "artifact"]
    );
    let depth = h.app.tab().journal.undo_depth();
    let count = h.app.doc().scene.nodes.len();
    let node = spawn(&mut h, image);
    let NodeKind::Portal(portal) = &h.app.doc().scene.node(node).unwrap().kind else {
        panic!("Web host expected")
    };
    assert_eq!(portal.kind, PortalKind::Web);
    assert!(portal.web.as_ref().unwrap().artifact.is_some());
    assert!(portal.source.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), count + 2);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(content_wires(&h, node).len(), 1);
    assert!(agent_inputs::artifact_memory_parent(&h.app.doc().scene, node).is_none());
    assert!(!h.app.artifact_is_busy(node));
    assert!(h.app.agents.dispatched.is_empty());
    assert_eq!(
        h.app.shape_properties.panel,
        Some(crate::app::board_properties::Panel::Agent)
    );
    assert!(h.app.take_flow_prompt_focus(node));
    assert!(!h.app.take_flow_prompt_focus(node));
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    h.app.board_redo();
    assert!(h.app.doc().scene.node(node).is_some());
}

#[test]
fn rebinding_the_harness_preserves_web_source_dimensions_and_viewport() {
    let (mut h, image) = board("artifact-provider-binding");
    let node = spawn(&mut h, image);
    let reference = fixture_revision(&mut h, node, "fixture-r1", b"<html><body>pie</body></html>");
    let before = h.app.doc().scene.node(node).unwrap().clone();
    let expected_work = h.app.agent_folder_for(node).unwrap();
    h.app.set_agent_program(node, "cursor");
    let after = h.app.doc().scene.node(node).unwrap();
    assert_eq!(after.rect, before.rect);
    let (NodeKind::Portal(before), NodeKind::Portal(after)) = (&before.kind, &after.kind) else {
        panic!("Web host expected")
    };
    assert_eq!(after.kind, PortalKind::Web);
    assert_eq!(after.source, before.source);
    assert_eq!(after.web, before.web);
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(node).unwrap())
            .unwrap()
            .provider,
        "cursor"
    );
    assert_eq!(h.app.agent_folder_for(node), Some(expected_work));
    assert_eq!(
        std::fs::read(&reference.materialized_entry).unwrap(),
        b"<html><body>pie</body></html>"
    );
}

#[test]
fn downstream_spawns_independent_pinned_content_and_memory_in_one_undo_step() {
    let (mut h, image) = board("artifact-downstream-wires");
    let first = spawn(&mut h, image);
    fixture_revision(
        &mut h,
        first,
        "fixture-r1",
        b"<html><body>original pie</body></html>",
    );
    let depth = h.app.tab().journal.undo_depth();
    let count = h.app.doc().scene.nodes.len();
    let child = spawn(&mut h, first);
    assert_eq!(h.app.doc().scene.nodes.len(), count + 3);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        agent_inputs::artifact_content_parent(&h.app.doc().scene, child),
        Some((first, "fixture-r1".into()))
    );
    assert_eq!(
        agent_inputs::artifact_memory_parent(&h.app.doc().scene, child),
        Some((first, 2))
    );
    assert_eq!(content_wires(&h, child).len(), 1);
    let memory = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|node| {
            let NodeKind::Connector(conn) = &node.kind else {
                return None;
            };
            conn.binding
                .as_ref()
                .filter(|b| b.kind == InputKind::Memory)
                .map(|_| conn)
        })
        .unwrap();
    assert!(matches!(memory.a,ConnectorEnd::Anchored {node,side:Side::Top,..} if node==first));
    assert!(matches!(memory.b,ConnectorEnd::Anchored {node,side:Side::Top,..} if node==child));
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(child).unwrap())
            .unwrap()
            .chat
            .parent
            .is_none()
    );
    assert!(h.app.agents.dispatched.is_empty());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    h.app.board_redo();
    h.app.patch_nodes(&[first], |n| {
        artifact_mut(n).unwrap().revision = Some("later-r2".into())
    });
    assert_eq!(
        agent_inputs::artifact_content_parent(&h.app.doc().scene, child),
        Some((first, "fixture-r1".into()))
    );
    assert_eq!(
        agent_inputs::artifact_memory_source(&h.app.doc().scene, child)
            .unwrap()
            .1
            .revision,
        "fixture-r1"
    );
}

#[test]
fn deleting_only_memory_preserves_content_revisions_and_is_undoable() {
    let (mut h, image) = board("artifact-fresh-eyes");
    let first = spawn(&mut h, image);
    let reference = fixture_revision(
        &mut h,
        first,
        "fixture-r1",
        b"<html><body>original pie</body></html>",
    );
    let child = spawn(&mut h, first);
    let content = agent_inputs::artifact_content_parent(&h.app.doc().scene, child);
    assert!(h.app.artifact_disconnect_memory(child));
    assert!(agent_inputs::artifact_memory_parent(&h.app.doc().scene, child).is_none());
    assert_eq!(
        agent_inputs::artifact_content_parent(&h.app.doc().scene, child),
        content
    );
    assert!(h.app.doc().scene.node(first).is_some());
    assert!(h.app.doc().scene.node(child).is_some());
    assert!(std::fs::read(&reference.materialized_entry)
        .unwrap()
        .starts_with(b"<html>"));
    assert!(!h.app.artifact_disconnect_memory(child));
    h.app.board_undo();
    assert_eq!(
        agent_inputs::artifact_memory_parent(&h.app.doc().scene, child),
        Some((first, 2))
    );
    h.app.board_redo();
    assert!(agent_inputs::artifact_memory_parent(&h.app.doc().scene, child).is_none());
    assert_eq!(
        agent_inputs::artifact_content_parent(&h.app.doc().scene, child),
        content
    );
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn saved_workbook_reopens_exact_revision_and_typed_ancestry_without_source_bytes() {
    let (mut h, image) = board("artifact-save-reopen");
    let first = spawn(&mut h, image);
    let reference = fixture_revision(
        &mut h,
        first,
        "fixture-r1",
        b"<html><body>immutable-source-sentinel</body></html>",
    );
    let child = spawn(&mut h, first);
    let path = h.base.join("charts.slate");
    h.app.doc().save_to(&path).unwrap();
    let doc = slate_doc::SlateDoc::load_from(&path).unwrap();
    let binding = doc.scene.node(first).and_then(artifact).unwrap();
    assert_eq!(binding.revision.as_deref(), Some("fixture-r1"));
    assert_eq!(binding.revisions, ["fixture-r1"]);
    assert_eq!(binding.checkpoint, 2);
    assert_eq!(
        agent_inputs::artifact_content_parent(&doc.scene, child),
        Some((first, "fixture-r1".into()))
    );
    assert_eq!(
        agent_inputs::artifact_memory_parent(&doc.scene, child),
        Some((first, 2))
    );
    assert_eq!(
        resolve_source(Some(&path), &binding.history_root.as_ref().unwrap().locator),
        reference.source_root
    );
    assert!(!std::fs::read_to_string(path)
        .unwrap()
        .contains("immutable-source-sentinel"));
}

#[test]
fn synthetic_pie_request_freezes_its_reference_and_preserves_fake_provider_html() {
    let (mut h, image) = board("artifact-pie-request");
    let node = spawn(&mut h, image);
    let original = h.app.image_item_path(image).unwrap();
    let source = std::fs::read(&original).unwrap();
    let prepared = prepare_fixture_run(&mut h, node);
    let request = &prepared.plan.request;
    request.validate_artifact_request().unwrap();
    assert_eq!(request.inputs.context, request.inputs.wired);
    assert!(request
        .inputs
        .context
        .iter()
        .all(|input| input.node == image.0));
    let wired = &request.inputs.wired[0];
    assert_eq!(wired.node, image.0);
    assert_ne!(std::path::Path::new(&wired.images[0]), original);
    assert_eq!(std::fs::read(&wired.images[0]).unwrap(), source);
    assert!(request
        .input_text()
        .contains("Visual fidelity is a requirement"));
    let html=b"<html><body><svg role='img' aria-label='Synthetic A 60 percent, B 40 percent pie'><circle r='40' cx='50' cy='50' fill='#123e6a'/><title>Synthetic pie</title></svg><button onclick='this.hidden=true'>Hide legend</button></body></html>";
    std::fs::write(prepared.run.staging_output.join("index.html"), html).unwrap();
    let provenance = ArtifactProvenance {
        prompt: request.prompt.clone(),
        provider: prepared.plan.provider.clone(),
        model: "fixture-model".into(),
        checkpoint: ArtifactCheckpoint {
            conversation: "fixture-conversation".into(),
            through: 2,
            turns: vec![
                AgentTurn {
                    role: "user".into(),
                    text: request.prompt.clone(),
                    at: 1,
                },
                AgentTurn {
                    role: "assistant".into(),
                    text: "Synthetic chart output".into(),
                    at: 2,
                },
            ],
        },
        inputs: prepared.input_provenance,
    };
    let (reference, revision) =
        atlas_ai::versions::publish_artifact_revision(&prepared.run, provenance).unwrap();
    assert_eq!(std::fs::read(&reference.materialized_entry).unwrap(), html);
    assert!(revision
        .provenance
        .inputs
        .iter()
        .any(|p| p.source == original.to_string_lossy()
            && p.source_hash == atlas_ai::versions::hash_artifact_input(&source)));
    assert_eq!(std::fs::read(original).unwrap(), source);
    assert!(h.app.agents.dispatched.is_empty());
    let depth = h.app.tab().journal.undo_depth();
    let marks = h.app.tab().edits.len();
    let tab = h.app.tab().id;
    h.app.agents.artifact.pending.insert(
        node,
        Pending {
            tab,
            previous: None,
            reference: reference.clone(),
            revision,
        },
    );
    assert!(h.app.artifact_preview_source(node).is_some());
    let source_of = |h: &Harness| {
        let NodeKind::Portal(p) = &h.app.doc().scene.node(node).unwrap().kind else {
            panic!("Web host expected")
        };
        p.source.clone()
    };
    assert!(
        source_of(&h).is_none(),
        "proposal does not mutate the accepted source"
    );
    assert!(h.app.accept_artifact_output(node));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(h.app.tab().edits.len(), marks + 1);
    assert_eq!(
        resolve_source(None, &source_of(&h).unwrap().locator),
        reference.materialized_entry
    );
    h.app.board_undo();
    assert!(source_of(&h).is_none());
    h.app.board_redo();
    assert_eq!(
        artifact(h.app.doc().scene.node(node).unwrap())
            .unwrap()
            .revision
            .as_deref(),
        Some(reference.revision_id.as_str())
    );
}

#[test]
fn disconnected_memory_prepares_exact_content_in_a_fresh_empty_conversation() {
    let (mut h, image) = board("artifact-fresh-eyes-preparation");
    let first = spawn(&mut h, image);
    let html = b"<html><body>previous exact source</body></html>";
    let reference = fixture_revision(&mut h, first, "fixture-r1", html);
    let first_session = h.app.agent_session_for(first).unwrap().0;
    let child = spawn(&mut h, first);
    assert!(h.app.artifact_disconnect_memory(child));
    let prepared = prepare_fixture_run(&mut h, child);
    assert!(prepared.plan.memory.is_none());
    assert!(!prepared.plan.resume);
    assert_ne!(prepared.plan.session, first_session);
    assert!(prepared.plan.request.history.is_empty());
    assert_eq!(prepared.run.parent, Some(reference.clone()));
    assert_ne!(prepared.run.source_root, reference.source_root);
    assert_eq!(
        std::fs::read(prepared.run.staging_output.join("index.html")).unwrap(),
        html
    );
    assert_eq!(std::fs::read(reference.materialized_entry).unwrap(), html);
}
