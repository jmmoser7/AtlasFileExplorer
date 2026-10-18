use crate::{validate_message, wire::Command, Event, Profile, QUEUE};
use crossbeam_channel::{Receiver, Sender};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub(crate) enum Input {
    Join {
        connection: u64,
        profile: Profile,
        outgoing: Sender<Event>,
        live: Arc<AtomicBool>,
    },
    Data {
        connection: u64,
        command: Command,
    },
    Leave {
        connection: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_participant_cannot_fill_the_room_with_unacknowledged_sends() {
        let mut room = Room::default();
        let (alice, alice_events) = crossbeam_channel::bounded(QUEUE);
        let (bob, bob_events) = crossbeam_channel::bounded(QUEUE);
        room.join(
            1,
            Profile::new("alice", "Alice"),
            alice,
            Arc::new(AtomicBool::new(true)),
        );
        room.join(
            2,
            Profile::new("bob", "Bob"),
            bob,
            Arc::new(AtomicBool::new(true)),
        );
        for i in 0..33 {
            room.send(
                "alice",
                "bob".into(),
                format!("message-{i}"),
                "Hello".into(),
            );
        }
        assert_eq!(room.deliveries.len(), 32);
        assert_eq!(
            bob_events
                .try_iter()
                .filter(|e| matches!(e, Event::Incoming { .. }))
                .count(),
            32
        );
        assert!(alice_events.try_iter().any(
            |e| matches!(e, Event::Rejected { message_id, .. } if message_id == "message-32")
        ));
        // Other participants still have capacity in the same room.
        room.send("bob", "alice".into(), "reply".into(), "Reply".into());
        assert_eq!(room.deliveries.len(), 33);
    }
}

struct Peer {
    connection: u64,
    profile: Profile,
    outgoing: Sender<Event>,
    live: Arc<AtomicBool>,
}

struct Delivery {
    sender: String,
    recipient: String,
    fingerprint: Vec<u8>,
    acknowledged: bool,
    unconfirmed: bool,
}

#[derive(Default)]
struct Room {
    peers: HashMap<String, Peer>,
    deliveries: HashMap<String, Delivery>,
    order: VecDeque<String>,
}

pub(crate) fn run(input: Receiver<Input>, stop: Arc<AtomicBool>) {
    let mut room = Room::default();
    while !stop.load(Ordering::Acquire) {
        match input.recv_timeout(Duration::from_millis(40)) {
            Ok(Input::Join {
                connection,
                profile,
                outgoing,
                live,
            }) => room.join(connection, profile, outgoing, live),
            Ok(Input::Data {
                connection,
                command,
            }) => {
                if let Some(id) = room
                    .peers
                    .iter()
                    .find_map(|(id, p)| (p.connection == connection).then(|| id.clone()))
                {
                    room.command(&id, command);
                }
            }
            Ok(Input::Leave { connection }) => {
                if let Some(id) = room
                    .peers
                    .iter()
                    .find_map(|(id, p)| (p.connection == connection).then(|| id.clone()))
                {
                    room.leave(&id);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }
        // A full command queue must not leave a dead socket present forever.
        // Socket liveness is independent of delivery of its Leave command.
        let expired: Vec<_> = room
            .peers
            .iter()
            .filter(|(_, p)| !p.live.load(Ordering::Acquire))
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            room.leave(&id);
        }
    }
    for p in room.peers.values() {
        let _ = p.outgoing.try_send(Event::Disconnected);
    }
}

impl Room {
    fn profiles(&self) -> Vec<Profile> {
        let mut peers: Vec<_> = self.peers.values().map(|p| p.profile.clone()).collect();
        peers.sort_by(|a, b| a.id.cmp(&b.id));
        peers
    }

    fn join(
        &mut self,
        connection: u64,
        profile: Profile,
        outgoing: Sender<Event>,
        live: Arc<AtomicBool>,
    ) {
        if profile.validate().is_err()
            || self.peers.contains_key(&profile.id)
            || self.peers.len() >= crate::MAX_PEERS
        {
            let _ = outgoing.try_send(Event::Error {
                message: "Participant identity is already connected or room is full".into(),
            });
            return;
        }
        let id = profile.id.clone();
        self.peers.insert(
            id.clone(),
            Peer {
                connection,
                profile,
                outgoing,
                live,
            },
        );
        let profiles = self.profiles();
        self.emit(
            &id,
            Event::Connected {
                peers: profiles.clone(),
            },
        );
        self.broadcast(Event::Peers { peers: profiles });
    }

    fn command(&mut self, sender: &str, command: Command) {
        match command {
            Command::Send {
                recipient,
                message_id,
                text,
            } => self.send(sender, recipient, message_id, text),
            Command::Ack { message_id } => self.ack(sender, message_id),
            Command::UpdateProfile { profile }
                if profile.id == sender && profile.validate().is_ok() =>
            {
                self.peers.get_mut(sender).unwrap().profile = profile.clone();
                self.broadcast(Event::Profile { profile });
            }
            Command::Disconnect => self.leave(sender),
            _ => {
                self.emit(
                    sender,
                    Event::Error {
                        message: "Invalid participant command".into(),
                    },
                );
            }
        }
    }

    fn send(&mut self, sender: &str, recipient: String, id: String, text: String) {
        if validate_message(&recipient, &id, &text).is_err() {
            self.reject(sender, id, "Invalid message");
            return;
        }
        let fingerprint = Sha256::digest(text.as_bytes()).to_vec();
        let retry = if let Some(previous) = self.deliveries.get(&id) {
            if previous.sender != sender
                || previous.recipient != recipient
                || previous.fingerprint != fingerprint
            {
                self.reject(sender, id, "Message identifier was already used");
                return;
            } else if previous.acknowledged {
                self.emit(
                    sender,
                    Event::Ack {
                        message_id: id,
                        recipient,
                    },
                );
                return;
            } else if !previous.unconfirmed {
                self.emit(sender, Event::Accepted { message_id: id });
                return;
            }
            // Only another explicit Send retries an unconfirmed delivery.
            // A recipient that persisted it before losing the ACK deduplicates
            // by ID and acknowledges again, without showing a second turn.
            true
        } else {
            false
        };
        if !retry {
            self.prune();
        }
        if !retry && self.deliveries.len() >= QUEUE * 8 {
            self.reject(sender, id, "Room delivery queue is full");
            return;
        }
        // One unresponsive sender/recipient pair cannot occupy the room's
        // entire delivery ledger. Twenty peers can each hold 32 outstanding
        // sends without exhausting its 1024-record duplicate window.
        if self
            .deliveries
            .values()
            .filter(|d| d.sender == sender && !d.acknowledged && !d.unconfirmed)
            .count()
            >= 32
        {
            self.reject(
                sender,
                id,
                "Waiting for recipient acknowledgments; message was not queued",
            );
            return;
        }
        let Some(from) = self.peers.get(sender).map(|p| p.profile.clone()) else {
            return;
        };
        let event = Event::Incoming {
            from,
            message_id: id.clone(),
            text,
        };
        if !self.emit(&recipient, event) {
            self.reject(
                sender,
                id,
                "Recipient is disconnected or busy; message was not queued",
            );
            return;
        }
        self.deliveries.insert(
            id.clone(),
            Delivery {
                sender: sender.into(),
                recipient,
                fingerprint,
                acknowledged: false,
                unconfirmed: false,
            },
        );
        if !retry {
            self.order.push_back(id.clone());
        }
        self.emit(sender, Event::Accepted { message_id: id });
    }

    fn ack(&mut self, recipient: &str, id: String) {
        let Some(delivery) = self.deliveries.get_mut(&id) else {
            return;
        };
        // An unrelated room member cannot fabricate a delivery receipt.
        if delivery.recipient != recipient {
            return;
        }
        delivery.acknowledged = true;
        delivery.unconfirmed = false;
        let sender = delivery.sender.clone();
        self.emit(
            &sender,
            Event::Ack {
                message_id: id,
                recipient: recipient.into(),
            },
        );
    }

    fn reject(&mut self, sender: &str, message_id: String, reason: &str) {
        self.emit(
            sender,
            Event::Rejected {
                message_id,
                reason: reason.into(),
            },
        );
    }

    fn emit(&self, id: &str, event: Event) -> bool {
        self.peers
            .get(id)
            .is_some_and(|p| p.outgoing.try_send(event).is_ok())
    }

    fn broadcast(&mut self, event: Event) {
        let failed: Vec<_> = self
            .peers
            .iter()
            .filter_map(|(id, p)| p.outgoing.try_send(event.clone()).err().map(|_| id.clone()))
            .collect();
        for id in failed {
            self.leave(&id);
        }
    }

    fn leave(&mut self, id: &str) {
        if self.peers.remove(id).is_none() {
            return;
        }
        let failed: Vec<_> = self
            .deliveries
            .iter()
            .filter(|(_, d)| !d.acknowledged && d.recipient == id)
            .map(|(id, d)| (id.clone(), d.sender.clone()))
            .collect();
        for (message, sender) in failed {
            if let Some(delivery) = self.deliveries.get_mut(&message) {
                delivery.unconfirmed = true;
            }
            self.reject(
                &sender,
                message,
                "Recipient disconnected before acknowledging; delivery is unconfirmed",
            );
        }
        self.broadcast(Event::Peers {
            peers: self.profiles(),
        });
    }

    fn prune(&mut self) {
        // Keep a bounded recent duplicate window. Unacknowledged deliveries
        // are never evicted to turn a duplicate into a second visible message.
        let mut attempts = self.order.len();
        while self.deliveries.len() >= QUEUE * 8 && attempts > 0 {
            attempts -= 1;
            let Some(id) = self.order.pop_front() else {
                break;
            };
            if self
                .deliveries
                .get(&id)
                .is_some_and(|d| d.acknowledged || d.unconfirmed)
            {
                self.deliveries.remove(&id);
            } else {
                self.order.push_back(id);
            }
        }
    }
}
