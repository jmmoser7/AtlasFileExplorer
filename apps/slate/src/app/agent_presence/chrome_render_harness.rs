//! Real busy-choice widgets; requests/completions are declared local fixtures.
use super::super::*;
use super::management::click_text;
use super::support::{has, Fixture, Scenario};
use crate::app::tests::painted_strings;
use eframe::egui::{self, Id};
use serde_json::json;

pub(super) fn run() {
    timing_and_completion();
    stop_paths();
}

fn start(f: &mut Fixture, who: &str, prompt: &str) {
    f.declare_task(who, prompt, false);
    let session = f.synthetic_session(
        who,
        atlas_agent::AgentStatus::Thinking,
        "Synthetic ongoing task; no provider has been launched.",
    );
    f.observe(who, &session);
    f.settle();
    assert!(f.h.app.presence.task_busy(who));
}
fn choose(f: &mut Fixture, label: &str) -> egui::FullOutput {
    let out = f.settle();
    let out = if painted_strings(&out).iter().any(|text| text == label) {
        out
    } else {
        let who = f.who.clone();
        f.click(Id::new(("provider-toggle", who.as_str())))
    };
    click_text(f, &out, label)
}
fn history(f: &Fixture) -> Vec<(String, String)> {
    f.h.app
        .presence_conversation(&f.who)
        .unwrap()
        .turns
        .iter()
        .map(|t| (t.role.clone(), t.text.clone()))
        .collect()
}
fn stable(
    f: &Fixture,
    who: &str,
    motion: &PresenceId,
    position: WorldPoint,
    original: &[(String, String)],
    scene: &slate_doc::Scene,
    depth: usize,
) {
    let a =
        f.h.app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap();
    assert_eq!(a.motion, *motion);
    assert_eq!(a.profile.name, "Dot");
    assert_eq!(a.profile.hat, "cap");
    assert_eq!(
        f.h.app
            .presence
            .roster
            .session(motion)
            .unwrap()
            .sample()
            .position,
        position
    );
    assert!(history(f).starts_with(original));
    assert_eq!(
        f.h.app.presence_conversation(who).unwrap().draft,
        "Unsent handoff draft"
    );
    assert_eq!(&f.h.app.doc().scene, scene);
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
    assert!(a.runtime.is_none() && a.cursor.is_none());
}

fn timing_and_completion() {
    let mut f = Fixture::for_scenario(Scenario::Harness);
    let who = f.who.clone();
    f.declare_harnesses(&who);
    f.settle();
    f.click(Id::new(("presence-chat", who.as_str())));
    f.type_into(
        Id::new(("presence-composer", who.as_str())),
        "Unsent handoff draft",
    );
    start(&mut f, &who, "Synthetic busy handoff fixture");
    let a = &f.h.app.presence.agents[0];
    let motion = a.motion.clone();
    let old_binding = a.binding.clone();
    let position =
        f.h.app
            .presence
            .roster
            .session(&motion)
            .unwrap()
            .sample()
            .position;
    let original = history(&f);
    let scene = f.h.app.doc().scene.clone();
    let depth = f.h.app.tab().journal.undo_depth();
    let out = choose(&mut f, "Codex");
    for label in ["After this task", "Stop task and switch", "Cancel"] {
        has(&out, label);
    }
    f.save("harness-busy-timing",out,json!({"evidence":"actual shared provider/timing controls","syntheticRequest":true,"providerSend":false}));
    let out = f.settle();
    click_text(&mut f, &out, "Cancel");
    assert!(f.h.app.presence.agents[0].pending_harness.is_none());
    assert_eq!(f.h.app.presence.agents[0].binding, old_binding);
    let out = f.settle();
    f.save(
        "harness-timing-cancelled",
        out,
        json!({"evidence":"actual Cancel button; task and draft unchanged"}),
    );
    let out = choose(&mut f, "Codex");
    click_text(&mut f, &out, "After this task");
    assert_eq!(
        f.h.app.presence.agents[0]
            .pending_harness
            .as_ref()
            .unwrap()
            .0,
        "codex"
    );
    let out = f.click(Id::new(("provider-toggle", who.as_str())));
    has(&out, "Switching to codex after this task");
    f.save(
        "harness-after-task-pending",
        out,
        json!({"evidence":"actual queued provider label and Cancel switch control"}),
    );
    let out = f.settle();
    click_text(&mut f, &out, "Cancel switch");
    assert!(f.h.app.presence.agents[0].pending_harness.is_none());
    stable(&f, &who, &motion, position, &original, &scene, depth);
    let out = f.settle();
    f.save(
        "harness-pending-cancelled",
        out,
        json!({"evidence":"actual Cancel switch; task remains in original harness"}),
    );
    let out = choose(&mut f, "Codex");
    click_text(&mut f, &out, "After this task");
    f.permit_terminal(&who);
    let done = f.synthetic_session(
        &who,
        atlas_agent::AgentStatus::Idle,
        "Synthetic ongoing task; no provider has been launched.",
    );
    f.observe_handoff(&who, &done, "codex");
    assert_ne!(
        f.h.app.presence.agents[0].binding.session_id,
        old_binding.session_id
    );
    assert!(f.h.app.presence.agents[0].pending_harness.is_none());
    stable(&f, &who, &motion, position, &original, &scene, depth);
    let out = f.settle();
    has(&out, "next message");
    f.save("harness-completed-codex-idle",out,json!({"evidence":"synthetic Idle through actual poller and queued handoff owner",
        "provider":"codex","requestEmpty":true,"newProviderTaskSent":false,"historyAndDraftPreserved":true}));
    let out = choose(&mut f, "Dot / local");
    assert_eq!(f.h.app.presence.agents[0].profile.provider, "local");
    stable(&f, &who, &motion, position, &original, &scene, depth);
    f.save("harness-switched-back-local",out,json!({"evidence":"actual idle provider choice; no request resubmission","provider":"local"}));
    f.finish_scenario();
}

fn stop_paths() {
    let mut f = Fixture::for_scenario(Scenario::Harness);
    let who = f.who.clone();
    f.declare_harnesses(&who);
    f.settle();
    f.click(Id::new(("presence-chat", who.as_str())));
    f.type_into(
        Id::new(("presence-composer", who.as_str())),
        "Unsent handoff draft",
    );
    start(&mut f, &who, "Synthetic stop-and-switch fixture");
    let old = f.h.app.presence.agents[0].binding.session_id.clone();
    let out = choose(&mut f, "Cursor");
    f.permit_terminal(&who);
    let out = click_text(&mut f, &out, "Stop task and switch");
    let a = &f.h.app.presence.agents[0];
    assert_eq!(a.profile.provider, "cursor");
    assert_ne!(a.binding.session_id, old);
    assert!(a.request.is_empty() && !a.inflight && a.pending_harness.is_none());
    assert_eq!(
        f.h.app.presence_conversation(&who).unwrap().draft,
        "Unsent handoff draft"
    );
    f.save("harness-stopped-and-switched",out,json!({"evidence":"actual Stop task and switch; no replacement provider task","provider":"cursor"}));
    choose(&mut f, "Dot / local");
    start(&mut f, &who, "Synthetic pending task stopped by header");
    let out = choose(&mut f, "Codex");
    click_text(&mut f, &out, "After this task");
    f.permit_terminal(&who);
    let out = f.click(Id::new((who.as_str(), "Stop task")));
    let a = &f.h.app.presence.agents[0];
    assert_eq!(a.profile.provider, "local");
    assert!(a.request.is_empty() && !a.inflight && a.pending_harness.is_none());
    assert_eq!(
        f.h.app.presence_conversation(&who).unwrap().draft,
        "Unsent handoff draft"
    );
    f.save(
        "harness-header-stop-cancels-pending",
        out,
        json!({"evidence":"actual header Stop task; queued change cancelled","providerSend":false}),
    );
    f.finish_scenario();
}
