//! Real participant messages; no scene synchronization or agent authority.
use super::*;
use atlas_relay::{Client, Event, Host, Invite, Profile};
use std::collections::{HashSet, VecDeque};
mod photos;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredHuman {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Vec<u8>>,
}

enum Pending {
    Incoming {
        id: String,
    },
    Outgoing {
        recipient: String,
        id: String,
        text: String,
        thread: String,
    },
}
struct Setup {
    host: Option<Host>,
    client: Client,
    invitation: Option<String>,
}
#[derive(Default)]
pub(super) struct HumanChat {
    pub setup: chrome::PeopleSetupState,
    host: Option<Host>,
    client: Option<Client>,
    setup_rx: Option<std::sync::mpsc::Receiver<Result<Setup, String>>>,
    pub online: HashSet<String>,
    pending: HashMap<u64, Pending>,
    incoming: VecDeque<(Profile, String, String)>,
    outbound: HashMap<String, String>,
    acks: VecDeque<String>,
    profile: Option<Profile>,
    photos: photos::PhotoCache,
    contacts_dirty: bool,
}
impl HumanChat {
    pub(super) fn photo(&self, id: &str) -> Option<egui::load::SizedTexture> {
        self.photos.photo(id)
    }
}
impl SlateApp {
    fn local_chat_profile(&self) -> Profile {
        Profile {
            id: self.settings.presence_identity.id.clone(),
            name: self.settings.presence_identity.name.clone(),
            avatar: self.presence.photo.avatar().map(<[u8]>::to_vec),
        }
    }
    pub(super) fn presence_people_setup(&mut self, ctx: &egui::Context) {
        let palette = self.palette();
        let actions = chrome::people_setup_ui(ctx, &palette, &mut self.presence.human_chat.setup);
        for action in actions {
            use chrome::PeopleSetupAction::*;
            match action {
                CopyInvite => ctx.copy_text(self.presence.human_chat.setup.host_invite.clone()),
                Leave => self.disconnect_people(),
                Host { bind } => {
                    self.disconnect_people();
                    let profile = self.local_chat_profile();
                    let (tx, rx) = std::sync::mpsc::channel();
                    self.presence.human_chat.setup_rx = Some(rx);
                    self.presence.human_chat.setup.status =
                        "Creating an encrypted conversation…".into();
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let result = (|| {
                            let address = bind.parse().map_err(|_| {
                                "Enter a numeric listen address, for example 127.0.0.1:8787"
                                    .to_owned()
                            })?;
                            let host =
                                atlas_relay::Host::start(address).map_err(|e| e.to_string())?;
                            let invite = host.invite();
                            let invitation = invite.encode().map_err(|e| e.to_string())?;
                            let client =
                                Client::connect(invite, profile).map_err(|e| e.to_string())?;
                            Ok(Setup {
                                host: Some(host),
                                client,
                                invitation: Some(invitation),
                            })
                        })();
                        let _ = tx.send(result);
                        ctx.request_repaint();
                    });
                }
                Join { invite } => {
                    let parsed = match Invite::decode(&invite) {
                        Ok(i) => i,
                        Err(e) => {
                            self.presence.human_chat.setup.status = e.to_string();
                            continue;
                        }
                    };
                    self.disconnect_people();
                    match Client::connect(parsed, self.local_chat_profile()) {
                        Ok(client) => {
                            self.presence.human_chat.client = Some(client);
                            self.presence.human_chat.setup.invite.clear();
                            self.presence.human_chat.setup.status = "Connecting…".into();
                        }
                        Err(e) => self.presence.human_chat.setup.status = e.to_string(),
                    }
                }
            }
        }
    }
    fn disconnect_people(&mut self) {
        self.fail_human_outbox();
        let chat = &mut self.presence.human_chat;
        if let Some(client) = chat.client.take() {
            client.disconnect();
        }
        chat.host = None;
        chat.online.clear();
        chat.setup_rx = None;
        chat.pending.clear();
        chat.incoming.clear();
        chat.acks.clear();
        chat.setup.host_invite.clear();
        chat.setup.status = "Not connected · saved conversations remain available".into();
    }
    fn remember_person(&mut self, profile: &Profile) {
        if profile.id == self.settings.presence_identity.id {
            return;
        }
        if let Some(old) = self
            .settings
            .presence_contacts
            .iter_mut()
            .find(|p| p.id == profile.id)
        {
            if old.name == profile.name && old.avatar == profile.avatar {
                return;
            }
            old.name = profile.name.clone();
            old.avatar = profile.avatar.clone();
        } else if self.settings.presence_contacts.len() < 64 {
            self.settings.presence_contacts.push(StoredHuman {
                id: profile.id.clone(),
                name: profile.name.clone(),
                avatar: profile.avatar.clone(),
            });
        } else {
            return;
        }
        self.presence.human_chat.contacts_dirty = true;
    }
    pub(super) fn pump_presence_people(&mut self, ctx: &egui::Context) {
        self.presence
            .human_chat
            .photos
            .pump(ctx, &self.settings.presence_contacts);
        if let Some(result) = self
            .presence
            .human_chat
            .setup_rx
            .as_ref()
            .and_then(|r| r.try_recv().ok())
        {
            self.presence.human_chat.setup_rx = None;
            match result {
                Ok(setup) => {
                    self.presence.human_chat.host = setup.host;
                    self.presence.human_chat.client = Some(setup.client);
                    self.presence.human_chat.setup.host_invite =
                        setup.invitation.unwrap_or_default();
                }
                Err(e) => self.presence.human_chat.setup.status = e,
            }
        }
        let profile = self.local_chat_profile();
        if self.presence.human_chat.profile.as_ref() != Some(&profile) {
            if let Some(client) = &self.presence.human_chat.client {
                if client.update_profile(profile.clone()).is_ok() {
                    self.presence.human_chat.profile = Some(profile);
                }
            }
        }
        let events: Vec<_> = self
            .presence
            .human_chat
            .client
            .as_ref()
            .map(|c| c.events().try_iter().take(24).collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Connected { peers } | Event::Peers { peers } => {
                    self.presence.human_chat.online = peers.iter().map(|p| p.id.clone()).collect();
                    for peer in peers {
                        self.remember_person(&peer);
                    }
                    self.presence.human_chat.setup.status =
                        "Connected · direct messages only; board edits are not shared".into();
                }
                Event::Profile { profile } => self.remember_person(&profile),
                Event::Incoming {
                    from,
                    message_id,
                    text,
                } => {
                    if self.presence.human_chat.incoming.len() < 128 {
                        self.remember_person(&from);
                        self.presence
                            .human_chat
                            .incoming
                            .push_back((from, message_id, text));
                    } else {
                        self.disconnect_people();
                        self.toast(
                            "Message queue filled; reconnect to retry unacknowledged messages",
                        );
                    }
                }
                Event::Accepted { message_id } => self.human_delivery(&message_id, "accepted"),
                Event::Ack { message_id, .. } => {
                    self.human_delivery(&message_id, "delivered");
                }
                Event::Rejected { message_id, reason } => {
                    self.human_delivery(&message_id, "failed");
                    self.toast(reason);
                }
                Event::Error { message } => self.presence.human_chat.setup.status = message,
                Event::Disconnected => {
                    self.presence.human_chat.online.clear();
                    self.fail_human_outbox();
                }
            }
        }
        for _ in 0..24 {
            if self.presence.human_chat.pending.len() >= 128 {
                break;
            }
            let Some((from, id, text)) = self.presence.human_chat.incoming.pop_front() else {
                break;
            };
            let thread = format!("human:{}", from.id);
            let Ok(conversation) = self.presence.conversations.edit(&thread) else {
                self.presence
                    .human_chat
                    .incoming
                    .push_front((from, id, text));
                break;
            };
            if let Some(old) = conversation
                .request_index(&id)
                .map(|i| &conversation.turns[i])
            {
                if old.text != text || old.role != "peer" {
                    self.toast("A conflicting message identifier was refused");
                    continue;
                }
            } else {
                conversation.push("peer", &text, &from.name, None, &id, "received");
            }
            let receipt = self.presence.conversations.save_with_receipt(&thread);
            self.presence
                .human_chat
                .pending
                .insert(receipt, Pending::Incoming { id });
        }
        for receipt in self.presence.conversations.take_receipts() {
            let Some(pending) = self.presence.human_chat.pending.remove(&receipt) else {
                continue;
            };
            match pending {
                Pending::Incoming { id } => {
                    self.presence.human_chat.acks.push_back(id);
                }
                Pending::Outgoing {
                    recipient,
                    id,
                    text,
                    thread,
                } => {
                    let result = self
                        .presence
                        .human_chat
                        .client
                        .as_ref()
                        .ok_or("Not connected".to_owned())
                        .and_then(|c| c.send(&recipient, &id, &text).map_err(|e| e.to_string()));
                    if let Err(e) = result {
                        self.human_delivery(&id, "failed");
                        if let Ok(c) = self.presence.conversations.edit(&thread) {
                            if c.draft.is_empty() {
                                c.set_draft(&text);
                            }
                        }
                        self.presence.conversations.save(&thread);
                        self.toast(e);
                    }
                }
            }
        }
        for receipt in self.presence.conversations.take_failed_receipts() {
            if let Some(Pending::Outgoing {
                id, thread, text, ..
            }) = self.presence.human_chat.pending.remove(&receipt)
            {
                self.human_delivery(&id, "failed");
                if let Ok(c) = self.presence.conversations.edit(&thread) {
                    if c.draft.is_empty() {
                        c.set_draft(&text);
                    }
                }
            }
            // A failed incoming write is never acknowledged. Explicit peer
            // retry will deduplicate the local row and retry its durable save.
        }
        for _ in 0..24 {
            let Some(id) = self.presence.human_chat.acks.front() else {
                break;
            };
            let result = self.presence.human_chat.client.as_ref().map(|c| c.ack(id));
            match result {
                Some(Ok(())) => {
                    self.presence.human_chat.acks.pop_front();
                }
                Some(Err(atlas_relay::RelayError::Backpressure)) => break,
                _ => {
                    self.presence.human_chat.acks.clear();
                    break;
                }
            }
        }
        if self
            .presence
            .human_chat
            .client
            .as_ref()
            .is_some_and(|c| c.is_disconnected())
        {
            self.presence.human_chat.online.clear();
            self.fail_human_outbox();
            if self
                .presence
                .human_chat
                .setup
                .status
                .starts_with("Connected")
            {
                self.presence.human_chat.setup.status =
                    "Disconnected · messages remain saved; reconnect to continue".into();
            }
        }
        if self.presence.human_chat.client.is_some() || self.presence.human_chat.setup_rx.is_some()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        // A room roster may change many profiles in one pump. Persist the
        // bounded local preferences once through their canonical owner.
        if std::mem::take(&mut self.presence.human_chat.contacts_dirty) {
            self.settings.save();
        }
    }
    fn human_delivery(&mut self, message: &str, status: &str) {
        let Some(id) = self.presence.human_chat.outbound.get(message).cloned() else {
            return;
        };
        if let Ok(c) = self.presence.conversations.edit(&id) {
            if let Some(index) = c.request_index(message) {
                if c.turns[index].role == "user" {
                    c.set_turn_status(index, status);
                    if status == "failed" && c.draft.is_empty() {
                        let text = c.turns[index].text.clone();
                        c.set_draft(&text);
                    }
                }
            }
        }
        self.presence.conversations.save(&id);
        if status != "accepted" {
            self.presence.human_chat.outbound.remove(message);
        }
    }
    fn fail_human_outbox(&mut self) {
        let ids: Vec<_> = self.presence.human_chat.outbound.keys().cloned().collect();
        for id in ids {
            self.human_delivery(&id, "failed");
        }
    }
    pub(crate) fn presence_send_human(&mut self, who: &str, text: &str) -> Result<(), String> {
        let peer = who.strip_prefix("human:").ok_or("Unknown person")?;
        if !self.presence.human_chat.online.contains(peer) {
            return Err(
                "This person is offline; reconnect before sending. Your draft is retained.".into(),
            );
        }
        if text.trim().is_empty() || text.len() > atlas_relay::MAX_TEXT_BYTES {
            return Err("Messages can contain up to 32 KiB".into());
        }
        if self.presence.human_chat.pending.len() >= 128
            || self.presence.human_chat.outbound.len() >= 128
        {
            return Err("Waiting for earlier messages to save; your draft is retained".into());
        }
        let name = self.settings.presence_identity.name.clone();
        let c = self.presence.conversations.edit(who)?;
        let id = if let Some(index) = c
            .turns
            .iter()
            .enumerate()
            .rev()
            .find(|(_, t)| t.role == "user" && t.status == "failed" && t.text == text.trim())
            .map(|(index, _)| index)
        {
            c.set_turn_status(index, "pending");
            c.turns[index].request.clone()
        } else {
            let id = atlas_agent::request_id();
            c.push("user", text.trim(), &name, None, &id, "pending");
            id
        };
        c.set_draft("");
        let receipt = self.presence.conversations.save_with_receipt(who);
        self.presence
            .human_chat
            .outbound
            .insert(id.clone(), who.into());
        self.presence.human_chat.pending.insert(
            receipt,
            Pending::Outgoing {
                recipient: peer.into(),
                id,
                text: text.trim().into(),
                thread: who.into(),
            },
        );
        Ok(())
    }
    pub(crate) fn presence_retry_message(
        &mut self,
        who: &str,
        message: &str,
    ) -> Result<(), String> {
        let text = self
            .presence_conversation(who)
            .and_then(|c| {
                c.turns.iter().find(|t| {
                    t.id == message
                        && t.role == "user"
                        && (t.status == "failed"
                            || !who.starts_with("human:")
                                && matches!(
                                    t.status.as_str(),
                                    "stopped" | "not-sent" | "unconfirmed"
                                ))
                })
            })
            .map(|t| (t.text.clone(), t.request.clone()))
            .ok_or("This message is not awaiting a retry")?;
        if who.starts_with("human:") {
            return self.presence_send_chat(who, &text.0);
        }
        let c = self.presence.conversations.edit(who)?;
        let draft = c.draft.clone();
        let pending = c.draft_images.clone();
        let images = c.message_images.get(&text.1).cloned().unwrap_or_default();
        c.set_draft_images(images);
        let result = self.presence_send_chat(who, &text.0);
        let c = self.presence.conversations.edit(who)?;
        c.set_draft(&draft);
        c.set_draft_images(pending);
        self.presence.conversations.save(who);
        result
    }
}
