//! Test-owned layout stress through the same full app and real roster input.
use super::super::*;
use super::{
    management,
    support::{has, rect_json, Fixture},
};
use crate::app::tests::painted_block;
use eframe::egui::{self, Id};
use serde_json::json;

pub(super) fn run() {
    let mut f = Fixture::new();
    let who = f.who.clone();
    f.h.app
        .presence_update_identity("Alexandria Sterling — Designer", None)
        .unwrap();
    f.h.app
        .apply_presence_action(
            &f.h.ctx,
            chrome::PresenceAction::Rename {
                id: who.clone(),
                name: "Amelia Research Coordinator".into(),
            },
        )
        .unwrap();
    let mut last = String::new();
    for n in 1..12 {
        last = atlas_agent::request_id();
        f.h.app
            .join_presence(
                StoredAgent {
                    id: last.clone(),
                    name: format!("Collaborator {n:02} · Research"),
                    provider: "local".into(),
                    hat: ["cap", "beanie", "wizard"][n % 3].into(),
                    retired: false,
                },
                None,
            )
            .unwrap();
    }
    assert_eq!(f.h.app.presence.agents.len(), 12);
    let thread = f.h.app.presence.conversations.edit(&who).unwrap();
    // Extend, rather than replace, the original two synthetic fixture turns.
    for n in 0..12 {
        let text = format!(
            "Synthetic layout message {n:02}. {}",
            "This paragraph makes the transcript scroll while its composer stays reachable. "
                .repeat(3)
        );
        thread.push(
            if n % 2 == 0 { "user" } else { "assistant" },
            &text,
            if n % 2 == 0 { "You" } else { "Amelia" },
            None,
            "layout-fixture",
            "complete",
        );
    }
    f.settle();
    f.click(Id::new(("presence-chat", who.as_str())));
    let out = f.type_into(
        Id::new(("presence-composer", who.as_str())),
        "Keep this unsent layout draft.",
    );
    let expected = json!(f.h.app.presence_conversation(&who));
    let profiles = json!(f.h.app.settings.collaborators);
    let panel = f.chat(&out, &who);
    let first_y = painted_block(&out, "How is the interface feeling?", 1.0)
        .unwrap()
        .origin
        .y;
    assert!(
        first_y < panel.top(),
        "fixture really overflows its transcript"
    );
    let composer = f.complete_target(Id::new(("presence-composer", who.as_str())));
    let header = f.complete_target(Id::new(("conversation-profile", who.as_str())));
    f.save(
        "stress-wide-chat",
        out,
        json!({"viewport":rect_json(f.viewport()),
        "participants":13,"syntheticHistory":true,"enclosingPanel":rect_json(panel),
        "header":rect_json(header),"composer":rect_json(composer),"providerSends":0}),
    );

    f.resize_viewport(800, 720);
    let out = f.settle();
    let panel = f.chat(&out, &who);
    let header = f.complete_target(Id::new(("conversation-profile", who.as_str())));
    let composer = f.complete_target(Id::new(("presence-composer", who.as_str())));
    let first_y = painted_block(&out, "How is the interface feeling?", 1.0)
        .unwrap()
        .origin
        .y;
    let scroll_at = egui::pos2(panel.center().x, (header.bottom() + composer.top()) * 0.5);
    f.step(|input| {
        input.events.push(egui::Event::PointerMoved(scroll_at));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 1200.0),
            modifiers: egui::Modifiers::NONE,
        });
    });
    let out = f.settle();
    let after_y = painted_block(&out, "How is the interface feeling?", 1.0)
        .unwrap()
        .origin
        .y;
    assert!(
        after_y - first_y > 10.0,
        "actual transcript geometry responds to wheel input"
    );
    assert_eq!(
        f.complete_target(Id::new(("conversation-profile", who.as_str()))),
        header
    );
    assert_eq!(
        f.complete_target(Id::new(("presence-composer", who.as_str()))),
        composer
    );
    let panel = f.chat(&out, &who);
    assert!((panel.width() - 480.0).abs() < 2.0);
    f.save("stress-narrow-chat", out, json!({"viewport":rect_json(f.viewport()),
        "enclosingPanel":rect_json(panel),"header":rect_json(header),"composer":rect_json(composer),
        "transcriptDisplacementY":after_y-first_y,"input":"real transcript wheel","providerSends":0}));

    let plus_before = f.complete_target(Id::new("presence-add"));
    let first_icon = f.complete_target(Id::new(("presence-chat", "self")));
    f.step(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(first_icon.center()));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -620.0),
            modifiers: egui::Modifiers::NONE,
        });
    });
    f.settle();
    let icon = f.complete_target(Id::new(("presence-chat", last.as_str())));
    let caret_id = Id::new(("presence-profile", last.as_str()));
    let caret = f.target(caret_id);
    let plus = f.complete_target(Id::new("presence-add"));
    assert_eq!(
        plus, plus_before,
        "Add remains pinned while the real roster scrolls"
    );
    assert!(icon.right() <= caret.left() && caret.right() < plus.left());
    let response = f.h.ctx.read_response(caret_id).unwrap();
    let hit =
        f.h.ctx
            .layer_transform_to_global(response.layer_id)
            .unwrap_or_default()
            * response.interact_rect;
    assert!(hit.is_positive() && hit.contains(caret.center()));
    let out = f.click(caret_id);
    has(&out, "PROFILE");
    let name = f.complete_target(Id::new(("presence-name", last.as_str())));
    let profile = f.enclosure(&out, name, 40.0);
    assert!((profile.right() - caret.right()).abs() < 2.0);
    assert!(profile.top() > caret.bottom() && profile.left() < caret.left());
    assert_eq!(
        f.h.app.presence.agents.last().unwrap().profile.name,
        "Collaborator 11 · Research"
    );
    f.save("stress-scrolled-profile", out, json!({"viewport":rect_json(f.viewport()),
        "profile":rect_json(profile),"name":rect_json(name),"participant":last,
        "rosterIcon":rect_json(icon),"rosterProfile":rect_json(caret),"rosterProfileHit":rect_json(hit),
        "entireProfileTargetInteractive":hit.contains_rect(caret.shrink(0.01)),"add":rect_json(plus)}));

    let out = f.click(Id::new("presence-add"));
    let title = management::required(&out, "Add a collaborator");
    let submit = management::required(&out, "Add collaborator");
    let panel = f.enclosure(&out, title.union(submit), 0.0);
    assert!(f.viewport().contains_rect(panel));
    assert_eq!(f.complete_target(Id::new("presence-add")), plus_before);
    assert_eq!(json!(f.h.app.presence_conversation(&who)), expected);
    assert_eq!(json!(f.h.app.settings.collaborators), profiles);
    f.save(
        "stress-pinned-add",
        out,
        json!({"viewport":rect_json(f.viewport()),
        "enclosingPanel":rect_json(panel),"add":rect_json(plus_before),
        "submitText":rect_json(submit),"submitted":false,"draftAndHistoryPreserved":true}),
    );
    f.finish_management();
}
