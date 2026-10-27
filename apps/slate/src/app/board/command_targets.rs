//! Scene-command target identity.
use super::{NodeId, SceneCmd};

/// The node a scene command adds, removes, or patches.
pub(crate) fn cmd_node_id(cmd: &SceneCmd) -> NodeId {
    match cmd {
        SceneCmd::Add { node, .. }
        | SceneCmd::Remove { node, .. }
        | SceneCmd::LayerNodeAdd { node, .. }
        | SceneCmd::LayerNodeRemove { node, .. } => node.id,
        SceneCmd::Patch { after, .. } | SceneCmd::LayerNodePatch { after, .. } => after.id,
    }
}
