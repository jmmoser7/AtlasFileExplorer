use super::*;
use crate::app::tests::{pointer_to, primary_button, Harness};

fn label(output: &egui::FullOutput, name: &str) -> Option<Pos2> {
    fn find(shape: &egui::Shape, name: &str) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == name => {
                Some(t.visual_bounding_rect().center())
            }
            egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, name)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|s| find(&s.shape, name).filter(|point| s.clip_rect.contains(*point)))
}
fn click(h: &mut Harness, point: Pos2) {
    for pressed in [true, false] {
        h.frame_with(primary_button(point, pressed, false));
    }
    h.frame();
}
fn open_agent(h: &mut Harness, id: NodeId) {
    crate::app::tests::frame_on(h, id, 1.0);
    h.app.board_sel = [id].into_iter().collect();
    for _ in 0..3 {
        h.frame();
    }
    let panel = crate::app::board_properties::Panel::Agent;
    // Selection memory can already restore Agent. Toggling again closes it,
    // leaving a painted but disabled editor during the closing transition.
    if h.app.shape_properties.panel != Some(panel) || h.app.shape_properties.ids != [id] {
        let tool = h
            .ctx
            .data(|d| d.get_temp::<Rect>(Id::new(("painted-agent-tool-hit", id.0))))
            .unwrap();
        click(h, tool.center());
    }
    h.frame();
    assert_eq!(h.app.shape_properties.panel, Some(panel));
    assert_eq!(h.app.shape_properties.ids, [id]);
}
fn send(h: &mut Harness, id: NodeId, words: &str) -> AgentRequest {
    let mut hit = None;
    let key = Id::new(("painted-generator-composer-hit", id.0));
    for _ in 0..30 {
        h.ctx
            .data_mut(|data| data.remove::<(Id, Rect, egui::LayerId, bool)>(key));
        let output = h.frame_output(|_| {});
        hit = h
            .ctx
            .data(|data| data.get_temp::<(Id, Rect, egui::LayerId, bool)>(key))
            .filter(|(_, visible, layer, enabled)| {
                *enabled
                    && visible.is_positive()
                    && h.ctx.layer_id_at(visible.center()) == Some(*layer)
                    && label(&output, "Message…").is_some()
            });
        if hit.is_some() {
            break;
        }
    }
    let (editor, visible, layer, _) =
        hit.expect("actual ON Image composer is enabled and visible on its current layer");
    let prompt = visible.center();
    click(h, prompt);
    let focused = h
        .ctx
        .memory(|memory| memory.focused())
        .expect("actual composer takes keyboard focus");
    assert_eq!(focused, editor, "the clicked source owns keyboard focus");
    let response = h
        .ctx
        .read_response(editor)
        .expect("current focused editor response");
    let transform = h
        .ctx
        .layer_transform_to_global(response.layer_id)
        .unwrap_or_default();
    assert!(response.enabled() && response.has_focus() && (transform * response.rect).contains(prompt),
        "painted composer click reaches its enabled current text editor: source={id:?}, point={prompt:?}, visible={visible:?}, layer={layer:?}, current={:?}, current_layer={:?}, transform={transform:?}, panel={:?}",
        response.rect,response.layer_id,h.app.shape_properties.panel);
    h.frame_with(|input| input.events.push(egui::Event::Text(words.into())));
    assert_eq!(
        h.app
            .pending_generator_prompt(id)
            .or_else(|| h.app.agents.prompt(id)),
        Some(words),
        "actual Message composer owns the typed question before Send"
    );
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }
    for _ in 0..6 {
        h.frame();
    }
    assert!(
        h.app.agents.dispatched.iter().any(|(node, _)| *node == id),
        "Send must dispatch: source {:?}, toasts {:?}",
        h.app.doc().scene.node(id),
        h.app
            .toasts
            .iter()
            .map(|(text, _)| text)
            .collect::<Vec<_>>()
    );
    let request = h
        .app
        .agents
        .dispatched
        .iter()
        .rev()
        .find(|(node, _)| *node == id)
        .expect("real Send reaches test transport")
        .1
        .clone();
    assert_eq!(request.prompt, words);
    assert_eq!(request.image_history, Some(true));
    request
}
fn complete(h: &mut Harness, id: NodeId, request: &AgentRequest) {
    // Provider completion fixture AFTER actual ordinary UI Send/bind. No paid call.
    let source = h.base.join(format!("{}.png", request.id));
    std::fs::copy(h.base.join("ordinary.png"), &source).unwrap();
    let mut turns = request.history.clone();
    turns.push(atlas_agent::AgentTurn {
        role: "user".into(),
        text: request.prompt.clone(),
        at: request.at,
    });
    let session = atlas_agent::AgentSession {
        request: request.id.clone(),
        provider: "comfy".into(),
        status: atlas_agent::AgentStatus::Idle,
        turns,
        bundle: atlas_agent::ImageBundle {
            images: vec![atlas_agent::ImageOutput {
                id: format!("{}-0", request.id),
                request: request.id.clone(),
                source: source.to_string_lossy().into(),
                prompt: request.prompt.clone(),
                model: String::new(),
                task: String::new(),
                live: String::new(),
            }],
        },
        ..Default::default()
    };
    let manifest = h
        .app
        .doc()
        .scene
        .node(id)
        .and_then(slate_doc::agent_chat::agent)
        .and_then(|a| a.bundle.as_ref())
        .unwrap()
        .locator
        .clone();
    let path = slate_doc::scene::resolve_source(h.app.tab().path.as_deref(), &manifest);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    atlas_ai::agent::atomic_write_json(&path, &session).unwrap();
    h.app
        .agents
        .sessions
        .insert(id, std::sync::Arc::new(session));
    for _ in 0..3 {
        h.frame();
    }
}
fn history_menu(h: &mut Harness, id: NodeId, incoming: bool) {
    crate::app::tests::frame_on(h, id, 1.0);
    h.frame();
    let xf = h.app.board_xf();
    let rect = h
        .app
        .image_history_handle(
            id,
            incoming,
            xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect),
            xf.z,
        )
        .unwrap();
    let point = rect.center();
    h.frame_with(pointer_to(point, false));
    let top_layer = h.ctx.layer_id_at(point);
    assert!(h.app.canvas_rect.contains(point) && top_layer.is_some_and(|layer| layer.order < atlas_shell::selection_tools::POPUP_ORDER),
        "actual history handle must be visible and unoccluded: node={id:?}, incoming={incoming}, handle={rect:?}, canvas={:?}, top={top_layer:?}, modal={:?}", h.app.canvas_rect, h.ctx.memory(|memory|memory.top_modal_layer()));
    click(h, point);
    h.frame();
    assert!(h.app.agents.flow.graph_menu.as_ref().is_some_and(|menu| menu.target == id && menu.incoming == incoming),
        "actual history click must open its requested popup: node={id:?}, incoming={incoming}, modal={:?}, history_view={}, graph_menu={:?}",
        h.ctx.memory(|memory|memory.top_modal_layer()),h.app.agents.flow.history_view.is_some(),
        h.app.agents.flow.graph_menu.as_ref().map(|menu|(menu.target,menu.incoming,menu.at)));
}

#[test]
fn ordinary_image_real_send_history_view_blank_child_refresh_and_cut_keep_source_and_scope() {
    let mut h = Harness::new("ordinary-image-history-ui");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.set_agent_programs_for_test(&["comfy"]);
    h.app
        .pin_pack_for_test("comfy", atlas_ai::packs::PackHealth::Ok);
    let source = crate::app::board_flow::tests::place(&mut h, "ordinary.png", 0.0);
    let authored = h.app.doc().scene.node(source).unwrap().clone();
    assert!(slate_doc::agent_chat::agent(&authored).is_none());
    open_agent(&mut h, source);
    assert_eq!(h.app.doc().scene.node(source), Some(&authored));
    let more = h
        .ctx
        .data(|d| d.get_temp::<Rect>(Id::new("painted-generator-options-hit")))
        .unwrap();
    click(&mut h, more.center());
    let output = h.frame_output(|_| {});
    click(&mut h, label(&output, "Preserve model history").unwrap());
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    h.frame();
    let first = send(&mut h, source, "First public image question");
    assert_eq!(first.image_history_prefix, Some(0));
    complete(&mut h, source, &first);
    // A reopened saved Image can have a bundle before its public cache loads.
    // The actual downstream action must reject that boundary without mutation.
    let cached = h.app.agents.sessions.remove(&source).unwrap();
    let before_unloaded = h.app.doc().scene.nodes.clone();
    assert!(!h.app.spawn_blank_image_child(source, None));
    assert_eq!(h.app.doc().scene.nodes, before_unloaded);
    h.app.agents.sessions.insert(source, cached);
    assert_eq!(h.app.doc().scene.node(source).unwrap().rect, authored.rect);
    let before_view = h.app.doc().scene.node(source).unwrap().clone();
    let run_count = h.app.agents.dispatched.len();
    history_menu(&mut h, source, false);
    let output = h.frame_output(|_| {});
    click(&mut h, label(&output, "View carried history").unwrap());
    for _ in 0..3 {
        h.frame();
    }
    let output = h.frame_output(|_| {});
    assert!(label(&output, "Carried conversation history").is_some());
    assert!(label(&output, "You: First public image question").is_some());
    let close = label(&output, "Close").unwrap();
    let close_top = h.ctx.layer_id_at(close);
    click(&mut h, close);
    assert!(h.app.agents.flow.history_view.is_none(), "actual Close must retire carried history: click={close:?}, top_before={close_top:?}, top_after={:?}, modal={:?}", h.ctx.layer_id_at(close),h.ctx.memory(|memory|memory.top_modal_layer()));
    assert!(h.ctx.memory(|memory|memory.top_modal_layer()).is_none(), "Close press/release/idle must retire canonical previous-pass modal ownership before another history gesture");
    assert_eq!(h.app.doc().scene.node(source), Some(&before_view));
    assert_eq!(h.app.agents.dispatched.len(), run_count);
    history_menu(&mut h, source, false);
    let output = h.frame_output(|_| {});
    let downstream = label(&output, "New downstream image").unwrap_or_else(|| {
        fn labels(shape: &egui::Shape, clip: Rect, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => out.push(format!(
                    "{:?} bounds={:?} clip={clip:?}",
                    text.galley.text(),
                    text.visual_bounding_rect()
                )),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        labels(shape, clip, out);
                    }
                }
                _ => {}
            }
        }
        let mut painted = Vec::new();
        for shape in &output.shapes {
            labels(&shape.shape, shape.clip_rect, &mut painted);
        }
        panic!(
            "valid outgoing history action missing: graph_menu={:?}; painted={painted:#?}",
            h.app
                .agents
                .flow
                .graph_menu
                .as_ref()
                .map(|menu| (menu.target, menu.incoming, menu.at))
        );
    });
    click(&mut h, downstream);
    let child = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| n.id != source && matches!(n.kind, NodeKind::Image(_)))
        .unwrap()
        .id;
    let node = h.app.doc().scene.node(child).unwrap();
    assert_eq!(node.rect.w, authored.rect.w);
    assert_eq!(node.rect.h, authored.rect.h);
    let NodeKind::Image(image) = &node.kind else {
        panic!()
    };
    assert_eq!(image.item, slate_doc::ItemId::NONE);
    let metadata = image.agent.as_ref().unwrap();
    assert!(metadata.seed.is_none() && metadata.bundle.is_none());
    assert_eq!(metadata.provider, "comfy");
    assert_eq!(metadata.generator.inherited_turn_count, Some(0));
    assert!(!h.app.board_sel.contains(&child) && !h.app.board_sel.contains(&source));
    assert_eq!(h.app.agents.dispatched.len(), run_count);
    let first_pin = agent_inputs::generator_memory_snapshot(&h.app.doc().scene, child).unwrap();
    assert_eq!(first_pin.turns.len(), 1);
    open_agent(&mut h, source);
    let second = send(&mut h, source, "Second public image question");
    assert_ne!(second.id, first.id);
    assert_eq!(second.image_history_prefix, Some(0));
    assert_eq!(
        second.history.len(),
        1,
        "second real Send carries the first public question"
    );
    assert_eq!(second.history[0].text, first.prompt);
    complete(&mut h, source, &second);
    let public = h.app.public_image_history(source).unwrap();
    assert_eq!(public.len(), 2);
    assert_eq!(public[1].text, second.prompt);
    assert_eq!(
        agent_inputs::generator_memory_snapshot(&h.app.doc().scene, child).unwrap(),
        first_pin,
        "parent completion cannot overwrite child history"
    );
    history_menu(&mut h, child, true);
    let output = h.frame_output(|_| {});
    click(&mut h, label(&output, "Refresh inherited history").unwrap());
    let refreshed = agent_inputs::generator_memory_snapshot(&h.app.doc().scene, child).unwrap();
    assert_eq!(refreshed.turns.len(), 2);
    assert_eq!(
        h.app.doc().scene.node(child).unwrap().rect.w,
        authored.rect.w
    );
    open_agent(&mut h, child);
    let own = send(&mut h, child, "Child public question");
    assert_eq!(own.image_history_prefix, Some(2));
    assert_eq!(own.history.len(), 2);
    complete(&mut h, child, &own);
    let outputs = h.app.agent_images(child).as_ref().clone();
    history_menu(&mut h, child, true);
    let output = h.frame_output(|_| {});
    click(&mut h, label(&output, "Cut model history").unwrap());
    let metadata = h
        .app
        .doc()
        .scene
        .node(child)
        .and_then(slate_doc::agent_chat::agent)
        .unwrap();
    assert_eq!(metadata.generator.history_source, Some(source));
    assert_eq!(metadata.generator.inherited_turn_count, Some(0));
    assert_eq!(metadata.generator.preserve_history, Some(false));
    assert!(metadata.generator.history_cut);
    assert!(h.app.image_history_edge(child, true).is_none());
    assert_eq!(h.app.agent_images(child).as_ref(), &outputs);
    let data=h.app.doc().scene.nodes.iter().filter(|n|matches!(&n.kind,NodeKind::Connector(c) if c.binding.as_ref().is_some_and(|b|b.kind==InputKind::Images))).count();
    assert_eq!(data, 1);
}
