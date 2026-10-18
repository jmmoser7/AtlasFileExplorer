//! Opt-in production-renderer fixture, paired with the existing SLATE_SHOT harness.
//! It never submits an agent task or saves user profile preferences. Normal
//! provider catalog probes may still run. Proposal
//! states save only a fresh synthetic workbook inside their capture workspace.
use super::*;
use serde_json::json;
impl SlateApp {
    pub(crate) fn presence_review_fixture(&mut self, ctx: &egui::Context) {
        let Some(workspace) = self
            .screenshot
            .as_ref()
            .and_then(|shot| shot.presence_workspace())
            .map(std::path::Path::to_path_buf)
        else {
            return;
        };
        let state = std::env::var("SLATE_PRESENCE_REVIEW_STATE").unwrap_or_default();
        let proposal_fixture = matches!(state.as_str(), "proposal" | "proposal-attached");
        if proposal_fixture {
            self.screenshot.as_mut().unwrap().set_fixture_status(Err(
                "Presence proposal fixture has not opened its requested conversation",
            ));
        }
        if self.frame_no != 2 {
            // These are ongoing fixture actions, not live-provider evidence.
            if self.doc().name != "Collaborators · native render fixture" {
                return;
            }
            let tab = self.tab().id;
            for agent in self.presence.agents.iter_mut().filter(|agent| {
                agent.tab == tab
                    && matches!(
                        agent.profile.name.as_str(),
                        "Dot · render fixture" | "Tess · render fixture"
                    )
            }) {
                agent.last_active = Instant::now();
                let status = if agent.paused && agent.activity == "listening" {
                    "Render fixture · no provider connected · paused · listening"
                } else if agent.paused {
                    "Render fixture · no provider connected · paused"
                } else {
                    "Render fixture · no provider connected"
                };
                if agent.status != status {
                    agent.status = status.into();
                }
                // Render sample bubbles without marking a thread dirty, saving
                // it, or representing these fixtures as provider responses.
                if let Ok(conversation) = self.presence.conversations.edit(&agent.profile.id) {
                    if conversation.turns.is_empty() {
                        for (role, text) in [
                            ("user", "hey hows it going?"),
                            ("assistant", "Hey! Going well—how’s it going with you?"),
                            ("user", "pretty well just testing out the interface"),
                            ("assistant", "Sounds good—happy to be your test conversation. How’s the interface feeling so far?"),
                        ] {
                            conversation.push(role, text, "Render fixture", None, "render-fixture", "");
                        }
                    }
                }
            }
            // Fixed renderer states exercise the real UI without agent task submission
            // or synthetic input. They do not establish interaction acceptance.
            let selected = match state.as_str() {
                "chat" | "proposal" => Some((false, 1.0)),
                "attached25" => Some((true, 0.25)),
                "attached80" => Some((true, 0.8)),
                "attached100" | "proposal-attached" => Some((true, 1.0)),
                "attached200" => Some((true, 2.0)),
                _ => None,
            };
            let marker = egui::Id::new(("presence-render-fixture-state", tab));
            if let Some((attached, zoom)) = selected.filter(|_| {
                self.frame_no >= 3
                    && !ctx.data(|data| data.get_temp::<bool>(marker).unwrap_or(false))
            }) {
                let who = self
                    .presence
                    .agents
                    .iter()
                    .find(|a| a.tab == tab && a.profile.name == "Dot · render fixture")
                    .map(|a| a.profile.id.clone());
                if let Some(who) = who.filter(|id| {
                    self.presence
                        .conversations
                        .get(id)
                        .is_some_and(|c| c.turns.len() >= 4)
                }) {
                    ctx.data_mut(|data| data.insert_temp(marker, true));
                    if attached {
                        self.tab_mut().cam.z = zoom;
                        // Pan the real camera so the whole scaled conversation
                        // is observable. The participant stays at its world point.
                        let position = self
                            .presence
                            .agents
                            .iter()
                            .find(|a| a.profile.id == who)
                            .and_then(|a| a.position(&self.presence.roster));
                        if let Some(position) = position {
                            // Reproduce the observed lower-screen 80% clip;
                            // other zooms keep the enlarged shell fully visible.
                            let y = if state == "attached80" { 420.0 } else { 40.0 };
                            let screen = self.canvas_rect.min + egui::vec2(380.0, y);
                            let before = self.board_xf().s2w(screen);
                            self.tab_mut().cam.offset +=
                                egui::pos2(position.x as f32, position.y as f32) - before;
                        }
                        if let Err(error) = self.presence_address(ctx, &who) {
                            eprintln!("Presence render fixture state failed: {error}");
                            if proposal_fixture {
                                std::process::exit(2);
                            }
                        }
                    } else {
                        self.presence.chrome.open_chat(&who);
                    }
                }
            }
            if proposal_fixture {
                let status = (|| {
                    let agent = self
                        .presence
                        .agents
                        .iter()
                        .find(|a| {
                            a.tab == tab
                                && a.departure.is_none()
                                && a.profile.name == "Dot · render fixture"
                        })
                        .ok_or("Presence proposal fixture collaborator is missing")?;
                    let who = agent.profile.id.as_str();
                    if self.presence.chrome.active_chat() != Some(who)
                        || self.presence.chrome.canvas_chat().is_some()
                            != (state == "proposal-attached")
                    {
                        return Err("Presence proposal fixture conversation is not open in the requested position");
                    }
                    if state == "proposal-attached"
                        && (!agent.paused || agent.activity != "listening")
                    {
                        return Err("Presence proposal fixture is not paused and listening");
                    }
                    if !self
                        .presence
                        .conversations
                        .get(who)
                        .is_some_and(|c| c.turns.len() >= 4)
                        || self.presence_proposal_rows(who).len() != 1
                    {
                        return Err(
                            "Presence proposal fixture history or decision card is not ready",
                        );
                    }
                    Ok(())
                })();
                self.screenshot.as_mut().unwrap().set_fixture_status(status);
            }
            return;
        }
        let previous_tab = self.tab().id;
        let previous_count = self.tabs.len();
        self.new_tab();
        if self.tabs.len() != previous_count + 1
            || self.tab().id == previous_tab
            || self.tab().path.is_some()
            || !self.doc().scene.nodes.is_empty()
        {
            eprintln!(
                "SLATE_SHOT presence fixture failed: a fresh untitled tab could not be created"
            );
            std::process::exit(2);
        }
        self.at_home = false;
        self.presenting = None;
        self.portal_chrome.maximized = None;
        self.doc_mut().name = "Collaborators · native render fixture".into();
        self.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        self.ai.config.workspace_dir = Some(workspace.clone());
        for (command, args) in [
            (
                "board.objects.create_frame",
                json!({"x":-420,"y":-240,"width":840,"height":480,"title":"Visible collaborators"}),
            ),
            (
                "board.objects.create_text",
                json!({"x":-320,"y":-150,"width":300,"height":230,"text":"Project brief\n\nReading uses the small animated book.\nEvery board edit has a visible approach, contact and placement."}),
            ),
            (
                "board.objects.create_shape",
                json!({"x":60,"y":-100,"width":180,"height":140}),
            ),
        ] {
            if let Ok(prepared) = self.prepare_targeted_command(command, &[], &args) {
                self.commit_scene(prepared.cmds);
            }
        }
        let target = self
            .doc()
            .scene
            .nodes
            .iter()
            .find(|n| matches!(n.kind, slate_doc::NodeKind::Text(_)))
            .map(|n| n.id);
        for (name, hat, activity, x, y) in [
            ("Dot · render fixture", "cap", "reading", 25.0, 90.0),
            ("Tess · render fixture", "beanie", "writing", 25.0, -60.0),
        ] {
            let profile = StoredAgent {
                id: atlas_agent::request_id(),
                name: name.into(),
                hat: hat.into(),
                provider: "local".into(),
                retired: false,
            };
            if self.join_presence(profile, None).is_ok() {
                let a = self.presence.agents.last_mut().unwrap();
                a.activity = activity.into();
                a.observed_target = target;
                a.status = "Render fixture · no provider connected".into();
                let session = self.presence.roster.session_mut(&a.motion).unwrap();
                let request = id(&atlas_agent::request_id()).unwrap();
                a.request = request.as_str().into();
                session.begin_request(request.clone()).unwrap();
                session
                    .enqueue(MotionPlan {
                        session_id: a.motion.clone(),
                        request_id: request,
                        action_id: id(&atlas_agent::request_id()).unwrap(),
                        target_nodes: vec![id("render-fixture").unwrap()],
                        start: session.sample().position,
                        end: point(x, y),
                        work_end: None,
                        durations: MotionDurations {
                            travel_ms: 180,
                            grab_ms: 50,
                            work_ms: 150,
                        },
                    })
                    .unwrap();
                for _ in 0..12 {
                    session.advance(std::time::Duration::from_millis(50));
                }
                session.resolve_ready(|_| true);
            }
        }
        if proposal_fixture {
            // Only these two opt-in states need saved workbook identity. A
            // newly reserved directory makes overwriting an existing file
            // impossible; ordinary save authority and encoding stay shared.
            let staged = (|| -> Result<(), String> {
                let directory =
                    workspace.join(format!("proposal-fixture-{}", atlas_agent::request_id()));
                std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
                let path = directory.join("proposal.slate");
                let lease =
                    match super::super::Lease::acquire(&path).map_err(|error| error.to_string())? {
                        super::super::LeaseState::Acquired(lease) => lease,
                        super::super::LeaseState::Held(_) => {
                            return Err("Synthetic workbook is already held".into())
                        }
                    };
                super::super::workbook_save::save_while_owned(self.doc(), &lease, &path)
                    .map_err(|error| error.to_string())?;
                let tab = self.tab_mut();
                tab.path = Some(path);
                tab.lease = Some(lease);
                let (index, who) = self
                    .presence
                    .agents
                    .iter()
                    .enumerate()
                    .find(|(_, a)| {
                        a.tab == self.tab().id && a.profile.name == "Dot · render fixture"
                    })
                    .map(|(index, a)| (index, a.profile.id.clone()))
                    .ok_or("Missing live fixture collaborator")?;
                let node = self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .find(|node| matches!(node.kind, slate_doc::NodeKind::Shape(_)))
                    .ok_or("Missing fixture shape")?;
                let (target, before) = (node.id, node.rect);
                let mut agent = self.presence.agents.remove(index);
                let call = BoardCall {
                    id: atlas_agent::request_id(),
                    session: agent.binding.clone(),
                    request_id: agent.request.clone(),
                    sequence: 0,
                    operation: BoardOperation::Invoke {
                        command: "board.objects.move".into(),
                        targets: vec![target.0.to_string()],
                        arguments: json!({"dx":80,"dy":20}),
                    },
                };
                let admitted = self.presence_call(&mut agent, &call).is_none();
                self.presence.agents.insert(index, agent);
                if !admitted {
                    return Err("Fixture move was not admitted".into());
                }
                // This fixture runs the ordinary bounded initial witness. It
                // never forges an approved endpoint or commits authored edits.
                for _ in 0..160 {
                    self.presence.last_tick =
                        Some(Instant::now() - std::time::Duration::from_millis(50));
                    self.presence_pump(ctx);
                    if self.presence.agents[index].work.is_none() {
                        break;
                    }
                }
                let proposal = self
                    .presence
                    .proposals
                    .iter()
                    .find(|(_, p)| p.call.id == call.id)
                    .map(|(id, _)| id.as_str())
                    .ok_or("Fixture move was not staged with its exact live context")?;
                let pending = self.presence_proposal_rows(&who);
                if pending.len() != 1
                    || pending[0].id != proposal
                    || self.doc().scene.node(target).map(|node| node.rect) != Some(before)
                {
                    return Err("Fixture proposal is missing or changed the authored shape".into());
                }
                Ok(())
            })();
            if let Err(error) = staged {
                eprintln!("SLATE_SHOT presence proposal fixture failed: {error}");
                std::process::exit(2);
            }
        }
        self.tab_mut().cam.offset = egui::Vec2::ZERO;
        self.tab_mut().cam.z = 1.0;
    }
}
