//! Opt-in screenshots from the real Slate app, never a component mock or provider run.
#[path = "chrome_render_support.rs"]
mod support;
use eframe::egui::Id;
use serde_json::json;
use support::{has, rect_json, Fixture};

#[test]
#[ignore = "retains full-app software screenshots; native GPU and human review remain separate"]
fn full_app_presence_chrome_retains_actual_ui_states() {
    let mut f = Fixture::new();
    let who = f.who.clone();
    let out = f.settle();
    assert!(!f.h.app.presence.chrome.is_open());
    let own_icon = f.target(Id::new(("presence-chat", "self")));
    let own_name = f.target(Id::new(("presence-profile", "self")));
    let agent_icon = f.target(Id::new(("presence-chat", who.as_str())));
    let plus = f.target(Id::new("presence-add"));
    assert!(own_icon.right() <= own_name.left() && own_name.right() <= agent_icon.left());
    assert!(agent_icon.right() < plus.left());
    for rect in [own_name, agent_icon, plus] {
        assert!((rect.center().y - own_icon.center().y).abs() < 2.0);
    }
    let row = f.enclosure(
        &out,
        own_icon.union(own_name).union(agent_icon).union(plus),
        4.0,
    );
    f.save(
        "closed-roster",
        out,
        json!({"unifiedCapsule":rect_json(row),
        "selfIcon":rect_json(own_icon),"selfName":rect_json(own_name),
        "agentIcon":rect_json(agent_icon),"add":rect_json(plus)}),
    );

    let out = f.click(Id::new(("presence-profile", "self")));
    has(&out, "YOUR PROFILE");
    has(&out, "Add profile picture");
    let name_edit = f.target(Id::new(("presence-name", "self")));
    let profile = f.enclosure(&out, name_edit, 40.0);
    assert!(
        (profile.right() - own_name.right()).abs() < 2.0,
        "profile must align to its own segment: {profile:?} name {own_name:?}"
    );
    assert!(profile.top() > own_name.bottom() && profile.left() < own_name.left());
    f.save(
        "own-profile",
        out,
        json!({"profile":rect_json(profile),"nameEdit":rect_json(name_edit)}),
    );

    let out = f.click(Id::new(("presence-chat", "self")));
    f.chat(&out, "self");
    has(&out, "Personal notes");
    let note = "Remember this board idea for tomorrow.";
    f.type_into(Id::new(("presence-composer", "self")), note);
    assert_eq!(f.h.app.presence_conversation("self").unwrap().draft, note);
    let out = f.click(Id::new(("presence-send", "self")));
    let thread = f.h.app.presence_conversation("self").unwrap();
    assert_eq!(thread.turns.len(), 1);
    assert_eq!(thread.turns[0].text, note);
    assert!(thread.draft.is_empty());
    has(&out, note);
    let panel = f.chat(&out, "self");
    f.save(
        "self-chat",
        out,
        json!({"enclosingPanel":rect_json(panel),"delivery":"local self-send"}),
    );

    f.reload_histories();
    f.click(Id::new(("presence-chat", "self")));
    assert!(!f.h.app.presence.chrome.is_open());
    let out = f.click(Id::new(("presence-chat", "self")));
    has(&out, note);
    let panel = f.chat(&out, "self");
    f.save(
        "self-chat-reopened",
        out,
        json!({"enclosingPanel":rect_json(panel),"history":"reloaded from temporary disk store"}),
    );

    let out = f.click(Id::new(("presence-chat", who.as_str())));
    has(&out, "How is the interface feeling?");
    has(&out, "Ready to help organize this board.");
    f.chat(&out, &who);
    let draft = "Can you organize these images?";
    let out = f.type_into(Id::new(("presence-composer", who.as_str())), draft);
    assert_eq!(f.h.app.presence_conversation(&who).unwrap().draft, draft);
    assert_eq!(f.h.app.presence_conversation(&who).unwrap().turns.len(), 2);
    let panel = f.chat(&out, &who);
    f.save("agent-chat", out, json!({"enclosingPanel":rect_json(panel),"history":"synthetic durable fixture turns","agentSend":false}));

    f.click(Id::new(("hat-toggle", who.as_str())));
    let out = f.hats_ready();
    let album_id = Id::new(("conversation-hats", who.as_str())).with("album-hit");
    let album = f.target(album_id);
    let before_hat = f.hat();
    let panel = f.chat(&out, &who);
    f.save(
        "hats-before",
        out,
        json!({"enclosingPanel":rect_json(panel),"sharedAlbum":rect_json(album),"hat":before_hat}),
    );
    let out = f.wheel(album_id);
    let after_hat = f.hat();
    assert_ne!(
        after_hat, before_hat,
        "real album wheel must change focused fixture hat"
    );
    let panel = f.chat(&out, &who);
    f.save(
        "hats-after-wheel",
        out,
        json!({"enclosingPanel":rect_json(panel),"sharedAlbum":rect_json(f.target(album_id)),
        "previousHat":before_hat,"hat":after_hat,"input":"MouseWheel Line (0,-3), no hat click"}),
    );

    f.click(Id::new(("hat-toggle", who.as_str())));
    let out = f.click(Id::new(("conversation-more", who.as_str())));
    for label in [
        "Full conversation",
        "Clone agent",
        "Access settings",
        "Schedule…",
        "Retire agent",
    ] {
        has(&out, label);
    }
    let panel = f.chat(&out, &who);
    f.save(
        "more-menu",
        out,
        json!({"enclosingPanel":rect_json(panel),"actionsActivated":false}),
    );

    f.click(Id::new(("conversation-more", who.as_str())));
    let out = f.click(Id::new(("provider-toggle", who.as_str())));
    for label in ["Codex", "Cursor", "Dot / local", "Model", "Default"] {
        has(&out, label);
    }
    let panel = f.chat(&out, &who);
    f.save(
        "harness-menu",
        out,
        json!({"enclosingPanel":rect_json(panel),"providerChanged":false,"taskSubmitted":false}),
    );
    f.finish();
}
