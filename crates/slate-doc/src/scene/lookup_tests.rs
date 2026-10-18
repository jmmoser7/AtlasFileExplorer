//! Canonical scene lookup/cache invariants; fixture stays with the existing scene tests.
use super::{tests::scene_with_frame_and_image, *};

#[test]
fn node_lookup_survives_live_edits_journal_and_reload() {
    let (mut scene, frame, image) = scene_with_frame_and_image();
    assert_eq!(scene.index_of(image), Some(1));
    let before = scene.node(image).unwrap().clone();
    scene.node_mut(image).unwrap().rect.x += 35.0;
    let after = scene.node(image).unwrap().clone();
    let mut journal = SceneJournal::default();
    journal.record(vec![SceneCmd::Patch {
        before: Box::new(before.clone()),
        after: Box::new(after.clone()),
    }]);
    assert!(journal.undo(&mut scene));
    assert_eq!(scene.node(image), Some(&before));
    assert!(journal.redo(&mut scene));
    assert_eq!(scene.node(image), Some(&after));
    let removed = scene.node(frame).unwrap().clone();
    assert!(journal.commit(
        &mut scene,
        vec![SceneCmd::Remove {
            index: 0,
            node: removed
        }]
    ));
    assert_eq!(scene.index_of(image), Some(0));
    assert!(scene.node(frame).is_none());
    assert!(journal.undo(&mut scene));
    assert_eq!(scene.index_of(image), Some(1));
    let json = serde_json::to_string(&scene).unwrap();
    assert!(!json.contains("node_positions"));
    let restored: Scene = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.node(image), Some(&after));
    assert_eq!(scene, restored);
}

#[test]
fn node_lookup_recovers_from_importer_vector_changes() {
    let (mut scene, frame, image) = scene_with_frame_and_image();
    assert_eq!(scene.index_of(image), Some(1));
    scene.nodes.swap(0, 1);
    assert_eq!(scene.index_of(image), Some(0));
    assert_eq!(scene.index_of(frame), Some(1));
    let replacement = scene.build_duplicate(&scene.nodes[0].clone(), 0.0, 0.0);
    let replacement_id = replacement.id;
    scene.nodes[0] = replacement;
    assert_eq!(scene.index_of(replacement_id), Some(0));
    assert!(scene.node(image).is_none());
    scene.nodes.clear();
    assert!(scene.node(frame).is_none());
}

#[test]
fn node_lookup_survives_bulk_add_and_identity_replacement() {
    let (mut scene, frame, image) = scene_with_frame_and_image();
    assert_eq!(scene.index_of(image), Some(1));
    let source = scene.node(image).unwrap().clone();
    let first = scene.build_duplicate(&source, 10.0, 0.0);
    let second = scene.build_duplicate(&source, 20.0, 0.0);
    let ids = [first.id, second.id];
    let mut journal = SceneJournal::default();
    assert!(journal.commit(
        &mut scene,
        vec![
            SceneCmd::Add {
                index: 2,
                node: first
            },
            SceneCmd::Add {
                index: 3,
                node: second
            },
        ]
    ));
    assert_eq!(scene.index_of(ids[0]), Some(2));
    assert_eq!(scene.index_of(ids[1]), Some(3));
    assert!(journal.undo(&mut scene));
    assert!(scene.node(ids[0]).is_none());
    assert_eq!(scene.index_of(frame), Some(0));
    assert!(journal.redo(&mut scene));
    assert_eq!(scene.index_of(ids[1]), Some(3));
    // Importer misuse must not return the old identity's node.
    scene.node_mut(ids[1]).unwrap().id = NodeId(100);
    assert!(scene.node(ids[1]).is_none());
    assert_eq!(scene.index_of(NodeId(100)), Some(3));
}

#[test]
fn grouped_patches_preserve_lookup_for_all_cache_states_and_structural_journal_edits() {
    for warm_lookup in [false, true] {
        for warm_spatial in [false, true] {
            let (fixture, _, image) = scene_with_frame_and_image();
            let json = serde_json::to_string(&fixture).unwrap();
            let mut scene: Scene = serde_json::from_str(&json).unwrap();
            let view = WorldRect::new(-100.0, -100.0, 2000.0, 2000.0);
            if warm_lookup {
                assert_eq!(scene.index_of(image), Some(1));
            }
            if warm_spatial {
                assert_eq!(scene.query_rect(view).len(), 2);
            }
            assert_eq!(
                scene.node_positions.borrow().len == scene.nodes.len(),
                warm_lookup
            );
            assert_eq!(
                scene.spatial.borrow().is_current(scene.scene_gen()),
                warm_spatial
            );
            let ids: Vec<_> = scene.nodes.iter().map(|node| node.id).collect();
            let generation = scene.scene_gen();
            let rebuilds = scene.spatial_rebuilds();
            let patches: Vec<_> = scene
                .nodes
                .iter()
                .map(|before| {
                    let mut after = before.clone();
                    after.rect = after.rect.translated(42.0, 17.0);
                    SceneCmd::Patch {
                        before: Box::new(before.clone()),
                        after: Box::new(after),
                    }
                })
                .collect();
            let mut journal = SceneJournal::default();
            let author = CmdAuthor::Agent("Lookup regression".into());
            assert!(journal.commit_as(&mut scene, patches, author.clone()));
            // No node()/index_of()/query has run since commit: these assertions
            // catch repeated invalidation rather than repairing it during observation.
            {
                let lookup = scene.node_positions.borrow();
                assert_eq!(
                    lookup.len,
                    scene.nodes.len(),
                    "lookup={warm_lookup} spatial={warm_spatial}"
                );
                for (index, node) in scene.nodes.iter().enumerate() {
                    assert_eq!(lookup.positions.get(&node.id), Some(&index));
                }
            }
            assert_eq!(
                scene.nodes.iter().map(|node| node.id).collect::<Vec<_>>(),
                ids
            );
            assert_eq!(scene.scene_gen(), generation + 2);
            assert_eq!(scene.spatial_rebuilds(), rebuilds);
            assert_eq!(
                scene.spatial.borrow().is_current(scene.scene_gen()),
                warm_spatial
            );
            assert_eq!(journal.last_author(), Some(&author));

            let parity = |scene: &Scene| {
                let reloaded: Scene =
                    serde_json::from_str(&serde_json::to_string(scene).unwrap()).unwrap();
                assert_eq!(scene, &reloaded);
                for (index, node) in scene.nodes.iter().enumerate() {
                    assert_eq!(scene.index_of(node.id), Some(index));
                    assert_eq!(reloaded.index_of(node.id), Some(index));
                    assert_eq!(scene.node(node.id), Some(node));
                    assert_eq!(scene.frame_of(node.id), reloaded.frame_of(node.id));
                }
                assert_eq!(scene.query_rect(view), reloaded.query_rect(view));
            };
            parity(&scene);
            let patched = scene.clone();
            assert!(journal.undo(&mut scene));
            assert_eq!(scene, fixture);
            parity(&scene);
            assert!(journal.redo(&mut scene));
            assert_eq!(scene, patched);
            parity(&scene);

            let source = scene.node(image).unwrap().clone();
            let inserted = scene.build_duplicate(&source, 80.0, 0.0);
            let inserted_id = inserted.id;
            assert!(journal.commit(
                &mut scene,
                vec![SceneCmd::Add {
                    index: 1,
                    node: inserted.clone()
                }]
            ));
            assert_eq!(scene.nodes[1].id, inserted_id);
            assert_eq!(scene.index_of(image), Some(2));
            parity(&scene);
            let added = scene.clone();
            assert!(journal.commit(
                &mut scene,
                vec![SceneCmd::Remove {
                    index: 1,
                    node: inserted
                }]
            ));
            assert!(scene.node(inserted_id).is_none());
            assert_eq!(scene.index_of(image), Some(1));
            parity(&scene);
            let removed = scene.clone();
            assert!(journal.undo(&mut scene));
            assert_eq!(scene, added);
            parity(&scene);
            assert!(journal.undo(&mut scene));
            assert_eq!(scene.nodes, patched.nodes);
            parity(&scene);
            assert!(journal.redo(&mut scene));
            assert_eq!(scene, added);
            parity(&scene);
            assert!(journal.redo(&mut scene));
            assert_eq!(scene, removed);
            parity(&scene);

            let before = scene.nodes[0].clone();
            let mut after = before.clone();
            after.id = NodeId(999_999);
            let generation = scene.scene_gen();
            let invalid = SceneCmd::Patch {
                before: Box::new(before.clone()),
                after: Box::new(after),
            };
            assert!(!scene.apply(&invalid));
            assert_eq!(scene.scene_gen(), generation);
            assert_eq!(scene, removed);
            let mut valid_after = before.clone();
            valid_after.rect.x += 100.0;
            let valid = SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(valid_after),
            };
            assert!(!scene.apply_all(&[valid, invalid]));
            assert_eq!(
                scene, removed,
                "failed grouped patch rolls back its successful prefix"
            );
            parity(&scene);
        }
    }
}
