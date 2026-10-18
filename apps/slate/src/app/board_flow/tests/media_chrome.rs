use super::*;

#[test]
fn ports_show_near_a_generator_and_the_text_block_paints_its_controls() {
    let (mut h, generator) = generator_with_output("flow_paint");
    let detail = serde_json::json!({"source": generator.0, "kind": "text"});
    assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
    let block = *h.app.board_sel.iter().next().unwrap();
    h.frame();
    let r = h.app.doc().scene.node(generator).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = 0.6;
    h.frame();
    h.app.agent_blur();
    h.app.board_sel.clear();
    let rest = painted(&mut h, Pos2::new(2.0, 890.0));
    assert!(
        !rest.iter().any(|t| t == "Style image"),
        "quiet at rest: {rest:?}"
    );
    let xf = h.app.board_xf();
    let near = xf.rect_w2s(r).center();
    let hover = painted(&mut h, near);
    for label in ["Primary image", "Text", "Style image"] {
        assert!(
            hover.iter().any(|t| t == label),
            "missing {label}: {hover:?}"
        );
    }
    assert!(matches!(
        h.app.doc().scene.node(block).map(|n| &n.kind),
        Some(NodeKind::Image(i)) if i.agent.is_some()
    ));
    let block_rect = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(block).unwrap().rect);
    let unselected = painted(&mut h, block_rect.center());
    for label in ["Run", "Describe the visual style"] {
        assert!(
            unselected.iter().any(|text| text.starts_with(label)),
            "unselected body keeps {label}: {unselected:?}"
        );
    }
    h.app.board_sel = std::iter::once(block).collect();
    let selected = painted(&mut h, Pos2::new(2.0, 890.0));
    for label in ["Image", "Prompt"] {
        assert!(
            selected.iter().any(|t| t.starts_with(label)),
            "missing {label}: {selected:?}"
        );
    }
    for label in ["Run", "Describe the visual style"] {
        assert!(
            !selected.iter().any(|text| text.starts_with(label)),
            "selected text uses shared Agent controls: {selected:?}"
        );
    }
    assert!(
        h.app
            .open_object_panel(crate::app::board_properties::Panel::Agent),
        "selected text exposes its shared Agent control"
    );
    assert_eq!(
        h.app.shape_properties.panel,
        Some(crate::app::board_properties::Panel::Agent)
    );
    assert!(
        !selected.iter().any(|t| t.starts_with("Local ·")),
        "no model chip: {selected:?}"
    );
}
