//! First-use and management evidence through the actual app's widgets.
use super::support::{has, rect_json, screen, Fixture};
use crate::app::tests::primary_button;
use eframe::egui::{self, Id, Pos2, Rect};
use serde_json::json;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

pub(super) fn run() {
    let mut f = Fixture::new();
    let parent = f.who.clone();
    f.settle();
    let out = f.click(Id::new("presence-add"));
    has(&out, "Add a collaborator");
    let out = f.hats_ready();
    let add = f.enclosure(&out, required(&out, "Add a collaborator"), 40.0);
    for text in [
        "Codex",
        "Cursor",
        "Dot / local",
        "Add collaborator",
        "Connect people…",
    ] {
        assert!(add.contains_rect(required_in(&out, text, add)));
    }
    has(&out, "edits begin as proposals");
    has(&out, "Save this workbook");
    f.save(
        "management-add",
        out,
        json!({"panel":rect_json(add),"joinActivated":false}),
    );
    let out = f.settle();
    click_text_in(&mut f, &out, "Dot / local", add);
    let before = ids(&f);
    let out = f.settle();
    click_text(&mut f, &out, "Add collaborator");
    let added: Vec<_> = ids(&f).difference(&before).cloned().collect();
    assert_eq!(added.len(), 1, "real Add must create exactly one identity");
    let who = added[0].clone();
    let joined =
        f.h.app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap();
    assert_eq!(joined.profile.provider, "local");
    assert!(!joined.grant.as_ref().unwrap().mutation_autonomy);
    let first_grant = joined.binding.grant_id.clone();
    let thread = f.h.app.presence.conversations.edit(&who).unwrap();
    thread.push(
        "user",
        "Keep this fixture identity when it returns.",
        "You",
        None,
        "management-fixture",
        "complete",
    );
    f.h.app.presence.conversations.save(&who);
    let out = f.click(Id::new(("presence-chat", who.as_str())));
    has(&out, "Keep this fixture identity");
    let out = f.type_into(
        Id::new(("presence-composer", who.as_str())),
        "Unsent fixture draft",
    );
    let history = serde_json::to_value(f.h.app.presence_conversation(&who).unwrap()).unwrap();
    let panel = f.chat(&out, &who);
    f.save("management-added-chat", out, json!({"participant":who,"panel":rect_json(panel),"history":"synthetic; actual unsent draft input","agentSend":false}));

    f.click(Id::new(("presence-chat", parent.as_str())));
    let out = f.click(Id::new(("conversation-more", parent.as_str())));
    let out = click_text(&mut f, &out, "Access settings");
    let access = f.enclosure(&out, required(&out, "Board access"), 80.0);
    let grant_before = grants(&f);
    for text in [
        "Autonomous board edits",
        "Read board images and wired inputs",
        "Allow chats created by this agent",
    ] {
        let _ = seek(&mut f, text, access);
    }
    let out = seek(&mut f, "Autonomous board edits", access);
    f.save(
        "management-access-top",
        out,
        json!({"panel":rect_json(access),"grantChanges":false}),
    );
    let out = seek(&mut f, "Revoke board access", access);
    f.save(
        "management-access-bottom",
        out,
        json!({"panel":rect_json(access),"grantChanges":false}),
    );
    assert_eq!(
        grants(&f),
        grant_before,
        "scrolling access never grants authority"
    );
    let out = seek(&mut f, "Add temporary helper", access);
    let before = ids(&f);
    click_text(&mut f, &out, "Add temporary helper");
    let added: Vec<_> = ids(&f).difference(&before).cloned().collect();
    assert_eq!(added.len(), 1);
    let helper = added[0].clone();
    let child =
        f.h.app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == helper)
            .unwrap();
    assert_eq!(child.parent.as_deref(), Some(parent.as_str()));
    let parent_grant =
        f.h.app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == parent)
            .unwrap()
            .grant
            .as_ref()
            .unwrap();
    assert!(parent_grant.allows_helper(child.grant.as_ref().unwrap()));
    assert!(!child.grant.as_ref().unwrap().mutation_autonomy);
    assert_eq!(child.grant.as_ref().unwrap().helper_limit, 0);
    f.click(Id::new(("presence-chat", helper.as_str())));
    let out = f.click(Id::new(("conversation-more", helper.as_str())));
    let out = click_text(&mut f, &out, "Access settings");
    let access = f.enclosure(&out, required(&out, "Board access"), 80.0);
    let out = seek(&mut f, "Temporary helper of Dot", access);
    f.save(
        "management-helper-access",
        out,
        json!({"helper":helper,"parent":parent,"taskSubmitted":false}),
    );

    f.click(Id::new(("presence-chat", who.as_str())));
    let out = f.click(Id::new(("conversation-more", who.as_str())));
    let out = click_text(&mut f, &out, "Retire agent");
    has(&out, "A little farewell");
    has(&out, "Identity and conversation stay");
    let chooser = f.enclosure(&out, required(&out, "A little farewell"), 70.0);
    for text in ["Retire agent", "Keep working"] {
        required_in(&out, text, chooser);
    }
    f.save(
        "management-retirement-chooser",
        out,
        json!({"panel":rect_json(chooser),"retirementActivated":false}),
    );
    let out = f.settle();
    click_text(&mut f, &out, "Keep working");
    assert_eq!(f.h.app.presence.chrome.active_chat(), Some(who.as_str()));
    assert_eq!(
        serde_json::to_value(f.h.app.presence_conversation(&who).unwrap()).unwrap(),
        history
    );
    let out = f.click(Id::new(("conversation-more", who.as_str())));
    let out = click_text(&mut f, &out, "Retire agent");
    click_text(&mut f, &out, "Retire agent");
    let retiring =
        f.h.app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap();
    assert!(retiring.departure.is_some() && retiring.grant.is_none() && retiring.profile.retired);
    let deadline = Instant::now()
        + Duration::from_secs_f64(atlas_shell::agent_presence::departure_duration("bow") + 2.0);
    while f.h.app.presence.agents.iter().any(|a| a.profile.id == who) {
        assert!(Instant::now() < deadline, "real retirement never completes");
        f.step(|_| {});
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(f
        .h
        .app
        .settings
        .collaborators
        .iter()
        .any(|p| p.id == who && p.retired));
    let metrics = atlas_shell::tokens::current();
    let icon = Pos2::new(
        metrics.topbar.icon_zone_width * 0.5,
        metrics.topbar.height * 0.5,
    );
    let out = click_point(&mut f, icon);
    let out = click_text(&mut f, &out, "Preferences");
    let out = click_text(&mut f, &out, "Advanced settings…");
    let advanced = egui::AreaState::load(&f.h.ctx, Id::new("Advanced"))
        .expect("actual Advanced window area")
        .rect();
    assert!(screen().contains_rect(advanced));
    assert!(advanced.contains_rect(required(&out, "Advanced")));
    let out = seek(&mut f, "Retired collaborators", advanced);
    let out = seek_from(&mut f, out, "Restore", advanced);
    has(&out, "Work and permissions start fresh");
    f.save(
        "management-restore-advanced",
        out,
        json!({"panel":rect_json(advanced),"retiredParticipant":who,"restored":false}),
    );
    let out = f.settle();
    click_text(&mut f, &out, "Restore");
    let restored =
        f.h.app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap();
    assert_ne!(restored.binding.grant_id, first_grant);
    assert!(!restored.profile.retired && restored.parent.is_none());
    let grant = restored.grant.as_ref().unwrap();
    assert!(!grant.mutation_autonomy && grant.readable_sources.is_empty());
    assert!(restored.request.is_empty() && restored.work.is_none() && restored.read.is_none());
    assert_eq!(
        serde_json::to_value(f.h.app.presence_conversation(&who).unwrap()).unwrap(),
        history
    );
    let out = f.settle();
    f.save(
        "management-restored",
        out,
        json!({"sameParticipant":who,"freshGrant":true,"historyAndDraftPreserved":true}),
    );
    f.finish_management();
    people();
}

fn people() {
    let mut f = Fixture::new();
    f.settle();
    let out = f.click(Id::new("presence-add"));
    let out = click_text(&mut f, &out, "Connect people…");
    for text in [
        "Invite a person",
        "Create an invitation",
        "Join with an invitation",
        "Connection settings",
        "Not connected",
    ] {
        required(&out, text);
    }
    assert!(f.h.app.presence.human_chat.setup.open);
    assert!(f.h.app.presence.human_chat.setup.invite.is_empty());
    assert!(f.h.app.presence.human_chat.setup.host_invite.is_empty());
    assert!(f.h.app.presence.human_chat.online.is_empty());
    let bind = f.h.app.presence.human_chat.setup.bind.clone();
    f.save(
        "management-people",
        out,
        json!({"hostActivated":false,"joinActivated":false,"transport":"not connected"}),
    );
    let out = f.settle();
    let out = click_text(&mut f, &out, "Connection settings");
    for text in ["Listen address", "Disconnect", "Not connected"] {
        required(&out, text);
    }
    assert_eq!(f.h.app.presence.human_chat.setup.status, "Not connected");
    assert_eq!(f.h.app.presence.human_chat.setup.bind, bind);
    assert!(f.h.app.presence.human_chat.setup.invite.is_empty());
    assert!(f.h.app.presence.human_chat.setup.host_invite.is_empty());
    f.save(
        "management-people-advanced",
        out,
        json!({"hostActivated":false,"joinActivated":false,"settingsChanged":false}),
    );
    f.finish_management();
}

fn ids(f: &Fixture) -> BTreeSet<String> {
    f.h.app
        .presence
        .agents
        .iter()
        .map(|a| a.profile.id.clone())
        .collect()
}
fn grants(f: &Fixture) -> serde_json::Value {
    json!(f
        .h
        .app
        .presence
        .agents
        .iter()
        .map(|a| (&a.profile.id, &a.grant))
        .collect::<Vec<_>>())
}
fn click_point(f: &mut Fixture, at: Pos2) -> egui::FullOutput {
    assert!(screen().contains(at));
    f.step(primary_button(at, true, false));
    f.step(primary_button(at, false, false));
    f.settle()
}
pub(super) fn click_text(f: &mut Fixture, out: &egui::FullOutput, text: &str) -> egui::FullOutput {
    click_point(f, required(out, text).center())
}
fn click_text_in(
    f: &mut Fixture,
    out: &egui::FullOutput,
    text: &str,
    inside: Rect,
) -> egui::FullOutput {
    click_point(f, required_in(out, text, inside).center())
}
pub(super) fn required(out: &egui::FullOutput, text: &str) -> Rect {
    required_in(out, text, screen())
}
fn required_in(out: &egui::FullOutput, text: &str, inside: Rect) -> Rect {
    label(out, text, inside).unwrap_or_else(|| panic!("missing or clipped actual control: {text}"))
}
fn label(out: &egui::FullOutput, text: &str, inside: Rect) -> Option<Rect> {
    fn walk(shape: &egui::Shape, text: &str, clip: Rect, inside: Rect, found: &mut Vec<Rect>) {
        match shape {
            egui::Shape::Vec(children) => children
                .iter()
                .for_each(|s| walk(s, text, clip, inside, found)),
            egui::Shape::Text(t) if t.galley.text() == text => {
                let rect = t.galley.rect.translate(t.pos.to_vec2());
                if clip.expand(0.5).contains_rect(rect)
                    && inside.contains_rect(rect)
                    && rect.is_positive()
                {
                    found.push(rect);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for shape in &out.shapes {
        walk(&shape.shape, text, shape.clip_rect, inside, &mut found);
    }
    assert!(
        found.len() <= 1,
        "ambiguous visible control {text}: {found:?}"
    );
    found.pop()
}
pub(super) fn seek(f: &mut Fixture, text: &str, panel: Rect) -> egui::FullOutput {
    let out = f.settle();
    seek_from(f, out, text, panel)
}
fn seek_from(
    f: &mut Fixture,
    mut out: egui::FullOutput,
    text: &str,
    panel: Rect,
) -> egui::FullOutput {
    // Try each end of the actual scrollable surface, with bounded real wheel input.
    for down in [true, false] {
        for _ in 0..24 {
            if label(&out, text, panel).is_some() {
                return out;
            }
            f.step(|input| {
                input.events.push(egui::Event::PointerMoved(panel.center()));
                input.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, if down { -3.0 } else { 3.0 }),
                    modifiers: egui::Modifiers::NONE,
                });
            });
            out = f.settle();
        }
    }
    panic!("actual scroll cannot reach {text} in {panel:?}");
}
