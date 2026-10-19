//! Incremental, generation-fenced board reads. No whole-board pass on a frame.
use super::*;
use serde_json::{json, Value};
mod participants;
mod text_page;
use self::participants::helper_result_preview;

pub(super) struct ReadJob {
    pub call: BoardCall,
    generation: u64,
    index: usize,
    skip: usize,
    limit: usize,
    offset: usize,
    objects: Vec<Value>,
    explicit: Vec<slate_doc::NodeId>,
    done: bool,
    image: Option<crate::app::board_agent::ScopedImageInput>,
    image_source: Option<PathBuf>,
    participant_ids: Option<Vec<String>>,
    text_cursor: Option<usize>,
    text_limit: Option<usize>,
    text_generation: Option<String>,
    text_page: Option<text_page::TextPage>,
    preview: Option<super::board_preview::PreviewJob>,
}
impl ReadJob {
    pub(super) fn new(call: BoardCall, generation: u64) -> Result<Self, String> {
        let text_page = text_page::TextPage::parse(&call, generation)?;
        let (node_ids, participant_ids, offset, limit, text_cursor, text_limit, text_generation): (
            &[String],
            Option<Vec<String>>,
            usize,
            usize,
            Option<usize>,
            Option<usize>,
            Option<String>,
        ) = match &call.operation {
            BoardOperation::Read {
                node_ids,
                offset,
                limit,
            } => (node_ids, None, *offset, *limit, None, None, None),
            BoardOperation::ReadParticipants {
                participant_ids,
                offset,
                limit,
                text_cursor,
                text_limit,
                text_generation,
            } => (
                &[][..],
                Some(participant_ids.clone()),
                *offset,
                *limit,
                *text_cursor,
                *text_limit,
                text_generation.clone(),
            ),
            BoardOperation::Invoke {
                command, targets, ..
            } if command == "presence.read_image" && targets.len() == 1 => {
                (targets, None, 0, 1, None, None, None)
            }
            BoardOperation::Invoke {
                command, targets, ..
            } if command == "presence.read_text" && targets.len() == 1 => {
                (targets, None, 0, 1, None, None, None)
            }
            BoardOperation::Invoke {
                command, targets, ..
            } if command == "presence.preview" => (targets, None, 0, 1, None, None, None),
            _ => {
                return Err(
                    "Choose one explicit image, authored text node, or frame preview".into(),
                )
            }
        };
        let explicit = node_ids
            .iter()
            .map(|id| {
                id.parse::<u64>()
                    .map(slate_doc::NodeId)
                    .map_err(|_| "Use explicit numeric board IDs".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            generation,
            index: 0,
            skip: offset,
            limit,
            offset,
            call,
            objects: vec![],
            explicit,
            done: false,
            image: None,
            image_source: None,
            participant_ids,
            text_cursor,
            text_limit,
            text_generation,
            text_page,
            preview: None,
        })
    }
}
impl SlateApp {
    pub(super) fn begin_presence_read(
        &mut self,
        a: &mut Collaborator,
        call: &BoardCall,
    ) -> Result<(), String> {
        if a.board_action_active() {
            return Err("Wait for the current board action or read".into());
        }
        let mut job = ReadJob::new(call.clone(), self.doc().scene.scene_gen())?;
        if matches!(&call.operation, BoardOperation::Invoke { command, .. } if command == "presence.preview")
        {
            job.preview = Some(super::board_preview::PreviewJob::new(
                self,
                a.grant.as_ref().ok_or("Read grant revoked")?,
                call,
            )?);
        }
        if matches!(&call.operation, BoardOperation::Invoke { command, .. } if command == "presence.read_image")
        {
            let grant = a.grant.as_ref().ok_or("Read grant revoked")?;
            let node = job.explicit[0];
            if !grant.allows_node(&node.0.to_string())
                || !grant
                    .readable_sources
                    .iter()
                    .any(|s| s == "board-file-inputs")
            {
                return Err(
                    "Allow board images and wired inputs for this participant before inspecting an image".into(),
                );
            }
            job.image = Some(self.prepare_scoped_image_input(node, &call.id)?);
        }
        if let Some(page) = &job.text_page {
            page.validate(self)?;
        }
        a.observed_target = job.explicit.first().copied();
        if let Some((target_id, (x, y))) = (!a.paused)
            .then(|| {
                a.observed_target
                    .and_then(|id| self.doc().scene.node(id))
                    .map(|n| (n.id, n.rect.center()))
            })
            .flatten()
        {
            if let Some(m) = self.presence.roster.session_mut(&a.motion) {
                m.enqueue(MotionPlan {
                    session_id: a.motion.clone(),
                    request_id: id(&a.request)?,
                    action_id: id(&atlas_agent::request_id())?,
                    target_nodes: vec![id(&target_id.0.to_string())?],
                    start: m.sample().position,
                    end: point(x, y),
                    work_end: None,
                    durations: MotionDurations {
                        travel_ms: 220,
                        grab_ms: 50,
                        work_ms: 150,
                    },
                })
                .map_err(|e| e.to_string())?;
            }
        }
        a.activity = "reading".into();
        a.read = Some(job);
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn advance_presence_read(
        &mut self,
        a: &mut Collaborator,
        budget: &mut usize,
        elapsed: std::time::Duration,
    ) -> Option<BoardReply> {
        self.advance_presence_read_with_context(None, a, budget, elapsed)
    }
    pub(super) fn advance_presence_read_with_context(
        &mut self,
        ctx: Option<&egui::Context>,
        a: &mut Collaborator,
        budget: &mut usize,
        elapsed: std::time::Duration,
    ) -> Option<BoardReply> {
        let mut job = a.read.take()?;
        let result = (|| -> Result<bool, String> {
            let grant = a.grant.as_ref().ok_or("Read grant revoked")?;
            grant
                .authorize(&job.call, &a.binding, &a.request)
                .map_err(str::to_owned)?;
            if a.tab != self.tab().id
                || (job.participant_ids.is_none() && job.generation != self.doc().scene.scene_gen())
            {
                return Err("Board changed during the read; request a fresh page".into());
            }
            if let Some(preview) = job.preview.as_mut() {
                let ctx = ctx.ok_or("A board preview requires the active Slate paint context")?;
                job.done = preview.poll(self, ctx, grant)?;
                let visible = self.advance_presence_read_motion(a, elapsed);
                return Ok(job.done && visible);
            }
            if job.participant_ids.is_some() {
                job.done = true;
                return Ok(true);
            }
            if let Some(image) = &job.image {
                // Recheck source authority after asynchronous preparation too.
                if !grant
                    .readable_sources
                    .iter()
                    .any(|s| s == "board-file-inputs")
                {
                    return Err("Image source grant revoked".into());
                }
                job.image_source = self.poll_scoped_image_input(image)?;
                if job.image_source.is_none() {
                    return Ok(false);
                }
            }
            if job.text_page.is_some() {
                if *budget == 0 {
                    return Ok(false);
                }
                *budget -= 1;
                job.done = true;
            }
            let nodes = &self.doc().scene.nodes;
            while !job.done && *budget > 0 {
                let node = if job.explicit.is_empty() {
                    nodes.get(job.index)
                } else {
                    match job.explicit.get(job.index) {
                        Some(id) => Some(self.doc().scene.node(*id).ok_or(
                            "A requested object no longer exists; request fresh board context",
                        )?),
                        None => None,
                    }
                };
                job.index += 1;
                *budget -= 1;
                let Some(node) = node else {
                    job.done = true;
                    break;
                };
                if !grant.allows_node(&node.id.0.to_string()) {
                    continue;
                }
                if job.skip > 0 {
                    job.skip -= 1;
                    continue;
                }
                job.objects
                    .push(self.presence_object_descriptor(node, grant));
                if job.objects.len() == job.limit {
                    job.done = true;
                }
            }
            let visible = self.advance_presence_read_motion(a, elapsed);
            Ok(job.done && visible)
        })();
        match result {
            Ok(false) => {
                a.read = Some(job);
                None
            }
            Ok(true) => {
                if let Some(preview) = job.preview.as_mut() {
                    a.activity = "working".into();
                    return Some(BoardReply {
                        id: job.call.id,
                        session_id: a.binding.session_id.clone(),
                        request_id: a.request.clone(),
                        success: true,
                        result: preview.result.take().unwrap_or(Value::Null),
                    });
                }
                if let Some(page) = &job.text_page {
                    a.activity = "working".into();
                    return Some(match page.value(self) {
                        Ok(result) => BoardReply {
                            id: job.call.id.clone(),
                            session_id: a.binding.session_id.clone(),
                            request_id: a.request.clone(),
                            success: true,
                            result,
                        },
                        Err(error) => BoardReply::error(&job.call, &error),
                    });
                }
                if let Some(participant_ids) = &job.participant_ids {
                    let grant = a.grant.as_ref().unwrap();
                    let result = self.participant_page(
                        a,
                        grant,
                        participant_ids,
                        job.offset,
                        job.limit,
                        job.text_cursor,
                        job.text_limit,
                        job.text_generation.as_deref(),
                    );
                    a.activity = "working".into();
                    return Some(match result {
                        Ok(value) => BoardReply {
                            id: job.call.id,
                            session_id: a.binding.session_id.clone(),
                            request_id: a.request.clone(),
                            success: true,
                            result: value,
                        },
                        Err(error) => BoardReply::error(&job.call, &error),
                    });
                }
                let total = if job.explicit.is_empty() {
                    self.doc().scene.nodes.len()
                } else {
                    job.explicit.len()
                };
                let next = if job.index < total {
                    Some(job.offset + job.objects.len())
                } else {
                    None
                };
                let grant = a.grant.as_ref().unwrap();
                let mut participants: Vec<_> = self
                    .presence
                    .agents
                    .iter()
                    .filter(|other| {
                        other.departure.is_none()
                            && grant.conversation_ids.contains(&other.profile.id)
                    })
                    .take(32)
                    // Preserve the original board-read descriptor and byte bounds.
                    .map(|other| json!({
                        "id": other.profile.id,
                        "name": other.profile.name,
                        "status": other.status,
                        "result": other.snapshot.as_ref()
                            .and_then(|snapshot| snapshot.turns.iter().rev().find(|turn| turn.role == "assistant"))
                            .map(|turn| turn.text.chars().take(256).collect::<String>())
                    }))
                    .collect();
                for result in self
                    .presence
                    .helper_results
                    .iter()
                    .filter(|r| {
                        r.parent_id == a.profile.id && grant.conversation_ids.contains(&r.id)
                    })
                    .take(8)
                {
                    participants.push(json!({
                        "id": result.id,
                        "parent_id": result.parent_id,
                        "request_id": result.request_id,
                        "name": result.name,
                        "retired": true,
                        "status": result.status.chars().take(512).collect::<String>(),
                        "outcome": result.outcome,
                        "expired": result.expired,
                        "result": helper_result_preview(&result.result),
                        "total_bytes": result.total_bytes,
                        "next_text_cursor": text_page::utf8_page_end(&result.result, 0, 512)
                            .ok().filter(|end| *end < result.result.len()),
                        "truncated": result.truncated || result.result.len() > 512,
                        "text_generation": result.request_id,
                        "text_pageable": true
                    }));
                }
                a.activity = "working".into();
                let value = json!({"workspace_id":a.binding.workspace_id,"objects":job.objects,"image_source":job.image_source,"next_offset":next,"commands":self.presence_command_schema(),"conversations":grant.conversation_ids,"participants":participants,"proposal_mode":!grant.mutation_autonomy,
                    "action_budget":{"consumed":a.calls,"remaining":usize::from(grant.max_actions).saturating_sub(a.calls),"limit":grant.max_actions},
                    "tag_catalog":grant.readable_board.then(||crate::app::dispatch::targeted::tags::catalog(self.doc()))});
                if serde_json::to_vec(&value).map_or(true, |v| v.len() > 60_000) {
                    return Some(BoardReply::error(&job.call, "Request a smaller page"));
                }
                Some(BoardReply {
                    id: job.call.id,
                    session_id: a.binding.session_id.clone(),
                    request_id: a.request.clone(),
                    success: true,
                    result: value,
                })
            }
            Err(error) => {
                a.note_helper_outcome("blocked", &error);
                if let Some(m) = self.presence.roster.session_mut(&a.motion) {
                    m.cancel_motion();
                }
                a.activity = "blocked".into();
                a.status = error.clone();
                Some(BoardReply::error(&job.call, &error))
            }
        }
    }
    fn advance_presence_read_motion(
        &mut self,
        a: &Collaborator,
        elapsed: std::time::Duration,
    ) -> bool {
        // Addressed chat reads do not release the avatar's paused board position.
        if a.paused && a.chat_only {
            return true;
        }
        let Some(m) = self.presence.roster.session_mut(&a.motion) else {
            return true;
        };
        let sample = m.advance(elapsed);
        if sample.phase == MotionPhase::ReadyToCommit {
            m.resolve_ready(|_| true);
            true
        } else {
            sample.phase == MotionPhase::Idle
        }
    }
}
