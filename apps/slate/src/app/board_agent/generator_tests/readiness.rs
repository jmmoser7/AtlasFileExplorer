//! Generate's paint-time readiness shares the resolved generator view.
use super::*;
use crate::app::board_agent::runtime::INPUT_RESOLVES_ON_THIS_THREAD;
use crate::app::board_agent::test_support::note;
use atlas_agent::InputSlot;

fn wire(h: &mut crate::app::tests::Harness, source: NodeId, target: NodeId, slot: InputSlot) {
    let t = slate_doc::agent_inputs::input_ports(&h.app.doc().scene, target)
        .iter()
        .find(|port| port.slot == slot)
        .unwrap()
        .t;
    let wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: source,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: target,
            side: Side::Left,
            t,
        },
    );
    h.app.add_nodes(vec![wire]);
}

#[test]
fn generate_readiness_requires_explicit_primary_and_prompt_wires() {
    let (mut h, id) = generator_with_output("readiness-explicit-wires");
    let prompt = note(&mut h, Pos2::new(-400.0, 0.0), "evening light");
    wire(&mut h, prompt, id, InputSlot::Prompt);
    assert!(h
        .app
        .generator_input_refs(id, true)
        .unwrap()
        .on(InputSlot::Media)
        .next()
        .is_some());
    assert!(
        !h.app.image_agent_has_wired_image_and_prompt(id),
        "current-image fallback is not an authored primary wire"
    );
    let style = place(&mut h, "style.png", -600.0);
    wire(&mut h, style, id, InputSlot::Style);
    assert!(
        !h.app.image_agent_has_wired_image_and_prompt(id),
        "style cannot replace Media"
    );
    let primary = place(&mut h, "primary.png", -800.0);
    wire(&mut h, primary, id, InputSlot::Media);
    assert!(h.app.image_agent_has_wired_image_and_prompt(id));
    h.app.text_edit = Some((prompt, "  ".into()));
    assert!(
        !h.app.image_agent_has_wired_image_and_prompt(id),
        "an empty live prompt disables the direct run"
    );
}

#[test]
fn generate_readiness_reuses_the_view_until_scene_output_or_live_edit_changes() {
    let (mut h, id) = generator_with_output("readiness-revision-cache");
    let prompt = note(&mut h, Pos2::new(-400.0, 0.0), "evening light");
    wire(&mut h, prompt, id, InputSlot::Prompt);
    let primary = place(&mut h, "primary.png", -800.0);
    wire(&mut h, primary, id, InputSlot::Media);
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| count.set(0));
    for _ in 0..20 {
        assert!(h.app.image_agent_has_wired_image_and_prompt(id));
        assert!(h.app.generator_view(id).wired_image_and_prompt);
    }
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| {
        assert_eq!(
            count.get(),
            1,
            "paint and layout share one input resolution"
        )
    });
    h.app.text_edit = Some((prompt, String::new()));
    assert!(!h.app.image_agent_has_wired_image_and_prompt(id));
    assert!(!h.app.image_agent_has_wired_image_and_prompt(id));
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| assert_eq!(count.get(), 2));
    h.app.agents.output_epoch += 1;
    assert!(!h.app.image_agent_has_wired_image_and_prompt(id));
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| assert_eq!(count.get(), 3));
    h.app.text_edit = None;
    h.app.patch_nodes(&[prompt], |node| {
        if let NodeKind::Text(text) = &mut node.kind {
            text.text.clear();
        }
    });
    assert!(!h.app.image_agent_has_wired_image_and_prompt(id));
    INPUT_RESOLVES_ON_THIS_THREAD.with(|count| assert_eq!(count.get(), 4));
}
