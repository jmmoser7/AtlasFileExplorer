use super::super::model::{ConversationMessage, PublicActivity};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TranscriptRow {
    Message(usize),
    Activity(usize),
}

pub(super) fn ordered_rows(
    messages: &[ConversationMessage<'_>],
    activity: &[PublicActivity<'_>],
) -> Vec<TranscriptRow> {
    let mut rows: Vec<_> = (0..messages.len()).map(TranscriptRow::Message).collect();
    let latest_prompt = messages.iter().rposition(|message| message.outgoing);
    let completed_reply = messages.iter().enumerate().rfind(|(index, message)| {
        latest_prompt.is_none_or(|prompt| *index > prompt)
            && !message.outgoing
            && !message.system
            && message.status == "complete"
    });
    let insertion = completed_reply.map_or(rows.len(), |(index, _)| index);
    rows.splice(
        insertion..insertion,
        (0..activity.len()).map(TranscriptRow::Activity),
    );
    rows
}
