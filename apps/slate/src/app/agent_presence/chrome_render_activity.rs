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
    let area = Rect::from_min_size(center + egui::vec2(-24.0, 10.0), egui::vec2(48.0, 45.0));
    let first = f.step(|_| {});
    let before = Fixture::mesh_vertices(&first, area, false);
    assert!(!before.is_empty());
    f.save("trace-book-page-a",first,json!({"evidence":"public Reading event, actual marble-size animated prop","area":super::support::rect_json(area)}));
    if !chrome::system_reduced_motion(&f.h.ctx) {
        let mut changed = None;
        for _ in 0..60 {
            let out = f.step(|_| {});
            if Fixture::mesh_vertices(&out, area, false) != before {
                changed = Some(out);
                break;
            }
        }
        f.save(
            "trace-book-page-b",
            changed.expect("a page visibly changes in actual app output"),
            json!({"evidence":"same observed Reading state; actual page geometry changed"}),
        );
    } else {
        f.note_omission("Normal book animation is disabled by the OS; no normal-motion claim.");
    }
    f.h.app.settings.presence_reduced_motion = true;
    let still = f.settle();
    let frozen = Fixture::mesh_vertices(&still, area, false);
    f.save(
        "trace-book-reduced-a",
        still,
        json!({"evidence":"app reduced-motion preference; public Reading event retained"}),
    );
    let mut out = f.step(|_| {});
    for _ in 0..30 {
        out = f.step(|_| {});
    }
    assert_eq!(
        Fixture::mesh_vertices(&out, area, false),
        frozen,
        "the actual reduced-motion book remains stable"
    );
    f.save(
        "trace-book-reduced-b",
        out,
        json!({"evidence":"same reduced prop after 31 existing 60Hz input frames"}),
    );
    f.h.app.settings.presence_reduced_motion = false;
}
