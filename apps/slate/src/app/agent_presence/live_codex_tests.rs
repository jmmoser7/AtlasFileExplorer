//! Opt-in paid/provider validation; separate from every synthetic render family.
//! Uses the real Presence adapter. Never records raw RPC, account data or keys.
use super::*;
use crate::app::tests::{capture_frame, review_shot, FrameRaster, Harness};
use serde_json::{json, Value};
use slate_doc::scene::{CmdAuthor, Node, Scene};
use std::{collections::BTreeMap, panic::AssertUnwindSafe, time::Duration};
const PROBE_FILL: [u8; 4] = [17, 199, 113, 255];
#[test]
#[ignore = "requires explicit SLATE_LIVE_CODEX=1 and installed, signed-in Codex"]
fn installed_codex_reads_proposes_and_visibly_applies_one_scoped_move() {
    assert_eq!(
        std::env::var("SLATE_LIVE_CODEX").as_deref(),
        Ok("1"),
        "live provider requests require a separate explicit opt-in"
    );
    let mut probe = Probe::new();
    let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| probe.run()));
    // Revoke host authority and use the normal cancellation path even on failure.
    // CodexLink exposes neither PID nor join, so this is not proof of process exit.
    let cleanup = probe.h.app.stop_presence(&probe.who, true);
    probe.write_receipt(outcome.is_ok() && cleanup.is_ok(), Some(&cleanup));
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
    assert!(cleanup.is_ok(), "normal cleanup failed: {cleanup:?}");
}
struct Probe {
    h: Harness,
    who: String,
    target: slate_doc::NodeId,
    original: Node,
    scene: Scene,
    depth: usize,
    grant: CollaboratorGrant,
    case: String,
    output: PathBuf,
    raster: FrameRaster,
    started: Instant,
    request: String,
    reads: BTreeMap<String, BoardCall>,
    commands: BTreeMap<String, BoardCall>,
    snapshots: Vec<AgentSession>,
    samples: Vec<Value>,
    frames: Vec<String>,
    proposal: Option<slate_doc::Proposal>,
    committed: Option<Node>,
    undo_verified: bool,
}
impl Probe {
    fn new() -> Self {
        assert!(atlas_ai::runtime::codex_executable().is_some());
        let mut h = Harness::new("explicit_live_codex");
        h.app.ensure_work_tab();
        h.app.at_home = false;
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.settings.recovery.enabled = false;
        h.app.settings.presence_reduced_motion = false;
        h.app.tab_mut().cam.z = 1.0;
        h.app.tab_mut().cam.offset = egui::Vec2::ZERO;
        let workspace = h.base.join("ai-workspace");
        std::fs::create_dir(&workspace).unwrap();
        h.app.ai.config.workspace_dir = Some(workspace);
        let workbook = h.base.join("live-provider-test.slate");
        h.app.tab_mut().path = Some(workbook.clone());
        let prepared = h
            .app
            .prepare_targeted_command(
                "board.objects.create_shape",
                &[],
                &json!({"x":320,"y":220,"width":100,"height":70}),
            )
            .unwrap();
        h.app.commit_scene(prepared.cmds);
        let target = h.app.doc().scene.nodes[0].id;
        h.app.patch_nodes(&[target], |node| {
            let slate_doc::scene::NodeKind::Shape(shape) = &mut node.kind else {
                unreachable!()
            };
            shape.fill = Some(slate_doc::scene::Rgba(PROBE_FILL));
            shape.stroke = slate_doc::scene::Stroke::none();
        });
        h.app.board_sel.clear();
        h.app.doc().save_to(&workbook).unwrap();
        assert_eq!(
            slate_doc::SlateDoc::load_from(&workbook).unwrap().scene,
            h.app.doc().scene
        );
        let original = h.app.doc().scene.nodes[0].clone();
        let scene = h.app.doc().scene.clone();
        let depth = h.app.tab().journal.undo_depth();
        h.app.presence.conversations = conversation_store::ConversationStore::at_directory(
            h.base.join("presence-conversations"),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while h.app.presence_history_status().is_some() {
            h.app.presence.conversations.poll();
            assert!(
                Instant::now() < deadline,
                "test conversation store did not load"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let who = atlas_agent::request_id();
        h.app
            .join_presence(
                StoredAgent {
                    id: who.clone(),
                    name: "Live validation agent".into(),
                    hat: "cap".into(),
                    provider: "codex".into(),
                    retired: false,
                },
                None,
            )
            .unwrap();
        let grant = h.app.presence.agents[0].grant.as_mut().unwrap();
        grant.readable_board = false;
        grant.readable_nodes = vec![original.id.0.to_string()];
        grant.readable_sources.clear();
        grant.commands = vec!["board.objects.move".into()];
        grant.conversation_ids.clear();
        grant.helper_limit = 0;
        grant.max_actions = 8;
        grant.mutation_autonomy = false;
        grant.validate().unwrap();
        let grant = grant.clone();
        let case = format!("presence-live-codex-{}", atlas_agent::request_id());
        let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/review")
            .join(&case);
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::create_dir(&output).expect("exclusive live-probe evidence directory");
        println!("Live Codex evidence: {}", output.display());
        Self {
            h,
            who,
            target: original.id,
            original,
            scene,
            depth,
            grant,
            case,
            output,
            raster: FrameRaster::new(1440, 900),
            started: Instant::now(),
            request: String::new(),
            reads: BTreeMap::new(),
            commands: BTreeMap::new(),
            snapshots: Vec::new(),
            samples: Vec::new(),
            frames: Vec::new(),
            proposal: None,
            committed: None,
            undo_verified: false,
        }
    }
    fn run(&mut self) {
        let choice = self.h.app.presence_model_choice(&self.who, "codex");
        assert!(choice.model.is_none() && choice.parameters.is_empty());
        let prompt = format!(
            "This is an explicitly authorized isolated Slate integration test. \
             Use only the Slate board tools; do not use shell, filesystem, network, \
             subagents or other tools. First call slate_board_read for node {} \
             with offset 0 and limit 1. Read the returned move command schema. \
             Then call slate_board_command exactly once: command board.objects.move, \
             targets [\"{}\"], arguments {{\"dx\":24,\"dy\":12}}. \
             The change must remain a proposal for approval, never claim it is applied. \
             After the tool returns, briefly report that you read the node and proposed \
             that translation, then end the turn. Do not wait for human approval, \
             retry the command, or make any other changes.",
            self.target.0, self.target.0
        );
        self.h.app.presence_send_chat(&self.who, &prompt).unwrap();
        self.request = self.h.app.presence.agents[0].request.clone();
        assert!(!self.request.is_empty());
        self.write_receipt(false, None);
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            self.step("provider");
            assert_eq!(
                self.h.app.doc().scene,
                self.scene,
                "proposal cannot mutate scene"
            );
            assert_eq!(self.h.app.tab().journal.undo_depth(), self.depth);
            let a = &self.h.app.presence.agents[0];
            assert!(
                !matches!(a.activity.as_str(), "error" | "offline" | "blocked"),
                "{}",
                a.status
            );
            if !a.inflight && a.work.is_none() && a.read.is_none() {
                break;
            }
            assert!(Instant::now() < deadline, "live provider deadline elapsed");
            std::thread::sleep(Duration::from_millis(20));
        }
        self.validate_provider();
        let pending = self.h.app.pending_stage_proposals();
        assert_eq!(pending.len(), 1, "exactly one proposal required");
        self.proposal = Some(pending[0].clone());
        let proposal = self.proposal.as_ref().unwrap().clone();
        let context = &self.h.app.presence.proposals[&proposal.id];
        assert_eq!(context.participant, self.who);
        assert_eq!(context.call.request_id, self.request);
        assert_eq!(context.prepared.command, "board.objects.move");
        assert_eq!(context.prepared.targets, vec![self.target]);
        let expected = self.expected_node();
        assert_eq!(context.previews[&self.target], expected);
        assert!(context.approval.is_none());
        assert!(self.h.app.presence_preview(self.target).is_none());
        self.shot("staged");
        self.accept(&proposal.id);
        assert_eq!(self.h.app.doc().scene, self.scene);
        assert_eq!(self.h.app.tab().journal.undo_depth(), self.depth);
        let work = self.h.app.presence.agents[0].work.as_ref().unwrap();
        assert_eq!(
            work.purpose,
            witness::WitnessPurpose::ApprovedProposal(proposal.id.clone())
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.h.app.presence.agents[0].work.is_some() {
            self.step("approved");
            if self.h.app.presence.agents[0].work.is_some() {
                assert_eq!(self.h.app.doc().scene, self.scene);
                assert_eq!(self.h.app.tab().journal.undo_depth(), self.depth);
            }
            assert!(
                Instant::now() < deadline,
                "approved motion deadline elapsed"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            self.samples.len() >= 3,
            "at least three distinct opaque motion samples"
        );
        assert!(self
            .samples
            .windows(2)
            .all(|pair| pair[0]["node"]["rect"]["x"].as_f64().unwrap()
                < pair[1]["node"]["rect"]["x"].as_f64().unwrap()));
        assert!(self
            .samples
            .windows(2)
            .all(|pair| pair[0]["leading_column"].as_i64().unwrap()
                < pair[1]["leading_column"].as_i64().unwrap()));
        assert_eq!(self.h.app.doc().scene.nodes, vec![expected.clone()]);
        assert_eq!(self.h.app.tab().journal.undo_depth(), self.depth + 1);
        assert_eq!(
            self.h.app.tab().journal.last_author(),
            Some(&CmdAuthor::Agent("Live validation agent".into()))
        );
        assert_eq!(
            slate_doc::stage::read_result(
                self.h.app.ai.config.valid_workspace().unwrap(),
                &proposal.id
            )
            .unwrap(),
            Some(slate_doc::stage::ProposalResult::Accepted)
        );
        self.committed = Some(expected);
        self.shot("committed");
        self.accept(&proposal.id);
        assert_eq!(self.h.app.tab().journal.undo_depth(), self.depth + 1);
        assert_eq!(
            self.h.app.doc().scene.nodes,
            vec![self.committed.clone().unwrap()]
        );
        let tab = self.h.app.tab_mut();
        assert!(tab.journal.undo(&mut tab.doc.scene));
        assert_eq!(self.h.app.doc().scene, self.scene);
        self.undo_verified = true;
        self.shot("undone");
    }
    fn expected_node(&self) -> Node {
        let mut node = self.original.clone();
        node.rect = node.rect.translated(24.0, 12.0);
        node
    }
    fn accept(&mut self, proposal: &str) {
        self.h
            .app
            .apply_presence_action(
                &self.h.ctx,
                chrome::PresenceAction::AcceptProposal {
                    participant: self.who.clone(),
                    id: proposal.into(),
                },
            )
            .unwrap();
    }

    fn step(&mut self, phase: &str) {
        let out = capture_frame(&mut self.h, &mut self.raster, |_| {});
        let a = &self.h.app.presence.agents[0];
        assert_eq!(a.grant.as_ref(), Some(&self.grant));
        if let Some(read) = &a.read {
            self.reads.insert(read.call.id.clone(), read.call.clone());
        }
        if let Some(work) = &a.work {
            self.commands
                .insert(work.call.id.clone(), work.call.clone());
        }
        if let Some(snapshot) = &a.snapshot {
            if self.snapshots.last() != Some(snapshot.as_ref()) && self.snapshots.len() < 64 {
                self.snapshots.push(snapshot.as_ref().clone());
            }
        }
        let mut save = None;
        let mut painted = None;
        if phase == "approved" {
            if let Some(node) = self.h.app.presence_preview(self.target) {
                if node.rect.x > self.original.rect.x && node.rect.x < self.original.rect.x + 24.0 {
                    assert_eq!(
                        node.opacity, self.original.opacity,
                        "approved preview is opaque"
                    );
                    assert_eq!(node.id, self.target);
                    assert_eq!(node.kind, self.original.kind);
                    assert_eq!(
                        (node.rect.w, node.rect.h),
                        (self.original.rect.w, self.original.rect.h)
                    );
                    assert!(
                        ((node.rect.y - self.original.rect.y)
                            - (node.rect.x - self.original.rect.x) * 0.5)
                            .abs()
                            < 0.001
                    );
                    let progress = (node.rect.x - self.original.rect.x) / 24.0;
                    let slot = if progress < 0.35 {
                        0
                    } else if progress < 0.7 {
                        1
                    } else {
                        2
                    };
                    let leading_column =
                        self.h.app.board_xf().rect_w2s(node.rect).right().floor() as i64;
                    let sample = json!({"elapsed_ms":self.started.elapsed().as_millis(),
                        "node":node, "leading_column":leading_column});
                    let fresh = self
                        .samples
                        .last()
                        .is_none_or(|old| old["node"] != sample["node"]);
                    if fresh && (0.12..0.94).contains(&progress) && slot == self.samples.len() {
                        self.samples.push(sample);
                        save = Some(format!("approved-{}", self.samples.len()));
                        painted = Some(node.clone());
                    }
                }
            }
        } else if a.read.is_some() && !self.frames.iter().any(|name| name == "reading") {
            save = Some("reading".into());
        } else if a.work.is_some() && !self.frames.iter().any(|name| name == "proposal-preview") {
            save = Some("proposal-preview".into());
        }
        if let Some(name) = save {
            assert_eq!(self.h.app.brush_tiles.last.pending_jobs, 0);
            review_shot(&mut self.h, &mut self.raster, out, &self.case, &name);
            if let Some(node) = painted {
                self.check_painted_edge(&node);
            }
            self.frames.push(name);
        }
    }

    fn shot(&mut self, name: &str) {
        let out = capture_frame(&mut self.h, &mut self.raster, |_| {});
        assert_eq!(self.h.app.brush_tiles.last.pending_jobs, 0);
        review_shot(&mut self.h, &mut self.raster, out, &self.case, name);
        self.frames.push(name.into());
    }

    fn check_painted_edge(&self, node: &Node) {
        // Bracket the moving edge and reject an opaque stale original copy.
        for (x, y, filled) in [
            (
                node.rect.x + node.rect.w - 1.5,
                node.rect.y + node.rect.h * 0.8,
                true,
            ),
            (
                node.rect.x + node.rect.w + 1.5,
                node.rect.y + node.rect.h * 0.8,
                false,
            ),
            (
                self.original.rect.x + 1.5,
                self.original.rect.y + self.original.rect.h * 0.8,
                false,
            ),
        ] {
            let point = self.h.app.board_xf().w2s(egui::pos2(x, y));
            assert!(self.h.app.canvas_rect.contains(point));
            let pixel = self.raster.px[point.y as usize * self.raster.w + point.x as usize];
            let matches = pixel[..3]
                .iter()
                .zip(PROBE_FILL)
                .all(|(value, expected)| (value * 255.0 - f32::from(expected)).abs() < 3.0);
            assert_eq!(
                matches, filled,
                "actual opaque paint must follow the moving edge"
            );
        }
    }

    fn validate_provider(&mut self) {
        let a = &self.h.app.presence.agents[0];
        assert!(a.runtime.is_some() && a.cursor.is_none());
        assert!(a.had_response && a.seen_running);
        assert!(
            !self.reads.is_empty(),
            "observe actual provider read admission"
        );
        for call in self.reads.values() {
            assert_eq!(call.session, a.binding);
            assert_eq!(call.request_id, self.request);
            assert!(
                matches!(&call.operation, BoardOperation::Read { node_ids, .. } if node_ids == &[self.target.0.to_string()])
            );
        }
        assert_eq!(
            self.commands.len(),
            1,
            "exactly one actual provider command"
        );
        let command = self.commands.values().next().unwrap();
        assert_eq!(command.session, a.binding);
        assert_eq!(command.request_id, self.request);
        assert!(
            matches!(&command.operation, BoardOperation::Invoke { command, targets, arguments }
            if command == "board.objects.move" && targets == &[self.target.0.to_string()]
            && arguments.as_object().is_some_and(|args| args.len() == 2)
            && arguments["dx"].as_f64() == Some(24.0)
            && arguments["dy"].as_f64() == Some(12.0))
        );
        let session: AgentSession =
            serde_json::from_slice(&std::fs::read(a.dir.join("session.json")).unwrap()).unwrap();
        assert_eq!(session.provider, "codex");
        assert_eq!(session.request, self.request);
        assert!(!session.conversation.is_empty());
        assert_eq!(session.status, atlas_agent::AgentStatus::Idle);
        assert_eq!(
            a.snapshot.as_ref().unwrap().conversation,
            session.conversation
        );
        assert!(session
            .turns
            .iter()
            .any(|t| t.role == "assistant" && !t.text.trim().is_empty()));
        self.snapshots.push(session);
        let conversation = self.h.app.presence_conversation(&self.who).unwrap();
        assert!(conversation
            .turns
            .iter()
            .any(|t| t.role == "assistant" && t.request == self.request && t.status == "complete"));
        assert!(conversation
            .bindings
            .iter()
            .any(|b| b.provider == "codex"
                && b.session == self.snapshots.last().unwrap().conversation));
    }

    fn write_receipt(&self, passed: bool, cleanup: Option<&Result<(), String>>) {
        let a = &self.h.app.presence.agents[0];
        let session = std::fs::read(a.dir.join("session.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<AgentSession>(&bytes).ok());
        let receipt = json!({
            "schema":1, "test":"installed_codex_reads_proposes_and_visibly_applies_one_scoped_move",
            "passed":passed, "elapsed_ms":self.started.elapsed().as_millis(),
            "state":if cleanup.is_none() {"running"} else if passed {"passed"} else {"failed"},
            "fixture_root":self.h.base, "ai_workspace":self.h.app.ai.config.valid_workspace(),
            "workbook":self.h.app.tab().path, "submitted_grant":self.grant,
            "participant":self.who, "session_binding":a.binding, "request":self.request,
            "provider":"codex", "executable_resolved":atlas_ai::runtime::codex_executable(),
            "model":null, "parameters":{}, "normal_presence_submission":true,
            "original":self.original, "observed_reads":self.reads, "observed_commands":self.commands,
            "proposal":self.proposal, "snapshots":self.snapshots, "current_session_file":session,
            "conversation":self.h.app.presence_conversation(&self.who),
            "approved_samples":self.samples, "committed":self.committed,
            "undo_verified":self.undo_verified, "frames":self.frames,
            "cleanup":{"stop_presence_revoke":cleanup, "provider_exit_confirmed":false,
                "limitation":"CodexLink has no public PID/join; external process corroboration required"},
            "limitations":["software app frames, not native GPU or human acceptance",
                "approval is dispatched by this explicitly opted-in test",
                "no final acceptance notification turn is sent to the provider"]
        });
        std::fs::write(
            self.output.join("receipt.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
    }
}
