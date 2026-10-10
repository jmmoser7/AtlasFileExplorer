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

fn settle(h: &mut Harness) {
    for _ in 0..6 {
        h.frame();
    }
}

fn aim(h: &mut Harness, id: NodeId, zoom: f32) {
    let r = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = zoom;
    settle(h);
}

fn card_screen(h: &Harness, id: NodeId) -> egui::Rect {
    h.app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect)
}

/// Two frames with the pointer at `at`, then a crop around `id`'s card.
fn shoot(
    h: &mut Harness,
    raster: &mut FrameRaster,
    item: &str,
    state: &str,
    at: Option<Pos2>,
    id: NodeId,
) {
    let area = card_screen(h, id).expand(24.0 * h.app.tab().cam.z.max(1.0));
    shoot_area(h, raster, item, state, at, area, 1);
}

fn shoot_area(
    h: &mut Harness,
    raster: &mut FrameRaster,
    item: &str,
    state: &str,
    at: Option<Pos2>,
    area: egui::Rect,
    magnify: u32,
) {
    let mut out = None;
    for _ in 0..3 {
        out = Some(capture_frame(h, raster, |i| {
            if let Some(p) = at {
                i.events.push(egui::Event::PointerMoved(p));
            }
        }));
    }
    review_shot_crop(h, raster, out.unwrap(), item, state, area, magnify);
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
    h.app.set_board_tool(crate::app::board::BoardTool::Select);
    let id = cursor_card(&mut h, "Cursor");
    seed(&mut h, id, TRANSCRIPT);
    fold(&mut h, id, collapsed, partial);
    settle(&mut h);
    aim(&mut h, id, 1.0);
    (h, id, FrameRaster::new(1440, 900))
}

/// Chevron order on the card (top to bottom): collapsed offers expand
/// single then double; partial offers collapse then expand; maximized
/// offers collapse double then single.
/// `(state, collapsed, partial, [(chevron name, hit index)])`.
type Fold = (&'static str, bool, bool, &'static [(&'static str, usize)]);

const FOLDS: &[Fold] = &[
    ("collapsed", true, false, &[("single", 0), ("double", 1)]),
    (
        "partial",
        false,
        true,
        &[("collapse_single", 0), ("expand_single", 1)],
    ),
    ("maximized", false, false, &[("double", 0), ("single", 1)]),
];

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct1_folds_with_chevrons_hovered() {
    for (state, collapsed, partial, marks) in FOLDS {
        let (mut h, id, mut raster) = prepare(&format!("ct1_{state}"), *collapsed, *partial);
        aim(&mut h, id, 2.0);
        for (mark, index) in marks.iter() {
            let at = h.app.chevron_hover_point(*index);
            assert!(at.is_some(), "{state} paints chevron {index}");
            shoot(
                &mut h,
                &mut raster,
                "CT1",
                &format!("{state}_{mark}_hovered"),
                at,
                id,
            );
        }
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct3_chevron_closeup() {
    for (state, collapsed, partial, _) in FOLDS {
        let (mut h, id, mut raster) = prepare(&format!("ct3_{state}"), *collapsed, *partial);
        aim(&mut h, id, 2.0);
        let r = card_screen(&h, id);
        let area = egui::Rect::from_min_max(
            Pos2::new(r.right() - 120.0, r.top() - 16.0),
            Pos2::new(r.right() + 16.0, r.top() + 56.0),
        );
        let at = h.app.chevron_hover_point(0);
        shoot_area(
            &mut h,
            &mut raster,
            "CT3",
            &format!("{state}_closeup_zoom2"),
            at,
            area,
            5,
        );
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct4_chevron_hovered() {
    let (mut h, id, mut raster) = prepare("ct4", false, false);
    aim(&mut h, id, 2.0);
    for (mark, index) in [("single", 1), ("double", 0)] {
        let at = h.app.chevron_hover_point(index);
        shoot(
            &mut h,
            &mut raster,
            "CT4",
            &format!("hovered_{mark}"),
            at,
            id,
        );
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct5_top_bar() {
    for (dark, theme) in [(true, "dark"), (false, "light")] {
        for zoom in [1.0, 2.0] {
            let (mut h, id, mut raster) = prepare(&format!("ct5_idle_{theme}"), false, true);
            h.app.dark_mode = dark;
            aim(&mut h, id, zoom);
            shoot(
                &mut h,
                &mut raster,
                "CT5",
                &format!("idle_{theme}_zoom{zoom}"),
                None,
                id,
            );

            let (mut h, id, mut raster) = prepare(&format!("ct5_stream_{theme}"), false, true);
            h.app.dark_mode = dark;
            h.app.review_chat_streaming(id, 1200);
            aim(&mut h, id, zoom);
            shoot(
                &mut h,
                &mut raster,
                "CT5",
                &format!("streaming_{theme}_zoom{zoom}"),
                None,
                id,
            );
        }
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
            ("user", &format!("First question. {extra}{extra}")),
            ("assistant", TRANSCRIPT[1].1),
            ("user", "And the latest question sits at the bottom."),
            ("assistant", TRANSCRIPT[0].1),
        ],
    );
    fold(&mut h, id, false, true);
    h.app.review_chat_streaming(id, 1200);
    settle(&mut h);
    aim(&mut h, id, 1.5);
    let center = card_screen(&h, id).center();
    shoot(
        &mut h,
        &mut raster,
        "CT6",
        "partial_at_bottom",
        Some(center),
        id,
    );
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(center));
        i.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 400.0),
            modifiers: Default::default(),
        });
    });
    for _ in 0..12 {
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(center)));
    }
    shoot(
        &mut h,
        &mut raster,
        "CT6",
        "partial_scrolled_up",
        Some(center),
        id,
    );
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct7_same_transcript() {
    for (state, collapsed, partial, _) in FOLDS {
        for zoom in [0.5, 1.0, 2.0] {
            let (mut h, id, mut raster) = prepare(&format!("ct7_{state}"), *collapsed, *partial);
            aim(&mut h, id, zoom);
            shoot(
                &mut h,
                &mut raster,
                "CT7",
                &format!("{state}_zoom{zoom}"),
                None,
                id,
            );
        }
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct8_composer_gap() {
    for (state, partial) in [("partial", true), ("maximized", false)] {
        for zoom in [1.0, 2.0] {
            let (mut h, id, mut raster) = prepare(&format!("ct8_{state}"), false, partial);
            aim(&mut h, id, zoom);
            shoot(
                &mut h,
                &mut raster,
                "CT8",
                &format!("{state}_zoom{zoom}"),
                None,
                id,
            );
        }
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct9_split_dot() {
    let mut h = line_board("ct9");
    h.app.set_board_tool(crate::app::board::BoardTool::Select);
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
    h.app
        .agents
        .sessions
        .insert(id, std::sync::Arc::new(session));
    settle(&mut h);
    aim(&mut h, id, 2.0);
    let rect = card_screen(&h, id);
    // On the lower half, so the hover label lands below both halves.
    let at = Pos2::new(
        rect.left() + slate_doc::agent_chat::PORT_INSET * 2.0,
        rect.center().y + 10.0,
    );
    let area = egui::Rect::from_center_size(
        Pos2::new(rect.left(), rect.center().y),
        egui::vec2(120.0, 120.0),
    );
    let mut raster = FrameRaster::new(1440, 900);
    let mut out = None;
    // The split animates over 0.18 s; stop before the 0.5 s tooltip delay.
    let start = h.ctx.input(|i| i.time);
    for frame in 0..18 {
        out = Some(capture_frame(&mut h, &mut raster, |i| {
            i.time = Some(start + frame as f64 / 60.0);
            i.events.push(egui::Event::PointerMoved(at));
        }));
    }
    review_shot_crop(
        &mut h,
        &mut raster,
        out.unwrap(),
        "CT9",
        "split_zoom2",
        area,
        5,
    );
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn ct10_link_dots() {
    let (mut h, id, mut raster) = prepare("ct10", false, false);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().chat.linear = true;
        }
    });
    settle(&mut h);
    aim(&mut h, id, 2.0);
    // Crosstalk ports show only near the pointer, so each is shot hovered.
    for (side, name) in [
        (slate_doc::scene::Side::Top, "top_port_hovered"),
        (slate_doc::scene::Side::Bottom, "bottom_port_hovered"),
    ] {
        let node = h.app.doc().scene.node(id).unwrap().clone();
        let w = slate_doc::connector_anchor_on(&node, side, 0.5);
        let at = h.app.board_xf().w2s(Pos2::new(w[0], w[1]));
        shoot(&mut h, &mut raster, "CT10", name, Some(at), id);
    }
}
