//! Agent runtime types: live preview, generator view, and session state.

use super::*;
use std::collections::VecDeque;

mod generation_preview;

/// Persisted MRU of folders the human bound to an agent portal.
pub(super) const AGENT_RECENTS_KEY: &str = "slate-agent-projects";

type CachedImages = (
    (u64, u64),
    std::sync::Arc<Vec<atlas_ai::agent::ImageOutput>>,
);
type CachedGenerator = ((u64, u64, Option<u64>), std::rc::Rc<GeneratorView>);

pub(crate) struct LivePreview {
    pub(super) tab: u64,
    pub(super) session: String,
    pub(super) request: String,
    pub(super) frame: u64,
    pub(super) step: u32,
    pub(super) steps: u32,
    pub(super) texture: egui::TextureHandle,
}

/// What an image generator reads, as its hover chips show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputRole {
    Prompt,
    Geometry,
    Image,
    Style,
}

impl InputRole {
    /// The role a port reads before anything is wired to it.
    pub fn of_slot(slot: atlas_agent::InputSlot) -> Self {
        match slot {
            atlas_agent::InputSlot::Media | atlas_agent::InputSlot::View => Self::Image,
            atlas_agent::InputSlot::Prompt => Self::Prompt,
            atlas_agent::InputSlot::Style => Self::Style,
        }
    }

    /// Chip name and the colour a chip dot and its port share.
    pub fn look(self, roles: &atlas_shell::tokens::AgentRoleInk) -> (&'static str, Color32) {
        match self {
            Self::Prompt => ("Prompt", roles.prompt),
            Self::Geometry => ("Geometry", roles.geometry),
            Self::Image => ("Image", roles.image),
            Self::Style => ("Style", roles.style),
        }
    }
}

/// Who a wheel notch over an image portal belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageWheel {
    Board,
    Album,
    Model,
}

#[derive(Clone, Debug)]
pub(crate) struct GeneratorInput {
    pub node: NodeId,
    pub role: InputRole,
    pub label: String,
}

/// One generator's resolved inputs, cached per scene and output revision.
#[derive(Clone, Debug, Default)]
pub(crate) struct GeneratorView {
    pub inputs: Vec<GeneratorInput>,
    pub prompt: String,
    pub error: Option<String>,
    /// Explicit Media and Prompt wires are ready; current-image fallback is excluded.
    pub wired_image_and_prompt: bool,
    /// Wired prompts and sources, without the camera of a wired model.
    pub signature: u64,
}

impl GeneratorView {
    pub fn geometry(&self) -> Option<NodeId> {
        self.inputs
            .iter()
            .find(|i| i.role == InputRole::Geometry)
            .map(|i| i.node)
    }

    /// The same rule the engine plans with, so the button names what will run.
    pub fn task(&self) -> atlas_ai::agent::ImageTask {
        atlas_ai::agent::ImageTask::for_inputs(
            self.geometry().is_some(),
            self.inputs.iter().any(|i| i.role == InputRole::Image),
        )
    }
}

#[cfg(test)]
thread_local! {
    pub(crate) static INPUT_RESOLVES_ON_THIS_THREAD: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
    /// Looks for an agent's `place.json` made on this thread.
    pub(crate) static LINK_PROBES_ON_THIS_THREAD: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

#[derive(Clone)]
pub(super) struct QueuedGeneration {
    pub(super) doc: u64,
    pub(super) author: slate_doc::scene::CmdAuthor,
    pub(super) request: atlas_agent::AgentRequest,
    pub(super) inputs: super::input_prep::InputPreparation,
}

#[derive(Clone)]
pub(super) struct ChatSendOrigin {
    pub(super) typing_here: bool,
    pub(super) relayed_from: Option<String>,
    pub(super) preserve_composer: bool,
}

#[derive(Clone)]
pub(super) struct PendingChatSend {
    pub(super) doc: u64,
    pub(super) portal: NodeId,
    pub(super) composer: NodeId,
    pub(super) prompt: String,
    pub(super) workspace: PathBuf,
    pub(super) origin: ChatSendOrigin,
    pub(super) inputs: super::input_prep::InputPreparation,
}

type ConnectionResult = (NodeId, String, Result<AgentSession, String>);
type ModelsResult = (String, Result<Vec<atlas_ai::agent::AgentModel>, String>);
type FitRevision = (
    u64,
    u64,
    u64,
    u64,
    Option<NodeId>,
    Option<NodeId>,
    Option<NodeId>,
    u64,
);
/// Text key and the world-unit line breaks it produced. Zoom is not part of
/// either; `canvas_text::world_text` scales the layout at paint time.
type CachedGalley = (
    u64,
    u64,
    u64,
    std::sync::Arc<atlas_shell::canvas_text::WorldLayout>,
);
type ChatsResult = (String, PathBuf, Result<Vec<CursorChat>, String>);

#[derive(Default)]
pub struct AgentRuntime {
    pub(super) approvals: super::approval::ApprovalDelivery,
    pub(super) artifact: super::artifact::ArtifactUi,
    pub(super) sources: atlas_ai::agent::AgentSources,
    pub(super) image_prep: std::cell::RefCell<super::input_prep::ImageInputPrep>,
    pub(super) chat_preparing: HashMap<NodeId, PendingChatSend>,
    pub(super) project_picker: Option<NodeId>,
    pub(super) catalog_error: Option<String>,
    pub(super) connection_rx: Option<Receiver<ConnectionResult>>,
    pub(super) connection_pending: Option<NodeId>,
    pub(super) connection_tick: Option<Instant>,
    pub(super) connection_background: bool,
    pub(super) models: HashMap<String, Vec<atlas_ai::agent::AgentModel>>,
    pub(super) models_rx: Option<Receiver<ModelsResult>>,
    pub(super) models_started: HashSet<String>,
    /// Synthetic test hosts suppress automatic discovery without inventing readiness.
    #[cfg(test)]
    pub(super) hold_catalog_probes: bool,
    /// Catalogs an open agent editor lists before any card uses them.
    pub(super) models_wanted: HashSet<String>,
    pub(super) models_error: HashMap<String, String>,
    pub(super) artifact_popup: Option<(NodeId, bool)>,
    pub(super) artifact_popup_rect: Option<Rect>,
    /// The open model list remains anchored under its title while zooming.
    pub(super) model_menu: Option<NodeId>,
    /// False until the press that opened the model list is released.
    pub(super) model_menu_armed: bool,
    pub(super) model_menu_rect: Option<Rect>,
    /// Streaming card whose Stop took the press; it stops on release there.
    pub(super) stop_press: Option<NodeId>,
    /// A presentation switch still settling: a card of the conversation, the
    /// journal depth after the switch, and the scene generation since.
    pub(super) projection_settle: Option<(NodeId, usize, u64)>,
    /// HTML (and the rest of the preview catalog) waiting for text vs graphic.
    pub(super) preview_ask: Option<(NodeId, String, Option<Pos2>)>,
    /// False until the pointer that opened the menu has been released.
    pub(super) preview_ask_ready: bool,
    /// File Atlas place requests already applied, `portal:id`.
    pub(super) placed_atlas: HashSet<String>,
    /// Last painted screen position of a train card's referenced-input dot.
    pub(super) context_auto: HashMap<NodeId, Pos2>,
    /// Last painted screen position of the manual-context dot, when it is showing.
    pub(super) context_human: HashMap<NodeId, Pos2>,
    /// Last painted screen position of the return dot, when the card has outputs.
    pub(super) return_dot: HashMap<NodeId, Pos2>,
    /// Press origin on a context dot, so a click can be told from a drag.
    pub(super) context_press: Option<(NodeId, Pos2)>,
    /// Temporary context cards. They vanish when the pointer leaves them.
    pub(super) context_preview: Option<NodeId>,
    pub(super) context_preview_rect: Option<Rect>,
    /// Composer field of every painted tail card, rebuilt each frame, and the
    /// field that currently has the caret.
    pub(super) composer_rects: HashMap<NodeId, Rect>,
    pub(super) composer_editing: Option<NodeId>,
    /// Scroll range of user-sized and partial cards whose content overflows, in world units.
    pub(super) card_overflow: HashMap<NodeId, f32>,
    /// Screen rects of the collapse chevrons painted last frame. A press here
    /// is the chevron's, not a frame resize.
    pub(super) chevron_hits: Vec<Rect>,
    /// Streaming transcript follow, in world units. Absent means follow the bottom.
    pub(super) follow: HashMap<NodeId, train_ux::FollowScroll>,
    /// Collapsed cards shown at twice the capsule while a reply streams.
    /// Display only (Art. VI.3): the card stays collapsed in the document and
    /// returns to the capsule when streaming ends, unless the person folds it.
    pub(super) stream_open: HashSet<NodeId>,
    /// Laid-out Responding labels of streaming cards.
    pub(super) responding: HashMap<NodeId, RespondingLabel>,
    /// Laid-out transcript height per card as (card width, height, content
    /// signature), in world units. Train cards fit to what paint drew, so
    /// bubbles, rules and status rows are never estimated twice. Zoom only ever
    /// grows an entry, so zooming cannot resize a card back and forth.
    pub(super) content_heights: HashMap<NodeId, (f32, f32, u64)>,
    /// Bumped when a painted transcript height changes, so the fit reruns.
    pub(super) measure_epoch: u64,
    /// Press on a human context handle, so a click can be told from a wire drag.
    pub(super) pocket_press: Option<(NodeId, Pos2)>,
    /// Context a handle click brought out of this card, so a second click pockets it.
    pub(super) pocket_open: HashMap<NodeId, Vec<NodeId>>,
    /// Cards holding pocketed context, per scene revision.
    pub(super) pocket_cache: std::cell::RefCell<((u64, u64), HashSet<NodeId>)>,
    /// API-key field. A click here edits text; it does not drag the card.
    pub(super) key_rect: Option<(NodeId, Rect)>,
    pub(super) key_focus: Option<NodeId>,
    pub(super) title_edit: Option<(NodeId, String)>,
    pub(super) spawn_drag: Option<(NodeId, Pos2)>,
    /// Press on the changed-documents dot. A small move places that file.
    pub(super) artifact_drag: Option<(NodeId, Pos2)>,
    pub(super) composer_focus: Option<NodeId>,
    pub(crate) prompt_epoch: u64,
    pub(super) fit_revision: Option<FitRevision>,
    pub(super) fit_keys: HashMap<NodeId, u64>,
    pub(super) title_widths: HashMap<NodeId, f32>,
    pub(super) rail_cache:
        std::cell::RefCell<((u64, u64), Vec<slate_doc::agent_chat::HistoryRail>)>,
    pub(super) bindings: HashMap<NodeId, String>,
    pub(super) context_selection: HashMap<String, Vec<NodeId>>,
    pub(super) requests: HashMap<NodeId, String>,
    pub(super) context_tick: Option<Instant>,
    pub(super) codex: HashMap<String, atlas_ai::runtime::CodexLink>,
    pub(super) programs: Vec<atlas_ai::agent::AgentProvider>,
    pub(super) programs_started: bool,
    pub(super) output_epoch: u64,
    pub(super) image_cache: std::cell::RefCell<HashMap<NodeId, CachedImages>>,
    pub(super) summary_cache: std::cell::RefCell<HashMap<NodeId, CachedGalley>>,

    pub(super) transcript_scroll: HashMap<NodeId, f32>,
    /// Chooser list scroll, in world units so zoom never shifts the rows.
    pub(super) pick_scroll: HashMap<NodeId, f32>,
    pub(crate) sessions: HashMap<NodeId, std::sync::Arc<AgentSession>>,
    pub(super) prompts: HashMap<NodeId, String>,
    /// Draft card → the card whose composer took its text in a presentation
    /// switch. Undo and redo bring the card back or take it away again; the
    /// unsent text follows whichever of the two is on the board.
    pub(super) absorbed_drafts: HashMap<NodeId, NodeId>,
    pub(super) pending: Vec<Proposal>,
    /// Bounded by the pending queue; presence proposals require this live permit.
    pub(super) presence_proposals: HashMap<String, Proposal>,
    pub(super) generation_authors: HashMap<NodeId, slate_doc::scene::CmdAuthor>,
    pub(super) cancelled_generations: HashMap<NodeId, String>,
    pub(super) stage: StageFeed,
    /// Sessions this user let act without asking (`atlas_ai::access`). Loaded
    /// at startup from the per-user store; never journaled.
    pub(super) full_access: std::collections::BTreeSet<String>,
    /// None in tests, so a toggle never touches the real per-user store.
    pub(super) access_path: Option<std::path::PathBuf>,
    pub(super) dashboard_full_access: bool,
    pub(super) access_snapshot: atlas_ai::access::Snapshot,
    pub(super) sidecar_policies: HashMap<String, atlas_ai::sidecar::SidecarPolicy>,
    pub(super) sidecar_startups: HashMap<String, super::prompt::SidecarStartup>,
    /// Cursor sessions whose permissions changed while a reply was running.
    /// Their sidecar restarts before the next message, never mid-reply.
    pub(crate) sidecar_restart: std::collections::BTreeSet<String>,
    /// Schedule per session, read once from its `schedule.json`.
    pub(super) schedules: HashMap<String, Option<atlas_ai::schedule::AgentSchedule>>,
    pub(super) schedule_dialog: Option<schedule::ScheduleDialog>,
    pub(super) recents: Vec<RecentEntry>,
    pub(super) provider_recents: HashMap<String, Vec<RecentEntry>>,
    pub(super) recents_rx: Option<Receiver<HashMap<String, Vec<RecentEntry>>>>,
    pub(super) recents_started: bool,
    pub(super) cover_focus: HashMap<NodeId, usize>,
    /// Generations accepted while this note is already running.
    pub(super) comfy_queue: HashMap<NodeId, VecDeque<QueuedGeneration>>,
    /// Input signature last sent by a live generator.
    pub(super) live_sent: HashMap<NodeId, u64>,
    /// Current live run. Keep starts a new one so the shown frame stays.
    pub(super) live_run: HashMap<NodeId, String>,
    /// When a queued Render began waiting for its 3D mesh.
    pub(super) capture_wait: HashMap<NodeId, Instant>,
    /// Models this generator unlocked so its focused drag can fly them.
    pub(super) steering: HashMap<NodeId, NodeId>,
    /// Image id whose aspect ratio the card was last fitted to.
    pub(super) fitted_image: HashMap<NodeId, String>,
    pub(super) generator_views: std::cell::RefCell<HashMap<NodeId, CachedGenerator>>,
    /// Live signature waiting for typing to pause, and when it first appeared.
    pub(super) live_settle: HashMap<NodeId, (u64, Instant)>,
    /// Newest sampling frame decoded for display: request id, frame number,
    /// step of steps, texture.
    pub(super) previews: HashMap<NodeId, LivePreview>,
    pub(super) live_decodes: generation_preview::GenerationPreviews,
    pub(super) image_imports: super::session::image_import::KnownImageImports,
    #[cfg(test)]
    pub(crate) dispatched: Vec<(NodeId, atlas_agent::AgentRequest)>,
    /// Contents focus — selection is not this (P1.portal.contents-focus).
    pub focused: Option<NodeId>,
    pub(super) ide: CursorIdeStatus,
    pub(super) ide_rx: Option<Receiver<CursorIdeStatus>>,
    pub(super) ide_inflight: bool,
    pub(super) ide_next: Option<Instant>,
    pub(super) chats: HashMap<(String, PathBuf), Vec<CursorChat>>,
    pub(super) chats_rx: Option<Receiver<ChatsResult>>,
    pub(super) chats_inflight: HashMap<PathBuf, Instant>,
    pub(super) pending_chat_pick: Option<NodeId>,
    pub chat_picker: Option<ChatPicker>,
    pub(super) local_turns: HashMap<NodeId, Vec<AgentTurn>>,
    pub(super) sidecar_spawned: HashSet<String>,
    pub(super) sidecar_child: HashMap<String, std::process::Child>,
    pub(super) sidecar_booting: HashSet<String>,
    pub(super) sidecar_boot_tx: Option<Sender<SidecarBoot>>,
    pub(super) sidecar_boot_rx: Option<Receiver<SidecarBoot>>,
    /// Document ownership, rejoin, "Connecting…" sends and sidecar stops.
    pub(crate) life: life::AgentLife,
    pub(super) awaiting: HashMap<NodeId, AgentAwait>,
    pub key_draft: String,
    pub(super) key_entry: Option<NodeId>,
    /// Artifact id → the portal placed for it, so a second click retracts.
    pub(super) spawned: HashMap<(NodeId, String), NodeId>,
    /// Portals animating back into the agent dot after retract.
    pub(super) retract_ghosts: Vec<(Node, Pos2, Instant)>,
    /// Linked `response.txt` sync for text-language-model windows.
    pub(crate) text_output: super::super::agent_text_output::AgentTextOutputWriter,
    /// Output-port menu and text block drafts of generators and text blocks.
    pub(crate) flow: super::super::board_flow::FlowUi,
    /// The capsule stack on chat cards' output circles.
    pub(crate) outputs: outputs::OutputsUi,
    /// Relays between two coding conversations and their wire chips.
    pub(crate) crosstalk: crosstalk::CrosstalkUi,
}

/// In-flight send. Failure is a named state — never a blank transcript.
#[derive(Clone)]
pub(super) enum AgentAwait {
    Sent {
        at: Instant,
        req_at: u64,
    },
    Thinking {
        req_at: u64,
    },
    Responding {
        req_at: u64,
    },
    Failed {
        reason: String,
        actions: Vec<AgentRecover>,
    },
}

/// A named next step — failure is never a dead-end sentence.
#[derive(Clone)]
pub(super) enum AgentRecover {
    OpenUrl {
        label: &'static str,
        url: &'static str,
    },
    OpenPath {
        label: &'static str,
        path: PathBuf,
    },
    PasteKey,
    PickWorkspace,
}

/// How long we wait for the sidecar to pick up a send when no process is alive.
pub(super) const AWAIT_TIMEOUT: Duration = Duration::from_secs(20);
/// Presses accepted while a generator is busy.
pub(super) const GENERATION_QUEUE: usize = 8;
/// How long a Render waits for its 3D mesh before naming the failure.
pub(super) const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);
/// Pause in typing before a live generator renders the new prompt.
pub(super) const LIVE_TYPING_SETTLE: Duration = Duration::from_millis(300);
/// Designed type size of generator hover chips and buttons (scales with zoom).
pub(crate) const GENERATOR_CHIP_PX: f32 = 12.0;

/// One seed per generator session keeps live frames and Keep coherent.
pub(super) fn stable_seed(session: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    session.hash(&mut hasher);
    hasher.finish() >> 1
}

/// Worker-thread result of finding Node, installing `@cursor/sdk`, and spawning.
pub(super) struct SidecarBoot {
    pub(super) generation: u64,
    pub(super) policy: atlas_ai::sidecar::SidecarPolicy,
    /// Tab that sent, so a failure lands on its document after a tab switch.
    pub(super) doc: u64,
    pub(super) portal: NodeId,
    pub(super) session: String,
    pub(super) result: Result<(), String>,
}

/// Palette of saved Cursor chats for one bound folder.
pub struct ChatPicker {
    pub portal: NodeId,
    pub folder: PathBuf,
    pub chats: Vec<CursorChat>,
}

impl AgentRuntime {
    pub fn prompt(&self, id: NodeId) -> Option<&str> {
        self.prompts.get(&id).map(String::as_str)
    }

    pub fn prompt_mut(&mut self, id: NodeId) -> &mut String {
        self.prompts.entry(id).or_default()
    }

    pub fn session(&self, id: NodeId) -> Option<&AgentSession> {
        self.sessions.get(&id).map(std::sync::Arc::as_ref)
    }

    /// The composer of this card has, or is about to take, the caret.
    pub fn composing(&self, id: NodeId) -> bool {
        self.composer_editing == Some(id) || self.composer_focus == Some(id) || self.flow.typing(id)
    }

    /// A focused generator unlocked this model and releases it itself.
    pub fn steers_model(&self, model: NodeId) -> bool {
        self.steering.values().any(|m| *m == model)
    }

    /// The run a card is waiting on or showing.
    pub fn request_of(&self, id: NodeId) -> Option<&str> {
        self.requests.get(&id).map(String::as_str)
    }

    pub fn has_draft(&self, id: NodeId) -> bool {
        self.prompts.contains_key(&id)
    }

    /// Installed models of one catalog, once discovery has run.
    pub fn catalog(&self, key: &str) -> &[atlas_ai::agent::AgentModel] {
        self.models.get(key).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Discover this catalog again (a key was just saved, for example).
    pub fn refresh_catalog(&mut self, key: &str) {
        self.models_started.remove(key);
        self.models_error.remove(key);
        self.want_catalog(key);
    }

    /// Discover a catalog off-thread even though no card uses it yet.
    pub fn want_catalog(&mut self, key: &str) {
        if !self.models_wanted.contains(key) {
            self.models_wanted.insert(key.to_string());
        }
    }

    pub fn pending_for<'a>(&'a self, session: &'a str) -> impl Iterator<Item = &'a Proposal> + 'a {
        self.pending
            .iter()
            .filter(move |p| {
                matches!(
                    p.status,
                    slate_doc::ProposalStatus::Pending
                        | slate_doc::ProposalStatus::RecoveryRequired
                )
            })
            .filter(move |p| p.session.as_deref() == Some(session))
    }

    pub(super) fn remove_pending(&mut self, id: &str) {
        self.presence_proposals.remove(id);
        self.pending.retain(|p| p.id != id);
    }
}
