//! Review sheets for the chat train card (ledger items CT1–CT10).

use super::*;

fn cursor_card(h: &mut Harness, title: &str) -> NodeId {
    h.app.place_agent_portal_at(Pos2::new(200.0, 80.0));
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_agent_program(id, "cursor");
    h.app.review_dismiss_picker();
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.title = title.into();
            let chat = &mut p.agent.as_mut().unwrap().chat;
            chat.train = true;
            chat.draft = false;
        }
    });
    id
}

fn seed(h: &mut Harness, id: NodeId, lines: &[(&str, &str)]) {
    h.app.review_chat_lines(id, lines);
}

fn fold(h: &mut Harness, id: NodeId, collapsed: bool, partial: bool) {
    h.app.review_chat_fold(id, collapsed, partial);
}

fn aim(h: &mut Harness, id: NodeId, zoom: f32) {
    let r = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = zoom;
}

fn shoot(h: &mut Harness, raster: &mut FrameRaster, item: &str, state: &str, at: Option<Pos2>) {
    let _ = capture_frame(h, raster, |i| {
        if let Some(p) = at {
            i.events.push(egui::Event::PointerMoved(p));
        }
    });
    let out = capture_frame(h, raster, |i| {
        if let Some(p) = at {
            i.events.push(egui::Event::PointerMoved(p));
        }
    });
    review_shot(h, raster, out, item, state);
}

const TRANSCRIPT: &[(&str, &str)] = &[
    (
        "user",
        "What delayed the first reply, but it's ongoing background work.",
    ),
    (
        "assistant",
        "Slate's own logs are the place to check for the 35-second launch delay. If you want, I can look through the AtlasFileExplorer repo for the code that launches the agent.",
    ),
];

fn prepare(tag: &str, collapsed: bool, partial: bool) -> (Harness, NodeId, FrameRaster) {
    let mut h = line_board(tag);
    let id = cursor_card(&mut h, "Cursor");
    seed(&mut h, id, TRANSCRIPT);
    fold(&mut h, id, collapsed, partial);
    h.frame();
    aim(&mut h, id, 1.0);
    h.frame();
    (h, id, FrameRaster::new(1440, 900))
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct1_folds_with_chevrons_hovered() {
    for (state, collapsed, partial) in [
        ("collapsed_hovered", true, false),
        ("partial_hovered", false, true),
        ("maximized_hovered", false, false),
    ] {
        let (mut h, _id, mut raster) = prepare(&format!("ct1_{state}"), collapsed, partial);
        h.frame();
        let at = h.app.chevron_hover_point();
        shoot(&mut h, &mut raster, "CT1", state, at);
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct3_chevron_closeup() {
    let (mut h, id, mut raster) = prepare("ct3", false, true);
    let r = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w - 36.0, r.y + 16.0);
    h.app.tab_mut().cam.z = 2.0;
    h.frame();
    let at = h.app.chevron_hover_point();
    shoot(&mut h, &mut raster, "CT3", "closeup_zoom2", at);
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct4_chevron_hovered() {
    let (mut h, _, mut raster) = prepare("ct4", false, true);
    h.frame();
    let at = h.app.chevron_hover_point();
    shoot(&mut h, &mut raster, "CT4", "hovered", at);
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct5_top_bar() {
    for (dark, theme) in [(true, "dark"), (false, "light")] {
        let (mut h, id, mut raster) = prepare(&format!("ct5_idle_{theme}"), false, true);
        h.app.dark_mode = dark;
        aim(&mut h, id, 1.0);
        shoot(&mut h, &mut raster, "CT5", &format!("idle_{theme}"), None);

        let (mut h, id, mut raster) = prepare(&format!("ct5_stream_{theme}"), false, true);
        h.app.dark_mode = dark;
        h.app.review_chat_streaming(id, 1200);
        aim(&mut h, id, 1.0);
        shoot(
            &mut h,
            &mut raster,
            "CT5",
            &format!("streaming_{theme}"),
            None,
        );
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct6_partial_scroll() {
    let (mut h, id, mut raster) = prepare("ct6", false, true);
    let extra = "Earlier notes stay reachable once the card is scrolled up from the latest line. ";
    seed(
        &mut h,
        id,
        &[
            ("user", &format!("{extra}{extra}{extra}")),
            ("assistant", TRANSCRIPT[1].1),
            ("user", "And the latest question sits at the bottom."),
            ("assistant", TRANSCRIPT[0].1),
        ],
    );
    fold(&mut h, id, false, true);
    h.frame();
    aim(&mut h, id, 1.0);
    let rect = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    shoot(
        &mut h,
        &mut raster,
        "CT6",
        "partial_at_bottom",
        Some(rect.center()),
    );
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(rect.center()));
        i.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 240.0),
            modifiers: Default::default(),
        });
    });
    shoot(
        &mut h,
        &mut raster,
        "CT6",
        "partial_scrolled_up",
        Some(rect.center()),
    );
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct7_same_transcript() {
    for (state, collapsed, partial) in [
        ("collapsed", true, false),
        ("partial", false, true),
        ("maximized", false, false),
    ] {
        let (mut h, _, mut raster) = prepare(&format!("ct7_{state}"), collapsed, partial);
        shoot(&mut h, &mut raster, "CT7", state, None);
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct8_composer_gap() {
    for (state, partial) in [("partial", true), ("maximized", false)] {
        let (mut h, _, mut raster) = prepare(&format!("ct8_{state}"), false, partial);
        shoot(&mut h, &mut raster, "CT8", state, None);
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct9_split_dot() {
    let mut h = line_board("ct9");
    let parent = cursor_card(&mut h, "Cursor");
    h.app.place_agent_portal_at(Pos2::new(700.0, 80.0));
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_agent_program(id, "cursor");
    h.app.review_dismiss_picker();
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.title = "Cursor".into();
            let agent = p.agent.as_mut().unwrap();
            agent.chat.train = true;
            agent.chat.parent = Some(parent);
            agent.chat.partial = true;
        }
    });
    seed(&mut h, id, TRANSCRIPT);
    let session = atlas_agent::AgentSession {
        approval: None,
        conversation: String::new(),
        artifacts: vec![atlas_agent::AgentArtifact {
            id: "read".into(),
            turn: 0,
            kind: atlas_agent::ArtifactKind::Read,
            source: "notes.md".into(),
            title: "notes".into(),
        }],
        status: atlas_agent::AgentStatus::Idle,
        provider: "cursor".into(),
        turns: Vec::new(),
        updated_at: 0,
        bundle: Default::default(),
        request: String::new(),
        usage: None,
    };
    h.app.agents.sessions.insert(id, std::sync::Arc::new(session));
    h.frame();
    aim(&mut h, id, 2.0);
    h.frame();
    let rect = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let at = Pos2::new(rect.left() + 8.0, rect.center().y);
    let mut raster = FrameRaster::new(1440, 900);
    let _ = capture_frame(&mut h, &mut raster, |i| {
        i.time = Some(0.0);
        i.events.push(egui::Event::PointerMoved(at));
    });
    let out = capture_frame(&mut h, &mut raster, |i| {
        i.time = Some(1.0);
        i.events.push(egui::Event::PointerMoved(at));
    });
    review_shot(&mut h, &mut raster, out, "CT9", "split_zoom2");
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct10_link_dots() {
    let (mut h, id, mut raster) = prepare("ct10", false, false);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().chat.linear = true;
            n.rect.h = 72.0;
        }
    });
    h.frame();
    aim(&mut h, id, 2.0);
    h.frame();
    let rect = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    shoot(
        &mut h,
        &mut raster,
        "CT10",
        "all_dots",
        Some(rect.center()),
    );
}
