//! Opt-in screenshots from the real Slate app, never a component mock or provider run.
#[path = "chrome_render_activity.rs"]
mod activity;
#[path = "chrome_render_catalogue.rs"]
mod catalogue;
#[path = "chat_firstuse_render.rs"]
mod chat_firstuse;
#[path = "chrome_render_departures.rs"]
mod departures;
#[path = "chrome_render_harness.rs"]
mod harness;
#[path = "chrome_render_management.rs"]
mod management;
#[path = "chrome_render_stress.rs"]
mod stress;
#[path = "chrome_render_support.rs"]
mod support;
use eframe::egui::{self, Id, Pos2, Rect};
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
    for (phase, zoom, y) in [
        ("attached25", 0.25, 40.0),
        ("attached80", 0.8, 420.0),
        ("attached100", 1.0, 40.0),
        ("attached200", 2.0, 40.0),
    ] {
        let out = f.address_at_zoom(zoom, egui::vec2(380.0, y));
        let agent = &f.h.app.presence.agents[0];
        assert!(agent.paused);
        assert_eq!(agent.activity, "listening");
        assert_eq!(f.h.app.presence.chrome.canvas_chat(), Some(who.as_str()));
        assert_eq!(f.h.app.presence.chrome.active_chat(), Some(who.as_str()));
        for text in [
            "Listening; task suspended",
            "How is the interface feeling?",
            "Ready to help organize this board.",
        ] {
            has(&out, text);
        }
        let position =
            f.h.app
                .presence
                .roster
                .session(&agent.motion)
                .unwrap()
                .sample()
                .position;
        let center =
            f.h.app
                .board_xf()
                .w2s(egui::pos2(position.x as f32, position.y as f32));
        assert!(center.distance(f.h.app.canvas_rect.min + egui::vec2(380.0, y)) < 0.1);
        let head = world_opaque_head(&out, center, zoom);
        let opaque_world = Rect::from_min_max(
            f.h.app.board_xf().s2w(head.min),
            f.h.app.board_xf().s2w(head.max),
        );
        let aa_feather_world = 1.0 / out.pixels_per_point;
        let nominal_world = [
            head.width() / zoom + aa_feather_world,
            head.height() / zoom + aa_feather_world,
        ];
        let header = f.complete_target(Id::new(("conversation-profile", who.as_str())));
        let composer = f.complete_target(Id::new(("presence-composer", who.as_str())));
        let send = f.complete_target(Id::new(("presence-send", who.as_str())));
        let controls = header.union(composer).union(send);
        let panel = f.enclosure(&out, controls, 0.0);
        assert!(
            (panel.width() / zoom - 340.0).abs() < 1.0,
            "scaled width: {panel:?}"
        );
        assert!(
            (panel.height() / zoom - 330.0).abs() < 1.0,
            "scaled height: {panel:?}"
        );
        assert!(panel.contains_rect(composer.union(send)));
        assert!(composer.width() / zoom > 240.0 && composer.height() / zoom >= 43.0);
        assert!((send.width() / zoom - 38.0).abs() < 0.1);
        assert!((send.height() / zoom - 38.0).abs() < 0.1);
        let surface = out
            .shapes
            .iter()
            .find(|shape| is_surface(&shape.shape, panel, f.h.app.palette().card))
            .unwrap();
        assert!(
            surface.clip_rect.contains_rect(controls),
            "painted controls clipped at {zoom}"
        );
        if phase == "attached80" {
            assert!(
                composer.bottom() > 720.0,
                "exercise the former lower-screen clip"
            );
        }
        f.save(
            phase,
            out,
            json!({"zoom":zoom,"avatarOpaqueMeshScreenBounds":rect_json(head),
            "avatarOpaqueMeshWorldBounds":rect_json(opaque_world),
            "opaqueWorldDiameter":[head.width()/zoom,head.height()/zoom],
            "aaFeather":{"world":aa_feather_world,"screen":aa_feather_world*zoom,
                "pixelsPerPoint":1.0/aa_feather_world},
            "inferredNominalWorldDiameter":nominal_world,
            "diameterAccounting":"measured opaque span plus one world AA feather; inferred nominal, not opaque bounds",
            "enclosingPanel":rect_json(panel),"composer":rect_json(composer),"send":rect_json(send),
            "paused":true,"activity":"listening","agentSend":false}),
        );
    }
    f.finish();
    management::run();
    activity::run();
    catalogue::run();
    departures::run();
    harness::run();
    stress::run();
}

fn is_surface(shape: &egui::Shape, panel: Rect, fill: egui::Color32) -> bool {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().any(|shape| is_surface(shape, panel, fill)),
        egui::Shape::Rect(rect) => rect.fill == fill && rect.rect == panel,
        _ => false,
    }
}

pub(super) fn world_opaque_head(out: &egui::FullOutput, center: Pos2, zoom: f32) -> Rect {
    fn collect(shape: &egui::Shape, center: Pos2, heads: &mut Vec<Rect>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes
                .iter()
                .for_each(|shape| collect(shape, center, heads)),
            egui::Shape::Mesh(mesh) => {
                // Opaque shell vertices exclude its translucent ground shadow and AA fringe.
                let bounds = mesh.vertices.iter().filter(|v| v.color.a() == 255).fold(
                    Rect::NOTHING,
                    |mut bounds, v| {
                        bounds.extend_with(v.pos);
                        bounds
                    },
                );
                if bounds.is_positive() && bounds.center().distance(center) < 0.05 {
                    heads.push(bounds);
                }
            }
            _ => {}
        }
    }
    let mut heads = Vec::new();
    for shape in &out.shapes {
        collect(&shape.shape, center, &mut heads);
    }
    let heads: Vec<_> = heads
        .into_iter()
        .filter(|r| r.width() > 30.0 * zoom)
        .collect();
    assert_eq!(
        heads.len(),
        1,
        "identify one actual painted world head at {center:?}"
    );
    let head = heads[0];
    // The cached head's AA boundary moves opaque fill inward by half a
    // feather at each edge; its subpixel outline has no opaque plateau.
    // The cache is built before world zoom, so one feather is 1 / PPP world
    // units. Reconstruct nominal diameter explicitly; reserve only 0.01 world
    // units for curved-boundary miter and f32 layer-transform rounding.
    assert!(out.pixels_per_point.is_finite() && out.pixels_per_point > 0.0);
    let aa_feather_world = 1.0 / out.pixels_per_point;
    assert!(
        (head.width() / zoom + aa_feather_world - 36.0).abs() < 0.01,
        "inferred nominal head width: opaque {head:?}, zoom {zoom}, AA feather {aa_feather_world}"
    );
    assert!(
        (head.height() / zoom + aa_feather_world - 36.0).abs() < 0.01,
        "inferred nominal head height: opaque {head:?}, zoom {zoom}, AA feather {aa_feather_world}"
    );
    assert!(support::screen().contains_rect(head));
    head
}

#[test]
fn zero_roster_ui_adds_a_local_collaborator_and_delivers_its_first_scoped_message() {
    use super::super::{conversation_store, lifecycle_tests::delivery, messaging};
    use crate::app::tests::{primary_button, pump_until};
    use std::time::{Duration, Instant};

    // Exercise the app widgets without the capture Fixture's preseeded Dot or
    // its deliberate no-request invariant. A local sidecar only gets a file.
    let mut h = super::host();
    h.app.ensure_work_tab();
    let ctx = h.ctx.clone();
    h.app.set_agent_programs_for_test(&[]);
    h.app.settings.recovery.enabled = false;
    h.app.presence.conversations =
        conversation_store::ConversationStore::at_directory(h.base.join("first-messages"));
    messaging::mention_tests::until(&mut h, |app| app.presence.conversations.status().is_none());
    assert!(h.app.presence.agents.is_empty());
    assert!(h.app.settings.collaborators.is_empty());
    assert_eq!(h.app.presence.roster.profiles().count(), 0);
    assert!(
        h.app.tab().path.is_none(),
        "first chat also works before Save"
    );
    for _ in 0..8 {
        h.frame();
    }
    let plus = ctx.read_response(Id::new("presence-add")).unwrap();
    let at = ctx
        .layer_transform_to_global(plus.layer_id)
        .unwrap_or_default()
        * plus.rect.center();
    h.frame_with(primary_button(at, true, false));
    h.frame_with(primary_button(at, false, false));
    for _ in 0..8 {
        h.frame();
    }
    let out = h.frame_output(|_| {});
    for label in ["Add a collaborator", "Codex", "Cursor", "Dot / local"] {
        management::required(&out, label);
    }
    has(&out, "Source files stay private; edits begin as proposals");
    has(&out, "Save this workbook before requesting board edits");
    let at = management::required(&out, "Dot / local").center();
    h.frame_with(primary_button(at, true, false));
    h.frame_with(primary_button(at, false, false));
    let out = h.frame_output(|_| {});
    let at = management::required(&out, "Add collaborator").center();
    h.frame_with(primary_button(at, true, false));
    h.frame_with(primary_button(at, false, false));
    assert_eq!(h.app.presence.agents.len(), 1);
    assert_eq!(h.app.settings.collaborators.len(), 1);
    let a = &h.app.presence.agents[0];
    let who = a.profile.id.clone();
    let session = a.binding.clone();
    let grant = a.grant.clone().unwrap();
    let dir = a.dir.clone();
    assert_eq!(a.profile.provider, "local");
    assert_eq!(a.profile.name, "Agent");
    assert_eq!(a.profile.hat, "cap");
    assert!(a.request.is_empty() && !a.inflight);
    assert!(a.runtime.is_none() && a.cursor.is_none() && a.launched.is_empty());
    assert!(
        !dir.join("request.json").exists(),
        "Add never submits a task"
    );
    assert!(grant.readable_board && grant.readable_nodes.is_empty());
    assert!(grant.readable_sources.is_empty() && !grant.mutation_autonomy);
    assert_eq!(grant.conversation_ids, vec![who.clone()]);
    assert_eq!(grant.id, session.grant_id);
    assert_eq!(grant.workspace_id, session.workspace_id);
    assert!(h
        .app
        .presence_conversation(&who)
        .is_none_or(|c| { c.turns.is_empty() && c.bindings.is_empty() && c.draft.is_empty() }));
    for _ in 0..8 {
        h.frame();
    }
    let icon = ctx
        .read_response(Id::new(("presence-chat", who.as_str())))
        .unwrap();
    let at = ctx
        .layer_transform_to_global(icon.layer_id)
        .unwrap_or_default()
        * icon.rect.center();
    h.frame_with(primary_button(at, true, false));
    h.frame_with(primary_button(at, false, false));
    for _ in 0..8 {
        h.frame();
    }
    let out = h.frame_output(|_| {});
    assert_eq!(h.app.presence.chrome.active_chat(), Some(who.as_str()));
    assert_eq!(h.app.presence.chrome.canvas_chat(), None);
    let panel = ctx
        .memory(|m| m.area_rect(Id::new("presence-expansion")))
        .unwrap();
    assert!(support::screen().contains_rect(panel));
    assert!(out
        .shapes
        .iter()
        .any(|s| is_surface(&s.shape, panel, h.app.palette().card)));
    let composer = Id::new(("presence-composer", who.as_str()));
    for id in [
        Id::new(("conversation-profile", who.as_str())),
        composer,
        Id::new(("presence-send", who.as_str())),
    ] {
        let response = ctx.read_response(id).expect("actual enclosed chat control");
        let transform = ctx
            .layer_transform_to_global(response.layer_id)
            .unwrap_or_default();
        let rect = transform * response.rect;
        assert!(panel.contains_rect(rect));
        assert!((transform * response.interact_rect).contains(rect.center()));
    }
    let edit = ctx.read_response(composer).unwrap();
    let at = ctx
        .layer_transform_to_global(edit.layer_id)
        .unwrap_or_default()
        * edit.rect.center();
    h.frame_with(primary_button(at, true, false));
    h.frame_with(primary_button(at, false, false));
    assert_eq!(ctx.memory(|m| m.focused()), Some(composer));
    let message = "Hello; please read this board.";
    h.frame_with(|input| input.events.push(egui::Event::Text(message.into())));
    let c = h.app.presence_conversation(&who).unwrap();
    assert_eq!(c.draft, message);
    assert!(c.turns.is_empty());
    let send = ctx
        .read_response(Id::new(("presence-send", who.as_str())))
        .unwrap();
    let at = ctx
        .layer_transform_to_global(send.layer_id)
        .unwrap_or_default()
        * send.rect.center();
    h.frame_with(primary_button(at, true, false));
    h.frame_with(primary_button(at, false, false));
    let request_id = h.app.presence.agents[0].request.clone();
    assert!(!request_id.is_empty());
    let request = delivery::wait_request_until(
        &dir.join("request.json"),
        &request_id,
        Instant::now() + Duration::from_secs(15),
    );
    assert_eq!(request.id, request_id);
    assert_eq!(request.prompt, message);
    assert_eq!(request.collaborator.as_ref(), Some(&session));
    assert!(request.history.is_empty() && request.relayed_from.is_none());
    assert!(request.inputs.context.is_empty() && request.inputs.wired.is_empty());
    assert!(pump_until(&mut h, |_| {
        std::fs::read(dir.join("board-session.json"))
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<atlas_ai::board_link::BoardBinding>(&bytes).ok()
            })
            .is_some_and(|b| b.session == session && b.request_id == request_id)
    }));
    let binding: atlas_ai::board_link::BoardBinding =
        serde_json::from_slice(&std::fs::read(dir.join("board-session.json")).unwrap()).unwrap();
    assert_eq!(binding.session, session);
    assert_eq!(binding.request_id, request_id);
    assert_eq!(binding.grant, grant);
    let c = h.app.presence_conversation(&who).unwrap();
    assert_eq!(c.turns.len(), 1);
    assert_eq!(c.turns[0].text, message);
    assert_eq!(c.turns[0].request, request_id);
    assert_eq!(c.turns[0].status, "sent"); // Adapter enqueue, not provider acknowledgement.
    assert!(c.draft.is_empty());
    let a = &h.app.presence.agents[0];
    assert!(a.runtime.is_none() && a.cursor.is_none() && a.launched.is_empty());
    has(&h.frame_output(|_| {}), message);
}
