use super::*;

fn notice(h: &Harness) -> &str {
    &h.app
        .toasts
        .last()
        .expect("the action must explain its result")
        .0
}

#[test]
fn settings_actions_explain_missing_revisions_and_memory() {
    let (mut h, image) = board("artifact-settings-prerequisites");
    let node = spawn(&mut h, image);
    for (command, expected) in [
        ("portal.artifact.fork", "Apply a completed"),
        ("portal.artifact.memory_disconnect", "No inherited Memory"),
        (
            "portal.artifact.memory_inspect",
            "no completed memory checkpoint",
        ),
        ("portal.artifact.source_open", "no source yet"),
        ("portal.artifact.accept", "no artifact preview to apply"),
        ("portal.artifact.reject", "no artifact preview to discard"),
    ] {
        h.app.toasts.clear();
        assert!(!h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId(command),
            Some(serde_json::json!({"node":node}).to_string())
        ));
        assert!(notice(&h).contains(expected), "{command}: {}", notice(&h));
    }
}

#[test]
fn retry_immediately_replaces_the_failed_state_with_preparing() {
    let (mut h, image) = board("artifact-play-retry-feedback");
    let node = spawn(&mut h, image);
    *h.app.agents.prompt_mut(node) = "Make the supplied pie interactive.".into();
    h.app
        .fail_agent_await(node, "Prior failure sentinel".into());
    assert!(h.app.queue_artifact_generation(node));
    assert!(h.app.artifact_is_busy(node));
    assert!(h.app.agent_failure_reason(node).is_none());
    assert!(h.app.agents.artifact.preparing.contains_key(&node));
}

#[test]
fn play_without_a_workspace_explains_the_prerequisite() {
    let (mut h, image) = board("artifact-play-missing-workspace");
    let node = spawn(&mut h, image);
    h.app.ai.config.workspace_dir = None;
    assert!(!h.app.queue_artifact_generation(node));
    assert!(notice(&h).contains("Set an AI workspace"));
    assert_eq!(
        h.app.agent_failure_reason(node),
        Some("Choose an AI workspace folder so this portal has a place to write the agent link.")
    );
    assert!(matches!(
        h.app.agents.awaiting.get(&node),
        Some(AgentAwait::Failed { actions, .. })
            if actions.iter().any(|action| matches!(action, AgentRecover::PickWorkspace))
    ));
    assert!(!h.app.artifact_is_busy(node));
}

#[test]
fn fresh_eyes_reports_success_and_keeps_the_pinned_content() {
    let (mut h, image) = board("artifact-fresh-eyes-feedback");
    let first = spawn(&mut h, image);
    fixture_revision(&mut h, first, "r1", b"<html><body>chart</body></html>");
    let child = spawn(&mut h, first);
    let content = agent_inputs::artifact_content_parent(&h.app.doc().scene, child);
    assert!(h.app.artifact_disconnect_memory(child));
    assert!(notice(&h).contains("Memory disconnected"));
    assert!(agent_inputs::artifact_memory_source(&h.app.doc().scene, child).is_none());
    assert_eq!(
        agent_inputs::artifact_content_parent(&h.app.doc().scene, child),
        content
    );
}

#[test]
fn preparation_errors_name_the_history_operation_and_path() {
    let (mut h, image) = board("artifact-prepare-error-context");
    let node = spawn(&mut h, image);
    *h.app.agents.prompt_mut(node) = "Make a dashboard.".into();
    assert!(h.app.queue_artifact_generation(node));
    let mut pending = h.app.agents.artifact.preparing.remove(&node).unwrap();
    let blocked = h.base.join("blocked-history-file");
    std::fs::write(&blocked, b"file, not a directory").unwrap();
    pending.plan.history_dir = blocked.clone();
    let error = super::super::generation::prepare(pending.plan)
        .err()
        .unwrap();
    assert!(error.contains("prepare artifact history"), "{error}");
    assert!(
        error.contains(blocked.to_string_lossy().as_ref()),
        "{error}"
    );
}

#[test]
fn fork_waits_for_the_selected_history_revision_instead_of_using_the_old_binding() {
    let (mut h, image) = board("artifact-fork-revision-loading");
    let node = spawn(&mut h, image);
    fixture_revision(&mut h, node, "r1", b"<html><body>chart</body></html>");
    h.app
        .agents
        .artifact
        .picks
        .insert(node, "loading-r0".into());
    let count = h.app.doc().scene.nodes.len();
    let depth = h.app.tab().journal.undo_depth();
    for (command, parameters) in [
        ("portal.artifact.fork", serde_json::json!({"node":node})),
        (
            "portal.agent.spawn",
            serde_json::json!({"source":node.0,"kind":"artifact"}),
        ),
    ] {
        h.app.toasts.clear();
        assert!(!h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId(command),
            Some(parameters.to_string()),
        ));
        assert_eq!(h.app.doc().scene.nodes.len(), count);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert!(notice(&h).contains("selected artifact revision to load"));
    }
}

#[test]
fn clearing_the_focused_prompt_never_submits_the_previous_instruction() {
    let (mut h, image) = board("artifact-cleared-prompt");
    let node = spawn(&mut h, image);
    h.app.patch_nodes(&[node], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().instruction = "old prompt sentinel".into();
    });
    h.app.agents.prompt_mut(node).clear();
    h.app.track_flow_prompt_focus(node, true);
    assert!(!h.app.queue_artifact_generation(node));
    assert!(notice(&h).contains("Type feedback or connect a prompt"));
    assert!(!h.app.artifact_is_busy(node));
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn an_await_failure_stops_artifact_work_with_or_without_a_matching_session() {
    for has_session in [false, true] {
        let (mut h, image) = board(if has_session {
            "artifact-await-failure-matching-session"
        } else {
            "artifact-await-failure-no-session"
        });
        let node = spawn(&mut h, image);
        let prepared = prepare_fixture_run(&mut h, node);
        if has_session {
            let session: AgentSession = serde_json::from_value(serde_json::json!({
                "provider":"codex", "status":"thinking", "request":prepared.plan.request.id
            }))
            .unwrap();
            h.app.agents.sessions.insert(node, session.into());
        }
        h.app.agents.artifact.running.insert(
            node,
            Running {
                prepared,
                publishing: false,
            },
        );
        h.app
            .fail_agent_await(node, "Harness startup failed sentinel".into());
        h.app.pump_artifact_generations(&h.ctx);
        assert!(!h.app.artifact_is_busy(node));
        assert!(h
            .app
            .agent_failure_reason(node)
            .unwrap()
            .contains("startup failed sentinel"));
        assert!(notice(&h).contains("startup failed sentinel"));
    }
}

#[test]
fn preparation_completion_keeps_newer_editor_feedback_and_the_submitted_snapshot() {
    let (mut h, image) = board("artifact-feedback-during-preparation");
    let node = spawn(&mut h, image);
    let prepared = prepare_fixture_run(&mut h, node);
    let plan = prepared.plan.clone();
    let submitted = plan.request.prompt.clone();
    h.app.agents.artifact.preparing.insert(
        node,
        Preparing {
            plan: plan.clone(),
            inputs: crate::app::board_agent::input_prep::InputPreparation {
                inputs: plan.request.inputs.clone(),
                replacements: vec![],
                bake_dependencies: vec![],
            },
            started: true,
        },
    );
    *h.app.agents.prompt_mut(node) = "New feedback typed while preparation runs".into();
    h.app.track_flow_prompt_focus(node, true);
    h.app
        .artifact_work_tx()
        .send(WorkDone::Prepared(
            plan.tab,
            node,
            plan.request.id,
            Ok(prepared),
        ))
        .unwrap();
    h.app.pump_artifact_generations(&h.ctx);
    let instruction = slate_doc::agent_chat::agent(h.app.doc().scene.node(node).unwrap())
        .unwrap()
        .instruction
        .clone();
    h.app.sync_instruction(node, &instruction);
    assert_eq!(
        h.app.agents.prompt(node),
        Some("New feedback typed while preparation runs")
    );
    assert_eq!(instruction, "New feedback typed while preparation runs");
    assert_eq!(h.app.agents.dispatched.last().unwrap().1.prompt, submitted);
    assert_eq!(
        h.app
            .agents
            .artifact
            .running
            .get(&node)
            .unwrap()
            .prepared
            .plan
            .request
            .prompt,
        submitted
    );
}

#[test]
fn stopping_preparation_succeeds_without_sending_a_stop_to_an_unstarted_harness() {
    let (mut h, image) = board("artifact-stop-preparation");
    let node = spawn(&mut h, image);
    *h.app.agents.prompt_mut(node) = "Make the supplied pie interactive".into();
    assert!(h.app.queue_artifact_generation(node));
    h.app.board_sel = [node].into_iter().collect();
    assert!(h.app.stop_selected_agent());
    assert!(!h.app.artifact_is_busy(node));
    assert!(notice(&h).contains("preparation canceled"));
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn idle_artifact_height_reserves_no_status_row_but_preparation_does() {
    let (mut h, image) = board("artifact-idle-height-status");
    let node = spawn(&mut h, image);
    *h.app.agents.prompt_mut(node) = "Make a dashboard".into();
    let _ = h.ctx.run(egui::RawInput::default(), |_| {});
    let painter = h.ctx.layer_painter(egui::LayerId::background());
    let idle = h.app.artifact_editor_height(&painter, node, 300.0);
    assert!(h.app.queue_artifact_generation(node));
    let preparing = h.app.artifact_editor_height(&painter, node, 300.0);
    assert!(
        preparing > idle,
        "only actionable work reserves the status row"
    );
}

#[test]
fn pending_model_catalog_preserves_the_authored_label_and_measured_prompt_height() {
    for provider in ["codex", "cursor"] {
        let (mut h, image) = board(&format!("artifact-pending-model-{provider}"));
        let node = spawn(&mut h, image);
        h.app.patch_nodes(&[node], |node| {
            let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
            agent.provider = provider.into();
            agent.model = Some("pending-a".into());
        });
        *h.app.agents.prompt_mut(node) = "Make a dashboard".into();
        let theme = atlas_shell::theme::Palette::dark();
        for advertised in [false, true] {
            h.app.agents.models.insert(
                provider.into(),
                if advertised {
                    vec![atlas_ai::agent::AgentModel {
                        id: "pending-a".into(),
                        name: "Ready model".into(),
                        ..Default::default()
                    }]
                } else {
                    vec![]
                },
            );
            let name = if advertised {
                "Ready model"
            } else {
                "pending-a"
            };
            let label = format!(
                "{} · {name}",
                if provider == "codex" {
                    "Codex"
                } else {
                    "Cursor"
                }
            );
            for width in [190.0, 220.0, 360.0] {
                let output = h.ctx.clone().run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(1200.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let height = h.app.artifact_editor_height(ui.painter(), node, width);
                            let rect = Rect::from_min_size(
                                Pos2::new(50.0, 50.0),
                                egui::vec2(width, height),
                            );
                            h.app.artifact_editor_body(ui, rect, node, 1.0, theme);
                        });
                    },
                );
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == label)
                }), "authored selection is painted: {label}, width={width}");
                let face = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(face)
                            if face.fill == theme.bg && (face.stroke.width - 0.7).abs() < 0.001 =>
                        {
                            Some(face.rect)
                        }
                        _ => None,
                    })
                    .expect("the prompt face is painted");
                assert_eq!(
                    face.height(),
                    40.0,
                    "height and paint agree while the model catalog resolves"
                );
                let agent =
                    slate_doc::agent_chat::agent(h.app.doc().scene.node(node).unwrap()).unwrap();
                assert_eq!(agent.provider, provider);
                assert_eq!(agent.model.as_deref(), Some("pending-a"));
            }
        }
    }
}
