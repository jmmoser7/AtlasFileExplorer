use super::*;

fn waiting(h: &mut Harness, node: NodeId, request: &str) -> PathBuf {
    let session: AgentSession = serde_json::from_value(serde_json::json!({
        "provider":"codex", "status":"thinking", "request":request,
        "approval":{"id":"same-provider-approval-id","description":"Write the staged dashboard HTML."}
    })).unwrap();
    h.app.agents.sessions.insert(node, session.into());
    h.app.agents.requests.insert(node, request.into());
    h.app.agent_link_dir(node, &h.base).unwrap()
}

fn finish_delivery(h: &mut Harness, node: NodeId) -> String {
    h.app.wait_agent_approval_delivery_for_test();
    h.app.pump_agent_approval_deliveries(&h.ctx);
    let description = h.app.agent_approval_description(node).unwrap();
    assert!(!description.starts_with("Sending your decision"));
    description
}

#[test]
fn artifact_paints_both_approval_choices_and_click_dispatches_the_existing_command() {
    for allow in [true, false] {
        let (mut h, image) = board(if allow {
            "artifact-approval-allow"
        } else {
            "artifact-approval-deny"
        });
        let node = spawn(&mut h, image);
        let dir = waiting(&mut h, node, "approval-request");
        std::fs::create_dir_all(&dir).unwrap();
        let mut point = Pos2::ZERO;
        for frame in 0..5 {
            let events = if frame >= 3 {
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 3,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else {
                vec![]
            };
            let output = h.ctx.clone().run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let height = h.app.artifact_editor_height(ui.painter(), node, 300.0);
                        let rect =
                            Rect::from_min_size(Pos2::new(50.0, 50.0), egui::vec2(300.0, height));
                        h.app.artifact_editor_body(
                            ui,
                            rect,
                            node,
                            1.0,
                            atlas_shell::theme::Palette::dark(),
                        );
                    });
                },
            );
            if frame < 3 {
                for label in ["Allow once", "Deny"] {
                    let center = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => {
                                Some(text.visual_bounding_rect().center())
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("{label} must be visible"));
                    if (label == "Allow once") == allow {
                        point = center;
                    }
                }
            }
        }
        assert!(finish_delivery(&mut h, node).starts_with("Decision sent"));
        let response: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("approval.json")).unwrap()).unwrap();
        assert_eq!(response["allow"], allow);
        assert_eq!(response["id"], "same-provider-approval-id");
        assert!(!h.app.agent_approval_interactive(node));
    }
}

#[test]
fn approval_delivery_failures_name_the_path_and_remain_retryable() {
    let (mut h, image) = board("artifact-approval-delivery-error");
    let node = spawn(&mut h, image);
    let dir = waiting(&mut h, node, "approval-failure");
    assert!(h
        .app
        .answer_agent_approval(Some(&serde_json::to_string(&(node.0, true)).unwrap())));
    let error = finish_delivery(&mut h, node);
    assert!(error.contains("Could not deliver approval"), "{error}");
    assert!(error.contains(dir.to_string_lossy().as_ref()), "{error}");
    assert!(h.app.agent_approval_interactive(node));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(h
        .app
        .answer_agent_approval(Some(&serde_json::to_string(&(node.0, false)).unwrap())));
    assert!(finish_delivery(&mut h, node).starts_with("Decision sent"));
}

#[test]
fn approval_prerequisites_and_stale_requests_are_explained() {
    let (mut h, image) = board("artifact-approval-prerequisites");
    let node = spawn(&mut h, image);
    let detail = serde_json::to_string(&(node.0, true)).unwrap();
    assert!(!h.app.answer_agent_approval(Some(&detail)));
    assert!(h
        .app
        .toasts
        .last()
        .unwrap()
        .0
        .contains("not waiting for approval"));
    waiting(&mut h, node, "old-request");
    h.app.agents.requests.insert(node, "new-request".into());
    assert!(!h.app.answer_agent_approval(Some(&detail)));
    assert!(h.app.toasts.last().unwrap().0.contains("earlier run"));
    h.app.agents.requests.insert(node, "old-request".into());
    h.app.ai.config.workspace_dir = None;
    assert!(!h.app.answer_agent_approval(Some(&detail)));
    assert!(h
        .app
        .toasts
        .last()
        .unwrap()
        .0
        .contains("Set an AI workspace"));
}

#[test]
fn an_old_delivery_cannot_disable_a_new_session_with_the_same_provider_approval_id() {
    let (mut h, image) = board("artifact-approval-stale-delivery");
    let node = spawn(&mut h, image);
    let dir = waiting(&mut h, node, "old-request");
    std::fs::create_dir_all(&dir).unwrap();
    assert!(h
        .app
        .answer_agent_approval(Some(&serde_json::to_string(&(node.0, true)).unwrap())));
    h.app.patch_nodes(&[node], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().session =
            "replacement-source-session".into();
    });
    waiting(&mut h, node, "new-request");
    let description = finish_delivery(&mut h, node);
    assert_eq!(description, "Write the staged dashboard HTML.");
    assert!(h.app.agent_approval_interactive(node));
}
