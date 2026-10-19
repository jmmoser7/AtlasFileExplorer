use super::*;

pub(super) fn helper_result_preview(text: &str) -> String {
    let end = text_page::utf8_page_end(text, 0, 512).unwrap_or(0);
    text[..end].to_owned()
}

impl SlateApp {
    fn live_participant_descriptor(other: &Collaborator) -> Value {
        let latest = other.snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .turns
                .iter()
                .rev()
                .find(|turn| turn.role == "assistant")
        });
        let text = latest.map_or("", |turn| turn.text.as_str());
        let preview = helper_result_preview(text);
        let truncated = text.len() > preview.len();
        json!({
            "id": other.profile.id,
            "name": other.profile.name.chars().take(256).collect::<String>(),
            "status": other.status.chars().take(512).collect::<String>(),
            "result": preview,
            "total_bytes": text.len(),
            "next_text_cursor": null,
            "truncated": truncated,
            "text_pageable": false
        })
    }

    fn retired_participant_descriptor(result: &helper_scope::HelperResult) -> Value {
        let preview_end = text_page::utf8_page_end(&result.result, 0, 512).unwrap_or(0);
        json!({
            "id": result.id,
            "parent_id": result.parent_id,
            "request_id": result.request_id,
            "name": result.name.chars().take(256).collect::<String>(),
            "retired": true,
            "status": result.status.chars().take(512).collect::<String>(),
            "outcome": result.outcome,
            "expired": result.expired,
            "result": &result.result[..preview_end],
            "total_bytes": result.total_bytes,
            "next_text_cursor": (preview_end < result.result.len()).then_some(preview_end),
            "truncated": result.truncated || result.result.len() > 512,
            "text_generation": result.request_id,
            "text_pageable": true
        })
    }

    pub(super) fn participant_page(
        &self,
        caller: &Collaborator,
        grant: &CollaboratorGrant,
        explicit: &[String],
        offset: usize,
        limit: usize,
        text_cursor: Option<usize>,
        text_limit: Option<usize>,
        text_generation: Option<&str>,
    ) -> Result<Value, String> {
        if explicit.iter().any(|id| {
            self.presence
                .helper_results
                .iter()
                .any(|result| result.id == *id && result.parent_id != caller.profile.id)
        }) {
            return Err("A retained helper outcome is available only to its parent".into());
        }
        if text_cursor.is_some() || text_limit.is_some() || text_generation.is_some() {
            let [id] = explicit else {
                return Err("Text pages require one explicit retained helper participant".into());
            };
            let generation = text_generation
                .ok_or("Text pages require the retained helper request generation")?;
            let result = self
                .presence
                .helper_results
                .iter()
                .find(|result| {
                    result.id == *id
                        && result.parent_id == caller.profile.id
                        && result.request_id == generation
                })
                .ok_or("Text pages are available only for a retained helper outcome")?;
            if !grant.conversation_ids.contains(id) {
                return Err("Participant read is outside this collaborator's scope".into());
            }
            let cursor = text_cursor.unwrap_or(0);
            let limit = text_limit.unwrap_or(4096);
            let end = text_page::utf8_page_end(&result.result, cursor, limit)?;
            let next_text_cursor = (end < result.result.len()).then_some(end);
            let descriptor = Self::retired_participant_descriptor(result);
            return Ok(json!({
                "participants": [descriptor],
                "total": 1,
                "next_offset": null,
                "text": &result.result[cursor..end],
                "text_cursor": cursor,
                "next_text_cursor": next_text_cursor,
                "total_bytes": result.total_bytes,
                "truncated": result.truncated,
                "text_generation": result.request_id
            }));
        }
        let requested = |id: &str| explicit.is_empty() || explicit.iter().any(|item| item == id);
        let mut available = Vec::new();
        // The dispatcher temporarily removes the caller from the live roster.
        for other in std::iter::once(caller).chain(
            self.presence
                .agents
                .iter()
                .filter(|other| other.profile.id != caller.profile.id),
        ) {
            if other.departure.is_none()
                && grant.conversation_ids.contains(&other.profile.id)
                && requested(&other.profile.id)
            {
                available.push(Self::live_participant_descriptor(other));
            }
        }
        for result in &self.presence.helper_results {
            if result.parent_id == caller.profile.id
                && grant.conversation_ids.contains(&result.id)
                && requested(&result.id)
            {
                available.push(Self::retired_participant_descriptor(result));
            }
        }
        let total = available.len();
        let mut participants = Vec::new();
        let mut bytes = serde_json::to_vec(&json!({
            "participants": [],
            "total": total,
            "next_offset": 1_000_000,
        }))
        .map_err(|_| "Participant reply could not be serialized")?
        .len();
        for participant in available.into_iter().skip(offset).take(limit) {
            let record_bytes = serde_json::to_vec(&participant)
                .map_err(|_| "Participant reply could not be serialized")?
                .len()
                + 1;
            if bytes + record_bytes > 60_000 {
                break;
            }
            bytes += record_bytes;
            participants.push(participant);
        }
        let emitted = participants.len();
        let next_offset = (offset + emitted < total).then_some(offset + emitted);
        let value = json!({
            "participants": participants,
            "total": total,
            "next_offset": next_offset,
        });
        if serde_json::to_vec(&value).map_or(true, |reply| reply.len() > 60_000) {
            return Err("Request a smaller participant page".into());
        }
        Ok(value)
    }
}
