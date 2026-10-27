//! Control wire inference shares the ordinary semantic graph, not a side table.
use super::*;

/// The runtime supplies declarations only for a validated local page. The pure
/// graph owns direction and binding; it never reads or executes an HTML file.
pub fn infer_control_binding(
    scene: &Scene,
    a: &ConnectorEnd,
    b: &ConnectorEnd,
    declared_input: &dyn Fn(NodeId) -> Option<String>,
    controllable: &dyn Fn(NodeId) -> bool,
) -> Option<WireBinding> {
    for (source, target, input_b) in [(a, b, true), (b, a, false)] {
        let Some(source_id) = endpoint_node(source) else {
            continue;
        };
        let Some(target_id) = endpoint_node(target) else {
            continue;
        };
        if !matches!(target, ConnectorEnd::Anchored { side:Side::Left,t,.. } if (*t-0.5).abs()<0.001)
        {
            continue;
        }
        if !matches!(&scene.node(target_id)?.kind, NodeKind::Portal(p) if p.kind==PortalKind::Web) {
            continue;
        }
        if !controllable(source_id) {
            continue;
        }
        let Some(slot) = declared_input(target_id) else {
            continue;
        };
        return Some(WireBinding {
            input_b,
            kind: InputKind::Control,
            output: None,
            order: vec![],
            all_images: false,
            consumed: false,
            slot: Some(slot),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_a_declared_page_gets_a_semantic_control_wire() {
        let mut scene = Scene::default();
        let mut preparation = scene.node_preparation();
        let model = preparation.build_node(
            WorldRect::new(0., 0., 100., 100.),
            NodeKind::Image(ImageNode::new(crate::ids::ItemId(1))),
        );
        let model_id = model.id;
        let page = preparation.build_node(
            WorldRect::new(200., 0., 100., 100.),
            NodeKind::Portal(PortalNode::bound_web("Controls", "controls.html")),
        );
        let page_id = page.id;
        assert_ne!(model_id, page_id);
        scene.nodes.push(model);
        scene.nodes.push(page);
        let a = ConnectorEnd::Anchored {
            node: model_id,
            side: Side::Right,
            t: 0.5,
        };
        let b = ConnectorEnd::Anchored {
            node: page_id,
            side: Side::Left,
            t: 0.5,
        };
        assert!(infer_control_binding(&scene, &a, &b, &|_| None, &|_| true).is_none());
        let binding = infer_control_binding(
            &scene,
            &a,
            &b,
            &|id| (id == page_id).then(|| "model".into()),
            &|id| id == model_id,
        )
        .unwrap();
        assert_eq!(binding.kind, InputKind::Control);
        assert!(binding.input_b);
        assert_eq!(binding.slot.as_deref(), Some("model"));
        assert_eq!(serde_json::to_value(binding).unwrap()["kind"], "control");
    }
}
