//! Optional live read-only smoke: one model turn, one synthetic board read.
//! cargo run --locked --release -p atlas-ai --example board_read_smoke
//! Uses the configured signed-in CLI/model. Never run as part of ordinary tests.
use atlas_agent::{
    collaborator::*, AgentActivityAction, AgentActivityKind, AgentRequest, AgentSession,
    AgentStatus, TurnPolicy,
};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

const EXPECTED_SUMMARY: &str = "Synthetic board contains 2 objects.";

#[derive(Serialize)]
struct Outcome {
    phase: &'static str,
    failure_class: Option<&'static str>,
    turn_started_observed: bool,
    completed: bool,
    summary_matches: bool,
    public_read_started: usize,
    public_read_finished: usize,
    unexpected_tool: bool,
}

fn main() {
    match smoke() {
        Ok((report, passed)) => {
            println!(
                "Live board read smoke {}. Report: {}",
                if passed { "passed" } else { "failed" },
                report.display()
            );
            if !passed {
                std::process::exit(1);
            }
        }
        Err(stage) => {
            eprintln!("Live board read smoke could not complete: {stage}.");
            std::process::exit(1);
        }
    }
}

fn smoke() -> Result<(PathBuf, bool), &'static str> {
    let started = Instant::now();
    let executable = atlas_ai::runtime::codex_executable().ok_or("Codex CLI unavailable")?;
    let target = std::env::current_dir()
        .map_err(|_| "working directory unavailable")?
        .join("target");
    std::fs::create_dir_all(&target).map_err(|_| "target directory unavailable")?;
    let target = target
        .canonicalize()
        .map_err(|_| "target directory could not be resolved")?;
    let workspace = target.join(format!("presence-read-smoke-{}", atlas_agent::request_id()));
    std::fs::create_dir(&workspace).map_err(|_| "smoke workspace creation failed")?;
    let workspace = workspace
        .canonicalize()
        .map_err(|_| "smoke workspace could not be resolved")?;
    if workspace.parent() != Some(target.as_path()) {
        return Err("smoke workspace escaped target");
    }
    let dir = workspace.join("link");
    let binding = BoardLinkSession {
        session_id: atlas_agent::request_id(),
        grant_id: atlas_agent::request_id(),
        workspace_id: "synthetic-smoke".into(),
    };
    let grant = CollaboratorGrant {
        id: binding.grant_id.clone(),
        workspace_id: binding.workspace_id.clone(),
        readable_board: false,
        readable_nodes: vec!["1".into(), "2".into()],
        readable_sources: vec![],
        commands: vec![],
        conversation_ids: vec![],
        mutation_autonomy: false,
        helper_limit: 0,
        max_actions: 1,
    };
    let request=AgentRequest {id:atlas_agent::request_id(),prompt:format!(
        "This is a bounded read-only integration smoke. Call slate_board_read exactly once with node_ids=[], offset=0, limit=32. The host returns a synthetic fixture only. Then reply with exactly: {EXPECTED_SUMMARY} Use no other tools, commands, filesystem reads/writes, agents, network lookups or external messages. Do not invoke any board commands."),
        at:atlas_ai::context::now_secs(),policy:TurnPolicy::ReadOnly,collaborator:Some(binding.clone()),..Default::default()};
    let mut links = atlas_ai::board_link::BoardLinks::default();
    links
        .set_session(&dir, binding.clone(), grant.clone(), request.id.clone())
        .map_err(|_| "board binding failed")?;
    let cancel = Arc::new(AtomicBool::new(false));
    let stopped = cancel.clone();
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let worker_dir = dir.clone();
    let worker_workspace = workspace.clone();
    let worker_request = request.clone();
    let handle = thread::spawn(move || {
        let mut outcome = Outcome {
            phase: "connect",
            failure_class: None,
            turn_started_observed: false,
            completed: false,
            summary_matches: false,
            public_read_started: 0,
            public_read_finished: 0,
            unexpected_tool: false,
        };
        let mut client = match atlas_codex::Client::start_bounded(
            &executable,
            &worker_workspace,
            None,
            Duration::from_secs(80),
        ) {
            Ok(client) => client,
            Err(error) => {
                outcome.failure_class = Some(classify(&error));
                let _ = result_tx.send(outcome);
                return;
            }
        };
        client.set_approval_dir(&worker_dir);
        client.set_board_cloud_guard(atlas_core::cloud::is_dehydrated);
        client.set_full_access(false);
        outcome.phase = "turn";
        let mut session = AgentSession::default();
        let mut seen = HashSet::new();
        let result = client.run(&worker_request, &mut session, &stopped, |snapshot| {
            outcome.turn_started_observed |= matches!(snapshot.status, AgentStatus::Thinking);
            if snapshot.approval.is_some() {
                outcome.unexpected_tool = true;
                stopped.store(true, Ordering::Relaxed);
            }
            for activity in &snapshot.activity {
                if !seen.insert(activity.id.clone()) {
                    continue;
                }
                let reading = activity
                    .semantics
                    .as_ref()
                    .is_some_and(|s| s.action == AgentActivityAction::Reading);
                match activity.kind {
                    AgentActivityKind::ToolStarted if reading => outcome.public_read_started += 1,
                    AgentActivityKind::ToolFinished if reading => outcome.public_read_finished += 1,
                    AgentActivityKind::ToolStarted => {
                        outcome.unexpected_tool = true;
                        stopped.store(true, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
        });
        outcome.completed = result.is_ok() && matches!(session.status, AgentStatus::Idle);
        outcome.failure_class = result.as_ref().err().map(|error| classify(error));
        outcome.summary_matches = session
            .turns
            .iter()
            .rev()
            .find(|t| t.role == "assistant")
            .is_some_and(|t| t.text.trim() == EXPECTED_SUMMARY);
        // The existing Client Drop owns app-server stop/wait before completion is reported.
        drop(client);
        let _ = result_tx.send(outcome);
    });
    let mut read_calls = 0usize;
    let mut other_calls = 0usize;
    let mut response_accepted = false;
    let mut call_ids = Vec::new();
    let outcome = loop {
        if started.elapsed() >= Duration::from_secs(75) {
            cancel.store(true, Ordering::Relaxed);
        }
        for call in links.poll_limit(&dir, 4) {
            let permitted = grant.authorize(&call, &binding, &request.id).is_ok();
            let reply = match &call.operation {
                BoardOperation::Read {
                    node_ids,
                    offset,
                    limit,
                } if permitted
                    && read_calls == 0
                    && node_ids.is_empty()
                    && *offset == 0
                    && *limit >= 2 =>
                {
                    read_calls += 1;
                    call_ids.push(call.id.clone());
                    BoardReply {
                        id: call.id.clone(),
                        session_id: binding.session_id.clone(),
                        request_id: request.id.clone(),
                        success: true,
                        result: json!({"workspace_id":"synthetic-smoke","objects":[
                            {"id":"1","kind":"shape","rect":{"x":0,"y":0,"w":40,"h":40}},
                            {"id":"2","kind":"text","rect":{"x":60,"y":0,"w":80,"h":40},"text":"Synthetic fixture only."}],
                            "total":2,"next_offset":null,"commands":{},"conversations":[],"proposal_mode":true}),
                    }
                }
                _ => {
                    other_calls += 1;
                    cancel.store(true, Ordering::Relaxed);
                    BoardReply::error(
                        &call,
                        "Only one synthetic board read is permitted by this smoke.",
                    )
                }
            };
            response_accepted |= links.reply(&dir, reply).is_ok();
        }
        match result_rx.try_recv() {
            Ok(outcome) => break Some(outcome),
            Err(mpsc::TryRecvError::Disconnected) => break None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if started.elapsed() >= Duration::from_secs(85) {
            cancel.store(true, Ordering::Relaxed);
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let worker_finished = outcome.is_some();
    if worker_finished {
        let _ = handle.join();
    }
    cancel.store(true, Ordering::Relaxed);
    links.revoke(&dir);
    links.retain(&HashSet::new());
    while dir.join("board-session.json").exists() && started.elapsed() < Duration::from_secs(90) {
        thread::sleep(Duration::from_millis(20));
    }
    let membership_removed = !dir.join("board-session.json").exists();
    let protocol_removed = dir
        .canonicalize()
        .ok()
        .filter(|path| {
            path.parent() == Some(workspace.as_path())
                && path.file_name().is_some_and(|name| name == "link")
        })
        .is_some_and(|path| std::fs::remove_dir_all(path).is_ok());
    let passed = outcome.as_ref().is_some_and(|o| {
        o.completed
            && o.summary_matches
            && !o.unexpected_tool
            && o.public_read_started == 1
            && o.public_read_finished == 1
    }) && read_calls == 1
        && other_calls == 0
        && response_accepted
        && membership_removed
        && protocol_removed;
    let report = dir
        .parent()
        .ok_or("smoke report parent missing")?
        .join("report.json");
    let result = json!({"passed":passed,"elapsed_ms":started.elapsed().as_millis(),"total_budget_seconds":90,
        "provider_budget_seconds":80,"automatic_retries":0,"model_override":null,"filesystem_policy":"read_only",
        "allowed_commands":[],"expected_objects":2,"accepted_read_calls":read_calls,"other_host_calls":other_calls,
        "response_accepted":response_accepted,"call_ids":call_ids,"worker_finished":worker_finished,
        "membership_removed":membership_removed,"protocol_removed":protocol_removed,"provider":outcome,
        "safe_summary":if passed{EXPECTED_SUMMARY}else{"Smoke incomplete; inspect public boolean outcomes."}});
    std::fs::write(
        &report,
        serde_json::to_vec_pretty(&result).map_err(|_| "report serialization failed")?,
    )
    .map_err(|_| "report write failed")?;
    Ok((report, passed))
}

fn classify(error: &str) -> &'static str {
    if error.contains("Access is denied") || error.contains("os error 5") {
        "execution_denied"
    } else if error.contains("Sign in") {
        "sign_in_required"
    } else if error.contains("timed out") || error.contains("expired") {
        "deadline"
    } else if error.contains("sandbox") {
        "provider_sandbox"
    } else if error.contains("dynamicTools") || error.contains("experimental") {
        "dynamic_tool_protocol"
    } else {
        "provider_error"
    }
}
