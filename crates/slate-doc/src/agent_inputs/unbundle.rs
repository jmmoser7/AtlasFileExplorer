//! Journaled fan-out of generated image albums.
use super::*;

/// All children keep settings and incoming wires, but own new sessions. The active
/// image retains the original NodeId, so its outgoing wires retain their identity.
pub fn unbundle(
    scene: &Scene,
    id: NodeId,
    images: &[ImageOutput],
    active: usize,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    if images.len() < 2 || active >= images.len() {
        return Err("This portal has no bundle to split.".into());
    }
    let original = scene.node(id).ok_or("Portal no longer exists.")?;
    let NodeKind::Portal(p) = &original.kind else {
        return Err("Select an image portal.".into());
    };
    if p.agent.is_none() {
        return Err("Select an image portal.".into());
    }
    let mut allocator = scene.clone();
    let mut commands = Vec::new();
    let mut ids = Vec::new();
    let mut insertion = scene.nodes.len();
    for (i, image) in images.iter().enumerate() {
        let mut child = original.clone();
        if i != active {
            child.id = allocator.build_node(child.rect, child.kind.clone()).id;
            child.rect.x += (ids.len() as f32 + 1.0) * (child.rect.w + 24.0);
        }
        let NodeKind::Portal(ref mut portal) = child.kind else {
            unreachable!()
        };
        let agent = portal.agent.as_mut().unwrap();
        agent.session = new_agent_session_id();
        agent.channel = None;
        agent.bundle = None;
        agent.seed = Some(image.clone());
        ids.push(child.id);
        if i == active {
            commands.push(SceneCmd::Patch {
                before: Box::new(original.clone()),
                after: Box::new(child),
            });
        } else {
            let child_id = child.id;
            commands.push(SceneCmd::Add {
                index: insertion,
                node: child,
            });
            insertion += 1;
            for wire in &scene.nodes {
                let NodeKind::Connector(c) = &wire.kind else {
                    continue;
                };
                let Some(binding) = &c.binding else { continue };
                let end = binding.target_end(c);
                if endpoint_node(end) != Some(id) {
                    continue;
                }
                let mut copy = wire.clone();
                copy.id = allocator.build_node(copy.rect, copy.kind.clone()).id;
                let NodeKind::Connector(ref mut c) = copy.kind else {
                    unreachable!()
                };
                if let Some(b) = &mut c.binding {
                    if b.order.is_empty() {
                        b.order = vec![wire.id.0];
                    }
                }
                let end = binding.target_end_mut(c);
                if let ConnectorEnd::Anchored { node, .. } = end {
                    *node = child_id;
                }
                commands.push(SceneCmd::Add {
                    index: insertion,
                    node: copy,
                });
                insertion += 1;
            }
        }
    }
    // Outgoing pins follow their matching child. An image-set wire fans out into
    // ordered image wires, preserving the destination's submission order.
    for wire in &scene.nodes {
        let NodeKind::Connector(c) = &wire.kind else {
            continue;
        };
        let Some(binding) = &c.binding else { continue };
        let source = binding.source_end(c);
        if endpoint_node(source) != Some(id) {
            continue;
        }
        if binding.kind != InputKind::Images {
            return Err("Disconnect the outgoing text wire before unbundling images.".into());
        }
        let destinations: Vec<usize> = if let Some(pin) = &binding.output {
            vec![images
                .iter()
                .position(|image| &image.id == pin)
                .ok_or("A pinned output is missing from this bundle.")?]
        } else if binding.all_images {
            (0..images.len()).collect()
        } else {
            vec![active]
        };
        for (ordinal, image_index) in destinations.into_iter().enumerate() {
            let mut copy = wire.clone();
            if ordinal > 0 {
                copy.id = allocator.build_node(copy.rect, copy.kind.clone()).id;
            }
            let NodeKind::Connector(ref mut c) = copy.kind else {
                unreachable!()
            };
            let source = binding.source_end_mut(c);
            if let ConnectorEnd::Anchored { node, .. } = source {
                *node = ids[image_index];
            }
            let b = c.binding.as_mut().unwrap();
            b.output = if binding.output.is_some() || binding.all_images {
                Some(images[image_index].id.clone())
            } else {
                None
            };
            b.all_images = false;
            if b.order.is_empty() {
                b.order.push(wire.id.0);
            }
            b.order.push(ordinal as u64);
            if ordinal == 0 {
                commands.push(SceneCmd::Patch {
                    before: Box::new(wire.clone()),
                    after: Box::new(copy),
                });
            } else {
                commands.push(SceneCmd::Add {
                    index: insertion,
                    node: copy,
                });
                insertion += 1;
            }
        }
    }
    // Validate the complete group before it reaches the journal, then update wire
    // bounds against the planned children so newly fanned inputs cull correctly.
    let mut planned = scene.clone();
    if !planned.apply_all(&commands) {
        return Err("Could not prepare the image bundle split.".into());
    }
    for command in &mut commands {
        let node = match command {
            SceneCmd::Add { node, .. } => node,
            SceneCmd::Patch { after, .. } => after.as_mut(),
            _ => continue,
        };
        if let NodeKind::Connector(c) = &node.kind {
            if let Some(rect) = connector_aabb(c, |id| planned.node(id).map(|n| n.rect)) {
                node.rect = rect;
            }
        }
    }
    Ok((commands, ids))
}
