//! Small current-revision render catalog from the production Slate application.
use super::support::Fixture;
use eframe::egui::Id;
use serde_json::json;

#[test]
#[ignore = "retains actual full-app compact chat renders; native interaction is separate"]
fn compact_chat_firstuse_retains_actual_ui_states() {
    let mut f = Fixture::new();
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
    f.h.app.presence.chrome.close_chat();
    f.settle();
    let out = f.click(Id::new("presence-add"));
    f.save(
        "chat-firstuse-harness-picker",
        out,
        json!({"fixture":"production Slate harness picker"}),
    );
}
