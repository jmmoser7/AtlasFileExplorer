//! Synthetic public observations through the real session poller and app paint.
use super::super::*;
use super::support::{Fixture, Scenario};
use atlas_agent::{
    ActivityTarget, AgentActivity, AgentActivityAction as Action, AgentActivityKind,
    AgentActivitySemantics,
};
use eframe::egui::{self, Rect};
use serde_json::json;

pub(super) fn run() {
    let mut f = Fixture::for_scenario(Scenario::Activity);
    let who = f.who.clone();
    listening(&mut f, &who);
    let prepared=f.h.app.prepare_targeted_command("board.objects.create_text",&[],
        &json!({"x":160,"y":60,"width":240,"height":150,"text":"Test-owned brief\nObserved activity uses only public events and explicit target IDs."})).unwrap();
    let target = prepared.targets[0];
    f.h.app.commit_scene(prepared.cmds);
    let request = f.declare_task(&who, "Synthetic public progress fixture", true);
    for (index, (action, expected)) in [
        (Action::Reading, "reading"),
        (Action::Searching, "searching"),
        (Action::Writing, "writing"),
        (Action::CreatingImage, "creating-image"),
        (Action::ToolWaiting, "tool-waiting"),
        (Action::Question, "question"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut session = f.synthetic_session(
            &who,
            atlas_agent::AgentStatus::Thinking,
            "Synthetic progress, never a provider result.",
        );
        let mut semantics = AgentActivitySemantics::new(action);
        semantics.target = ActivityTarget::new(target.0.to_string());
        session.record_activity(AgentActivity {
            id: format!("public-event-{index}"),
            request: request.clone(),
            kind: AgentActivityKind::ToolStarted,
            text: format!("Explicit synthetic {expected} observation"),
            at: index as u64 + 1,
            semantics: Some(semantics),
        });
        f.observe(&who, &session);
        let out = f.settle();
        assert_eq!(f.h.app.presence.agents[0].activity, expected);
        let center = f.board_avatar_center(&who);
        let head = super::world_opaque_head(&out, center, 1.0);
        f.save(&format!("trace-{expected}"),out,json!({"evidence":"synthetic public event -> actual poller/reducer -> actual app paint",
            "event":session.activity,"expectedActivity":expected,"worldHead":super::support::rect_json(head),"providerResult":false}));
        if action == Action::Reading {
            book(&mut f, &who);
        }
    }
    // Real scoped board read admission; the declaration permits only this
    // request's read job, not a write command or a provider send.
    let mut a = f.h.app.presence.agents.remove(0);
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: 0,
        operation: BoardOperation::Read {
            node_ids: vec![target.0.to_string()],
            offset: 0,
            limit: 1,
        },
    };
    assert!(f.h.app.presence_call(&mut a, &call).is_none());
    assert!(a.read.is_some());
    f.h.app.presence.agents.insert(0, a);
    let out = f.step(|_| {});
    f.save(
        "trace-scoped-board-read",
        out,
        json!({"evidence":"actual scoped read command, synthetic request identity",
        "call":call,"readStillPending":f.h.app.presence.agents[0].read.is_some()}),
    );
    f.permit_terminal(&who);
    let failed = f.synthetic_session(
        &who,
        atlas_agent::AgentStatus::Error("Synthetic tool failure for visual inspection".into()),
        "",
    );
    f.observe(&who, &failed);
    let out = f.settle();
    assert_eq!(f.h.app.presence.agents[0].activity, "error");
    f.save("trace-error",out,json!({"evidence":"synthetic public error -> actual stop/status/face path","providerFailure":false}));
    let out = f.address_at_zoom(1.0, egui::vec2(680.0, 300.0));
    assert_eq!(f.h.app.presence.agents[0].activity, "listening");
    f.save(
        "trace-addressed-listening",
        out,
        json!({"evidence":"production presence_address helper path; input gesture is not covered by this state","paused":true}),
    );
    f.finish_scenario();
}

fn book(f: &mut Fixture, who: &str) {
    let center = f.board_avatar_center(who);
    let area = Rect::from_min_max(
        center + egui::vec2(5.0, 8.0),
        center + egui::vec2(30.0, 29.0),
    );
    // Observe the approved moving-page and stationary-paper fills in the actual
    // output. This constructs neither artwork nor an animation phase. Exact
    // colors distinguish the page from eye, hat, shell and name geometry.
    let bounds = |out: &egui::FullOutput, color: [u8; 4]| {
        let mut result = Rect::NOTHING;
        let mut count = 0;
        for vertex in Fixture::mesh_vertices_at_least_alpha(out, area, false, 255)
            .into_iter()
            .filter(|vertex| vertex[2] == u32::from_le_bytes(color))
        {
            result.extend_with(egui::pos2(
                f32::from_bits(vertex[0]),
                f32::from_bits(vertex[1]),
            ));
            count += 1;
        }
        assert!(count >= 3, "observe the actual opaque book fill: {color:?}");
        result
    };
    let page_color = [229, 237, 216, 255];
    let paper_color = [255, 254, 247, 255];
    let first = f.step(|_| {});
    let page = bounds(&first, page_color);
    let paper = bounds(&first, paper_color);
    f.save("trace-book-page-a", first, json!({
        "evidence":"public Reading event, actual marble-size book with observed page and paper geometry",
        "area":super::support::rect_json(area),
        "pageBounds":super::support::rect_json(page),
        "paperBounds":super::support::rect_json(paper),
        "pageFillRgba":page_color,"paperFillRgba":paper_color,
    }));
    if !chrome::system_reduced_motion(&f.h.ctx) {
        let mut changed = None;
        for _ in 0..60 {
            let out = f.step(|_| {});
            let current = bounds(&out, page_color);
            assert_eq!(bounds(&out, paper_color), paper, "book paper stays fixed");
            assert_eq!(
                current.y_range(),
                page.y_range(),
                "page motion remains horizontal"
            );
            let delta = (current.center().x - page.center().x).abs();
            if delta >= 2.0 {
                changed = Some((out, current, delta));
                break;
            }
        }
        let (out, current, delta) =
            changed.expect("an actual book page must move materially while its paper stays fixed");
        f.save("trace-book-page-b", out, json!({
            "evidence":"same observed Reading state; material page travel with stationary book paper",
            "pageBounds":super::support::rect_json(current),
            "paperBounds":super::support::rect_json(paper),
            "pageCenterTravelLogicalPx":delta,"requiredPageCenterTravelLogicalPx":2.0,
        }));
    } else {
        f.note_omission("Normal book animation is disabled by the OS; no normal-motion claim.");
    }
    f.h.app.settings.presence_reduced_motion = true;
    let still = f.settle();
    let frozen = Fixture::mesh_vertices(&still, area, false);
    let frozen_page = bounds(&still, page_color);
    let frozen_paper = bounds(&still, paper_color);
    f.save("trace-book-reduced-a", still, json!({
        "evidence":"app reduced-motion preference; public Reading event and actual book retained",
        "pageBounds":super::support::rect_json(frozen_page),
        "paperBounds":super::support::rect_json(frozen_paper),
    }));
    let mut out = f.step(|_| {});
    for _ in 0..30 {
        out = f.step(|_| {});
    }
    assert_eq!(
        Fixture::mesh_vertices(&out, area, false),
        frozen,
        "the actual reduced-motion book remains stable"
    );
    assert_eq!(bounds(&out, page_color), frozen_page);
    assert_eq!(bounds(&out, paper_color), frozen_paper);
    f.save(
        "trace-book-reduced-b",
        out,
        json!({
            "evidence":"same reduced page and paper geometry after 31 existing 60 Hz input frames",
            "pageBounds":super::support::rect_json(frozen_page),
            "paperBounds":super::support::rect_json(frozen_paper),
        }),
    );
    f.h.app.settings.presence_reduced_motion = false;
}

fn listening(f: &mut Fixture, who: &str) {
    f.h.app.settings.presence_reduced_motion = true;
    f.address_at_zoom(1.0, egui::vec2(680.0, 180.0));
    f.h.app
        .apply_presence_action(
            &f.h.ctx,
            chrome::PresenceAction::Pause {
                id: who.into(),
                paused: false,
            },
        )
        .unwrap();
    let idle = f.settle();
    assert_eq!(
        f.h.app
            .presence
            .avatar_activity(&f.h.app.presence.agents[0]),
        "idle"
    );
    let center = f.board_avatar_center(who);
    // The right edge lies beyond the 18-world-point shell and above the chat.
    // This region cannot pass merely because an attached chat was opened.
    let edge = Rect::from_min_max(
        center + egui::vec2(20.0, 8.0),
        center + egui::vec2(29.0, 25.0),
    );
    let pulse = Rect::from_min_max(
        center + egui::vec2(8.0, 8.0),
        center + egui::vec2(28.0, 25.0),
    );
    let eyes = Rect::from_center_size(center, egui::vec2(30.0, 18.0));
    assert!(Fixture::mesh_vertices(&idle, edge, false).is_empty());
    let idle_eyes = Fixture::mesh_vertices(&idle, eyes, true);
    assert!(!idle_eyes.is_empty());
    f.save("trace-listening-idle-baseline", idle, json!({
        "evidence":"actual idle after canonical Resume, retained at the same anchor as listening",
        "propEdge":super::support::rect_json(edge), "gestureEvidence":false,
    }));
    f.h.app.presence_address(&f.h.ctx, who).unwrap();
    let still = f.settle();
    assert!(f.h.app.presence.agents[0].paused);
    assert_eq!(
        f.h.app
            .presence
            .avatar_activity(&f.h.app.presence.agents[0]),
        "listening"
    );
    assert!(
        !Fixture::mesh_vertices(&still, edge, false).is_empty(),
        "listening needs its own visible prop beyond the idle shell"
    );
    assert_ne!(
        Fixture::mesh_vertices(&still, eyes, true),
        idle_eyes,
        "the curious human-facing pose must differ from idle"
    );
    let frozen = Fixture::mesh_vertices(&still, pulse, false);
    f.save("trace-listening-reduced-a", still, json!({
        "evidence":"actual local address -> paused listening -> app eye/gaze and attentive prop",
        "reducedMotion":true, "providerStateAnnouncements":0,
        "propArea":super::support::rect_json(pulse), "gestureEvidence":false,
    }));
    let mut out = f.step(|_| {});
    for _ in 0..30 {
        out = f.step(|_| {});
    }
    assert_eq!(Fixture::mesh_vertices(&out, pulse, false), frozen);
    f.save("trace-listening-reduced-b", out, json!({
        "evidence":"same attentive prop after 31 existing 60Hz frames; reduced motion remains static",
    }));
    f.h.app.settings.presence_reduced_motion = false;
    if !chrome::system_reduced_motion(&f.h.ctx) {
        let out = f.settle();
        let before = Fixture::mesh_vertices_at_least_alpha(&out, pulse, false, 0);
        let fixed_eyes = Fixture::mesh_vertices(&out, eyes, true);
        f.save(
            "trace-listening-pulse-a",
            out,
            json!({
                "evidence":"same actual listening state, normal existing attentive pulse",
            }),
        );
        let mut changed = None;
        for _ in 0..60 {
            let out = f.step(|_| {});
            assert_eq!(Fixture::mesh_vertices(&out, eyes, true), fixed_eyes);
            let current = Fixture::mesh_vertices_at_least_alpha(&out, pulse, false, 0);
            assert_eq!(
                current.len(),
                before.len(),
                "opacity-only pulse keeps every vertex"
            );
            let delta = before
                .iter()
                .zip(&current)
                .map(|(a, b)| {
                    assert_eq!(
                        &a[..2],
                        &b[..2],
                        "pulse comparison must keep fixed geometry"
                    );
                    ((a[2] >> 24) as u8).abs_diff((b[2] >> 24) as u8)
                })
                .max()
                .unwrap();
            if delta >= 64 {
                changed = Some((out, delta));
                break;
            }
        }
        let (out, delta) = changed
            .expect("the attentive dot reaches a materially different phase while eyes stay fixed");
        f.save(
            "trace-listening-pulse-b",
            out,
            json!({
                "evidence":"actual marble-size prop changes materially with fixed eye/gaze and prop geometry",
                "maxVertexAlphaDelta":delta, "requiredVertexAlphaDelta":64,
            }),
        );
    } else {
        f.note_omission(
            "OS reduced motion disables normal listening pulse; no normal-motion claim.",
        );
    }
    f.h.app
        .apply_presence_action(
            &f.h.ctx,
            chrome::PresenceAction::Pause {
                id: who.into(),
                paused: false,
            },
        )
        .unwrap();
    f.h.app.presence.chrome.close_chat();
}
