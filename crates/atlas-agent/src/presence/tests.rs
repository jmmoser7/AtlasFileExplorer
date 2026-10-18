use super::*;
use crate::{AgentActivityAction, AgentActivitySemantics};
use std::time::Duration;

fn id(value: &str) -> PresenceId {
    PresenceId::new(value).unwrap()
}
fn point(x: f64, y: f64) -> WorldPoint {
    WorldPoint::new(x, y).unwrap()
}
fn profile(value: &str) -> AgentProfile {
    AgentProfile {
        id: id(value),
        name: value.into(),
        hat: Some("cap".into()),
        retired: false,
    }
}
fn roster() -> (PresenceRoster, PresenceId) {
    let mut roster = PresenceRoster::default();
    roster.register(profile("main")).unwrap();
    let session = roster.join(&id("main"), None, point(0.0, 0.0)).unwrap();
    roster
        .session_mut(&session)
        .unwrap()
        .begin_request(id("request-1"))
        .unwrap();
    (roster, session)
}
fn plan(session: &PresenceSession, action: &str, start: f64, end: f64) -> MotionPlan {
    MotionPlan {
        session_id: session.id().clone(),
        request_id: session.request_id().unwrap().clone(),
        action_id: id(action),
        target_nodes: vec![id("stable-node")],
        start: point(start, 0.0),
        end: point(end, 0.0),
        work_end: None,
        durations: MotionDurations {
            travel_ms: TRAVEL_MIN_MS,
            grab_ms: GRAB_MIN_MS,
            work_ms: WORK_MIN_MS,
        },
    }
}
fn event(session: &PresenceSession, sequence: u64) -> PresenceEvent {
    PresenceEvent {
        session_id: session.id().clone(),
        request_id: session.request_id().unwrap().clone(),
        sequence,
        activity: AgentActivitySemantics::new(AgentActivityAction::Reading),
    }
}

#[test]
fn manipulation_is_continuous_after_contact_and_interruption_keeps_that_position() {
    let mut roster = PresenceRoster::default();
    roster.register(profile("worker")).unwrap();
    let session_id = roster.join(&id("worker"), None, point(0.0, 0.0)).unwrap();
    let session = roster.session_mut(&session_id).unwrap();
    session.begin_request(id("task")).unwrap();
    let mut action = plan(session, "move", 0.0, 30.0);
    action.work_end = Some(point(90.0, 0.0));
    session.enqueue(action).unwrap();
    for _ in 0..4 {
        session.advance(Duration::from_millis(50));
    }
    let middle = session.advance(Duration::from_millis(50));
    assert_eq!(middle.phase, MotionPhase::Work);
    assert!(middle.position.x > 30.0 && middle.position.x < 90.0);
    session.cancel_request();
    assert_eq!(session.sample().position, middle.position);
}

#[test]
fn malformed_inputs_are_refused_before_they_enter_runtime_state() {
    assert_eq!(
        PresenceId::new("x".repeat(ID_LIMIT + 1)),
        Err(PresenceError::InvalidId)
    );
    assert_eq!(
        PresenceId::new("bad\nidentity"),
        Err(PresenceError::InvalidId)
    );
    assert!(serde_json::from_str::<PresenceId>("\"bad identity\"").is_err());
    assert_eq!(
        WorldPoint::new(f64::NAN, 0.0),
        Err(PresenceError::InvalidPoint)
    );
    assert_eq!(
        WorldPoint::new(WORLD_COORD_LIMIT + 1.0, 0.0),
        Err(PresenceError::InvalidPoint)
    );
    let (mut roster, session_id) = roster();
    let mut invalid = profile("invalid");
    invalid.name = "\n".into();
    assert_eq!(roster.register(invalid), Err(PresenceError::InvalidProfile));
    let mut oversized = profile("oversized");
    oversized.name = "x".repeat(1_000);
    assert_eq!(
        roster.register(oversized),
        Err(PresenceError::InvalidProfile)
    );
    let session = roster.session_mut(&session_id).unwrap();
    let mut bad = plan(session, "bad", 0.0, 100.0);
    bad.end.x = f64::INFINITY;
    assert_eq!(
        session.enqueue(bad.clone()),
        Err(PresenceError::InvalidPoint)
    );
    bad.end.x = 100.0;
    bad.durations.travel_ms = 0;
    assert_eq!(
        session.enqueue(bad.clone()),
        Err(PresenceError::InvalidDuration)
    );
    bad.durations.travel_ms = PHASE_MS_LIMIT + 1;
    assert_eq!(
        session.enqueue(bad.clone()),
        Err(PresenceError::InvalidDuration)
    );
    bad.durations.travel_ms = TRAVEL_MIN_MS;
    bad.target_nodes.push(id("stable-node"));
    assert_eq!(session.enqueue(bad), Err(PresenceError::InvalidTargets));
    assert_eq!(session.queued_len(), 0);
}

#[test]
fn elapsed_motion_is_witnessed_and_applicability_gates_exactly_one_ready_signal() {
    let (mut roster, session_id) = roster();
    let session = roster.session_mut(&session_id).unwrap();
    session.enqueue(plan(session, "move", 0.0, 100.0)).unwrap();
    let gap = Duration::from_secs(3600);
    let first = session.advance(gap);
    assert_eq!(first.phase, MotionPhase::Travel);
    assert!(first.position.x > 0.0 && first.position.x < 100.0);
    assert!(session.resolve_ready(|_| panic!("not ready")).is_none());
    assert_eq!(session.advance(gap).phase, MotionPhase::Travel);
    assert_eq!(session.advance(gap).phase, MotionPhase::Grab);
    assert_eq!(session.leased_targets(), &[id("stable-node")]);
    assert_eq!(session.advance(gap).phase, MotionPhase::Work);
    assert_eq!(session.advance(gap).phase, MotionPhase::Work);
    assert_eq!(session.advance(gap).phase, MotionPhase::Work);
    assert_eq!(session.advance(gap).phase, MotionPhase::ReadyToCommit);
    let mut checks = 0;
    let signal = session.resolve_ready(|pending| {
        checks += 1;
        pending.target_nodes == [id("stable-node")]
    });
    assert!(
        matches!(signal, Some(PresenceSignal::ReadyToCommit { action_id, .. }) if action_id == id("move"))
    );
    assert_eq!(checks, 1);
    assert!(session
        .resolve_ready(|_| panic!("already resolved"))
        .is_none());
    assert!(session.leased_targets().is_empty());
}

#[test]
fn cancellation_or_a_stale_real_target_never_emits_ready() {
    let (mut roster, session_id) = roster();
    let session = roster.session_mut(&session_id).unwrap();
    session
        .enqueue(plan(session, "cancelled", 0.0, 100.0))
        .unwrap();
    session.advance(Duration::from_millis(30));
    let cancelled = session.cancel_request();
    assert!(
        matches!(&cancelled[..], [PresenceSignal::Cancelled { action_id, .. }] if *action_id == id("cancelled"))
    );
    assert_eq!(
        session.advance(Duration::from_secs(100)).phase,
        MotionPhase::Idle
    );
    assert!(session.resolve_ready(|_| panic!("cancelled")).is_none());
    session.begin_request(id("request-2")).unwrap();
    let start = session.sample().position.x;
    session
        .enqueue(plan(session, "deleted-target", start, 100.0))
        .unwrap();
    for _ in 0..7 {
        session.advance(Duration::from_millis(50));
    }
    assert!(matches!(
        session.resolve_ready(|_| false),
        Some(PresenceSignal::Cancelled { .. })
    ));
    assert!(session.resolve_ready(|_| true).is_none());
}

#[test]
fn interruption_starts_at_current_point_and_cancels_dependent_queue() {
    let (mut roster, session_id) = roster();
    let session = roster.session_mut(&session_id).unwrap();
    session.enqueue(plan(session, "first", 0.0, 100.0)).unwrap();
    session
        .enqueue(plan(session, "dependent", 100.0, 200.0))
        .unwrap();
    let visible = session.advance(Duration::from_millis(25)).position;
    let cancelled = session
        .interrupt(plan(session, "replacement", 999.0, -100.0))
        .unwrap();
    assert_eq!(cancelled.len(), 2);
    assert!(cancelled
        .iter()
        .all(|event| matches!(event, PresenceSignal::Cancelled { .. })));
    assert_eq!(session.advance(Duration::ZERO).position, visible);
    assert_eq!(session.current_plan().unwrap().start, visible);
    assert_eq!(session.queued_len(), 0);
    assert_eq!(
        session.enqueue(plan(session, "first", -100.0, 0.0)),
        Err(PresenceError::DuplicateAction)
    );
}

#[test]
fn reducer_rejects_old_session_request_and_sequence_without_changing_activity() {
    let (mut roster, session_id) = roster();
    let session = roster.session_mut(&session_id).unwrap();
    session.reduce(event(session, 2)).unwrap();
    let mut oversized = event(session, 3);
    oversized.activity.target = Some(crate::ActivityTarget {
        node_id: "x".repeat(crate::ACTIVITY_TARGET_ID_LIMIT * 4 + 1),
    });
    assert_eq!(
        session.reduce(oversized),
        Err(PresenceError::InvalidTargets)
    );
    assert_eq!(
        session.reduce(event(session, 1)),
        Err(PresenceError::StaleSequence)
    );
    let mut foreign = event(session, 3);
    foreign.session_id = id("other-session");
    assert_eq!(session.reduce(foreign), Err(PresenceError::SessionMismatch));
    let stale = event(session, 3);
    session.begin_request(id("new-request")).unwrap();
    assert_eq!(session.reduce(stale), Err(PresenceError::RequestMismatch));
    assert!(session.activity().is_none());
    session.reduce(event(session, 1)).unwrap();
    assert_eq!(
        session.begin_request(id("request-1")),
        Err(PresenceError::DuplicateRequest)
    );
}

#[test]
fn retirement_cancels_helpers_and_restore_has_fresh_identity_position_and_work() {
    let (mut roster, old_id) = roster();
    roster.register(profile("helper")).unwrap();
    let helper_id = roster
        .join(&id("helper"), Some(&old_id), point(0.0, 0.0))
        .unwrap();
    let parent = roster.session_mut(&old_id).unwrap();
    parent
        .enqueue(plan(parent, "parent-job", 0.0, 100.0))
        .unwrap();
    parent.advance(Duration::from_millis(50));
    parent.advance(Duration::from_millis(50));
    parent.advance(Duration::from_millis(50));
    assert!(!parent.leased_targets().is_empty());
    let helper = roster.session_mut(&helper_id).unwrap();
    helper.begin_request(id("helper-request")).unwrap();
    helper
        .enqueue(plan(helper, "helper-job", 0.0, 100.0))
        .unwrap();
    let cancelled = roster.retire(&id("main")).unwrap();
    assert_eq!(cancelled.len(), 2);
    assert_eq!(roster.sessions().count(), 0);
    let encoded = serde_json::to_string(&roster.profiles().collect::<Vec<_>>()).unwrap();
    for ephemeral in ["position", "request_id", "queue", "elapsed_ms"] {
        assert!(!encoded.contains(ephemeral));
    }
    let restored_id = roster.restore(&id("main"), point(-20.0, 8.0)).unwrap();
    assert_ne!(restored_id, old_id);
    let restored = roster.session(&restored_id).unwrap();
    assert_eq!(
        restored.sample(),
        MotionSample {
            position: point(-20.0, 8.0),
            phase: MotionPhase::Idle,
            progress: 0.0,
        }
    );
    assert!(restored.request_id().is_none());
    assert!(restored.leased_targets().is_empty());
    assert_eq!(restored.queued_len(), 0);
}

#[test]
fn helper_depth_and_child_count_are_bounded() {
    let (mut roster, root) = roster();
    for name in ["child", "grandchild", "too-deep", "c2", "c3", "c4", "c5"] {
        roster.register(profile(name)).unwrap();
    }
    let child = roster
        .join(&id("child"), Some(&root), point(0.0, 0.0))
        .unwrap();
    let grandchild = roster
        .join(&id("grandchild"), Some(&child), point(0.0, 0.0))
        .unwrap();
    assert_eq!(
        roster.join(&id("too-deep"), Some(&grandchild), point(0.0, 0.0)),
        Err(PresenceError::HelperLimit)
    );
    for name in ["c2", "c3", "c4"] {
        roster
            .join(&id(name), Some(&root), point(0.0, 0.0))
            .unwrap();
    }
    assert_eq!(
        roster.join(&id("c5"), Some(&root), point(0.0, 0.0)),
        Err(PresenceError::HelperLimit)
    );
    roster.leave(&root).unwrap();
    assert_eq!(roster.sessions().count(), 0);
}

#[test]
fn queue_action_and_request_memories_apply_backpressure_instead_of_growing() {
    let (mut roster, session_id) = roster();
    let session = roster.session_mut(&session_id).unwrap();
    for n in 0..QUEUE_LIMIT {
        session
            .enqueue(plan(session, &format!("a-{n}"), n as f64, n as f64 + 1.0))
            .unwrap();
    }
    assert_eq!(
        session.enqueue(plan(session, "overflow", QUEUE_LIMIT as f64, 20.0)),
        Err(PresenceError::QueueFull)
    );
    session.cancel_request();
    session.begin_request(id("bounded-actions")).unwrap();
    for n in 0..ACTION_LIMIT {
        let item = plan(session, &format!("bounded-{n}"), n as f64, n as f64 + 1.0);
        session.enqueue(item).unwrap();
        for _ in 0..7 {
            session.advance(Duration::from_millis(50));
        }
        assert!(session.resolve_ready(|_| true).is_some());
    }
    assert_eq!(
        session.enqueue(plan(session, "too-many", ACTION_LIMIT as f64, 0.0)),
        Err(PresenceError::ActionLimit)
    );
    for n in 2..REQUEST_LIMIT {
        session.begin_request(id(&format!("request-{n}"))).unwrap();
    }
    assert_eq!(
        session.begin_request(id("request-overflow")),
        Err(PresenceError::RequestLimit)
    );
}

#[test]
fn short_plans_are_refused_and_coarse_frames_witness_every_phase() {
    let (mut roster, session_id) = roster();
    let session = roster.session_mut(&session_id).unwrap();
    for durations in [
        MotionDurations {
            travel_ms: 1,
            grab_ms: 1,
            work_ms: 1,
        },
        MotionDurations {
            travel_ms: TRAVEL_MIN_MS - 1,
            grab_ms: GRAB_MIN_MS,
            work_ms: WORK_MIN_MS,
        },
        MotionDurations {
            travel_ms: TRAVEL_MIN_MS,
            grab_ms: GRAB_MIN_MS - 1,
            work_ms: WORK_MIN_MS,
        },
        MotionDurations {
            travel_ms: TRAVEL_MIN_MS,
            grab_ms: GRAB_MIN_MS,
            work_ms: WORK_MIN_MS - 1,
        },
    ] {
        let mut short = plan(session, "too-fast", 0.0, 100.0);
        short.durations = durations;
        assert_eq!(session.enqueue(short), Err(PresenceError::InvalidDuration));
    }
    let mut pending = plan(session, "visible", 0.0, 100.0);
    pending.durations = MotionDurations {
        travel_ms: 175,
        grab_ms: 75,
        work_ms: 175,
    };
    session.enqueue(pending).unwrap();
    let samples: Vec<_> = (0..10)
        .map(|_| session.advance(Duration::from_secs(3600)))
        .collect();
    use MotionPhase::*;
    assert_eq!(
        samples
            .iter()
            .map(|sample| sample.phase)
            .collect::<Vec<_>>(),
        [
            Travel,
            Travel,
            Travel,
            Grab,
            Grab,
            Work,
            Work,
            Work,
            Work,
            ReadyToCommit
        ]
    );
    for sample in &samples[..3] {
        assert!(sample.position.x > 0.0 && sample.position.x < 100.0);
    }
    assert!(samples[..3]
        .windows(2)
        .all(|pair| pair[0].position.x < pair[1].position.x));
    for sample in &samples[3..] {
        assert_eq!(sample.position, point(100.0, 0.0));
    }
    assert!(matches!(
        session.resolve_ready(|_| true),
        Some(PresenceSignal::ReadyToCommit { .. })
    ));
    assert!(session
        .resolve_ready(|_| panic!("already resolved"))
        .is_none());
}

#[test]
fn idle_contact_settlement_rejects_busy_motion_and_preserves_replay_and_next_start() {
    let (mut roster, session_id) = roster();
    let session = roster.session_mut(&session_id).unwrap();
    session.reduce(event(session, 1)).unwrap();
    let request = session.request_id().cloned();
    let initial = session.sample();
    assert_eq!(
        session.settle_idle_position(WorldPoint {
            x: f64::NAN,
            y: 0.0
        }),
        Err(PresenceError::InvalidPoint)
    );
    assert_eq!(session.sample(), initial);
    let first = plan(session, "first", 0.0, 30.0);
    session.enqueue(first.clone()).unwrap();
    assert_eq!(
        session.settle_idle_position(point(14.0, 4.0)),
        Err(PresenceError::DiscontinuousStart)
    );
    assert_eq!(session.queued_len(), 1);
    session.advance(Duration::from_millis(50));
    let active = session.sample();
    assert_eq!(
        session.settle_idle_position(point(14.0, 4.0)),
        Err(PresenceError::DiscontinuousStart)
    );
    assert_eq!(session.sample(), active);
    assert_eq!(session.cancel_motion().len(), 1);
    let physical = point(14.0, 4.0);
    session.settle_idle_position(physical).unwrap();
    assert_eq!(session.sample().position, physical);
    assert_eq!(session.request_id().cloned(), request);
    assert_eq!(
        session.reduce(event(session, 1)),
        Err(PresenceError::StaleSequence)
    );
    assert_eq!(session.enqueue(first), Err(PresenceError::DuplicateAction));
    let mut next = plan(session, "next", 14.0, 30.0);
    next.start = physical;
    session.enqueue(next).unwrap();
    for _ in 0..40 {
        if session.advance(Duration::from_millis(50)).phase == MotionPhase::ReadyToCommit {
            break;
        }
    }
    assert_eq!(session.sample().phase, MotionPhase::ReadyToCommit);
    assert!(session.resolve_ready(|_| true).is_some());
    session.settle_idle_position(point(29.0, 1.0)).unwrap();
    assert_eq!(session.sample().position, point(29.0, 1.0));
    assert!(session.resolve_ready(|_| true).is_none());
    assert_eq!(session.request_id().cloned(), request);
}
