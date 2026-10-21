//! Pointer interaction for the held result strip.
use super::*;

#[test]
fn hover_only_strip_survives_the_gap_and_a_held_click_without_selecting_the_host_first() {
    let (mut h, host) = fixture("result-hover-only-held-click");
    h.app.board_sel.clear();
    let tick = |h: &mut Harness, events: Vec<egui::Event>| {
        let time = h.ctx.input(|input| input.time) + 1.0 / 30.0;
        h.frame_with(|input| {
            input.time = Some(time);
            input.events.extend(events);
        });
    };
    tick(
        &mut h,
        vec![egui::Event::PointerMoved(Pos2::new(1300.0, 800.0))],
    );
    for _ in 0..20 {
        tick(&mut h, vec![]);
    }
    let host_rect = h.app.doc().scene.node(host).unwrap().rect;
    let body = h.app.board_xf().rect_w2s(host_rect);
    let at = point(&h, host, 1);
    tick(&mut h, vec![egui::Event::PointerMoved(at)]);
    assert!(!h
        .app
        .result_strip_hovered(&h.ctx, host, body, h.app.board_xf().z));
    assert!(h.app.agents.flow.results.menu.is_none());
    assert!(h.app.board_sel.is_empty());
    tick(&mut h, vec![egui::Event::PointerMoved(body.center())]);
    for _ in 0..8 {
        tick(&mut h, vec![]);
    }
    assert!(h.app.board_sel.is_empty());
    let placement = h.app.result_strip_placement(host, body);
    let gap = Pos2::new(at.x, placement.bottom() + 5.0 * h.app.board_xf().z);
    tick(&mut h, vec![egui::Event::PointerMoved(gap)]);
    assert!(
        h.app
            .result_strip_hovered(&h.ctx, host, body, h.app.board_xf().z),
        "the gap retains the visible strip"
    );
    tick(&mut h, vec![egui::Event::PointerMoved(at)]);
    tick(
        &mut h,
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    for _ in 0..20 {
        tick(&mut h, vec![]);
    }
    assert!(h.app.board_sel.is_empty(), "press belongs to the thumbnail");
    assert!(
        h.app.board_drag.is_none(),
        "the canvas never starts a gesture"
    );
    assert_eq!(
        h.app.doc().scene.node(host).unwrap().rect,
        host_rect,
        "the fitted preview must not move the thumbnail during its held press"
    );
    assert!(
        h.app
            .agents
            .flow
            .results
            .strips
            .get(&(h.app.tab().id, host))
            .unwrap()
            .is_positive(),
        "a held press outlasts the strip fade without hiding its target"
    );
    tick(
        &mut h,
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(h.app.agent_shown_index(host), Some(1));
    assert!(h.app.board_sel.contains(&host));
    assert!(h.app.agents.flow.results.menu.is_some());
}
