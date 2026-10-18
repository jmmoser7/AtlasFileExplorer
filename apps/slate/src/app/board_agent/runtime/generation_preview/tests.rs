use super::*;
use crate::app::board_agent::test_support::generator_with_note;

fn ticket(tab: u64, node: u64, frame: u64) -> Ticket {
    Ticket {
        identity: Identity {
            tab,
            node: NodeId(node),
            session: "session".into(),
            request: "run".into(),
        },
        frame,
        step: 1,
        steps: 5,
    }
}

#[test]
fn admission_is_bounded_across_tabs_and_samples_each_frame_once() {
    let mut state = GenerationPreviews::default();
    for slot in 0..LIVE_INFLIGHT as u32 {
        let ticket = ticket(u64::from(slot), 1, 10);
        assert!(state.available(&ticket.identity));
        state.remember(&ticket);
        assert!(state.seen(&ticket.identity, 10));
        assert!(state.seen(&ticket.identity, 9));
        assert!(!state.seen(&ticket.identity, 11));
        state.pending.insert(slot, ticket);
    }
    assert!(!state.available(&ticket(5, 1, 11).identity));
    state.pending.remove(&0);
    assert!(state.available(&ticket(5, 1, 11).identity));
    assert!(
        !state.available(&ticket(1, 1, 11).identity),
        "one decode per source"
    );
}

#[test]
fn changed_tab_request_session_and_stop_reject_a_ready_reply() {
    for scenario in 0..4 {
        let (mut h, node, _) = generator_with_note(&format!("preview-stale-{scenario}"), "Draw");
        let session = h.app.agents.bindings[&node].clone();
        let identity = Identity {
            tab: h.app.tab().id,
            node,
            session,
            request: "run".into(),
        };
        h.app.agents.requests.insert(node, "run".into());
        h.app
            .agents
            .awaiting
            .insert(node, AgentAwait::Responding { req_at: 0 });
        assert!(h.app.generation_preview_identity_current(&identity));
        h.app.agents.live_decodes.pending.insert(
            12,
            Ticket {
                identity,
                frame: 3,
                step: 1,
                steps: 5,
            },
        );
        match scenario {
            0 => h.app.tab_mut().id = h.app.tab().id.wrapping_add(1),
            1 => {
                h.app.agents.requests.insert(node, "new-run".into());
            }
            2 => {
                h.app.agents.bindings.insert(node, "new-session".into());
            }
            _ => {
                h.app.agents.awaiting.remove(&node);
            }
        }
        assert!(h
            .app
            .consume_generation_preview(
                PreviewResult {
                    id: 12,
                    outcome: PreviewOutcome::Ready(1, 1, vec![0; 4]),
                },
                &h.ctx
            )
            .is_none());
        assert!(h.app.agents.live_decodes.pending.is_empty());
        assert!(h.app.agents.previews.is_empty());
    }
}

#[test]
fn cancellation_retires_live_bookkeeping_without_marking_source_failed() {
    let (mut h, node, _) = generator_with_note("live-preview-cancel", "Draw");
    let identity = Identity {
        tab: h.app.tab().id,
        node,
        session: h.app.agents.bindings[&node].clone(),
        request: "run".into(),
    };
    h.app.agents.requests.insert(node, "run".into());
    h.app
        .agents
        .awaiting
        .insert(node, AgentAwait::Responding { req_at: 0 });
    let ticket = Ticket {
        identity: identity.clone(),
        frame: 3,
        step: 3,
        steps: 5,
    };
    h.app.agents.live_decodes.remember(&ticket);
    h.app.agents.live_decodes.pending.insert(8, ticket);
    assert!(h
        .app
        .consume_generation_preview(
            PreviewResult {
                id: 8,
                outcome: PreviewOutcome::Cancelled,
            },
            &h.ctx
        )
        .is_none());
    assert!(h.app.agents.live_decodes.pending.is_empty());
    assert!(!h.app.agents.live_decodes.seen(&identity, 3));
    assert!(h.app.agents.live_decodes.available(&identity));
    assert!(h.app.preview_failed.is_empty());
}

#[test]
fn switching_tabs_preserves_the_global_pending_bound_and_routes_old_replies() {
    let mut h = crate::app::tests::Harness::new("live-preview-tab-pending");
    h.app.sync_agent_doc();
    let original = h.app.tab().id;
    let original_index = h.app.active_tab;
    seed_residents(&mut h);
    let seen = ticket(original, 1, 3);
    h.app.agents.live_decodes.remember(&seen);
    for slot in 0..LIVE_INFLIGHT as u32 {
        h.app
            .agents
            .live_decodes
            .pending
            .insert(slot, ticket(original, u64::from(slot) + 1, 3));
    }
    h.app.new_tab();
    assert!(h.app.agents.previews.is_empty());
    assert!(!h.app.agents.live_decodes.seen(&seen.identity, 3));
    assert_eq!(h.app.agents.live_decodes.pending.len(), LIVE_INFLIGHT);
    assert!(!h
        .app
        .agents
        .live_decodes
        .available(&ticket(h.app.tab().id, 1, 4).identity));
    assert!(h
        .app
        .consume_generation_preview(
            PreviewResult {
                id: 0,
                outcome: PreviewOutcome::Cancelled,
            },
            &h.ctx
        )
        .is_none());
    h.app.switch_tab(original_index);
    assert!(
        h.app.agents.previews.is_empty(),
        "parked tabs retain no sampling textures"
    );
    assert_eq!(h.app.agents.live_decodes.pending.len(), LIVE_INFLIGHT - 1);
    assert!(!h.app.agents.live_decodes.pending.contains_key(&0));
}

fn seed_residents(h: &mut crate::app::tests::Harness) {
    for index in 0..LIVE_RESIDENTS {
        let texture = h.ctx.load_texture(
            format!("resident-{index}"),
            egui::ColorImage::new([1, 1], Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        h.app.agents.previews.insert(
            NodeId(10_000 + index as u64),
            LivePreview {
                tab: h.app.tab().id,
                session: "session".into(),
                request: "run".into(),
                frame: 3,
                step: 3,
                steps: 5,
                texture,
            },
        );
    }
}

#[test]
fn ninth_resident_is_refused_before_admission_and_before_upload_then_can_retry() {
    let (mut h, node, _) = generator_with_note("live-preview-resident-cap", "Draw");
    h.app.agents.requests.insert(node, "run".into());
    h.app
        .agents
        .awaiting
        .insert(node, AgentAwait::Responding { req_at: 0 });
    seed_residents(&mut h);
    let allocated = h.ctx.tex_manager().read().num_allocated();
    assert_eq!(h.app.agents.previews.len(), LIVE_RESIDENTS);
    assert!(!h.app.generation_preview_resident_available(node));
    assert!(
        h.app.generation_preview_resident_available(NodeId(10_000)),
        "existing residents can keep receiving newer sampling frames"
    );
    assert!(h.app.agents.live_decodes.pending.is_empty());
    assert_eq!(
        h.ctx.tex_manager().read().num_allocated(),
        allocated,
        "admission check creates no texture"
    );
    let identity = Identity {
        tab: h.app.tab().id,
        node,
        session: h.app.agents.bindings[&node].clone(),
        request: "run".into(),
    };
    let rejected = Ticket {
        identity: identity.clone(),
        frame: 4,
        step: 4,
        steps: 5,
    };
    h.app.agents.live_decodes.remember(&rejected);
    h.app.agents.live_decodes.pending.insert(8, rejected);
    assert!(h
        .app
        .consume_generation_preview(
            PreviewResult {
                id: 8,
                outcome: PreviewOutcome::Ready(1, 1, vec![0; 4]),
            },
            &h.ctx
        )
        .is_none());
    assert_eq!(h.app.agents.previews.len(), LIVE_RESIDENTS);
    assert_eq!(h.ctx.tex_manager().read().num_allocated(), allocated);
    assert!(!h.app.agents.live_decodes.seen(&identity, 4));
    assert!(!h.app.agents.previews.contains_key(&node));
    h.app.agents.previews.remove(&NodeId(10_000));
    assert!(h.app.generation_preview_resident_available(node));
    h.app.agents.live_decodes.pending.insert(
        9,
        Ticket {
            identity,
            frame: 4,
            step: 4,
            steps: 5,
        },
    );
    assert!(h
        .app
        .consume_generation_preview(
            PreviewResult {
                id: 9,
                outcome: PreviewOutcome::Ready(1, 1, vec![0; 4]),
            },
            &h.ctx
        )
        .is_none());
    assert_eq!(h.app.agents.previews.len(), LIVE_RESIDENTS);
    assert_eq!(h.app.agents.previews[&node].frame, 4);
    assert!(
        h.app.agent_is_running(node),
        "display capacity never stops generation"
    );
}

#[test]
fn malformed_and_oversized_ready_dimensions_never_allocate_a_texture() {
    let (mut h, node, _) = generator_with_note("live-preview-dimension-cap", "Draw");
    h.app.agents.requests.insert(node, "run".into());
    h.app
        .agents
        .awaiting
        .insert(node, AgentAwait::Responding { req_at: 0 });
    let identity = Identity {
        tab: h.app.tab().id,
        node,
        session: h.app.agents.bindings[&node].clone(),
        request: "run".into(),
    };
    let allocated = h.ctx.tex_manager().read().num_allocated();
    for (slot, (w, hgt)) in [(0, 1), (1, 0), (1025, 1), (1, 1025), (1, 1)]
        .into_iter()
        .enumerate()
    {
        h.app.agents.live_decodes.pending.insert(
            slot as u32,
            Ticket {
                identity: identity.clone(),
                frame: 3,
                step: 3,
                steps: 5,
            },
        );
        assert!(h
            .app
            .consume_generation_preview(
                PreviewResult {
                    id: slot as u32,
                    outcome: PreviewOutcome::Ready(w, hgt, Vec::new()),
                },
                &h.ctx
            )
            .is_none());
    }
    assert!(h.app.agents.previews.is_empty());
    assert_eq!(h.ctx.tex_manager().read().num_allocated(), allocated);
}

#[test]
fn older_live_reply_never_overwrites_a_newer_texture() {
    let (mut h, node, _) = generator_with_note("live-preview-out-of-order", "Draw");
    let session = h.app.agents.bindings[&node].clone();
    let tab = h.app.tab().id;
    h.app.agents.requests.insert(node, "run".into());
    h.app
        .agents
        .awaiting
        .insert(node, AgentAwait::Responding { req_at: 0 });
    let texture = h.ctx.load_texture(
        "newer",
        egui::ColorImage::new([1, 1], Color32::WHITE),
        egui::TextureOptions::LINEAR,
    );
    let texture_id = texture.id();
    h.app.agents.previews.insert(
        node,
        LivePreview {
            tab,
            session: session.clone(),
            request: "run".into(),
            frame: 4,
            step: 4,
            steps: 5,
            texture,
        },
    );
    h.app.agents.live_decodes.pending.insert(
        8,
        Ticket {
            identity: Identity {
                tab,
                node,
                session,
                request: "run".into(),
            },
            frame: 3,
            step: 3,
            steps: 5,
        },
    );
    assert!(h
        .app
        .consume_generation_preview(
            PreviewResult {
                id: 8,
                outcome: PreviewOutcome::Ready(1, 1, vec![0; 4]),
            },
            &h.ctx
        )
        .is_none());
    assert_eq!(h.app.agents.previews[&node].frame, 4);
    assert_eq!(h.app.agents.previews[&node].texture.id(), texture_id);
}
