//! Decisions are real input targets inside the existing scrollable conversation.
use super::*;

fn decision_id(proposal: &str, accept: bool) -> Id {
    Id::new(("presence-proposal-decision", "dot", proposal, accept))
}

fn decision(f: &Fixture, proposal: &str, accept: bool) -> (Rect, Rect) {
    let response = f
        .ctx
        .read_response(decision_id(proposal, accept))
        .expect("the transcript painted its decision target");
    let transform = f
        .ctx
        .layer_transform_to_global(response.layer_id)
        .unwrap_or_default();
    (
        transform * response.rect,
        transform * response.interact_rect,
    )
}

fn decision_trace(f: &Fixture, proposal: &str) -> String {
    let id = decision_id(proposal, true);
    let target = f.ctx.read_response(id).map(|response| {
        let transform = f.ctx.layer_transform_to_global(response.layer_id).unwrap_or_default();
        format!(
            "id={id:?} layer={:?} transform={transform:?} local={:?} local_hit={:?} screen={:?} screen_hit={:?} hovered={} contains_pointer={} pointer_down={} top_layer={:?}",
            response.layer_id, response.rect, response.interact_rect,
            transform * response.rect, transform * response.interact_rect,
            response.hovered(), response.contains_pointer(), response.is_pointer_button_down_on(),
            f.ctx.layer_id_at(transform * response.rect.center()),
        )
    });
    let input = f.ctx.input(|i| format!(
        "pointer={:?} raw_scroll={:?} remaining_smooth_scroll={:?} seconds_since_scroll={} any_down={}",
        i.pointer.interact_pos(), i.raw_scroll_delta, i.smooth_scroll_delta,
        i.time_since_last_scroll(), i.pointer.any_down(),
    ));
    let interaction = f.ctx.interaction_snapshot(|s| {
        format!(
            "clicked={:?} dragged={:?} hovered={:?} contains={:?}",
            s.clicked, s.dragged, s.hovered, s.contains_pointer,
        )
    });
    format!(
        "target={target:?}; {input}; {interaction}; panel={:?}; active_chat={:?}",
        f.state.last_popup_rect,
        f.state.active_chat()
    )
}

fn settle_wheel(f: &mut Fixture, proposal: &str, zoom: f32) {
    let mut previous = None;
    for _ in 0..90 {
        f.frame(vec![]);
        let current = f
            .ctx
            .read_response(decision_id(proposal, true))
            .map(|response| (response.rect, response.interact_rect));
        // ScrollArea consumes smooth_scroll_delta, so its post-frame zero is
        // not evidence that egui's queued wheel smoothing has finished.
        let quiet = f
            .ctx
            .input(|input| input.time_since_last_scroll() > 2.0 / 60.0);
        if quiet && current.is_some() && current == previous {
            return;
        }
        previous = current;
    }
    panic!(
        "wheel did not settle for {proposal} at {zoom}: {}",
        decision_trace(f, proposal)
    );
}

#[test]
fn proposal_buttons_emit_only_the_selected_conversation_and_proposal() {
    for accept in [true, false] {
        let mut f = Fixture::new();
        f.proposals
            .push(("proposal-one".into(), "Move board objects".into(), false));
        let out = f.open_chat("dot");
        assert!(text_rect(&out, "Proposed board change").is_some());
        assert!(text_rect(&out, "Move board objects").is_some());
        let (button, hit) = decision(&f, "proposal-one", accept);
        assert!(hit.contains_rect(button));
        let expected = if accept {
            PresenceAction::AcceptProposal {
                participant: "dot".into(),
                id: "proposal-one".into(),
            }
        } else {
            PresenceAction::RejectProposal {
                participant: "dot".into(),
                id: "proposal-one".into(),
            }
        };
        assert_eq!(f.click(button.center()), vec![expected]);
    }
}

#[test]
fn recovery_warning_keeps_reject_visible_without_offering_a_second_acceptance() {
    let mut f = Fixture::new();
    f.proposals
        .push(("interrupted".into(), "Move board objects".into(), true));
    let out = f.open_chat("dot");
    assert!(text_rect(
        &out,
        "Acceptance was interrupted. Review the board before dismissing this proposal."
    )
    .is_some());
    let (accept, _) = decision(&f, "interrupted", true);
    assert!(!f
        .click(accept.center())
        .iter()
        .any(|a| matches!(a, PresenceAction::AcceptProposal { .. })));
    let (reject, hit) = decision(&f, "interrupted", false);
    assert!(hit.contains_rect(reject));
    assert!(f
        .click(reject.center())
        .contains(&PresenceAction::RejectProposal {
            participant: "dot".into(),
            id: "interrupted".into()
        }));
}

#[test]
fn applying_is_compact_blocks_repeat_accept_and_keeps_reject_and_other_decisions_live() {
    let mut f = Fixture::new();
    f.proposals = vec![
        ("witnessing".into(), "Move board objects".into(), false),
        ("other".into(), "Another board change".into(), false),
    ];
    f.open_chat("dot");
    let before = [
        decision(&f, "witnessing", true).0,
        decision(&f, "witnessing", false).0,
    ];
    f.applying_proposal = Some("witnessing".into());
    let out = f.settle();
    assert!(text_rect(&out, "Applying visibly…").is_some());
    assert_eq!(
        [
            decision(&f, "witnessing", true).0,
            decision(&f, "witnessing", false).0
        ],
        before
    );
    assert!(!f
        .click(before[0].center())
        .iter()
        .any(|action| matches!(action, PresenceAction::AcceptProposal { .. })));
    let (reject, hit) = decision(&f, "witnessing", false);
    assert!(hit.contains_rect(reject));
    assert_eq!(
        f.click(reject.center()),
        vec![PresenceAction::RejectProposal {
            participant: "dot".into(),
            id: "witnessing".into(),
        }]
    );
    let (other, _) = decision(&f, "other", true);
    assert_eq!(
        f.click(other.center()),
        vec![PresenceAction::AcceptProposal {
            participant: "dot".into(),
            id: "other".into(),
        }]
    );
}

#[test]
fn pending_cards_scroll_while_the_attached_composer_remains_reachable_at_all_zooms() {
    for zoom in [0.25, 0.8, 1.0, 2.0] {
        let mut f = Fixture::new();
        f.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1200.0));
        f.proposals = (0..12)
            .map(|n| (format!("proposal-{n}"), format!("Board change {n}"), false))
            .collect();
        f.state.open_chat_at("dot", Pos2::new(400.0, 150.0), zoom);
        f.settle();
        let panel = f.state.last_popup_rect;
        assert!(f.screen.contains_rect(panel));
        let (composer, hit) =
            super::visual::screen_response(&f, Id::new(("presence-composer", "dot")));
        assert!(panel.contains_rect(composer) && hit.contains_rect(composer.shrink(zoom)));
        let (last, hit) = decision(&f, "proposal-11", true);
        assert!(last.bottom() < composer.top() && hit.is_positive());
        let scroll_at = hit.center();
        // Real wheel events must expose both ends of this same persistent
        // ScrollArea. Click the exact first/last targets after each traversal.
        for (index, delta) in [(0, 1200.0), (11, -1200.0)] {
            let proposal = format!("proposal-{index}");
            let mut reachable = false;
            for _ in 0..24 {
                f.frame(vec![
                    egui::Event::PointerMoved(scroll_at),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: Vec2::new(0.0, delta),
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
                settle_wheel(&mut f, &proposal, zoom);
                let (button, hit) = decision(&f, &proposal, true);
                if hit.contains(button.center()) {
                    let before = decision_trace(&f, &proposal);
                    let pressed_actions = f.frame(pointer(button.center(), true)).0;
                    let pressed = decision_trace(&f, &proposal);
                    let released_actions = f.frame(pointer(button.center(), false)).0;
                    let released = decision_trace(&f, &proposal);
                    assert!(released_actions.contains(&PresenceAction::AcceptProposal {
                            participant: "dot".into(),
                            id: proposal.clone(),
                        }),
                        "exact decision click failed: zoom={zoom} proposal={proposal} target={button:?} hit={hit:?} press_actions={pressed_actions:?} release_actions={released_actions:?}\nbefore: {before}\npressed: {pressed}\nreleased: {released}",
                    );
                    reachable = true;
                    break;
                }
            }
            assert!(
                reachable,
                "wheel reaches {proposal} at {zoom}: {}",
                decision_trace(&f, &proposal)
            );
        }
        let (composer_after, hit_after) =
            super::visual::screen_response(&f, Id::new(("presence-composer", "dot")));
        assert_eq!(
            composer_after, composer,
            "scrolling leaves the composer fixed"
        );
        assert!(hit_after.contains_rect(composer_after.shrink(zoom)));
        f.click(composer.center());
        let text = format!("Keep the proposal list at {zoom}");
        assert!(f.frame(vec![egui::Event::Text(text.clone())]).0.contains(
            &PresenceAction::Draft {
                id: "dot".into(),
                text: text.clone()
            }
        ));
        let (send, hit) = super::visual::screen_response(&f, Id::new(("presence-send", "dot")));
        assert!(hit.contains_rect(send));
        assert!(f.click(send.center()).contains(&PresenceAction::Send {
            id: "dot".into(),
            text
        }));
    }
}
