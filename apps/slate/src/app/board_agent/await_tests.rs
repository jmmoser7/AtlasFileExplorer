//! Generator, provider, and live-frame tests.

use super::test_support::*;
use super::*;

#[test]
fn generator_frame_follows_image_aspect() {
    assert_eq!(fit_frame_height(960.0, 512, 512), 960.0);
    assert_eq!(fit_frame_height(960.0, 512, 768), 1400.0);
    assert_eq!(fit_frame_height(800.0, 1024, 512), 400.0);
}

#[test]
fn focused_agent_keeps_canvas_zoom_and_title_rename() {
    for z in [0.5, 1.0, 2.0] {
        let mut h = super::super::tests::Harness::new("agent_header_zoom");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "ollama");
        h.app.agents.models_started.insert("ollama".into());
        h.app.patch_nodes(&[id], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                p.title = "Review".into();
                p.agent.as_mut().unwrap().model = Some("llama3.1:8b".into());
            }
        });
        h.frame();
        let r = h.app.doc().scene.node(id).unwrap().rect;
        h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
        h.app.tab_mut().cam.z = z;
        h.frame();
        h.frame();
        let r = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let point = r.center();
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(point)));
        h.frame_with(|i| {
            i.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 60.0),
                modifiers: Default::default(),
            })
        });
        assert!(h.app.tab().cam.z > z, "focused agent swallowed zoom at {z}");
        let z = h.app.tab().cam.z;
        let r = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let point = r.min + egui::vec2(30.0, 15.0) * z;
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(point)));
        for pressed in [true, false] {
            h.frame_with(|i| {
                i.events.push(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                })
            });
        }
        assert!(
            h.app.agents.title_edit.is_some(),
            "single title click at {z}"
        );
        assert!(h.app.board_drag.is_none());
    }
}

#[test]
fn provider_tiles_accept_first_click_near_their_edges() {
    for (z, offset) in [0.5, 1.0, 2.0].into_iter().flat_map(|z| {
        [(-47.0, -39.0), (47.0, -39.0), (-47.0, 39.0), (47.0, 39.0)]
            .into_iter()
            .map(move |offset| (z, offset))
    }) {
        let mut h = super::super::tests::Harness::new("picker_edge");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.agents.programs_started = true;
        h.app.agents.programs = vec![atlas_ai::agent::provider_by_id("local")];
        h.app.agents.focused = None;
        h.app.portals.contents = None;
        let r = h.app.doc().scene.node(id).unwrap().rect;
        h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
        h.app.tab_mut().cam.z = z;
        h.frame();
        h.frame();
        let rect = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let point = rect.center() + egui::vec2(offset.0, offset.1) * z;
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
        for pressed in [true, false] {
            h.frame_with(|input| {
                input.events.push(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                })
            });
        }
        assert_eq!(
            h.app.agent_session_for(id).unwrap().1,
            "local",
            "tile edge at {z}"
        );
    }
}

#[test]
fn picker_exit_and_streaming_text_remeasure_cards() {
    let mut h = super::super::tests::Harness::new("picker_fit");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.app.agents.project_picker = Some(id);
    h.frame();
    let picker_h = h.app.doc().scene.node(id).unwrap().rect.h;
    h.app.agents.project_picker = None;
    h.frame();
    assert!(h.app.doc().scene.node(id).unwrap().rect.h < picker_h);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let a = p.agent.as_mut().unwrap();
            a.chat.train = true;
            a.chat.detail = slate_doc::agent_chat::Detail::Summary;
            a.bundle = None;
        }
    });
    h.app.agents.local_turns.insert(
        id,
        vec![AgentTurn {
            role: "assistant".into(),
            text: "Short reply".into(),
            at: 0,
        }],
    );
    h.app.agents.output_epoch += 1;
    h.frame();
    let short = h.app.doc().scene.node(id).unwrap().rect.h;
    h.app.agents.local_turns.get_mut(&id).unwrap()[0].text =
        "A response that grows while streaming. ".repeat(30);
    h.app.agents.output_epoch += 1;
    h.frame();
    assert!(h.app.doc().scene.node(id).unwrap().rect.h > short * 2.0);
}

/// A link folder whose conversation wrote outputs into `project`.
fn prior_link(ws: &std::path::Path, session: &str, project: &std::path::Path, at: u64) {
    let link = atlas_ai::agent::agent_dir(ws, session);
    std::fs::create_dir_all(&link).unwrap();
    let out = project
        .join("slate-outputs")
        .join("untitled-board")
        .join("2026-09-24-cursor-abc123");
    let record = link.join("output.json");
    atlas_ai::agent::atomic_write_json(&record, &serde_json::json!({ "dir": out })).unwrap();
    std::fs::File::options()
        .write(true)
        .open(record)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(at))
        .unwrap();
}

fn listed(h: &super::super::tests::Harness, provider: &str) -> Vec<PathBuf> {
    h.app
        .agents
        .provider_recents
        .get(provider)
        .map(|list| list.iter().map(|e| e.path.clone()).collect())
        .unwrap_or_default()
}

fn same_folder(a: &std::path::Path, b: &std::path::Path) -> bool {
    atlas_ai::projects::same_folder(a, b)
}

#[test]
fn projects_the_agent_portal_used_before_are_offered_again() {
    let mut h = super::super::tests::Harness::new("agent_projects_mru");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    let ws = h.base.join("ai-ws");
    let older = h.base.join("older-project");
    let newer = h.base.join("newer-project");
    let open = h.base.join("open-project");
    let gone = h.base.join("deleted-project");
    for dir in [&ws, &older, &newer, &open] {
        std::fs::create_dir_all(dir).unwrap();
    }
    prior_link(&ws, "agent-req-1-new", &newer, 1_790_000_300);
    prior_link(&ws, "agent-req-1-gone", &gone, 1_790_000_400);
    prior_link(&ws, "agent-req-1-dup", &older, 1_790_000_200);
    prior_link(&ws, "agent-req-1-old", &older, 1_790_000_100);
    h.app.ai.config.workspace_dir = Some(ws);
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "cursor");
    h.app.bind_portal_source(id, open.clone());
    h.app.agents.project_picker = Some(id);

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        h.frame();
        let done = h.app.agents.recents_started && h.app.agents.recents_rx.is_none();
        if done && !listed(&h, "cursor").is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "project discovery never finished"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    for provider in ["cursor", "codex"] {
        let list = listed(&h, provider);
        let at = |p: &std::path::Path| list.iter().position(|e| same_folder(e, p));
        let (Some(n), Some(o), Some(w)) = (at(&newer), at(&older), at(&open)) else {
            panic!("{provider} lists earlier agent projects: {list:?}");
        };
        assert!(n < o, "newest first for {provider}: {list:?}");
        assert!(w < list.len());
        assert!(at(&gone).is_none(), "a deleted folder is not offered");
        assert_eq!(
            list.iter().filter(|e| same_folder(e, &older)).count(),
            1,
            "one entry per folder"
        );
    }
}

#[test]
fn local_dropdown_changes_model_without_replacing_conversation() {
    let mut h = super::super::tests::Harness::new("local_model");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "ollama/old-model");
    h.app.board_sel = [id].into_iter().collect();
    let session = h.app.agent_session_for(id).unwrap().0;
    h.app.agents.models.insert(
        "ollama".into(),
        vec![atlas_ai::agent::AgentModel {
            id: "local-model".into(),
            name: "local-model".into(),
        }],
    );
    assert!(h.app.agent_set_model("local-model"));
    let a = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(a.provider, "ollama");
    assert_eq!(a.model.as_deref(), Some("local-model"));
    assert_eq!(a.session, session);
    assert!(!h.app.agent_set_model("not-installed"));
}

#[test]
fn comfy_checkpoint_choice_keeps_the_image_portal() {
    let mut h = super::super::tests::Harness::new("comfy_model");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "comfy");
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    assert_eq!(rect.w, 960.0);
    assert_eq!(rect.h, 540.0);
    h.app.board_sel = [id].into_iter().collect();
    h.app.agents.models.insert(
        "comfy".into(),
        vec![atlas_ai::agent::AgentModel {
            id: "sd_xl_base_1.0.safetensors".into(),
            name: "sd_xl_base_1.0.safetensors".into(),
        }],
    );
    assert!(h.app.agent_set_model("sd_xl_base_1.0.safetensors"));
    assert!(!h.app.agent_set_model("not-installed.safetensors"));
    let a = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(a.provider, "comfy");
    assert_eq!(a.view, atlas_ai::agent::PortalView::Images);
    assert_eq!(a.model.as_deref(), Some("sd_xl_base_1.0.safetensors"));
    // An empty choice returns the generator to Auto.
    assert!(h.app.agent_set_model(""));
    let a = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert!(a.model.is_none());
}

#[test]
fn live_frames_use_the_wired_note_and_one_seed() {
    let note = "a nice public park setting with happy people";
    let (mut h, id, note_id) = generator_with_note("gen_prompt", note);
    let view = h.app.generator_view(id);
    assert_eq!(view.inputs.len(), 1);
    assert_eq!(view.inputs[0].role, InputRole::Prompt);
    assert_eq!(view.inputs[0].node, note_id);
    assert_eq!(view.task().action(), "Generate");
    assert!(view.error.is_none());

    h.app.set_generator_live(id, true);
    let first = h.app.generation_request(id, true).unwrap();
    let second = h.app.generation_request(id, true).unwrap();
    assert_eq!(
        first.prompt, note,
        "the note is the prompt, never a stand-in"
    );
    let (a, b) = (first.image.unwrap(), second.image.unwrap());
    assert_eq!(a.seed, b.seed, "live frames repeat one seed");
    assert_eq!(a.live, b.live);
    assert!(a.live.is_some());
    let still = h.app.generation_request(id, false).unwrap();
    let still = still.image.unwrap();
    assert!(still.seed.is_none(), "a pressed Render varies its seed");
    assert!(still.live.is_none());
}

#[test]
fn rapid_presses_wait_their_turn_and_stop_clears_them() {
    let (mut h, id, _) = generator_with_note("gen_queue", "timber facade");
    h.app.queue_generation(id);
    h.app.queue_generation(id);
    h.app.queue_generation(id);
    assert_eq!(h.app.agents.dispatched.len(), 1, "one run at a time");
    assert_eq!(h.app.generations_waiting(id), 2);
    assert_eq!(
        h.app.agents.requests.get(&id),
        Some(&h.app.agents.dispatched[0].1.id),
        "the running request is tracked, not the newest press"
    );
    h.app.agents.awaiting.remove(&id);
    h.app.pump_comfy_queue();
    assert_eq!(h.app.agents.dispatched.len(), 2);
    assert_eq!(h.app.generations_waiting(id), 1);
    assert_ne!(
        h.app.agents.dispatched[0].1.id,
        h.app.agents.dispatched[1].1.id
    );
    assert!(h.app.stop_selected_agent());
    assert_eq!(h.app.generations_waiting(id), 0);
}

#[test]
fn keep_starts_a_new_slot_and_stop_ends_live() {
    let (mut h, id, _) = generator_with_note("gen_live", "watercolor");
    assert!(!h.app.agent_keep_live(), "Keep applies only while live");
    assert!(h.app.agent_toggle_live());
    let live = |h: &super::super::tests::Harness| {
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .live
    };
    assert!(live(&h));
    let run = h.app.agents.live_run.get(&id).cloned().unwrap();
    assert!(h.app.agent_keep_live());
    assert_ne!(h.app.agents.live_run.get(&id), Some(&run));
    assert!(h.app.stop_selected_agent());
    assert!(!live(&h), "Stop also ends live rendering");
    assert!(!h.app.agents.live_run.contains_key(&id));
}
