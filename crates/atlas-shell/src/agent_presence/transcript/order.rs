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
    let mut rows: Vec<_> = messages
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let completed_reply = !row.outgoing && !row.system && row.status == "complete";
            (
                row.at,
                u8::from(completed_reply),
                TranscriptRow::Message(index),
            )
        })
        .chain(
            activity
                .iter()
                .enumerate()
                .map(|(index, row)| (row.at, 0, TranscriptRow::Activity(index))),
        )
        .collect();
    rows.sort_by_key(|(at, tie_order, _)| (*at, *tie_order));
    rows.into_iter().map(|(_, _, row)| row).collect()
}
