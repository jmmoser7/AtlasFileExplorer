use super::{conversation_copy_text, history_window};
use crate::agent_presence::ConversationMessage;

#[test]
fn every_stored_turn_is_reachable_with_at_most_sixty_four_painted_rows() {
    let total = 513;
    let mut seen = vec![false; total];
    for offset in (0..total).step_by(64) {
        let page = history_window(total, offset, false, false);
        assert!(page.len() <= 64);
        for index in page {
            assert!(!seen[index]);
            seen[index] = true;
        }
    }
    assert!(seen.into_iter().all(|turn| turn));
    assert_eq!(history_window(100_000, 0, true, false), 99_996..100_000);
    assert_eq!(history_window(100_000, 0, false, true), 99_936..100_000);
    assert_eq!(history_window(100_000, 64, false, true), 99_872..99_936);
    assert_eq!(history_window(0, 0, false, false), 0..0);
}

#[test]
fn selected_history_stays_bounded_while_copy_can_include_earliest_and_latest_turns() {
    let total = 10_000;
    let window = history_window(total, 0, false, true);
    assert_eq!(window, total - 64..total);
    assert!(window.len() <= 64);

    let texts: Vec<_> = (0..total).map(|index| format!("turn-{index}")).collect();
    let messages: Vec<_> = texts
        .iter()
        .enumerate()
        .map(|(index, text)| ConversationMessage {
            id: text,
            text,
            outgoing: index % 2 == 0,
            system: false,
            status: "delivered",
            attachment_count: 0,
        })
        .collect();
    let copied = super::conversation_copy_text("Dot", &messages);
    assert!(copied.starts_with("You:\nturn-0"));
    assert!(copied.ends_with("Dot:\nturn-9999"));
}

#[test]
fn conversation_copy_preserves_turn_boundaries_roles_and_attachment_captions() {
    let messages = [
        ConversationMessage {
            id: "human",
            text: "Please sort these.",
            outgoing: true,
            system: false,
            status: "delivered",
            attachment_count: 1,
        },
        ConversationMessage {
            id: "reply",
            text: "Done.",
            outgoing: false,
            system: false,
            status: "delivered",
            attachment_count: 0,
        },
        ConversationMessage {
            id: "notice",
            text: "Task complete",
            outgoing: false,
            system: true,
            status: "",
            attachment_count: 0,
        },
    ];
    assert_eq!(
        conversation_copy_text("Dot", &messages),
        "You:\nPlease sort these.\n[1 image attached]\n\nDot:\nDone.\n\nNotice:\nTask complete"
    );
}

#[test]
fn completed_reply_follows_later_public_progress_without_dropping_history() {
    use super::order::{ordered_rows, TranscriptRow};
    use crate::agent_presence::PublicActivity;

    let messages = [
        ConversationMessage {
            id: "question",
            text: "Sort these",
            outgoing: true,
            system: false,
            status: "complete",
            attachment_count: 0,
        },
        ConversationMessage {
            id: "reply",
            text: "Done",
            outgoing: false,
            system: false,
            status: "complete",
            attachment_count: 0,
        },
    ];
    let activity = [
        PublicActivity {
            label: "Working",
            text: "Reading files",
        },
        PublicActivity {
            label: "Completed",
            text: "Saved the result",
        },
    ];

    assert_eq!(
        ordered_rows(&messages, &activity),
        [
            TranscriptRow::Message(0),
            TranscriptRow::Activity(0),
            TranscriptRow::Activity(1),
            TranscriptRow::Message(1),
        ]
    );
    assert_eq!(
        ordered_rows(&messages[1..], &activity),
        [
            TranscriptRow::Activity(0),
            TranscriptRow::Activity(1),
            TranscriptRow::Message(0),
        ]
    );

    let legacy = [
        ConversationMessage {
            id: "old-question",
            text: "First",
            outgoing: true,
            system: false,
            status: "complete",
            attachment_count: 0,
        },
        ConversationMessage {
            id: "steering",
            text: "Keep going",
            outgoing: true,
            system: false,
            status: "steering",
            attachment_count: 0,
        },
        ConversationMessage {
            id: "old-reply",
            text: "Done",
            outgoing: false,
            system: false,
            status: "complete",
            attachment_count: 0,
        },
    ];
    assert_eq!(
        ordered_rows(&legacy, &activity),
        [
            TranscriptRow::Message(0),
            TranscriptRow::Message(1),
            TranscriptRow::Activity(0),
            TranscriptRow::Activity(1),
            TranscriptRow::Message(2),
        ]
    );

    let streaming = [
        ConversationMessage {
            id: "old-question",
            text: "First",
            outgoing: true,
            system: false,
            status: "complete",
            attachment_count: 0,
        },
        ConversationMessage {
            id: "old-reply",
            text: "Done",
            outgoing: false,
            system: false,
            status: "complete",
            attachment_count: 0,
        },
        ConversationMessage {
            id: "next-question",
            text: "Next",
            outgoing: true,
            system: false,
            status: "complete",
            attachment_count: 0,
        },
        ConversationMessage {
            id: "partial",
            text: "Thinking",
            outgoing: false,
            system: false,
            status: "streaming",
            attachment_count: 0,
        },
    ];
    assert_eq!(
        ordered_rows(&streaming, &activity),
        [
            TranscriptRow::Message(0),
            TranscriptRow::Message(1),
            TranscriptRow::Message(2),
            TranscriptRow::Message(3),
            TranscriptRow::Activity(0),
            TranscriptRow::Activity(1),
        ]
    );
}
