use super::*;
use atlas_agent::{AgentTurn, TurnPolicy};
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::sync_channel,
    },
    time::{Duration, Instant},
};

fn readonly_request(id: &str, model: &str, prompt: &str, history: Vec<AgentTurn>) -> AgentRequest {
    AgentRequest {
        id: id.into(),
        model: Some(model.into()),
        prompt: prompt.into(),
        history,
        policy: TurnPolicy::ReadOnly,
        ..Default::default()
    }
}

#[test]
#[ignore = "live Codex smoke; set ATLAS_CODEX_LIVE_SMOKE=1 and run this test explicitly"]
fn live_codex_sequential_history_steer_interrupt_and_retry() {
    assert_eq!(
        std::env::var("ATLAS_CODEX_LIVE_SMOKE").ok().as_deref(),
        Some("1"),
        "set ATLAS_CODEX_LIVE_SMOKE=1 to enable the ignored live smoke"
    );
    let executable = codex_executable().expect("Codex CLI is installed");
    let root =
        std::env::temp_dir().join(format!("atlas-codex-smoke-{}", atlas_agent::request_id()));
    fs::create_dir_all(&root).expect("create isolated fixture directory");
    fs::write(
        root.join("fixture-note.txt"),
        "First clue: cedar-lamp-731. Second clue: blue-kite-294.\n",
    )
    .expect("write fixture text");

    let catalog = atlas_codex::Client::models_bounded(&executable, &root, Duration::from_secs(20))
        .unwrap_or_else(|_| panic!("read installed Codex model catalog within 20 seconds"));
    let model = catalog
        .iter()
        .find(|model| model.id.eq_ignore_ascii_case("gpt-6-luna"))
        .or_else(|| {
            catalog.iter().find(|model| {
                model.id.to_ascii_lowercase().contains("luna")
                    || model.name.to_ascii_lowercase().contains("luna")
            })
        })
        .map(|model| model.id.clone())
        .expect("an advertised Luna model is required; no larger-model fallback is used");
    let started = Instant::now();
    let mut client =
        atlas_codex::Client::start_bounded(&executable, &root, None, Duration::from_secs(60))
            .unwrap_or_else(|_| panic!("connect to installed Codex app-server within 60 seconds"));
    let (steer_tx, steer_rx) = sync_channel(4);
    let first_id = format!("smoke-{}", atlas_agent::request_id());
    let first = readonly_request(
        &first_id,
        &model,
        "Read fixture-note.txt using read-only access. Reply with the first clue only, then wait for a follow-up.",
        Vec::new(),
    );
    let steer_id = atlas_agent::request_id();
    steer_tx
        .send(atlas_codex::SteerCommand {
            id: steer_id.clone(),
            expected_request_id: first.id.clone(),
            text: "When replying, append the exact phrase: steering marker amber-otter-592.".into(),
        })
        .expect("queue the first turn's steering input");
    let mut session = AgentSession::default();
    client
        .run_with_steering(
            &first,
            &mut session,
            &AtomicBool::new(false),
            Some(&steer_rx),
            |_| {},
        )
        .unwrap_or_else(|_| panic!("first read-only Codex turn failed"));
    assert_eq!(session.status, AgentStatus::Idle);
    assert!(session
        .turns
        .iter()
        .any(|turn| { turn.role == "assistant" && turn.text.contains("cedar-lamp-731") }));
    assert!(session.activity.iter().any(|event| {
        event.id == format!("codex-steer:{steer_id}:accepted") && event.request == first_id
    }));

    let second_id = format!("smoke-{}", atlas_agent::request_id());
    let second = readonly_request(
        &second_id,
        &model,
        "From the fixture-note.txt already in this conversation, reply with the second clue only.",
        session.turns.clone(),
    );
    client
        .run_with_steering(
            &second,
            &mut session,
            &AtomicBool::new(false),
            Some(&steer_rx),
            |_| {},
        )
        .unwrap_or_else(|_| panic!("second read-only Codex turn failed"));
    assert_eq!(session.status, AgentStatus::Idle);
    assert!(session
        .turns
        .iter()
        .any(|turn| turn.role == "assistant" && turn.text.contains("blue-kite-294")));

    let mut interrupted = false;
    if started.elapsed() < Duration::from_secs(35) {
        let third_id = format!("smoke-{}", atlas_agent::request_id());
        let third = readonly_request(
            &third_id,
            &model,
            "Read fixture-note.txt again and explain both clues in one sentence.",
            session.turns.clone(),
        );
        let stop = AtomicBool::new(false);
        client
            .run_with_steering(&third, &mut session, &stop, Some(&steer_rx), |state| {
                if state.status == AgentStatus::Thinking {
                    stop.store(true, Ordering::Relaxed);
                }
            })
            .unwrap_or_else(|_| panic!("active-turn interruption failed"));
        assert!(
            matches!(&session.status, AgentStatus::Error(message) if message.contains("stopped")),
            "expected a confirmed stop, received {:?}",
            session.status
        );
        interrupted = true;

        let fourth_id = format!("smoke-{}", atlas_agent::request_id());
        let fourth = readonly_request(
            &fourth_id,
            &model,
            "Reply with the first clue from fixture-note.txt again.",
            session.turns.clone(),
        );
        client
            .run_with_steering(
                &fourth,
                &mut session,
                &AtomicBool::new(false),
                Some(&steer_rx),
                |_| {},
            )
            .unwrap_or_else(|_| panic!("post-interruption Codex retry failed"));
        assert_eq!(session.status, AgentStatus::Idle);
    }
    let status = match session.status {
        AgentStatus::Idle => "idle",
        AgentStatus::Thinking => "thinking",
        AgentStatus::Offline => "offline",
        AgentStatus::Error(_) => "error",
    };
    eprintln!(
        "Codex live smoke receipt: turns={}, steer_ack=1, public_activity={}, interruption_retry={}, status={status}",
        session.turns.iter().filter(|turn| turn.role == "user").count(),
        session.activity.len(),
        interrupted,
    );
}
