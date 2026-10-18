//! Thin Slate adapter for ephemeral, scoped board collaborators.
//! Shared shell paints; atlas-agent owns motion/grants; normal commands mutate.
use super::SlateApp;
use atlas_agent::{collaborator::*, presence::*, AgentSession};
use atlas_shell::agent_presence as chrome;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};

mod calls;
mod consent_policy;
mod controls;
pub(crate) use consent_policy::AgentConsentPolicy;
mod attachments;
mod brush_preview;
pub(crate) use brush_preview::BrushCoverage;
mod conversation;
mod conversation_store;
#[cfg(test)]
mod conversation_tests;
mod harness;
mod updates;
pub(crate) use conversation::HumanProfile;
mod avatar_input;
mod helper_scope;
mod human_chat;
pub(crate) use human_chat::StoredHuman;
mod lifecycle;
#[cfg(test)]
mod lifecycle_tests;
mod messaging;
mod surface;
pub(crate) use messaging::message_text_changed;
mod creation_conflicts;
#[cfg(test)]
mod live_codex_tests;
mod own_chats;
mod portal_ops;
mod preview;
mod proposals;
mod read_descriptors;
mod reads;
mod review;
mod schedule_surface;
mod scheduling;
mod submission;
#[cfg(test)]
mod tests;
mod view;
mod witness;
mod work;

pub(crate) const BOARD_COMMANDS: &[&str] = &[
    "board.objects.move",
    "board.objects.resize",
    "board.objects.crop",
    "board.objects.order",
    "board.objects.create_frame",
    "board.objects.create_text",
    "board.objects.create_shape",
    "board.objects.create_train",
    "board.objects.create_generator",
    "board.objects.place_images",
    "board.objects.wire",
    "board.objects.delete",
    "presence.send",
    "presence.helper",
    "presence.read_image",
    "presence.read_text",
    "board.objects.set_text",
    "board.shape.edit",
    "board.wire.edit",
    "portal.agent.run",
    "app.save",
    "app.feedback.report",
    "board.paths.draw",
    "board.eraser.stroke",
    "board.path.join",
    "board.objects.create_portal",
    "portal.atlas.list",
    "portal.atlas.highlight",
    "portal.atlas.read_file",
    "portal.web.read_text",
    "portal.web.capture",
    "portal.web.navigate",
    "portal.web.click",
    "portal.web.scroll",
    "portal.web.type",
    "portal.web.key",
    "portal.web.focus",
];

/// Identity preferences only. Joining, grants, jobs and positions are never saved.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredAgent {
    pub id: String,
    pub name: String,
    pub hat: String,
    pub provider: String,
    #[serde(default)]
    pub retired: bool,
}

pub(crate) struct Collaborator {
    profile: StoredAgent,
    tab: u64,
    motion: PresenceId,
    binding: BoardLinkSession,
    grant: Option<CollaboratorGrant>,
    dir: PathBuf,
    request: String,
    sequences: u128,
    calls: usize,
    paused: bool,
    resume_task: bool,
    parent: Option<String>,
    helper_outcome: Option<helper_scope::TaskOutcome>,
    runtime: Option<atlas_ai::runtime::CodexLink>,
    cursor: Option<atlas_ai::sidecar::CursorLink>,
    snapshot: Option<Arc<AgentSession>>,
    status: String,
    activity: String,
    last_event: Option<atlas_agent::AgentActivity>,
    last_active: Instant,
    work: Option<witness::WitnessedCommand>,
    read: Option<reads::ReadJob>,
    portal: Option<portal_ops::PortalOp>,
    control_epoch: u64,
    owned_conversations: Vec<String>,
    departure: Option<(Instant, String, WorldPoint)>,
    had_response: bool,
    relays: usize,
    observed_target: Option<slate_doc::NodeId>,
    joined_at: Instant,
    inflight: bool,
    seen_running: bool,
    launched: Vec<(slate_doc::NodeId, String)>,
    pending_harness: Option<(String, String)>,
    switch_offer: Option<String>,
    chat_only: bool,
    reply_from: usize,
}

impl Collaborator {
    fn board_action_active(&self) -> bool {
        self.work.is_some() || self.read.is_some() || self.portal.is_some()
    }
    fn position(&self, roster: &PresenceRoster) -> Option<WorldPoint> {
        let sample = roster.session(&self.motion)?.sample();
        Some(
            self.work
                .as_ref()
                .map_or(sample.position, |work| work.display_position(sample)),
        )
    }
    fn cancel_motion(&self, roster: &mut PresenceRoster) -> Result<(), String> {
        if let Some(motion) = roster.session_mut(&self.motion) {
            if let Some(work) = &self.work {
                work.cancel_motion(motion)?;
            } else {
                motion.cancel_motion();
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct PresenceRuntime {
    proposals: HashMap<String, proposals::ProposalContext>,
    roster: PresenceRoster,
    agents: Vec<Collaborator>,
    links: atlas_ai::board_link::BoardLinks,
    sources: atlas_ai::agent::AgentSources,
    painter: chrome::PresencePainter,
    chrome: chrome::RosterState,
    avatar_hold: Option<avatar_input::InputHold>,
    avatar_gesture_captured: bool,
    last_tick: Option<Instant>,
    inspect: Option<String>,
    activity_view: atlas_shell::selection_tools::generator::activity::ActivityState,
    mention_hashes: HashMap<slate_doc::NodeId, u64>,
    mentions: messaging::MentionQueue,
    helper_results: Vec<helper_scope::HelperResult>,
    helper_endpoints: std::collections::HashSet<String>,
    held_nodes: std::collections::HashSet<slate_doc::NodeId>,
    conversations: conversation_store::ConversationStore,
    attachments: attachments::AttachmentJobs,
    photo: avatar_input::ProfilePhoto,
    human_chat: human_chat::HumanChat,
    scheduling: scheduling::PresenceSchedules,
    brush_preview: brush_preview::DrawingBrushPreview,
}
impl PresenceRuntime {
    fn next_helper_name(&self, parent: &str, parent_name: &str) -> String {
        (1..=129)
            .map(|number| format!("{parent_name} helper {number}"))
            .find(|name| {
                !self
                    .agents
                    .iter()
                    .any(|a| a.parent.as_deref() == Some(parent) && &a.profile.name == name)
            })
            .expect("the bounded roster always has a free helper name")
    }
    pub(crate) fn holds_node(&self, id: slate_doc::NodeId) -> bool {
        self.held_nodes.contains(&id)
    }
    fn refresh_held(&mut self, tab: u64) {
        self.held_nodes.clear();
        self.held_nodes.extend(
            self.agents
                .iter()
                .filter(|a| a.tab == tab)
                .filter_map(|a| a.work.as_ref())
                .filter(|work| work.purpose.replaces_objects())
                .flat_map(|w| w.previews.keys().copied()),
        );
    }
    pub(crate) fn agents_empty(&self) -> bool {
        self.agents.is_empty()
    }
    pub(crate) fn has_live_agents(&self, tab: u64) -> bool {
        self.agents
            .iter()
            .any(|agent| agent.tab == tab && agent.departure.is_none())
    }
}

fn id(value: &str) -> Result<PresenceId, String> {
    PresenceId::new(value).map_err(|e| e.to_string())
}
fn point(x: f32, y: f32) -> WorldPoint {
    WorldPoint {
        x: f64::from(x),
        y: f64::from(y),
    }
}
fn activity_id(action: atlas_agent::AgentActivityAction) -> String {
    serde_json::to_value(action)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "working".into())
}

impl SlateApp {
    pub(crate) fn dispatch_presence(
        &mut self,
        ctx: &egui::Context,
        command: &str,
        detail: Option<&str>,
    ) -> bool {
        let value: serde_json::Value = detail
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let who = value["id"].as_str().unwrap_or("").to_owned();
        self.presence_release_avatar_hold(ctx, Some(&who), command == "presence.address");
        if command == "presence.schedule" {
            if let Err(error) = self.presence_schedule_command(&who, &value) {
                self.toast(error);
            }
            return true;
        }
        if command == "presence.address" {
            if let Err(error) = self.presence_address(ctx, &who) {
                self.toast(error);
            }
            return true;
        }
        if command == "presence.accept" {
            self.accept_stage_proposal(&who);
            return true;
        }
        if command == "presence.reject" {
            self.reject_stage_proposal(&who);
            return true;
        }
        let action = match command {
            "presence.retry" => chrome::PresenceAction::Retry {
                id: who,
                message: value["message"].as_str().unwrap_or("").into(),
            },
            "presence.people" => chrome::PresenceAction::ConnectPeople,
            "presence.workspace" => chrome::PresenceAction::SetupWorkspace,
            "presence.add" => chrome::PresenceAction::Add {
                name: value["name"].as_str().unwrap_or("Agent").into(),
                hat: value["hat"].as_str().unwrap_or("none").into(),
                provider: value["provider"].as_str().unwrap_or("codex").into(),
            },
            "presence.name" => chrome::PresenceAction::Rename {
                id: who,
                name: value["name"].as_str().unwrap_or("Agent").into(),
            },
            "presence.hat" => chrome::PresenceAction::SetHat {
                id: who,
                hat: value["hat"].as_str().unwrap_or("none").into(),
            },
            "presence.provider" => chrome::PresenceAction::SetProvider {
                id: who,
                provider: value["provider"].as_str().unwrap_or("codex").into(),
            },
            "presence.provider.switch" => chrome::PresenceAction::SwitchProvider {
                id: who,
                provider: value["provider"].as_str().unwrap_or("").into(),
                after_task: value["after_task"].as_bool().unwrap_or(false),
            },
            "presence.provider.cancel" => chrome::PresenceAction::CancelProviderSwitch { id: who },
            "presence.model" => chrome::PresenceAction::SetModel {
                id: who,
                model: value["model"].as_str().unwrap_or("").into(),
            },
            "presence.photo" => chrome::PresenceAction::SetPhoto { id: who },
            "presence.photo.remove" => chrome::PresenceAction::RemovePhoto { id: who },
            "presence.attach" => chrome::PresenceAction::Attach { id: who },
            "presence.attach.remove" => chrome::PresenceAction::RemoveAttachment {
                id: who,
                index: value["index"].as_u64().unwrap_or(u64::MAX) as usize,
            },
            "presence.full_access" => chrome::PresenceAction::SetFullAccess {
                id: who,
                enabled: value["enabled"].as_bool().unwrap_or(false),
            },
            "presence.draft" => chrome::PresenceAction::Draft {
                id: who,
                text: value["text"].as_str().unwrap_or("").into(),
            },
            "presence.clone" => chrome::PresenceAction::Clone { id: who },
            "presence.schedule" => chrome::PresenceAction::Schedule { id: who },
            "presence.pause" => chrome::PresenceAction::Pause {
                id: who,
                paused: value["paused"].as_bool().unwrap_or(true),
            },
            "presence.cancel" => chrome::PresenceAction::Cancel { id: who },
            "presence.retire" => chrome::PresenceAction::Retire {
                id: who,
                style: value["style"].as_str().unwrap_or("bow").into(),
            },
            "presence.restore" => chrome::PresenceAction::Restore { id: who },
            "presence.locate" => chrome::PresenceAction::Locate { id: who },
            "presence.helper" => chrome::PresenceAction::SpawnHelper { id: who },
            "presence.activity" => chrome::PresenceAction::ShowActivity { id: who },
            "presence.chat" => chrome::PresenceAction::ShowConversation { id: who },
            "presence.send" => chrome::PresenceAction::Send {
                id: who,
                text: value["text"].as_str().unwrap_or("").into(),
            },
            "presence.grant" => chrome::PresenceAction::SetAutonomy {
                id: who,
                enabled: value["enabled"].as_bool().unwrap_or(false),
            },
            "presence.revoke" => chrome::PresenceAction::Revoke { id: who },
            "presence.identity" => chrome::PresenceAction::CopyIdentity { id: who },
            "presence.link" => chrome::PresenceAction::CopyLink { id: who },
            "presence.file_inputs" => chrome::PresenceAction::SetFileInputs {
                id: who,
                enabled: value["enabled"].as_bool().unwrap_or(false),
            },
            "presence.own_chats" => chrome::PresenceAction::SetOwnChats {
                id: who,
                enabled: value["enabled"].as_bool().unwrap_or(false),
            },
            _ => return false,
        };
        if let Err(error) = self.apply_presence_action(ctx, action) {
            self.toast(error);
        }
        true
    }
    fn persist_presence_profiles(&mut self) {
        for a in &self.presence.agents {
            if a.parent.is_some() {
                continue;
            }
            if let Some(saved) = self
                .settings
                .collaborators
                .iter_mut()
                .find(|p| p.id == a.profile.id)
            {
                *saved = a.profile.clone();
            } else {
                self.settings.collaborators.push(a.profile.clone());
            }
        }
        self.settings.collaborators.truncate(PROFILE_LIMIT);
        self.settings.save();
    }
}
