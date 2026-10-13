//! Stacking commands must update warm paint/pick caches and the undo timeline.

use super::*;

fn assert_stack(app: &SlateApp, expected: &[NodeId]) {
    let scene = &app.doc().scene;
    assert_eq!(
        scene.nodes.iter().map(|n| n.id).collect::<Vec<_>>(),
        expected
    );
    assert_eq!(scene.query_point(25.0, 25.0), expected);
    assert_eq!(app.board_pick_node(25.0, 25.0), expected.last().copied());
    let screen = ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0));
    assert_eq!(
        app.board_paint_nodes(screen)
            .iter()
            .map(|n| n.id)
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn stacking_updates_warm_paint_and_pick_order_and_undo_redo() {
    for to_front in [true, false] {
        let mut h = Harness::new("stacking");
        h.app.leave_home();
        h.app.ensure_work_tab();
        let a = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0);
        let b = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0);
        let c = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0);
        assert_stack(&h.app, &[a, b, c]); // Warm the spatial and identity caches.
        let depth = h.app.tab().journal.undo_depth();
        let selected = if to_front { a } else { c };
        h.app.reorder_nodes(&[selected], to_front);
        let expected = if to_front { [b, c, a] } else { [c, a, b] };
        assert_stack(&h.app, &expected);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        h.app.board_undo();
        assert_stack(&h.app, &[a, b, c]);
        h.app.board_redo();
        assert_stack(&h.app, &expected);
    }
}

#[test]
fn stacking_keeps_selection_order_and_refuses_read_only_edits() {
    for to_front in [true, false] {
        let mut h = Harness::new("stacking_selection");
        h.app.leave_home();
        h.app.ensure_work_tab();
        let ids: Vec<_> = (0..4)
            .map(|_| add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0))
            .collect();
        let [a, b, c, d] = ids[..] else {
            unreachable!()
        };
        assert_stack(&h.app, &ids);
        // Caller order and duplicate IDs must not determine stacking order.
        let selected = [c, a, c];
        let depth = h.app.tab().journal.undo_depth();
        h.app.tab_mut().read_only = true;
        h.app.reorder_nodes(&selected, to_front);
        assert_stack(&h.app, &ids);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        h.app.tab_mut().read_only = false;
        h.app.reorder_nodes(&selected, to_front);
        let expected = if to_front { [b, d, a, c] } else { [a, c, b, d] };
        assert_stack(&h.app, &expected);
        h.app.board_undo();
        assert_stack(&h.app, &ids);
        h.app.board_redo();
        assert_stack(&h.app, &expected);
    }
}
