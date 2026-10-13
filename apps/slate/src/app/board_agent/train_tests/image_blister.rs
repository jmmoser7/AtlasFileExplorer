use super::*;
use crate::app::board_properties::Panel;

#[test]
fn generator_inputs_and_controls_live_in_the_selected_agent_blister() {
    let (mut h, id, note) = generator_with_note("gen_hover", "a nice public park");
    let model = wire_model(&mut h, id);
    h.frame();
    let r = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    let body = h.app.board_xf().rect_w2s(r);
    // Choosing a program focuses the portal, which reveals its controls.
    h.app.agent_blur();
    h.app.board_sel.clear();
    let rest = painted_text(&mut h, None);
    assert!(
        !rest
            .iter()
            .any(|t| t.starts_with("Prompt ·") || t == "Render"),
        "quiet at rest: {rest:?}"
    );
    assert!(
        !rest.iter().any(|t| t.contains("Connect")),
        "a wired generator asks for nothing: {rest:?}"
    );
    let hover = painted_text(&mut h, Some(body.center()));
    for hidden in [
        "Prompt · a nice public park",
        "Geometry · pavilion.3dm",
        "Render",
    ] {
        assert!(
            !hover.iter().any(|t| t == hidden),
            "passive hover keeps generator controls hidden: {hover:?}"
        );
    }
    assert!(
        !hover.iter().any(|t| t == "ComfyUI · Auto" || t == "Live"),
        "passive hover keeps model controls hidden: {hover:?}"
    );

    // Wired inputs are available from the selected Agent blister.
    h.app.board_sel = std::iter::once(id).collect();
    painted_text(&mut h, None); // selection change settles the property strip
    h.app.shape_properties.panel = Some(Panel::Agent);
    for _ in 0..12 {
        h.frame();
    }
    let selected = painted_text(&mut h, None);
    for expected in ["ComfyUI · Auto", "Source", "Single", "Settings"] {
        assert!(
            selected.iter().any(|t| t == expected),
            "missing {expected:?} in selected Agent blister: {selected:?}"
        );
    }
    let inputs = h.app.generator_view(id).inputs.clone();
    assert!(inputs
        .iter()
        .any(|i| i.node == note && i.label == "a nice public park"));
    assert!(inputs
        .iter()
        .any(|i| i.node == model && i.label == "pavilion.3dm"));
    let editor = *h
        .app
        .shape_properties
        .chrome_hits
        .iter()
        .max_by(|a, b| (a.width() * a.height()).total_cmp(&(b.width() * b.height())))
        .expect("selected Agent blister is open");
    let prompt_index = inputs
        .iter()
        .position(|input| input.node == note)
        .expect("wired prompt input has a blister circle");
    let prompt_input = Pos2::new(
        editor.left() + 23.0 + prompt_index as f32 * 27.0,
        editor.top() + 53.0,
    );
    press(&mut h, prompt_input);
    assert!(
        h.app.board_sel.contains(&note),
        "the wired prompt input circle selects its source"
    );

    // Live appears as a bare mode label; Play keeps the current result.
    h.app.board_sel = std::iter::once(id).collect();
    painted_text(&mut h, None);
    h.app.shape_properties.panel = Some(Panel::Agent);
    h.app.set_generator_live(id, true);
    for _ in 0..12 {
        h.frame();
    }
    let live = painted_text(&mut h, Some(body.center()));
    assert!(live.iter().any(|t| t == "Live"), "{live:?}");
    assert!(
        !live.iter().any(|t| t == "Keep"),
        "bare Play action: {live:?}"
    );
    let editor = *h
        .app
        .shape_properties
        .chrome_hits
        .iter()
        .max_by(|a, b| (a.width() * a.height()).total_cmp(&(b.width() * b.height())))
        .expect("selected live Agent blister is open");
    let datum = editor.shrink(12.0);
    let play = Pos2::new(datum.right() - 10.0, datum.top() + 12.0);
    let live_run = h.app.agents.live_run.get(&id).cloned().unwrap();
    press(&mut h, play);
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .live,
        "the bare Play action stays in Live mode"
    );
    assert_ne!(
        h.app.agents.live_run.get(&id),
        Some(&live_run),
        "the bare Play action keeps this result and starts a new live slot"
    );
}
