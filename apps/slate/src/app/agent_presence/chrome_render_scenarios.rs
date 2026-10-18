//! Explicit fixture permissions; none changes a production UI control or grants.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(in super::super) enum Scenario {
    Chrome,
    Activity,
    Catalogue,
    Departure,
    Harness,
}
#[derive(serde::Serialize)]
struct Task {
    request: String,
    prompt: String,
    read: bool,
    terminal: bool,
    dir: PathBuf,
}
#[derive(serde::Serialize)]
pub(super) struct Declarations {
    pub(super) scenario: Scenario,
    tasks: HashMap<String, Task>,
    harnesses: HashMap<String, Vec<String>>,
    omissions: Vec<String>,
}
impl Declarations {
    pub(super) fn new(scenario: Scenario) -> Self {
        Self {
            scenario,
            tasks: HashMap::new(),
            harnesses: HashMap::new(),
            omissions: Vec::new(),
        }
    }
    pub(super) fn assert_agent(&self, a: &Collaborator) {
        let allowed = self.harnesses.get(&a.profile.id);
        assert!(
            a.profile.provider == "local"
                || allowed.is_some_and(|ids| ids.contains(&a.profile.provider))
        );
        if let Some(task) = self.tasks.get(&a.profile.id) {
            assert!(
                a.request == task.request || (task.terminal && a.request.is_empty() && !a.inflight)
            );
            if let Some(read) = &a.read {
                assert!(
                    task.read
                        && read.call.request_id == task.request
                        && read.call.session == a.binding
                );
            }
        } else {
            assert!(a.request.is_empty() && !a.inflight && a.read.is_none());
        }
        for destination in a
            .pending_harness
            .as_ref()
            .map(|p| &p.0)
            .into_iter()
            .chain(a.switch_offer.as_ref())
        {
            assert!(
                allowed.is_some_and(|ids| ids.contains(destination)),
                "undeclared harness transition"
            );
        }
        if let Some((_, request)) = &a.pending_harness {
            assert_eq!(
                self.tasks.get(&a.profile.id).map(|t| &t.request),
                Some(request)
            );
        }
    }
}
impl Fixture {
    /// Synthetic backend identity only; no AgentRequest or provider launcher.
    pub fn declare_task(&mut self, who: &str, prompt: &str, read: bool) -> String {
        assert!(matches!(
            self.declarations.scenario,
            Scenario::Activity | Scenario::Departure | Scenario::Harness
        ));
        wait_for_store(&mut self.h);
        let a = self
            .h
            .app
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == who)
            .unwrap();
        assert_eq!(
            a.profile.provider, "local",
            "never fabricate a running vendor harness"
        );
        let request = atlas_agent::request_id();
        a.request = request.clone();
        a.inflight = true;
        a.had_response = false;
        a.seen_running = false;
        a.reply_from = 0;
        a.snapshot = None;
        a.calls = 0;
        a.sequences = 0;
        self.h
            .app
            .presence
            .roster
            .session_mut(&a.motion)
            .unwrap()
            .begin_request(id(&request).unwrap())
            .unwrap();
        self.h.app.presence.conversations.edit(who).unwrap().push(
            "user",
            prompt,
            "Synthetic request fixture",
            None,
            &request,
            "sent",
        );
        let dir = a.dir.clone();
        self.declarations.tasks.insert(
            who.into(),
            Task {
                request: request.clone(),
                prompt: prompt.into(),
                read,
                terminal: false,
                dir,
            },
        );
        request
    }
    pub fn permit_terminal(&mut self, who: &str) {
        self.declarations
            .tasks
            .get_mut(who)
            .expect("declared synthetic request")
            .terminal = true;
    }
    pub fn declare_harnesses(&mut self, who: &str) {
        assert_eq!(self.declarations.scenario, Scenario::Harness);
        self.declarations.harnesses.insert(
            who.into(),
            vec!["local".into(), "codex".into(), "cursor".into()],
        );
    }
    pub fn synthetic_session(
        &self,
        who: &str,
        status: atlas_agent::AgentStatus,
        reply: &str,
    ) -> AgentSession {
        let task = &self.declarations.tasks[who];
        AgentSession {
            request: task.request.clone(),
            status,
            turns: vec![
                atlas_agent::AgentTurn {
                    role: "user".into(),
                    text: task.prompt.clone(),
                    at: 1,
                },
                atlas_agent::AgentTurn {
                    role: "assistant".into(),
                    text: reply.into(),
                    at: 2,
                },
            ],
            ..Default::default()
        }
    }
    pub fn observe(&mut self, who: &str, session: &AgentSession) {
        assert_eq!(session.request, self.declarations.tasks[who].request);
        crate::app::agent_presence::lifecycle_tests::session_support::publish_synthetic_and_observe(
            &mut self.h,
            who,
            session,
        );
        self.assert_local();
    }
    pub fn observe_handoff(&mut self, who: &str, session: &AgentSession, destination: &str) {
        assert_eq!(self.declarations.scenario, Scenario::Harness);
        assert_eq!(session.request, self.declarations.tasks[who].request);
        let old = self
            .h
            .app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap()
            .binding
            .session_id
            .clone();
        crate::app::agent_presence::lifecycle_tests::session_support::publish_synthetic_and_observe_until(
            &mut self.h,
            who,
            session,
            |h| {
                h.app.presence.agents.iter().any(|a| {
                    a.profile.id == who
                        && a.profile.provider == destination
                        && a.request.is_empty()
                        && a.binding.session_id != old
                })
            },
        );
        self.assert_local();
    }
    pub fn board_avatar_center(&self, who: &str) -> Pos2 {
        let a = self
            .h
            .app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap();
        let p = self
            .h
            .app
            .presence
            .roster
            .session(&a.motion)
            .unwrap()
            .sample()
            .position;
        self.h
            .app
            .board_xf()
            .w2s(egui::pos2(p.x as f32, p.y as f32))
    }
    pub fn finish_scenario(&self) {
        assert_ne!(self.declarations.scenario, Scenario::Chrome);
        for a in &self.h.app.presence.agents {
            assert!(
                !a.dir.join("request.json").exists(),
                "no outgoing provider request is permitted in these scenarios"
            );
        }
        for task in self.declarations.tasks.values() {
            assert!(
                !task.dir.join("request.json").exists(),
                "a departed task cannot hide an outgoing provider request"
            );
        }
        self.write_json("metadata.json", &json!({"status":"assertions-passed",
            "evidence":"actual app software output; synthetic public-session inputs and direct-model catalogues are labeled per frame",
            "scenario":self.declarations,"reducedMotion":self.h.app.settings.presence_reduced_motion,
            "systemReducedMotion":chrome::system_reduced_motion(&self.h.ctx),
            "clock":"existing Fixture 60Hz egui input clock for paint; lifecycle Instant elapsed is never reset or backdated",
            "catalogOverrides":["codex","cursor","ollama","comfy","openai-image","openai-text"],
            "programHealthOverrides":if self.declarations.scenario==Scenario::Harness {vec!["codex","cursor"]}else{vec![]},
            "providerLaunches":0,"providerSends":0,"liveProviderEvidence":false,"phases":self.rows}));
        println!(
            "Retained Presence intent sequence: {}",
            self.directory().display()
        );
    }
    pub fn note_omission(&mut self, reason: &str) {
        self.declarations.omissions.push(reason.into());
    }
}
