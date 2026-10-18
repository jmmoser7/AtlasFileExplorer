use super::*;

#[test]
fn a_small_update_transfers_only_new_data_after_thousands_of_retained_turns() {
    let mut conversation = Conversation::default();
    let old_body = "Retained response ".repeat(128);
    for index in 0..5000 {
        conversation.push(
            "user",
            &old_body,
            "Jamie",
            None,
            &format!("message-{index}"),
            "delivered",
        );
    }
    let mut mirror = Conversation::default();
    mirror.apply_changes(conversation.take_changes());
    assert_eq!(conversation.request_index("message-2500"), Some(2500));
    conversation.push(
        "user",
        "A new short message",
        "Jamie",
        None,
        "new-message",
        "pending",
    );
    conversation.set_draft("next draft");
    conversation.set_turn_status(2500, "failed");
    let changes = conversation.take_changes();
    assert_eq!(changes.len(), 3);
    assert!(
        matches!(&changes[0],ConversationChange::Append(turn) if turn.text=="A new short message")
    );
    assert!(matches!(&changes[1],ConversationChange::Draft(text) if text=="next draft"));
    assert!(
        matches!(&changes[2],ConversationChange::Status {index:2500,status} if status=="failed")
    );
    mirror.apply_changes(changes);
    assert_eq!(mirror.turns.len(), 5001);
    assert_eq!(mirror.turns[2500].text, old_body);
    assert_eq!(mirror.turns[2500].status, "failed");
    assert_eq!(mirror, conversation);
}

#[test]
fn a_current_request_span_keeps_the_prefix_and_request_indices_stable() {
    let mut conversation = Conversation::default();
    for index in 0..2000 {
        conversation.push(
            "user",
            "Old",
            "Jamie",
            None,
            &format!("old-{index}"),
            "complete",
        );
    }
    conversation.push("user", "Question", "Jamie", None, "current", "sent");
    conversation.push(
        "assistant",
        "Partial",
        "Dot",
        Some("local"),
        "current",
        "streaming",
    );
    let mut mirror = Conversation::default();
    mirror.apply_changes(conversation.take_changes());
    let mut replacement = conversation.turns[2001].clone();
    replacement.text = "Complete answer".into();
    replacement.status = "complete".into();
    conversation.replace_turn_span(2001, 2002, vec![replacement]);
    assert_eq!(conversation.request_index("old-0"), Some(0));
    assert_eq!(conversation.request_index("old-1999"), Some(1999));
    assert_eq!(conversation.request_index("current"), Some(2000));
    assert_eq!(conversation.request_indices("current"), &[2000, 2001]);
    let changes = conversation.take_changes();
    assert_eq!(changes.len(), 1);
    assert!(
        matches!(&changes[0],ConversationChange::Span {start:2001,end:2002,turns} if turns.len()==1)
    );
    mirror.apply_changes(changes);
    assert_eq!(mirror, conversation);
}
