//! Typed input ports shared by hit testing, binding, and both renderers.
use super::*;

#[cfg(test)]
mod tests;

/// One input port on the left edge of a generator or text block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputPort {
    pub slot: InputSlot,
    /// Fraction down the left edge.
    pub t: f32,
    pub label: &'static str,
}

/// The one output port sits at the right midpoint.
pub const OUTPUT_T: f32 = 0.5;

pub(super) const GENERATOR_PORTS: [InputPort; 3] = [
    InputPort {
        slot: InputSlot::Media,
        t: 0.25,
        label: "Primary image",
    },
    InputPort {
        slot: InputSlot::Prompt,
        t: 0.5,
        label: "Text",
    },
    InputPort {
        slot: InputSlot::Style,
        t: 0.75,
        label: "Style image",
    },
];

/// Saved views wired from screenshot images into a 3D viewport.
pub const MODEL_VIEW_PORT_T: f32 = 0.72;

/// A placed model file that can host the View input port (no format gap).
pub fn image_model_view_port(path: &std::path::Path) -> bool {
    use crate::media::{media_kind, MediaKind};
    media_kind(path) == MediaKind::Model
}

pub(super) fn model_view_target<'a>(
    scene: &Scene,
    target_id: NodeId,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> bool {
    let Some(node) = scene.node(target_id) else {
        return false;
    };
    let NodeKind::Image(img) = &node.kind else {
        return false;
    };
    item_path(img.item).is_some_and(image_model_view_port)
}

const MODEL_VIEW_PORTS: [InputPort; 1] = [InputPort {
    slot: InputSlot::View,
    t: MODEL_VIEW_PORT_T,
    label: "View",
}];

pub(super) const TEXT_PORTS: [InputPort; 2] = [
    InputPort {
        slot: InputSlot::Media,
        t: 1.0 / 3.0,
        label: "Image",
    },
    InputPort {
        slot: InputSlot::Prompt,
        t: 2.0 / 3.0,
        label: "Prompt",
    },
];

const ARTIFACT_PORTS: [InputPort; 2] = [
    InputPort {
        slot: InputSlot::Media,
        t: 0.5,
        label: "Content",
    },
    InputPort {
        slot: InputSlot::Prompt,
        t: 0.75,
        label: "Prompt",
    },
];

/// The input ports a node draws and binds, top to bottom. Empty for every node
/// that is not a generator or text block. `WireHost::ports` reads this table.
pub fn input_ports_of(node: &Node, model_view_port: bool) -> &'static [InputPort] {
    if artifact(node).is_some() {
        return &ARTIFACT_PORTS;
    }
    if model_view_port {
        return &MODEL_VIEW_PORTS;
    }
    match flow_view(node) {
        Some(atlas_agent::PortalView::Images) => &GENERATOR_PORTS,
        Some(atlas_agent::PortalView::Text) => &TEXT_PORTS,
        _ => &[],
    }
}

pub(super) fn flow_view(node: &Node) -> Option<atlas_agent::PortalView> {
    crate::agent_chat::agent(node)
        .map(|a| a.view)
        .filter(|v| *v != atlas_agent::PortalView::Chat)
}

pub fn input_ports(scene: &Scene, id: NodeId) -> &'static [InputPort] {
    scene
        .node(id)
        .map(|n| input_ports_of(n, false))
        .unwrap_or(&[])
}

pub fn input_ports_with<'a>(
    scene: &Scene,
    id: NodeId,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> &'static [InputPort] {
    let Some(node) = scene.node(id) else {
        return &[];
    };
    input_ports_for(node, item_path)
}

/// Direct-node form for a frame's node traversal and not-yet-committed hosts.
/// No scene lookup or file I/O is needed to classify a linked image.
pub fn input_ports_for<'a>(
    node: &Node,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> &'static [InputPort] {
    let model_view = matches!(&node.kind, NodeKind::Image(image)
        if item_path(image.item).is_some_and(image_model_view_port));
    if image_generation_target(node, item_path) {
        &GENERATOR_PORTS
    } else {
        input_ports_of(node, model_view)
    }
}

/// A linked raster image can acquire generation inputs without replacing it.
/// Other placed media keep their existing input semantics.
pub fn image_generation_target<'a>(
    node: &Node,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> bool {
    matches!(&node.kind, NodeKind::Image(image) if image.agent.is_none()
        && item_path(image.item).is_some_and(|path|
            crate::media::media_kind(path) == crate::media::MediaKind::Image))
}
