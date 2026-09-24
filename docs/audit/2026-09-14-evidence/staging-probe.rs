//! Disposable audit reproductions of existing defects, NOT product tests.
//! Every assertion intentionally confirms the currently incorrect behavior.
use std::{collections::BTreeMap, path::PathBuf};
use slate_doc::{
    scene::{CmdAuthor, FrameNode, Node, NodeKind, Rgba, Scene, SceneCmd, SceneJournal, WorldRect},
    stage::{self, Proposal, ProposalResult, ProposalStatus, ProposalTarget, StageWatcher},
    SlateDoc,
};

fn frame(scene: &mut Scene, title: &str) -> Node {
    scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 80.0),
        NodeKind::Frame(FrameNode {
            title: title.into(), order: 0, fill: Rgba::WHITE, fill_authored: false,
            assignments: BTreeMap::new(),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
        }),
    )
}

fn proposal(id: &str, target: PathBuf, cmds: Vec<SceneCmd>) -> Proposal {
    Proposal {
        id: id.into(), author: "audit-probe".into(), title: id.into(), created_at: 1,
        target: ProposalTarget { workbook: Some(target), format_version: SlateDoc::CURRENT },
        cmds, session: Some("audit-disposable-session".into()), status: ProposalStatus::Pending,
    }
}

fn title(node: &Node) -> &str {
    match &node.kind { NodeKind::Frame(f) => &f.title, _ => unreachable!() }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("artifacts");
    std::fs::create_dir_all(&output)?;
    let target_a = output.join("workbook-a.slate");
    let active_b = output.join("workbook-b.slate");
    println!("AUDIT REPRODUCTIONS: assertions below confirm existing defects; these are not product test passes.");

    // Mirrors board_agent.rs: active tab's scene/journal go to stage::accept,
    // while the proposal carries a different target workbook.
    let mut doc_b = SlateDoc::new("Workbook B (active)");
    let added = frame(&mut doc_b.scene, "Agent frame intended for A");
    let wrong_target = proposal("wrong-target", target_a.clone(), vec![SceneCmd::Add { index: 0, node: added }]);
    let mut journal_b = SceneJournal::default();
    stage::accept(&wrong_target, &mut doc_b.scene, &mut journal_b).map_err(|e| format!("{e:?}"))?;
    assert_eq!(doc_b.scene.nodes.len(), 1);
    assert_ne!(wrong_target.target.workbook.as_ref(), Some(&active_b));
    assert_eq!(journal_b.last_author(), Some(&CmdAuthor::Agent("audit-probe".into())));
    println!("REPRODUCED wrong_target: target={} active={} accepted=true active_nodes={}", target_a.display(), active_b.display(), doc_b.scene.nodes.len());

    let mut doc = SlateDoc::new("Stale edit fixture");
    let original = frame(&mut doc.scene, "Original title");
    let node_id = original.id;
    let mut journal = SceneJournal::default();
    assert!(journal.commit(&mut doc.scene, vec![SceneCmd::Add { index: 0, node: original.clone() }]));
    let mut agent_after = original.clone();
    agent_after.rect = agent_after.rect.translated(250.0, 0.0);
    let stale_proposal = proposal("stale-before", active_b.clone(), vec![SceneCmd::Patch {
        before: Box::new(original.clone()), after: Box::new(agent_after),
    }]);
    let mut human_after = original.clone();
    if let NodeKind::Frame(f) = &mut human_after.kind { f.title = "Human intervening title".into(); }
    assert!(journal.commit(&mut doc.scene, vec![SceneCmd::Patch {
        before: Box::new(original), after: Box::new(human_after.clone()),
    }]));
    assert_eq!(title(doc.scene.node(node_id).unwrap()), "Human intervening title");
    stage::accept(&stale_proposal, &mut doc.scene, &mut journal).map_err(|e| format!("{e:?}"))?;
    let accepted_title = title(doc.scene.node(node_id).unwrap()).to_owned();
    assert_eq!(accepted_title, "Original title");
    assert!(journal.undo(&mut doc.scene));
    let after_undo = doc.scene.node(node_id).unwrap();
    assert_eq!(title(after_undo), "Original title");
    assert_ne!(after_undo, &human_after);
    println!("REPRODUCED stale_before: before_accept_title={:?} after_accept_title={:?} after_undo_title={:?}; undo_restores_intervening_human_edit=false", title(&human_after), accepted_title, title(after_undo));

    // Each run gets a unique workspace so no cleanup or stale files are involved.
    let workspace = output.join(format!("restart-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()));
    std::fs::create_dir_all(stage::stage_dir(&workspace))?;
    let mut restart_scene = Scene::default();
    let restart_node = frame(&mut restart_scene, "Accepted once");
    let accepted = proposal("accepted-once", active_b.clone(), vec![SceneCmd::Add { index: 0, node: restart_node }]);
    let rejected = proposal("rejected-once", active_b, vec![]);
    for p in [&accepted, &rejected] {
        std::fs::write(stage::stage_dir(&workspace).join(format!("{}.json", p.id)), serde_json::to_vec_pretty(p)?)?;
    }
    let mut first_watcher = StageWatcher::new();
    assert_eq!(first_watcher.tick_read(&workspace).len(), 2);
    let mut restart_journal = SceneJournal::default();
    stage::accept(&accepted, &mut restart_scene, &mut restart_journal).map_err(|e| format!("{e:?}"))?;
    stage::write_result(&workspace, &accepted.id, &ProposalResult::Accepted)?;
    stage::write_result(&workspace, &rejected.id, &stage::reject(&rejected))?;
    drop(first_watcher);
    let mut restarted_watcher = StageWatcher::new();
    let replayed = restarted_watcher.tick_read(&workspace);
    assert_eq!(replayed.len(), 2);
    assert!(replayed.iter().all(|p| p.status == ProposalStatus::Pending));
    let mut replayed_ids: Vec<_> = replayed.iter().map(|p| p.id.as_str()).collect();
    replayed_ids.sort();
    println!("REPRODUCED restart_replay: decided_proposals_reloaded_as_pending={replayed_ids:?} workspace={}", workspace.display());
    println!("SUMMARY: all 3 existing staging defects reproduced via public slate-doc APIs; no application launched; no existing workbook modified.");
    Ok(())
}
