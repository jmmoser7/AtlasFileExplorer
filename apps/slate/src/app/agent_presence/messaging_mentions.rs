//! Bounded live eligibility for durable board messages; never restored as jobs.
use super::super::conversation::{BoardMessageOrigin, ConversationTurn};
use super::super::conversation_store::ReceiptConsumer;
use super::super::*;
use std::collections::HashSet;

#[derive(Default)]
pub(in super::super) struct MentionQueue {
    entries: Vec<QueuedMention>,
}
struct QueuedMention {
    who: String,
    request: String,
    origin: BoardMessageOrigin,
    binding: BoardLinkSession,
    grant: CollaboratorGrant,
    task_request: String,
    control_epoch: u64,
    receipt: Option<u64>,
    dispatching: bool,
    canceled: bool,
    checked_generation: Option<u64>,
}
pub(super) fn text_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hash);
    hash.finish()
}
impl SlateApp {
    pub(in super::super) fn queue_presence_mention(
        &mut self,
        who: &str,
        node: slate_doc::NodeId,
        text_hash: u64,
        text: &str,
    ) -> Result<(), String> {
        submission::validate_message_text(text)?;
        let source = self
            .doc()
            .scene
            .node(node)
            .and_then(|n| {
                if let slate_doc::scene::NodeKind::Text(t) = &n.kind {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .ok_or("Board message source disappeared")?;
        if source.len() > submission::MESSAGE_BYTES {
            return Err("Board address card exceeds 32768 UTF-8 bytes".into());
        }
        let a = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .ok_or("Collaborator is no longer present")?;
        let grant = a.grant.clone().ok_or("Board grant was revoked")?;
        let request = atlas_agent::request_id();
        let origin = BoardMessageOrigin {
            tab: self.tab().id,
            workbook: self.tab().path.clone(),
            node,
            text_hash,
            recipient: who.into(),
            human: self.settings.presence_identity.id.clone(),
        };
        let mut queued = QueuedMention {
            who: who.into(),
            request: request.clone(),
            origin: origin.clone(),
            binding: a.binding.clone(),
            grant,
            task_request: a.request.clone(),
            control_epoch: a.control_epoch,
            receipt: None,
            dispatching: false,
            canceled: false,
            checked_generation: None,
        };
        let full = self.presence.mentions.entries.len() >= 64
            || self
                .presence
                .mentions
                .entries
                .iter()
                .filter(|q| q.who == who)
                .count()
                >= 16;
        let c = self.presence.conversations.edit(who)?;
        c.append_turn(ConversationTurn {
            id: atlas_agent::request_id(),
            role: "user".into(),
            text: text.trim().into(),
            author: self.settings.presence_identity.name.clone(),
            provider: Some(a.profile.provider.clone()),
            request,
            at: atlas_ai::context::now_secs(),
            status: if full { "not-sent" } else { "queued" }.into(),
            board_origin: Some(origin),
        });
        if full {
            c.push(
                "system",
                "Board message queue is full; this message was not sent.",
                "Slate",
                None,
                "",
                "complete",
            );
            self.presence.conversations.save(who);
        } else {
            match self
                .presence
                .conversations
                .save_with_receipt_for(who, ReceiptConsumer::BoardMentions)
            {
                Ok(receipt) => {
                    queued.receipt = Some(receipt);
                    self.presence.mentions.entries.push(queued);
                }
                Err(reason) => self.fail_presence_mention(&queued, &reason),
            }
        }
        Ok(())
    }
    fn fail_presence_mention(&mut self, q: &QueuedMention, reason: &str) {
        if let Ok(c) = self.presence.conversations.edit(&q.who) {
            if let Some(index) = c.request_index(&q.request) {
                c.set_turn_status(index, "not-sent");
            }
            c.push("system", reason, "Slate", None, "", "complete");
            self.presence.conversations.save(&q.who);
        }
    }
    fn validate_presence_mention(&self, q: &mut QueuedMention) -> Result<bool, &'static str> {
        if self.tab().id != q.origin.tab
            || self.tab().path != q.origin.workbook
            || self.tab().read_only
        {
            return Err("Board changed; queued message was not sent.");
        }
        if self
            .pending_stage_proposals()
            .iter()
            .any(|p| p.status == slate_doc::ProposalStatus::RecoveryRequired)
        {
            return Err("Board recovery needs human attention; queued message was not sent.");
        }
        let a = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == q.who)
            .ok_or("Collaborator retired; queued message was not sent.")?;
        if a.tab != q.origin.tab
            || a.departure.is_some()
            || a.binding != q.binding
            || a.grant.as_ref().is_none_or(|g| g.id != q.grant.id)
            || a.control_epoch != q.control_epoch
            || a.request != q.task_request
            || a.pending_harness.is_some()
            || self.settings.presence_identity.id != q.origin.human
        {
            return Err("Collaborator controls or task changed; queued message was not sent.");
        }
        let source = self
            .doc()
            .scene
            .node(q.origin.node)
            .and_then(|n| {
                if let slate_doc::scene::NodeKind::Text(t) = &n.kind {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .ok_or("Board message was removed; queued message was not sent.")?;
        if source.len() > submission::MESSAGE_BYTES {
            return Err(
                "Board address card exceeds 32768 UTF-8 bytes; queued message was not sent.",
            );
        }
        let generation = self.doc().scene.scene_gen();
        if q.checked_generation != Some(generation) && text_hash(source) != q.origin.text_hash {
            return Err("Board message was edited; its older queued message was not sent.");
        }
        q.checked_generation = Some(generation);
        // The human resolved this stable recipient when enqueueing. A cosmetic
        // display-name edit cannot revoke that address while source is unchanged.
        Ok(!a.paused && !self.presence.task_busy(&q.who) && a.pending_harness.is_none())
    }
    pub(in super::super) fn pump_presence_mentions(&mut self, board_visible: bool) {
        let saved: HashSet<_> = self
            .presence
            .conversations
            .take_receipts_for(ReceiptConsumer::BoardMentions, false)
            .into_iter()
            .collect();
        let failed: HashSet<_> = self
            .presence
            .conversations
            .take_receipts_for(ReceiptConsumer::BoardMentions, true)
            .into_iter()
            .collect();
        let mut entries = std::mem::take(&mut self.presence.mentions.entries);
        let mut send = None;
        let mut first_for_recipient = HashSet::new();
        for (index, q) in entries.iter_mut().enumerate() {
            if let Some(receipt) = q.receipt {
                if saved.contains(&receipt) {
                    q.receipt = None;
                } else if failed.contains(&receipt) {
                    q.receipt = None;
                    if !q.canceled {
                        self.fail_presence_mention(
                            q,
                            "Could not persist board message; it was not sent.",
                        );
                    }
                    q.canceled = true;
                }
            }
            if q.canceled {
                continue;
            }
            let ready = match self.validate_presence_mention(q) {
                Ok(ready) => ready,
                Err(reason) => {
                    self.fail_presence_mention(q, reason);
                    q.canceled = true;
                    continue;
                }
            };
            let first = first_for_recipient.insert(q.who.clone());
            if !first || !board_visible || !ready || q.receipt.is_some() || send.is_some() {
                continue;
            }
            if q.dispatching {
                send = Some(index);
            } else {
                let result = self
                    .presence
                    .conversations
                    .edit(&q.who)
                    .and_then(|c| {
                        let index = c
                            .request_index(&q.request)
                            .ok_or("Queued message disappeared")?;
                        c.set_turn_status(index, "dispatching");
                        Ok(())
                    })
                    .and_then(|()| {
                        self.presence
                            .conversations
                            .save_with_receipt_for(&q.who, ReceiptConsumer::BoardMentions)
                    });
                match result {
                    Ok(receipt) => {
                        q.receipt = Some(receipt);
                        q.dispatching = true;
                    }
                    Err(reason) => {
                        self.fail_presence_mention(q, &reason);
                        q.canceled = true;
                    }
                }
            }
        }
        // Remove eligibility before the single send attempt. Failures never retry.
        if let Some(index) = send {
            let q = entries.remove(index);
            let text = self
                .presence
                .conversations
                .get(&q.who)
                .and_then(|c| c.request_index(&q.request).and_then(|i| c.turns.get(i)))
                .map(|t| t.text.clone());
            let exact_grant = self
                .presence
                .agents
                .iter()
                .find(|a| a.profile.id == q.who)
                .is_some_and(|a| a.grant.as_ref() == Some(&q.grant));
            let result = text
                .filter(|_| exact_grant)
                .ok_or_else(|| "Queued message or grant changed; message was not sent".to_owned())
                .and_then(|text| {
                    self.submit_presence_message(&q.who, &text, None, Some(&q.request))
                });
            if let Err(reason) = result {
                self.fail_presence_mention(&q, &reason);
            } else {
                for waiting in &mut entries {
                    if waiting.who == q.who && waiting.task_request == q.task_request {
                        waiting.task_request = q.request.clone();
                    }
                }
            }
        }
        entries.retain(|q| !q.canceled || q.receipt.is_some());
        self.presence.mentions.entries = entries;
    }
}
