use super::*;
use crate::app::board_flow::tests::generator_with_output;

#[test]
fn canvas_settings_apply_to_hidden_wires_preserve_selection_and_undo_together() {
    let (mut h, image) = generator_with_output("canvas-wire-settings");
    let a = h
        .app
        .add_connector(
            ConnectorEnd::Free { point: [0.0, 0.0] },
            ConnectorEnd::Free {
                point: [100.0, 0.0],
            },
        )
        .unwrap();
    let b = h
        .app
        .add_connector(
            ConnectorEnd::Free {
                point: [0.0, 100.0],
            },
            ConnectorEnd::Free {
                point: [100.0, 100.0],
            },
        )
        .unwrap();
    h.app.patch_nodes(&[b], |node| node.hidden = true);
    let before = [a, b].map(|id| h.app.doc().scene.node(id).unwrap().clone());
    h.app.board_sel = std::iter::once(image).collect();
    let selected = h.app.board_sel.clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(h.app.apply_canvas_wire_properties(vec![
        Property::StrokeWidth(7.0),
        Property::StrokeRgb([120, 30, 90]),
        Property::WireRouting(WireRouting::Orthogonal)
    ]));
    for id in [a, b] {
        let NodeKind::Connector(connector) = &h.app.doc().scene.node(id).unwrap().kind else {
            unreachable!()
        };
        assert_eq!(connector.stroke.width, 7.0);
        assert_eq!(connector.color.unwrap().0[..3], [120, 30, 90]);
        assert_eq!(connector.routing, Some(WireRouting::Orthogonal));
    }
    assert_eq!(h.app.board_sel, selected);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(h.app.tab().dirty);
    h.app.board_undo();
    for node in before {
        assert_eq!(h.app.doc().scene.node(node.id), Some(&node));
    }
}

#[test]
fn global_width_changes_leave_locked_wires_and_authored_colors_alone() {
    let (mut h, _) = generator_with_output("canvas-wire-locks");
    let a = h
        .app
        .add_connector(
            ConnectorEnd::Free { point: [0.0, 0.0] },
            ConnectorEnd::Free {
                point: [100.0, 0.0],
            },
        )
        .unwrap();
    let b = h
        .app
        .add_connector(
            ConnectorEnd::Free {
                point: [0.0, 100.0],
            },
            ConnectorEnd::Free {
                point: [100.0, 100.0],
            },
        )
        .unwrap();
    h.app.patch_nodes(&[a, b], |node| {
        if let NodeKind::Connector(c) = &mut node.kind {
            c.set_color(Some(slate_doc::scene::Rgba([10, 20, 30, 255])));
        }
        if node.id == b {
            node.locked = true;
        }
    });
    let locked = h.app.doc().scene.node(b).unwrap().clone();
    assert!(h
        .app
        .apply_canvas_wire_properties(vec![Property::StrokeWidth(9.0)]));
    assert_eq!(h.app.doc().scene.node(b), Some(&locked));
    let NodeKind::Connector(c) = &h.app.doc().scene.node(a).unwrap().kind else {
        unreachable!()
    };
    assert_eq!(c.color.unwrap().0, [10, 20, 30, 255]);
    assert_eq!(c.stroke.width, 9.0);
}
