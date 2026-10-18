//! Small current-revision render catalog from the production Slate application.
use super::support::{Fixture, Scenario};
use eframe::egui::Id;
use serde_json::json;

#[test]
#[ignore = "retains actual full-app compact chat renders; native interaction is separate"]
fn compact_chat_firstuse_retains_actual_ui_states() {
    let mut f = Fixture::for_scenario(Scenario::Harness);
    // Display the Codex controls without starting a provider or granting access.
    f.h.app.presence.agents[0].profile.provider = "codex".into();
    let who = f.who.clone();
    f.settle();
    let out = f.click(Id::new(("presence-chat", who.as_str())));
    f.save(
        "chat-firstuse-compact",
        out,
        json!({"fixture":"production Slate widgets, synthetic conversation, no provider task"}),
    );
    let out = f.click(Id::new(("conversation-more", who.as_str())));
    f.save(
        "chat-firstuse-settings",
        out,
        json!({"fixture":"production Slate More menu"}),
    );
    let out = f.settle();
    let out = super::management::click_text(&mut f, &out, "Agent settings");
    f.save("chat-firstuse-agent-settings", out, json!({"fixture":"production Slate identity and permissions; no permission toggle is activated"}));
    f.h.app.presence.chrome.open_chat(&who);
    // The running-state fixture is a declared local observation, never a fake vendor run.
    f.h.app.presence.agents[0].profile.provider = "local".into();
    f.declare_task(&who, "Organize the reference captions", false);
    let mut session = f.synthetic_session(
        &who,
        atlas_agent::AgentStatus::Thinking,
        "Synthetic ongoing task; no provider launched.",
    );
    session.record_activity(atlas_agent::AgentActivity {
        id: "review-public-update".into(),
        request: session.request.clone(),
        kind: atlas_agent::AgentActivityKind::Commentary,
        text: "Reading the reference captions before arranging them.".into(),
        at: 1,
        semantics: None,
    });
    f.observe(&who, &session);
    f.h.app
        .presence_save_draft(&who, "Please keep each caption with its image.")
        .unwrap();
    let out = f.settle();
    f.save("chat-firstuse-working", out, json!({"fixture":"synthetic public progress in production Slate chat; editable follow-up composer"}));
    f.resize_viewport(760, 720);
    let out = f.settle();
    f.save(
        "chat-firstuse-working-narrow",
        out,
        json!({"fixture":"production Slate chat at 760 by 720"}),
    );
    f.h.app.presence.chrome.close_chat();
    f.settle();
    let out = f.click(Id::new("presence-add"));
    f.save(
        "chat-firstuse-harness-picker",
        out,
        json!({"fixture":"production Slate harness picker"}),
    );
}
