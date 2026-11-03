//! Creation provenance is local to the newest author-owned journal group.
use super::*;

#[test]
fn creation_query_excludes_other_authors_older_groups_and_redo() {
    let mut scene = Scene::default();
    let mut journal = SceneJournal::default();
    let human = CmdAuthor::Human;
    let other = CmdAuthor::Agent("other".into());
    let first = scene.build_node(
        WorldRect::new(0.0, 0.0, 20.0, 20.0),
        NodeKind::Image(ImageNode::new(ItemId::NONE)),
    );
    let first_id = first.id;
    assert!(journal.commit(
        &mut scene,
        vec![SceneCmd::Add {
            index: 0,
            node: first
        }]
    ));
    let first_step = journal.top_token();
    assert!(journal.top_creates_node(first_id, &human));
    assert!(!journal.top_creates_node(first_id, &other));
    let depth = journal.undo_depth();
    assert!(journal.undo(&mut scene));
    assert!(!journal.top_creates_node(first_id, &human));
    assert!(journal.redo(&mut scene));
    assert_eq!(journal.undo_depth(), depth);
    assert!(journal.top_creates_node(first_id, &human));

    let second = scene.build_node(
        WorldRect::new(40.0, 0.0, 20.0, 20.0),
        NodeKind::Image(ImageNode::new(ItemId::NONE)),
    );
    let second_id = second.id;
    assert!(journal.commit(
        &mut scene,
        vec![SceneCmd::Add {
            index: 1,
            node: second
        }]
    ));
    assert_ne!(
        journal.top_token(),
        first_step,
        "an old creation token is stale"
    );
    assert!(!journal.top_creates_node(first_id, &human));
    assert!(journal.top_creates_node(second_id, &human));
    assert!(journal.undo(&mut scene));
    assert!(
        !journal.top_creates_node(first_id, &human),
        "a pending redo excludes amendment"
    );
}
