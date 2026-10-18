use super::*;

fn recover_request_history(request: &AgentRequest, session: &AgentSession) -> Option<AgentRequest> {
    if request.oneshot || request.image.is_some() || !request.history.is_empty() {
        return None;
    }
    let mut replay = request.clone();
    replay.history = session
        .turns
        .iter()
        .filter(|turn| matches!(turn.role.as_str(), "user" | "assistant"))
        .cloned()
        .collect();
    (!replay.history.is_empty()).then_some(replay)
}

fn completed_before_interrupt(status: Option<&str>, error: Option<&str>) -> bool {
    status == Some("completed")
        && error
            .and_then(|error| serde_json::from_str::<Value>(error).ok())
            .is_some_and(|error| {
                error["code"] == -32600 && error["message"] == "no active turn to interrupt"
            })
}

impl Client {
    pub fn run(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        cancel: &AtomicBool,
        changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        self.run_with_steering(request, session, cancel, None, changed)
    }

    /// Worker entry point used by Slate's bounded steering channel.
    pub fn run_with_steering(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        cancel: &AtomicBool,
        steering: Option<&std::sync::mpsc::Receiver<SteerCommand>>,
        mut changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        let replay_history = self.replay_visible_history;
        self.replay_visible_history = false;
        let request_for_thread = if replay_history {
            if request.history.is_empty() {
                recover_request_history(request, session)
            } else {
                Some(request.clone())
            }
        } else if !request.history.is_empty() {
            let mut current = request.clone();
            current.history.clear();
            Some(current)
        } else {
            None
        };
        let request = request_for_thread.as_ref().unwrap_or(request);
        if cancel.load(Ordering::Relaxed) {
            return Err("Codex request cancelled before submission.".into());
        }
        session.request = request.id.clone();
        session.activity.clear();
        self.prepare_board_session(request)?;
        // A text block or picture run is one-shot: a fresh thread, so earlier
        // runs never ride along as context.
        if request.oneshot || request.image.is_some() {
            let response = self.rpc("thread/start", thread_params(&self.cwd.clone(), None))?;
            self.thread = response
                .pointer("/thread/id")
                .and_then(Value::as_str)
                .ok_or("Codex did not return a conversation id")?
                .into();
            self.permissions = TurnPermissions::default();
            self.permissions.own = Permissions::from_thread_response(&response);
        }
        // An image run asks the built-in image tool for pictures and collects
        // them from Codex's generated-images folder as they appear.
        let mut pictures = match (&request.image, &self.image_dir) {
            (Some(_), Some(dir)) => Some(Pictures::new(&self.thread, dir.clone())),
            (Some(_), None) => return Err("This image run has no output folder.".into()),
            _ => None,
        };
        let image_inputs = images::image_inputs(request);
        let text = if pictures.is_some() {
            images::image_turn_with_inputs(request, &image_inputs)
        } else if request.oneshot {
            oneshot_turn(request)
        } else {
            request.input_text_in(self.approval_dir.as_deref())
        };
        let mut input = vec![json!({"type":"text","text":text})];
        let attached: Vec<&str> = if pictures.is_some() {
            image_inputs.iter().map(|(_, path)| *path).collect()
        } else {
            request
                .inputs
                .wired
                .iter()
                .flat_map(|v| v.images.iter().map(String::as_str))
                .collect()
        };
        for path in attached {
            input.push(json!({"type":"localImage","path":path}));
        }
        let filesystem = filesystem_policy(request, self.full_access);
        let restore = self.permissions.next(filesystem, self.full_access);
        let mut params = turn_params(
            &self.thread,
            input,
            &request.id,
            request.model.as_deref(),
            filesystem,
            self.full_access,
            restore.as_ref(),
        );
        self.apply_model_parameters(request, &mut params)?;
        if cancel.load(Ordering::Relaxed) {
            return Err("Codex request cancelled before submission.".into());
        }
        let result = self.rpc("turn/start", params)?;
        let turn = result
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or("Missing Codex turn id")?
            .to_string();
        session.provider = "codex".into();
        session.conversation = self.thread.clone();
        session.status = AgentStatus::Thinking;
        session.turns.push(AgentTurn {
            role: "user".into(),
            text: request.prompt.clone(),
            at: request.at,
        });
        let artifact_turn = session.turns.len();
        let mut messages = MessageStream::new(session.turns.len(), request.at);
        let mut interrupted = false;
        let mut interruption_error = None;
        let mut approvals: VecDeque<Value> = VecDeque::new();
        let mut board_tools = board_tools::PendingCalls::default();
        let mut last = Instant::now() - Duration::from_secs(1);
        let deadline = capped_deadline(self.deadline, Duration::from_secs(1800));
        changed(session);
        loop {
            if Instant::now() >= deadline {
                return Err("Codex timed out. Open Codex to inspect the conversation.".into());
            }
            board_tools.poll(self, request, cancel)?;
            if let (Some(event), Some(dir)) = (approvals.front(), self.approval_dir.as_ref()) {
                let path = dir.join("approval.json");
                let response = std::fs::read(&path)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
                let key = format!("{}:{}", request.id, event["id"]);
                if let Some(response) = response.filter(|r| r["id"].as_str() == Some(&key)) {
                    self.write(json!({"id":event["id"],"result":{"decision":if response["allow"]==true {"accept"} else {"decline"}}}))?;
                    let _ = std::fs::remove_file(path);
                    approvals.pop_front();
                    session.approval = None;
                    changed(session);
                }
            }
            if session.approval.is_none() {
                if let Some(event) = approvals.front() {
                    let p = &event["params"];
                    session.approval = Some(atlas_agent::AgentApproval {
                        id: format!("{}:{}", request.id, event["id"]),
                        description: format!(
                            "{}\n{}",
                            p["command"].as_str().unwrap_or("File changes"),
                            p["reason"]
                                .as_str()
                                .unwrap_or("The provider requests permission for this action.")
                        ),
                    });
                    changed(session);
                }
            }
            if cancel.load(Ordering::Relaxed) && !interrupted {
                if let Err(error) = self.rpc(
                    "turn/interrupt",
                    json!({"threadId":self.thread,"turnId":turn}),
                ) {
                    interruption_error = Some(error);
                }
                interrupted = true;
            }
            if let Some(steering) = steering {
                for command in steering.try_iter().take(16) {
                    let accepted = if command.expected_request_id != request.id {
                        Err("Active request changed before delivery".to_string())
                    } else if cancel.load(Ordering::Relaxed) {
                        Err("The response is stopping".to_string())
                    } else {
                        self.steer(&turn, &command.text).map(|_| ())
                    };
                    let succeeded = accepted.is_ok();
                    if succeeded {
                        messages.push_user(session, &command.text);
                    }
                    let status_text = match &accepted {
                        Ok(()) => "Steering message accepted".to_string(),
                        Err(reason) => format!(
                            "Steering failed: {}",
                            reason.chars().take(160).collect::<String>()
                        ),
                    };
                    session.record_activity(AgentActivity {
                        id: format!(
                            "codex-steer:{}:{}",
                            command.id,
                            if succeeded { "accepted" } else { "error" }
                        ),
                        request: request.id.clone(),
                        kind: AgentActivityKind::Commentary,
                        text: status_text,
                        at: now(),
                        semantics: Some(AgentActivitySemantics::new(if succeeded {
                            AgentActivityAction::Chat
                        } else {
                            AgentActivityAction::Error
                        })),
                    });
                    changed(session);
                }
            }
            if let Some(pictures) = pictures.as_mut() {
                if pictures.collect(request, session)? {
                    changed(session);
                }
            }
            let event = if let Some(e) = self.pending.pop_front() {
                e
            } else {
                match self.events.recv_timeout(Duration::from_millis(100)) {
                    Ok(v) => v?,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => return Err("Codex closed the connection.".into()),
                }
            };
            if event.get("id").is_some() && event.get("method").is_some() {
                if event["method"] == "item/tool/call" && request.collaborator.is_some() {
                    board_tools.start(self, request, &turn, event)?;
                } else if matches!(
                    event["method"].as_str(),
                    Some(
                        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
                    )
                ) && self.approval_dir.is_some()
                {
                    approvals.push_back(event);
                } else {
                    self.decline(&event)?;
                }
                continue;
            }
            let p = &event["params"];
            if p.get("threadId")
                .and_then(Value::as_str)
                .is_some_and(|id| id != self.thread)
            {
                continue;
            }
            if p.get("turnId")
                .and_then(Value::as_str)
                .is_some_and(|id| id != turn)
            {
                continue;
            }
            if event["method"] == "item/completed" {
                collect_artifacts(session, &p["item"], artifact_turn, &turn);
            }
            let method = event["method"].as_str().unwrap_or("");
            session::collect_activity(method, p, session);
            messages.apply(method, p, session);
            match method {
                "turn/completed"
                    if p.pointer("/turn/id").and_then(Value::as_str) == Some(turn.as_str()) =>
                {
                    session.status = if p.pointer("/turn/status").and_then(Value::as_str)
                        == Some("failed")
                    {
                        AgentStatus::Error(
                            p.pointer("/turn/error/message")
                                .and_then(Value::as_str)
                                .unwrap_or("Codex turn failed")
                                .into(),
                        )
                    } else if completed_before_interrupt(
                        p.pointer("/turn/status").and_then(Value::as_str),
                        interruption_error.as_deref(),
                    ) {
                        // The completion belongs to this turn. A late Stop cannot
                        // interrupt it, and must not turn a finished reply into an error.
                        AgentStatus::Idle
                    } else if interrupted
                        || p.pointer("/turn/status").and_then(Value::as_str) == Some("interrupted")
                    {
                        AgentStatus::Error(
                            interruption_error
                                .take()
                                .unwrap_or_else(|| "Response stopped.".into()),
                        )
                    } else {
                        AgentStatus::Idle
                    };
                    if let Some(pictures) = pictures.as_mut() {
                        pictures.settle();
                        pictures.collect(request, session)?;
                        if pictures.found == 0 && session.status == AgentStatus::Idle {
                            session.status = AgentStatus::Error(
                                "Codex finished without an image. Check that image generation is enabled for your ChatGPT sign-in.".into(),
                            );
                        }
                    }
                    session.approval = None;
                    session.updated_at = now();
                    changed(session);
                    return Ok(());
                }
                _ => {}
            }
            if last.elapsed() >= Duration::from_millis(100) {
                session.updated_at = now();
                changed(session);
                last = Instant::now();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_a_completed_turn_resolves_the_known_inactive_interrupt_race() {
        let inactive = r#"{"code":-32600,"message":"no active turn to interrupt"}"#;
        assert!(completed_before_interrupt(
            Some("completed"),
            Some(inactive)
        ));
        for status in [None, Some("failed"), Some("interrupted")] {
            assert!(!completed_before_interrupt(status, Some(inactive)));
        }
        for error in [
            None,
            Some("transport error"),
            Some(r#"{"code":-32600,"message":"no rollout found for thread id x"}"#),
        ] {
            assert!(!completed_before_interrupt(Some("completed"), error));
        }
    }
    #[test]
    fn a_fresh_thread_replays_the_visible_history_checkpoint_once() {
        let request = AgentRequest {
            id: "next".into(),
            prompt: "Continue".into(),
            ..Default::default()
        };
        let session = AgentSession {
            turns: vec![
                AgentTurn {
                    role: "user".into(),
                    text: "First question".into(),
                    at: 1,
                },
                AgentTurn {
                    role: "assistant".into(),
                    text: "First answer".into(),
                    at: 2,
                },
            ],
            ..Default::default()
        };
        let replay = recover_request_history(&request, &session).unwrap();
        assert_eq!(replay.history, session.turns);
        let text = replay.prompt_with_history();
        assert_eq!(text.matches("Prior conversation checkpoint").count(), 1);
        assert_eq!(text.matches("First question").count(), 1);
        assert_eq!(text.matches("First answer").count(), 1);
    }
}
