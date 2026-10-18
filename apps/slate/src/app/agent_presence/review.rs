//! Opt-in production-renderer fixture, paired with the existing SLATE_SHOT harness.
//! It never contacts a provider or saves workbook/profile preferences.
use super::*;
use serde_json::json;
impl SlateApp {
    pub(crate) fn presence_review_fixture(&mut self, ctx: &egui::Context) {
        if std::env::var_os("SLATE_SHOT").is_none() {
            return;
        }
        let Some(workspace) = std::env::var_os("SLATE_PRESENCE_REVIEW").map(PathBuf::from) else {
            return;
        };
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
            // Fixed renderer states exercise the real UI without provider work
            // or synthetic input. They do not establish interaction acceptance.
            let state = std::env::var("SLATE_PRESENCE_REVIEW_STATE").unwrap_or_default();
            let selected = match state.as_str() {
                "chat" => Some((false, 1.0)),
                "attached25" => Some((true, 0.25)),
                "attached80" => Some((true, 0.8)),
                "attached100" => Some((true, 1.0)),
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
                            .and_then(|a| self.presence.roster.session(&a.motion))
                            .map(|s| s.sample().position);
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
                        }
                    } else {
                        self.presence.chrome.open_chat(&who);
                    }
                }
            }
            return;
        }
        self.new_tab();
        self.at_home = false;
        self.presenting = None;
        self.portal_chrome.maximized = None;
        self.doc_mut().name = "Collaborators · native render fixture".into();
        self.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        self.ai.config.workspace_dir = Some(workspace);
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
        self.tab_mut().cam.offset = egui::Vec2::ZERO;
        self.tab_mut().cam.z = 1.0;
    }
}
