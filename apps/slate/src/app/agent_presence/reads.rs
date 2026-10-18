//! Incremental, generation-fenced board reads. No whole-board pass on a frame.
use super::*;
use serde_json::{json, Value};

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
}
impl ReadJob {
    pub(super) fn new(call: BoardCall, generation: u64) -> Result<Self, String> {
        let (node_ids, participant_ids, offset, limit): (
            &[String],
            Option<Vec<String>>,
            usize,
            usize,
        ) = match &call.operation {
            BoardOperation::Read {
                node_ids,
                offset,
                limit,
            } => (node_ids, None, *offset, *limit),
            BoardOperation::ReadParticipants {
                participant_ids,
                offset,
                limit,
            } => (&[][..], Some(participant_ids.clone()), *offset, *limit),
            BoardOperation::Invoke {
                command, targets, ..
            } if command == "presence.read_image" && targets.len() == 1 => (targets, None, 0, 1),
            _ => return Err("Choose one explicit image node".into()),
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
        })
    }
}
impl SlateApp {
    pub(super) fn begin_presence_read(
        &mut self,
        a: &mut Collaborator,
        call: &BoardCall,
    ) -> Result<(), String> {
        if a.read.is_some() || a.work.is_some() {
            return Err("Wait for the current board action or read".into());
        }
        let mut job = ReadJob::new(call.clone(), self.doc().scene.scene_gen())?;
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
    pub(super) fn advance_presence_read(
        &mut self,
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
            let visible = if a.paused && a.chat_only {
                // Reading context to answer the person does not release the
                // addressed avatar's paused board position.
                true
            } else if let Some(m) = self.presence.roster.session_mut(&a.motion) {
                let sample = m.advance(elapsed);
                if sample.phase == MotionPhase::ReadyToCommit {
                    m.resolve_ready(|_| true);
                    true
                } else {
                    sample.phase == MotionPhase::Idle
                }
            } else {
                true
            };
            Ok(job.done && visible)
        })();
        match result {
            Ok(false) => {
                a.read = Some(job);
                None
            }
            Ok(true) => {
                if let Some(participant_ids) = &job.participant_ids {
                    let grant = a.grant.as_ref().unwrap();
                    let result =
                        self.participant_page(a, grant, participant_ids, job.offset, job.limit);
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
                        "result": result.result.chars().take(512).collect::<String>()
                    }));
                }
                a.activity = "working".into();
                let value = json!({"workspace_id":a.binding.workspace_id,"objects":job.objects,"image_source":job.image_source,"next_offset":next,"commands":self.presence_command_schema(),"conversations":grant.conversation_ids,"participants":participants,"proposal_mode":!grant.mutation_autonomy});
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

    fn live_participant_descriptor(other: &Collaborator) -> Value {
        json!({
            "id": other.profile.id,
            "name": other.profile.name.chars().take(256).collect::<String>(),
            "status": other.status.chars().take(512).collect::<String>(),
            "result": other.snapshot.as_ref()
                .and_then(|snapshot| snapshot.turns.iter().rev().find(|turn| turn.role == "assistant"))
                .map(|turn| turn.text.chars().take(512).collect::<String>())
        })
    }

    fn retired_participant_descriptor(result: &helper_scope::HelperResult) -> Value {
        json!({
            "id": result.id,
            "parent_id": result.parent_id,
            "request_id": result.request_id,
            "name": result.name.chars().take(256).collect::<String>(),
            "retired": true,
            "status": result.status.chars().take(512).collect::<String>(),
            "outcome": result.outcome,
            "expired": result.expired,
            "result": result.result.chars().take(512).collect::<String>()
        })
    }

    fn participant_page(
        &self,
        caller: &Collaborator,
        grant: &CollaboratorGrant,
        explicit: &[String],
        offset: usize,
        limit: usize,
    ) -> Result<Value, String> {
        if explicit.iter().any(|id| {
            self.presence
                .helper_results
                .iter()
                .any(|result| result.id == *id && result.parent_id != caller.profile.id)
        }) {
            return Err("A retained helper outcome is available only to its parent".into());
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
