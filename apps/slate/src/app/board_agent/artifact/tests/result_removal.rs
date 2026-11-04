//! Saved local outputs exercise the real revision worker; no provider or native window.
use super::*;
use slate_doc::scene::HiddenOutputRef;

fn drain(h: &mut Harness) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while h.app.agents.artifact.active_workers != 0 {
        h.app.pump_artifact_generations(&h.ctx);
        assert!(
            std::time::Instant::now() < deadline,
            "saved-revision worker did not finish"
        );
        std::thread::yield_now();
    }
}

#[test]
fn active_web_result_removal_selects_saved_fallback_in_one_undo_step() {
    let (mut h, source) = board("artifact-result-remove-atomic");
    let host = spawn(&mut h, source);
    let first = fixture_revision(&mut h, host, "first", b"<html>first saved output</html>");
    let last = fixture_revision(&mut h, host, "last", b"<html>last saved output</html>");
    h.app.patch_nodes(&[host], |node| {
        let binding = artifact_mut(node).unwrap();
        binding.active_memory = Some(slate_doc::agent_inputs::ArtifactMemoryPin {
            revision: last.revision_id.clone(),
            through: binding.checkpoint,
        });
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .preserve_history = Some(true);
    });
    let before = h.app.doc().scene.node(host).unwrap().clone();
    let family = artifact(&before).unwrap().family.clone();
    let agent_before = slate_doc::agent_chat::agent(&before).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(h
        .app
        .remove_artifact_result(host, &family, &last.revision_id));
    assert_eq!(
        h.app.doc().scene.node(host),
        Some(&before),
        "worker preparation is not an early metadata mutation"
    );
    drain(&mut h);
    let after = h.app.doc().scene.node(host).unwrap();
    assert_eq!(
        artifact(after).unwrap().revision.as_deref(),
        Some(first.revision_id.as_str())
    );
    assert_eq!(
        artifact(after).unwrap().active_memory,
        artifact(&before).unwrap().active_memory
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let agent = slate_doc::agent_chat::agent(after).unwrap();
    assert_eq!(agent.session, agent_before.session);
    assert_eq!(agent.bundle, agent_before.bundle);
    assert_eq!(
        agent.generator.hidden_outputs,
        [HiddenOutputRef::Artifact {
            family,
            revision: last.revision_id.clone()
        }]
    );
    assert!(first.materialized_entry.is_file());
    assert!(last.materialized_entry.is_file());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(host), Some(&before));
}

#[test]
fn saved_revision_removal_rejects_changed_family_locked_target_and_cancelled_pick() {
    for mode in ["family", "source", "locked", "readonly", "cancel"] {
        let (mut h, source) = board(&format!("artifact-result-remove-{mode}"));
        let host = spawn(&mut h, source);
        fixture_revision(&mut h, host, "first", b"<html>first</html>");
        let last = fixture_revision(&mut h, host, "last", b"<html>last</html>");
        let family = artifact(h.app.doc().scene.node(host).unwrap())
            .unwrap()
            .family
            .clone();
        assert!(h
            .app
            .remove_artifact_result(host, &family, &last.revision_id));
        match mode {
            "family" => {
                h.app.patch_nodes(&[host], |node| {
                    artifact_mut(node).unwrap().family.push_str("-changed")
                });
            }
            "locked" => {
                h.app.patch_nodes(&[host], |node| node.locked = true);
            }
            "readonly" => {
                h.app.tab_mut().read_only = true;
            }
            "source" => {
                let other = h.base.join("different.html");
                std::fs::write(&other, b"<html>human source change</html>").unwrap();
                let locator =
                    crate::app::board_portal::source_locator(h.app.tab().path.as_deref(), &other);
                h.app.patch_nodes(&[host], |node| {
                    if let NodeKind::Portal(portal) = &mut node.kind {
                        portal.source = Some(SourceUri {
                            locator: locator.clone(),
                        });
                    }
                });
            }
            _ => {
                assert!(h.app.cancel_artifact_generation(host));
            }
        }
        let expected = h.app.doc().scene.node(host).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        drain(&mut h);
        assert_eq!(h.app.doc().scene.node(host), Some(&expected));
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert!(slate_doc::agent_chat::agent(&expected)
            .unwrap()
            .generator
            .hidden_outputs
            .is_empty());
    }
}

#[test]
fn removing_last_web_entry_keeps_actual_saved_source_as_explicit_native_compatibility() {
    let (mut h, source) = board("artifact-last-result-source-retained");
    let host = spawn(&mut h, source);
    let last = fixture_revision(&mut h, host, "only", b"<html>only saved output</html>");
    let before = h.app.doc().scene.node(host).unwrap().clone();
    let family = artifact(&before).unwrap().family.clone();
    assert!(h
        .app
        .remove_artifact_result(host, &family, &last.revision_id));
    let after = h.app.doc().scene.node(host).unwrap();
    let (NodeKind::Portal(old), NodeKind::Portal(new)) = (&before.kind, &after.kind) else {
        panic!("same Web host");
    };
    assert_eq!(new.source, old.source);
    assert_eq!(artifact(after), artifact(&before));
    assert!(h.app.result_outputs(host).is_empty());
    assert!(last.materialized_entry.is_file());
}
