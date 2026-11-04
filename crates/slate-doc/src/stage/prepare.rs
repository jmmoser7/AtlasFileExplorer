//! Preflight against evolving proposal state before touching the real journal.

use super::{same_workbook, Proposal, ProposalStatus, StaleReason};
use crate::image_paint::{find_layer_node, layers_for_location, PaintLayerId};
use crate::scene::{Node, NodeId, NodeKind, Scene, SceneCmd};
use crate::SlateDoc;
use std::path::Path;

pub(super) fn prepared_cmds(
    proposal: &Proposal,
    workbook: Option<&Path>,
    scene: &Scene,
) -> Result<Vec<SceneCmd>, StaleReason> {
    if proposal.target.format_version != SlateDoc::CURRENT {
        return Err(StaleReason::UnsupportedFormat {
            found: proposal.target.format_version,
            expected: SlateDoc::CURRENT,
        });
    }
    let (Some(target), Some(workbook)) = (proposal.target.workbook.as_deref(), workbook) else {
        return Err(StaleReason::UnsavedWorkbook);
    };
    if !same_workbook(target, workbook) {
        return Err(StaleReason::WorkbookMismatch);
    }
    if proposal.status != ProposalStatus::Pending {
        return Err(StaleReason::NotPending);
    }
    if proposal.cmds.is_empty() {
        return Err(StaleReason::EmptyProposal);
    }

    let mut check = scene.clone();
    let mut cmds = Vec::with_capacity(proposal.cmds.len());
    for original in &proposal.cmds {
        // A later command may address a node added or patched earlier in this
        // group. Compare its expected state with that evolving scratch scene.
        let cmd = match original {
            SceneCmd::Add { node, .. } => SceneCmd::Add {
                // Reordering an existing stable ID is Remove + Add. Keep its
                // intended z-position; genuinely new objects append as before.
                index: if scene.node(node.id).is_some() {
                    match original {
                        SceneCmd::Add { index, .. } => *index,
                        _ => unreachable!(),
                    }
                } else {
                    check.nodes.len()
                },
                node: node.clone(),
            },
            SceneCmd::Patch { before, .. } => {
                if check.node(before.id) != Some(before.as_ref()) {
                    return Err(StaleReason::NodeChanged { node: before.id });
                }
                original.clone()
            }
            SceneCmd::Remove { node, .. } => {
                if check.node(node.id) != Some(node) {
                    return Err(StaleReason::NodeChanged { node: node.id });
                }
                original.clone()
            }
            SceneCmd::LayerNodeRemove {
                host,
                layer,
                index,
                node,
            } => {
                if !layer_node_matches(&check, *host, *layer, *index, node) {
                    return Err(StaleReason::NodeChanged { node: node.id });
                }
                original.clone()
            }
            SceneCmd::LayerNodePatch {
                host,
                layer,
                index,
                before,
                ..
            } => {
                if !layer_node_matches(&check, *host, *layer, *index, before) {
                    return Err(StaleReason::NodeChanged { node: before.id });
                }
                original.clone()
            }
            SceneCmd::LayerNodeAdd { .. } => original.clone(),
            SceneCmd::Presentations { .. } => original.clone(),
        };
        if !check.apply(&cmd) {
            return Err(StaleReason::CommandRejected);
        }
        cmds.push(cmd);
    }
    Ok(cmds)
}

fn layer_node_matches(
    scene: &Scene,
    host: NodeId,
    layer: PaintLayerId,
    index: usize,
    expected: &Node,
) -> bool {
    // Reuse the paint-layer locator owner, including generated result stacks.
    let Some(location) = find_layer_node(scene, expected.id) else {
        return false;
    };
    if location.image != host || location.node_index != index {
        return false;
    }
    let Some(host) = scene.node(host) else {
        return false;
    };
    let NodeKind::Image(image) = &host.kind else {
        return false;
    };
    let Some(layers) = layers_for_location(image, location.result.as_ref()) else {
        return false;
    };
    let Some(actual) = layers.get(location.layer_index) else {
        return false;
    };
    actual.id == layer && actual.nodes.get(index) == Some(expected)
}
/// The same stage preflight, for owners that must stop old work immediately
/// before the accepted journal mutation changes its execution identity.
pub fn preflight_record(
    proposal: &Proposal,
    workbook: Option<&Path>,
    scene: &Scene,
    ai_workspace: &Path,
) -> Result<(), String> {
    if super::read_result(ai_workspace, &proposal.id)
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("A stage decision already exists".into());
    }
    prepared_cmds(proposal, workbook, scene)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod history_preflight_tests {
    use super::*;
    use crate::scene::{Corner, Rgba, SceneJournal, ShapeKind, ShapeNode, Stroke, WorldRect};
    #[test]
    fn shared_stage_preflight_rejects_stale_decisions_and_failed_applying_keeps_document() {
        let workspace = std::env::temp_dir().join(crate::scene::new_agent_session_id());
        std::fs::create_dir_all(&workspace).unwrap();
        let workbook = workspace.join("test.slate");
        let mut scene = Scene::default();
        let node = scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 80.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: Some(Rgba([30, 40, 50, 255])),
                stroke: Stroke::none(),
                corner: Corner::default(),
                sides: crate::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: None,
                text: None,
            }),
        );
        scene.nodes.push(node.clone());
        let mut after = node.clone();
        after.rect.x = 20.0;
        let proposal = Proposal {
            id: "history-preflight".into(),
            author: "agent".into(),
            title: "board.image.history".into(),
            created_at: 1,
            target: super::super::ProposalTarget {
                workbook: Some(workbook.clone()),
                format_version: SlateDoc::CURRENT,
            },
            cmds: vec![SceneCmd::Patch {
                before: Box::new(node),
                after: Box::new(after),
            }],
            session: Some("scope".into()),
            status: ProposalStatus::Pending,
        };
        let before = scene.clone();
        let mut journal = SceneJournal::default();
        assert!(preflight_record(&proposal, Some(&workbook), &scene, &workspace).is_ok());
        let error = super::super::accept_and_record_with_writer(
            &proposal,
            Some(&workbook),
            &mut scene,
            &mut journal,
            &workspace,
            |_| Err(std::io::Error::other("Applying write failed")),
        )
        .unwrap_err();
        assert!(!error.applied);
        assert_eq!(scene, before);
        assert_eq!(journal.undo_depth(), 0);
        super::super::write_result(
            &workspace,
            &proposal.id,
            &super::super::ProposalResult::Rejected,
        )
        .unwrap();
        assert!(preflight_record(&proposal, Some(&workbook), &scene, &workspace).is_err());
        std::fs::remove_file(super::super::result_path(&workspace, &proposal.id)).unwrap();
        scene.nodes[0].locked = true;
        assert!(preflight_record(&proposal, Some(&workbook), &scene, &workspace).is_err());
        std::fs::remove_dir_all(workspace).unwrap();
    }
}
