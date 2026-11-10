use super::*;
#[test]
fn actual_oneshot_leaf_keeps_explicit_source_and_history_for_both_thread_paths() {
    for image in [false, true] {
        for replay in [false, true] {
            let mut request = AgentRequest {
                prompt: "  Exact visible question  ".into(),
                oneshot: !image,
                text_source: (!image).then(|| "SOURCE_é".into()),
                history: vec![AgentTurn {
                    role: "user".into(),
                    text: "ADMITTED_CONTEXT".into(),
                    at: 0,
                }],
                image: image.then(Default::default),
                ..Default::default()
            };
            let session = AgentSession {
                turns: vec![AgentTurn {
                    role: "user".into(),
                    text: "EXCLUDED_OLD_TURN".into(),
                    at: 0,
                }],
                ..Default::default()
            };
            let prepared = request_for_thread(replay, &request, &session);
            let actual = prepared.as_ref().unwrap_or(&request);
            let payload = if image {
                images::image_turn(actual)
            } else {
                oneshot_turn(actual)
            };
            assert_eq!(payload.matches("ADMITTED_CONTEXT").count(), 1);
            assert!(!payload.contains("EXCLUDED_OLD_TURN"));
            if !image {
                assert_eq!(payload.matches("SOURCE_é").count(), 1);
            }
            assert_eq!(request.prompt, "  Exact visible question  ");
            request.history.clear();
            let prepared = request_for_thread(replay, &request, &session);
            let actual = prepared.as_ref().unwrap_or(&request);
            let payload = if image {
                images::image_turn(actual)
            } else {
                oneshot_turn(actual)
            };
            assert!(!payload.contains("ADMITTED_CONTEXT"));
            assert!(!payload.contains("EXCLUDED_OLD_TURN"));
        }
    }
}
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

#[test]
fn replacement_tools_thread_replays_visible_training_but_not_hidden_events() {
    let request = AgentRequest {
        prompt: "Use the wired model".into(),
        ..Default::default()
    };
    let session = AgentSession {
        turns: vec![
            AgentTurn {
                role: "user".into(),
                text: "Use metres and preserve accessibility".into(),
                at: 1,
            },
            AgentTurn {
                role: "assistant".into(),
                text: "I will apply those design criteria".into(),
                at: 2,
            },
            AgentTurn {
                role: "tool".into(),
                text: "private tool event".into(),
                at: 3,
            },
        ],
        ..Default::default()
    };
    let replay = request_for_thread(true, &request, &session).unwrap();
    assert_eq!(replay.history.len(), 2);
    assert!(replay
        .prompt_with_history()
        .contains("preserve accessibility"));
    assert!(!replay.prompt_with_history().contains("private tool event"));
    assert!(request_for_thread(false, &request, &session).is_none());
}
