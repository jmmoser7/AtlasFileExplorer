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
