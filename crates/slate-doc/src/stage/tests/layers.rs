use super::*;
use crate::image_paint::{ImageResultId, ImageResultLayers, PaintLayer, PaintLayerId};
use crate::scene::{ImageNode, Node};

const LAYER: PaintLayerId = PaintLayerId(7);

fn layered_scene(generated: bool) -> (Scene, NodeId, Node) {
    let mut scene = Scene::default();
    let child = sample_node(&mut scene);
    let mut image = ImageNode::new(crate::ItemId(1));
    let mut layer = PaintLayer::new(LAYER);
    layer.nodes.push(child.clone());
    if generated {
        let mut stack = ImageResultLayers::new(ImageResultId::Output("render-1".into()));
        stack.layers.push(layer);
        image.result_layers.push(stack);
    } else {
        image.paint_layers.push(layer);
    }
    let host = scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 80.0),
        NodeKind::Image(image),
    );
    let id = host.id;
    assert!(scene.apply(&SceneCmd::Add {
        index: 0,
        node: host
    }));
    (scene, id, child)
}

fn patch(host: NodeId, before: Node, after: Node) -> SceneCmd {
    SceneCmd::LayerNodePatch {
        host,
        layer: LAYER,
        index: 0,
        before: Box::new(before),
        after: Box::new(after),
    }
}

fn assert_stale_layer_refusal(remove: bool) {
    // Both the transitional card-wide stack and generated result stacks use
    // the same preflight and preserve the existing human undo entry.
    for generated in [false, true] {
        let (mut scene, host, original) = layered_scene(generated);
        let original_nodes = scene.nodes.clone();
        let mut human = original.clone();
        human.rect.x += 0.25;
        let mut journal = SceneJournal::default();
        assert!(journal.commit(
            &mut scene,
            vec![patch(host, original.clone(), human.clone())]
        ));
        let mut agent = original.clone();
        agent.rect.y += 0.5;
        let stale = if remove {
            SceneCmd::LayerNodeRemove {
                host,
                layer: LAYER,
                index: 0,
                node: original.clone(),
            }
        } else {
            patch(host, original.clone(), agent)
        };
        let added = sample_node(&mut scene);
        let proposal = proposal(vec![
            SceneCmd::Add {
                index: 0,
                node: added,
            },
            stale,
        ]);
        let before = serde_json::to_value(&scene).unwrap();
        let undo_depth = journal.undo_depth();
        assert_eq!(
            accept(&proposal, Some(target_workbook()), &mut scene, &mut journal),
            Err(StaleReason::NodeChanged { node: original.id })
        );
        assert_eq!(serde_json::to_value(&scene).unwrap(), before);
        assert_eq!(
            journal.undo_depth(),
            undo_depth,
            "no partial proposal journal group"
        );
        assert_eq!(journal.last_author(), Some(&CmdAuthor::Human));
        assert!(journal.undo(&mut scene));
        assert_eq!(scene.nodes, original_nodes);
        assert!(!journal.can_undo(), "only the human edit was undone");
    }
}

#[test]
fn stale_layer_patch_preserves_human_content_and_refuses_the_entire_group() {
    assert_stale_layer_refusal(false);
}

#[test]
fn stale_layer_remove_preserves_human_content_and_refuses_the_entire_group() {
    assert_stale_layer_refusal(true);
}

#[test]
fn layer_preflight_follows_sequential_expected_states_and_undoes_as_one_group() {
    for generated in [false, true] {
        let (mut scene, host, original) = layered_scene(generated);
        let mut first = original.clone();
        first.rect.x += 0.2;
        let mut second = first.clone();
        second.rect.y += 0.3;
        let proposal = proposal(vec![
            patch(host, original.clone(), first.clone()),
            patch(host, first, second.clone()),
            SceneCmd::LayerNodeRemove {
                host,
                layer: LAYER,
                index: 0,
                node: second,
            },
        ]);
        let before = serde_json::to_value(&scene).unwrap();
        let mut journal = SceneJournal::default();
        accept(&proposal, Some(target_workbook()), &mut scene, &mut journal).unwrap();
        assert!(crate::image_paint::find_layer_node(&scene, original.id).is_none());
        assert_eq!(journal.undo_depth(), 1);
        assert!(journal.undo(&mut scene));
        assert_eq!(serde_json::to_value(&scene).unwrap(), before);
    }
}

#[test]
fn stale_later_layer_patch_does_not_apply_a_valid_earlier_patch() {
    let (mut scene, host, original) = layered_scene(false);
    let mut first = original.clone();
    first.rect.x += 0.2;
    let mut second = original.clone();
    second.rect.y += 0.3;
    let proposal = proposal(vec![
        patch(host, original.clone(), first),
        patch(host, original.clone(), second),
    ]);
    let before = serde_json::to_value(&scene).unwrap();
    let mut journal = SceneJournal::default();
    assert_eq!(
        accept(&proposal, Some(target_workbook()), &mut scene, &mut journal),
        Err(StaleReason::NodeChanged { node: original.id })
    );
    assert_eq!(serde_json::to_value(&scene).unwrap(), before);
    assert!(!journal.can_undo());
}

#[test]
fn layer_preflight_observes_an_earlier_whole_host_patch() {
    let (mut scene, host, original) = layered_scene(false);
    let before_host = scene.node(host).unwrap().clone();
    let mut after_host = before_host.clone();
    let mut first = original.clone();
    first.rect.x += 0.2;
    let NodeKind::Image(image) = &mut after_host.kind else {
        panic!("image host");
    };
    image.paint_layers[0].nodes[0] = first.clone();
    let mut second = first.clone();
    second.rect.y += 0.3;
    let proposal = proposal(vec![
        SceneCmd::Patch {
            before: Box::new(before_host.clone()),
            after: Box::new(after_host),
        },
        patch(host, first, second.clone()),
    ]);
    let mut journal = SceneJournal::default();
    accept(&proposal, Some(target_workbook()), &mut scene, &mut journal).unwrap();
    let NodeKind::Image(image) = &scene.node(host).unwrap().kind else {
        panic!("image host");
    };
    assert_eq!(image.paint_layers[0].nodes[0], second);
    assert_eq!(journal.undo_depth(), 1);
    assert!(journal.undo(&mut scene));
    assert_eq!(scene.node(host), Some(&before_host));
}
