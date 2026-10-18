use crate::*;
#[test]
fn checkpoint_excludes_future_messages_and_refuses_partial_load() {
    let turns: Vec<_> = (0..3)
        .map(|i| AgentTurn {
            role: "user".into(),
            text: i.to_string(),
            at: i,
        })
        .collect();
    assert_eq!(checkpoint(&turns, Some(1)).unwrap().len(), 1);
    assert_eq!(checkpoint(&turns, Some(1)).unwrap()[0].text, "0");
    assert!(checkpoint(&[], Some(1)).is_err());
    assert!(checkpoint(&turns, Some(4)).is_err());
}
fn session(conversation: &str, turns: &[(&str, &str, u64)]) -> AgentSession {
    AgentSession {
        activity: Vec::new(),
        usage: None,
        approval: None,
        conversation: conversation.into(),
        artifacts: Vec::new(),
        status: AgentStatus::Idle,
        provider: "cursor".into(),
        turns: turns
            .iter()
            .map(|(role, text, at)| AgentTurn {
                role: (*role).into(),
                text: (*text).into(),
                at: *at,
            })
            .collect(),
        updated_at: 0,
        bundle: ImageBundle::default(),
        request: "r1".into(),
    }
}
#[test]
fn a_reload_never_shrinks_or_rewrites_the_shown_conversation() {
    let shown = session(
        "c",
        &[
            ("user", "q0", 5),
            ("assistant", "a0", 5),
            ("system", "Response stopped.", 6),
            ("user", "q1", 7),
            ("assistant", "a1", 7),
        ],
    );
    let shorter = session("c", &[("user", "q0", 0), ("assistant", "a0", 0)]);
    assert_eq!(reloaded(&shown, shorter), None);
    let reworded = session(
        "c",
        &[
            ("user", "Slate board: ... q0", 0),
            ("assistant", "a0", 0),
            ("user", "q1", 0),
            ("assistant", "a1", 0),
        ],
    );
    assert_eq!(reloaded(&shown, reworded), None, "same length is no news");
    assert_eq!(reloaded(&shown, session("c", &[])), None);

    let longer = session(
        "c",
        &[
            ("user", "q0", 0),
            ("assistant", "a0", 0),
            ("user", "q1", 0),
            ("assistant", "a1", 0),
            ("user", "asked in Cursor", 0),
            ("assistant", "answered in Cursor", 0),
        ],
    );
    let next = reloaded(&shown, longer).unwrap();
    assert_eq!(next.turns[..5], shown.turns[..]);
    assert_eq!(next.turns[5].text, "asked in Cursor");
    assert_eq!(next.turns[6].text, "answered in Cursor");
    assert_eq!(next.request, "r1", "Slate's own fields stay");

    let diverged = session(
        "c",
        &[
            ("assistant", "?", 0),
            ("user", "?", 0),
            ("assistant", "?", 0),
            ("user", "?", 0),
            ("assistant", "?", 0),
        ],
    );
    assert_eq!(reloaded(&shown, diverged), None);
}
#[test]
fn a_reload_fills_an_empty_card_and_replaces_another_conversation() {
    let fetched = session("c", &[("user", "q0", 0), ("assistant", "a0", 0)]);
    let empty = session("c", &[]);
    assert_eq!(reloaded(&empty, fetched.clone()), Some(fetched.clone()));
    let other = session("old", &[("user", "x", 1)]);
    assert_eq!(reloaded(&other, fetched.clone()), Some(fetched.clone()));
    assert_eq!(reloaded(&fetched, fetched.clone()), None);
    let unnamed = session("", &[("user", "q0", 3), ("assistant", "a0", 3)]);
    let named = reloaded(&unnamed, fetched).unwrap();
    assert_eq!(named.conversation, "c");
    assert_eq!(named.turns, unnamed.turns);
}
#[test]
fn rapid_requests_have_distinct_identity() {
    let ids: std::collections::BTreeSet<_> = (0..1000).map(|_| request_id()).collect();
    assert_eq!(ids.len(), 1000);
}
#[test]
fn old_session_contract_remains_readable() {
    let value: AgentSession =
        serde_json::from_str(r#"{"status":"idle","provider":"cursor","turns":[],"updated_at":1}"#)
            .unwrap();
    assert!(value.bundle.images.is_empty());
    assert!(value.request.is_empty());
    let image: ImageOutput =
        serde_json::from_str(r#"{"id":"a","source":"a.png","request":"r","prompt":"p"}"#).unwrap();
    assert!(image.model.is_empty());
    assert!(image.task.is_empty());
    assert!(image.live.is_empty());
    let item: ContextItem = serde_json::from_str(r#"{"node":1,"text":"","images":[]}"#).unwrap();
    assert!(item.depth.is_none());
    let request: AgentRequest = serde_json::from_str(r#"{"id":"r","prompt":"p","at":0}"#).unwrap();
    assert!(request.image.is_none());
    assert_eq!(ImageTask::for_inputs(true, true), ImageTask::Render);
    assert_eq!(ImageTask::for_inputs(false, true), ImageTask::Vary);
    assert_eq!(ImageTask::for_inputs(false, false).label(), "generate");
}
