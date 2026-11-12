//! Binding rules when a connector endpoint changes.

use super::{
    artifact, directional_binding, endpoint_node, infer_binding_with, InputKind, WireBinding,
};
use crate::scene::{ConnectorNode, Scene};

/// Existing direction takes precedence when an endpoint changes. Pins describe
/// the source, so moving only the receiving endpoint preserves them.
pub fn rebind(scene: &Scene, before: &ConnectorNode, after: &ConnectorNode) -> Option<WireBinding> {
    rebind_with(scene, before, after, &|_| None)
}

pub fn rebind_with<'a>(
    scene: &Scene,
    before: &ConnectorNode,
    after: &ConnectorNode,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> Option<WireBinding> {
    if endpoint_node(&after.a).is_none() || endpoint_node(&after.b).is_none() {
        return before.binding.clone();
    }
    if let Some(old) = &before.binding {
        if old.kind == InputKind::Control {
            return (endpoint_node(old.source_end(before)) == endpoint_node(old.source_end(after))
                && endpoint_node(old.target_end(before)) == endpoint_node(old.target_end(after)))
            .then(|| old.clone());
        }
        if old.kind == InputKind::Memory {
            return artifact::rebind_memory(scene, before, after, old);
        }
        let (old_source, source, target) = if old.input_b {
            (&before.a, &after.a, &after.b)
        } else {
            (&before.b, &after.b, &after.a)
        };
        if let Some(mut binding) =
            directional_binding(scene, source, target, old.input_b, item_path)
        {
            binding.order = old.order.clone();
            binding.consumed = old.consumed;
            if binding.kind == old.kind && endpoint_node(old_source) == endpoint_node(source) {
                binding.output = old.output.clone();
                binding.all_images = old.all_images;
                // A port slot follows the receiving anchor; a dashboard slot is authored.
                if binding.kind == InputKind::Table {
                    binding.slot = old.slot.clone();
                }
            }
            return Some(binding);
        }
    }
    let mut binding = infer_binding_with(scene, &after.a, &after.b, item_path)?;
    if let Some(old) = &before.binding {
        binding.order = old.order.clone();
        binding.consumed = old.consumed;
    }
    Some(binding)
}
