use super::*;

fn entry(repeat: Repeat) -> ArmedSchedule {
    let plan = AgentSchedule {
        start: "2026-10-17T10:00".into(),
        end: Some("2026-10-18T10:00".into()),
        repeat,
        prompt: "Check the presentation".into(),
        provider: "local".into(),
        cwd: String::new(),
        ai_workspace: String::new(),
        model: None,
    };
    ArmedSchedule {
        due: plan.start_time().map(|t| t.and_utc().timestamp()),
        plan,
        motion: id("live-session").unwrap(),
        grant: "live-grant".into(),
        tab: 1,
        waiting: false,
    }
}
fn eligible() -> Eligibility {
    Eligibility {
        live: true,
        visible: true,
        paused: false,
        busy: false,
    }
}
#[test]
fn paused_or_busy_due_message_waits_for_explicit_resume_without_overlapping() {
    let mut e = entry(Repeat::Hourly);
    let due = e.due.unwrap();
    assert_eq!(
        e.tick(
            due,
            Eligibility {
                paused: true,
                ..eligible()
            },
            false
        ),
        Tick::Waiting
    );
    assert_eq!(e.due, Some(due));
    assert!(e.waiting);
    assert_eq!(
        e.tick(
            due + 60,
            Eligibility {
                busy: true,
                ..eligible()
            },
            false
        ),
        Tick::Waiting
    );
    assert_eq!(e.tick(due + 65, eligible(), false), Tick::Run);
    assert_eq!(e.due, Some(due + 3600));
    assert_eq!(e.tick(due + 66, eligible(), false), Tick::None);
}
#[test]
fn inactive_board_and_sleep_gaps_skip_occurrences_instead_of_catching_up() {
    let mut e = entry(Repeat::Hourly);
    let due = e.due.unwrap();
    assert_eq!(
        e.tick(
            due + 10,
            Eligibility {
                visible: false,
                ..eligible()
            },
            false
        ),
        Tick::None
    );
    assert_eq!(e.due, Some(due + 3600));
    assert_eq!(e.tick(due + 3600 * 5 + 10, eligible(), true), Tick::None);
    assert_eq!(e.due, Some(due + 3600 * 6));
    let mut once = entry(Repeat::Once);
    assert_eq!(once.tick(due + 1, eligible(), true), Tick::Disarm);
}
#[test]
fn revoked_or_retired_membership_and_expired_end_never_run() {
    let mut e = entry(Repeat::Hourly);
    let due = e.due.unwrap();
    assert_eq!(
        e.tick(
            due,
            Eligibility {
                live: false,
                ..eligible()
            },
            false
        ),
        Tick::Disarm
    );
    let end = e.plan.end_time().unwrap().and_utc().timestamp();
    assert_eq!(e.tick(end, eligible(), false), Tick::Disarm);
}
#[test]
fn durable_configuration_does_not_restore_armed_jobs_or_authority() {
    let e = entry(Repeat::Daily);
    let c = conversation::Conversation {
        schedule: Some(e.plan),
        ..Default::default()
    };
    let restored: conversation::Conversation =
        serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
    assert!(restored.schedule.is_some());
    let runtime = PresenceSchedules::default();
    assert!(runtime.entries.is_empty());
    assert!(runtime.draft.is_none());
}
