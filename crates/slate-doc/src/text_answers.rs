//! Authored links from exact text ranges to immutable agent replies.
//!
//! Offsets are UTF-8 byte boundaries, like Rust string slices. Native text
//! widgets convert their character cursors with [`byte_range`]. A link is only
//! valid while its quote still matches; viewing it never changes agent memory.
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// A reply within a particular conversation checkpoint, independent of its UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextAnswerRef {
    pub session: String,
    /// Number of turns through and including the linked assistant reply.
    pub through: usize,
}

/// Displayed output checkpoint; links to the existing transcript source, never copies it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextOutputRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_id: Option<String>,
    pub source: crate::scene::SourceUri,
    pub response: TextAnswerRef,
}

/// A response option reviews captured source wording; it never replaces the
/// editable document or its outgoing value until a human applies it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextReview {
    pub id: String,
    pub group: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Range<usize>>,
    pub question: String,
    pub text: String,
    pub edit: bool,
    pub output: TextOutputRef,
}

impl TextReview {
    pub fn is_current(&self, text: &str, revision: Option<&str>) -> bool {
        self.source == text
            && self.source_revision.as_deref() == revision
            && self
                .selection
                .as_ref()
                .is_none_or(|range| text.get(range.clone()).is_some())
    }
    pub fn replacement(&self, text: &str, revision: Option<&str>) -> Option<String> {
        if !self.edit || !self.is_current(text, revision) {
            return None;
        }
        let mut result = text.to_owned();
        let range = self.selection.clone().unwrap_or(0..text.len());
        result.replace_range(range, &self.text);
        Some(result)
    }
}

/// The visible answer snapshot is authored content, not a live session grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextAnswer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_id: Option<String>,
    pub start: usize,
    pub end: usize,
    pub quote: String,
    pub question: String,
    pub answer: String,
    pub response: TextAnswerRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<crate::scene::SourceUri>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
}

impl TextAnswer {
    pub fn new(
        text: &str,
        range: Range<usize>,
        response: TextAnswerRef,
        question: String,
        answer: String,
    ) -> Option<Self> {
        let quote = text.get(range.clone())?;
        if quote.trim().is_empty() || response.session.is_empty() || response.through == 0 {
            return None;
        }
        Some(Self {
            review_id: None,
            start: range.start,
            end: range.end,
            quote: quote.into(),
            question,
            answer,
            response,
            origin: None,
            source_revision: None,
        })
    }

    pub fn is_valid_for(&self, text: &str) -> bool {
        self.start < self.end
            && !self.quote.is_empty()
            && text.get(self.start..self.end) == Some(self.quote.as_str())
    }

    pub fn is_valid_for_revision(&self, text: &str, revision: Option<&str>) -> bool {
        self.is_valid_for(text)
            && self
                .source_revision
                .as_deref()
                .is_none_or(|expected| Some(expected) == revision)
    }

    /// Character range consumed by native galley and TextEdit cursor helpers.
    pub fn char_range(&self, text: &str) -> Option<Range<usize>> {
        self.is_valid_for(text)
            .then(|| text[..self.start].chars().count()..text[..self.end].chars().count())
    }
}

/// Convert a widget's Unicode character offsets without splitting a codepoint.
pub fn byte_range(text: &str, chars: Range<usize>) -> Option<Range<usize>> {
    if chars.start >= chars.end {
        return None;
    }
    let offset = |n| {
        if n == text.chars().count() {
            Some(text.len())
        } else {
            text.char_indices().nth(n).map(|(index, _)| index)
        }
    };
    Some(offset(chars.start)?..offset(chars.end)?)
}

/// Replace overlapping answers when a passage is queried again.
pub fn upsert(answers: &mut Vec<TextAnswer>, text: &str, answer: TextAnswer) -> bool {
    if !answer.is_valid_for(text) {
        return false;
    }
    answers.retain(|old| {
        old.is_valid_for(text) && (old.end <= answer.start || old.start >= answer.end)
    });
    answers.push(answer);
    answers.sort_by_key(|answer| (answer.start, answer.end));
    true
}

/// Ordered, non-overlapping valid links for native paint and HTML export.
pub fn valid_answers<'a>(text: &str, answers: &'a [TextAnswer]) -> Vec<&'a TextAnswer> {
    let mut ordered: Vec<_> = answers.iter().filter(|a| a.is_valid_for(text)).collect();
    ordered.sort_by_key(|a| (a.start, a.end));
    let mut end = 0;
    ordered.retain(|answer| {
        if answer.start < end {
            return false;
        }
        end = answer.end;
        true
    });
    ordered
}

/// Rebase links through one text edit. Edited passages lose their stale links.
/// The caller commits the text and returned links in the same scene command,
/// so Undo restores both together instead of reconstructing an old answer.
pub fn rebase(answers: &mut Vec<TextAnswer>, before: &str, after: &str) {
    let prefix: usize = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum();
    let suffix: usize = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum();
    let old_end = before.len() - suffix;
    let delta = after.len() as i128 - before.len() as i128;
    answers.retain_mut(|answer| {
        if !answer.is_valid_for(before) {
            return false;
        }
        if answer.end <= prefix {
            return answer.is_valid_for(after);
        }
        if answer.start < old_end {
            return false;
        }
        let shifted = |offset: usize| usize::try_from(offset as i128 + delta).ok();
        let (Some(start), Some(end)) = (shifted(answer.start), shifted(answer.end)) else {
            return false;
        };
        answer.start = start;
        answer.end = end;
        answer.is_valid_for(after)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn review_replacement_requires_the_immutable_utf8_source_revision_and_range() {
        let review = TextReview {
            id: "real-request-branch-0-1".into(),
            group: "real-request".into(),
            source: "A café & tea.".into(),
            source_revision: Some("rev1".into()),
            selection: Some(2..7),
            question: "Rewrite this passage".into(),
            text: "coffee".into(),
            edit: true,
            output: TextOutputRef {
                review_id: Some("real-request-branch-0-1".into()),
                source: crate::scene::SourceUri {
                    locator: "private/session.json".into(),
                },
                response: TextAnswerRef {
                    session: "real-provider-session".into(),
                    through: 2,
                },
            },
        };
        assert_eq!(
            review.replacement("A café & tea.", Some("rev1")).as_deref(),
            Some("A coffee & tea.")
        );
        assert!(review.replacement("A newer source", Some("rev1")).is_none());
        assert!(review.replacement("A café & tea.", Some("rev2")).is_none());
        let bytes = serde_json::to_vec(&review).unwrap();
        let reopened: TextReview = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reopened, review);
        let legacy: crate::scene::GeneratorSettings = serde_json::from_str("{}").unwrap();
        assert!(!legacy.text_response_only);
        assert!(legacy.text_reviews.is_empty());
        assert!(legacy.shown_review.is_none());
    }

    fn link(text: &str, range: Range<usize>) -> TextAnswer {
        TextAnswer::new(
            text,
            range,
            TextAnswerRef {
                session: "relative/conversation.json".into(),
                through: 2,
            },
            "Why this passage?".into(),
            "The recorded answer.".into(),
        )
        .unwrap()
    }

    #[test]
    fn unicode_widget_ranges_are_byte_safe() {
        let text = "A 🌳 café 道";
        let range = byte_range(text, 2..8).unwrap();
        assert_eq!(&text[range.clone()], "🌳 café");
        let answer = link(text, range);
        assert_eq!(answer.char_range(text), Some(2..8));
        assert!(byte_range(text, 4..4).is_none());
        assert!(byte_range(text, 0..99).is_none());
        assert!(TextAnswer::new(
            text,
            3..5,
            answer.response.clone(),
            String::new(),
            String::new()
        )
        .is_none());
    }

    #[test]
    fn outside_edits_shift_only_the_offsets() {
        let before = "First. Shade trees. Last.";
        let original = link(before, 7..18);
        let mut answers = vec![original.clone()];
        let after = format!("🌳 {before}");
        rebase(&mut answers, before, &after);
        assert_eq!(answers[0].start, 12);
        assert_eq!(answers[0].end, 23);
        assert_eq!(answers[0].response, original.response);
        let next = format!("{after} More.");
        rebase(&mut answers, &after, &next);
        assert!(answers[0].is_valid_for(&next));
        rebase(&mut answers, &next, before);
        // A combined replacement spanning the anchor cannot claim it survived.
        assert!(answers.is_empty());
    }

    #[test]
    fn edits_inside_or_removing_a_passage_invalidate_it() {
        let text = "Keep the trees here.";
        for edited in ["Keep the flowers here.", "Keep here."] {
            let mut answers = vec![link(text, 9..14)];
            rebase(&mut answers, text, edited);
            assert!(answers.is_empty());
        }
    }

    #[test]
    fn unicode_diff_never_slices_a_partial_scalar() {
        let before = "é: keep 🌳";
        let after = "ê: keep 🌳";
        let mut answers = vec![link(before, 4..before.len())];
        rebase(&mut answers, before, after);
        assert!(answers[0].is_valid_for(after));
        let mut changed = vec![link(before, 0..2)];
        rebase(&mut changed, before, after);
        assert!(changed.is_empty());
    }

    #[test]
    fn repeat_query_replaces_overlapping_link_and_round_trips() {
        let text = "First. Shade trees. Last.";
        let mut answers = vec![link(text, 7..18)];
        let mut next = link(text, 7..18);
        next.response.through = 4;
        next.answer = "A newer response.".into();
        assert!(upsert(&mut answers, text, next.clone()));
        assert_eq!(answers, vec![next]);
        let encoded = serde_json::to_string(&answers).unwrap();
        assert_eq!(
            serde_json::from_str::<Vec<TextAnswer>>(&encoded).unwrap(),
            answers
        );
        let mut stale = answers[0].clone();
        stale.quote = "Something else".into();
        assert!(!upsert(&mut answers, text, stale));
    }
    #[test]
    fn linked_source_revision_refresh_invalidates_same_quote_and_preserves_origin() {
        let text = "A café & tea.";
        let mut answer = link(text, 2..7);
        answer.source_revision = Some("source-v1".into());
        answer.origin = Some(crate::scene::SourceUri {
            locator: "custom/archive/session.json".into(),
        });
        assert!(answer.is_valid_for_revision(text, Some("source-v1")));
        assert!(!answer.is_valid_for_revision(text, Some("source-v2")));
        assert!(!answer.is_valid_for_revision(text, None));
        let restored: TextAnswer =
            serde_json::from_value(serde_json::to_value(&answer).unwrap()).unwrap();
        assert_eq!(restored, answer);
        let legacy:TextAnswer=serde_json::from_str(r#"{"start":2,"end":7,"quote":"café","question":"why","answer":"coffee","response":{"session":"old","through":2}}"#).unwrap();
        assert!(legacy.origin.is_none());
        assert!(legacy.source_revision.is_none());
    }
}
