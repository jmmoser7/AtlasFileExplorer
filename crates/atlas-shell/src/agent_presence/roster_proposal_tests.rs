//! Decisions are real input targets inside the existing scrollable conversation.
use super::*;

fn decision(f: &Fixture, proposal: &str, accept: bool) -> (Rect, Rect) {
    let response = f
        .ctx
        .read_response(Id::new((
            "presence-proposal-decision",
            "dot",
            proposal,
            accept,
        )))
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
                f.settle();
                let (button, hit) = decision(&f, &proposal, true);
                if hit.contains(button.center()) {
                    assert!(f
                        .click(button.center())
                        .contains(&PresenceAction::AcceptProposal {
                            participant: "dot".into(),
                            id: proposal.clone(),
                        }));
                    reachable = true;
                    break;
                }
            }
            assert!(reachable, "wheel reaches {proposal} at {zoom}");
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
