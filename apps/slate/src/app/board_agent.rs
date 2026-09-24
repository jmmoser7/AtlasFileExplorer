//! Agent portal runtime: file-link context, session status, and staged proposal
//! handling. Everything here is derived state until a human accepts a proposal.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use atlas_ai::agent::{
    launch_provider, AgentContext, AgentRequest, AgentSession, AgentStatus, AgentTurn,
};
use atlas_ai::cursor_chats::CursorChat;
use atlas_ai::launch::CursorIdeStatus;
use atlas_shell::home::{image_album, AlbumImage};
use atlas_shell::recent::{RecentEntry, RecentList};
use atlas_shell::{canvas_scale, canvas_text};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Align2, Color32, FontId, Id, Pos2, Rect, Sense};
use slate_doc::reject;
use slate_doc::scene::{AgentContextScope, Node, NodeId, NodeKind, PortalKind, PortalNode};
use slate_doc::stage::{self, Proposal, ProposalResult, StageWatcher};

use super::board::BoardXf;
use super::board_portal::resolve_source;
use super::board_portal_chrome::layout_for_portal;
use super::{PickerMsg, SlateApp};

mod life;
#[path = "board_agent_outputs.rs"]
mod outputs;
mod schedule;

/// Persisted MRU of folders the human bound to an agent portal.
const AGENT_RECENTS_KEY: &str = "slate-agent-projects";

type CachedImages = (
    (u64, u64),
    std::sync::Arc<Vec<atlas_ai::agent::ImageOutput>>,
);
type CachedGenerator = ((u64, u64, Option<u64>), std::rc::Rc<GeneratorView>);

pub(crate) struct LivePreview {
    request: String,
    frame: u64,
    step: u32,
    steps: u32,
    texture: egui::TextureHandle,
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
            atlas_agent::InputSlot::Media => Self::Image,
            atlas_agent::InputSlot::Prompt => Self::Prompt,
            atlas_agent::InputSlot::Style => Self::Style,
        }
    }

    /// Chip name and the colour a chip dot and its port share.
    pub fn look(self) -> (&'static str, Color32) {
        match self {
            Self::Prompt => ("Prompt", Color32::from_rgb(240, 196, 92)),
            Self::Geometry => ("Geometry", Color32::from_rgb(92, 204, 170)),
            Self::Image => ("Image", Color32::from_rgb(120, 170, 250)),
            Self::Style => ("Style", Color32::from_rgb(196, 146, 250)),
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

#[derive(Default)]
pub struct AgentRuntime {
    sources: atlas_ai::agent::AgentSources,
    project_picker: Option<NodeId>,
    catalog_error: Option<String>,
    connection_rx: Option<Receiver<(NodeId, String, Result<AgentSession, String>)>>,
    connection_pending: Option<NodeId>,
    connection_tick: Option<Instant>,
    connection_background: bool,
    models: HashMap<String, Vec<atlas_ai::agent::AgentModel>>,
    models_rx: Option<Receiver<(String, Result<Vec<atlas_ai::agent::AgentModel>, String>)>>,
    models_started: HashSet<String>,
    /// Catalogs an open agent editor lists before any card uses them.
    models_wanted: HashSet<String>,
    /// An open model list and the pass it was painted in. The board camera
    /// runs before popups paint, so it reads last pass's rect.
    menu_popup: std::cell::Cell<Option<(Rect, u64)>>,
    models_error: HashMap<String, String>,
    artifact_popup: Option<(NodeId, bool)>,
    artifact_popup_rect: Option<Rect>,
    /// HTML (and the rest of the preview catalog) waiting for text vs graphic.
    preview_ask: Option<(NodeId, String, Option<Pos2>)>,
    /// False until the pointer that opened the menu has been released.
    preview_ask_ready: bool,
    /// File Atlas place requests already applied, `portal:id`.
    placed_atlas: HashSet<String>,
    /// Last painted screen position of a train card's referenced-input dot.
    context_auto: HashMap<NodeId, Pos2>,
    /// Last painted screen position of the manual-context dot, when it is showing.
    context_human: HashMap<NodeId, Pos2>,
    /// Press origin on a context dot, so a click can be told from a drag.
    context_press: Option<(NodeId, Pos2)>,
    /// Temporary context cards. They vanish when the pointer leaves them.
    context_preview: Option<NodeId>,
    context_preview_rect: Option<Rect>,
    /// Composer field of every painted tail card, rebuilt each frame, and the
    /// field that currently has the caret.
    composer_rects: HashMap<NodeId, Rect>,
    composer_editing: Option<NodeId>,
    /// Scroll range of user-sized cards whose content overflows, in world units.
    card_overflow: HashMap<NodeId, f32>,
    /// Laid-out transcript height per card as (card width, height, content
    /// signature), in world units. Train cards fit to what paint drew, so
    /// bubbles, rules and status rows are never estimated twice. Zoom only ever
    /// grows an entry, so zooming cannot resize a card back and forth.
    content_heights: HashMap<NodeId, (f32, f32, u64)>,
    /// Bumped when a painted transcript height changes, so the fit reruns.
    measure_epoch: u64,
    /// Press on a human context handle, so a click can be told from a wire drag.
    pocket_press: Option<(NodeId, Pos2)>,
    /// Context a handle click brought out of this card, so a second click pockets it.
    pocket_open: HashMap<NodeId, Vec<NodeId>>,
    /// Cards holding pocketed context, per scene revision.
    pocket_cache: std::cell::RefCell<((u64, u64), HashSet<NodeId>)>,
    /// API-key field. A click here edits text; it does not drag the card.
    key_rect: Option<(NodeId, Rect)>,
    key_focus: Option<NodeId>,
    title_edit: Option<(NodeId, String)>,
    spawn_drag: Option<(NodeId, Pos2)>,
    /// Press on the changed-documents dot. A small move places that file.
    artifact_drag: Option<(NodeId, Pos2)>,
    composer_focus: Option<NodeId>,
    pub(crate) prompt_epoch: u64,
    fit_revision: Option<(
        u64,
        u64,
        u64,
        u64,
        Option<NodeId>,
        Option<NodeId>,
        Option<NodeId>,
        u64,
    )>,
    fit_keys: HashMap<NodeId, u64>,
    title_widths: HashMap<NodeId, f32>,
    rail_cache: std::cell::RefCell<((u64, u64), Vec<slate_doc::agent_chat::HistoryRail>)>,
    bindings: HashMap<NodeId, String>,
    context_selection: HashMap<String, Vec<NodeId>>,
    requests: HashMap<NodeId, String>,
    context_tick: Option<Instant>,
    codex: HashMap<String, atlas_ai::runtime::CodexLink>,
    programs: Vec<atlas_ai::agent::AgentProvider>,
    programs_rx: Option<Receiver<Vec<atlas_ai::agent::AgentProvider>>>,
    programs_started: bool,
    output_epoch: u64,
    image_cache: std::cell::RefCell<HashMap<NodeId, CachedImages>>,
    summary_cache:
        std::cell::RefCell<HashMap<NodeId, (u64, u64, u64, std::sync::Arc<egui::Galley>, f32)>>,

    transcript_cache: HashMap<(NodeId, usize), (u64, u64, u64, std::sync::Arc<egui::Galley>, f32)>,
    transcript_scroll: HashMap<NodeId, f32>,
    pub(crate) sessions: HashMap<NodeId, std::sync::Arc<AgentSession>>,
    prompts: HashMap<NodeId, String>,
    pending: Vec<Proposal>,
    stage: StageWatcher,
    /// Sessions this user let act without asking (`atlas_ai::access`). Loaded
    /// at startup from the per-user store; never journaled.
    full_access: std::collections::BTreeSet<String>,
    /// None in tests, so a toggle never touches the real per-user store.
    access_path: Option<std::path::PathBuf>,
    /// Schedule per session, read once from its `schedule.json`.
    schedules: HashMap<String, Option<atlas_ai::schedule::AgentSchedule>>,
    schedule_dialog: Option<schedule::ScheduleDialog>,
    recents: Vec<RecentEntry>,
    provider_recents: HashMap<String, Vec<RecentEntry>>,
    recents_rx: Option<Receiver<HashMap<String, Vec<RecentEntry>>>>,
    recents_started: bool,
    cover_focus: HashMap<NodeId, usize>,
    /// Generations accepted while this note is already running.
    comfy_queue: HashMap<NodeId, std::collections::VecDeque<atlas_agent::AgentRequest>>,
    /// Input signature last sent by a live generator.
    live_sent: HashMap<NodeId, u64>,
    /// Current live run. Keep starts a new one so the shown frame stays.
    live_run: HashMap<NodeId, String>,
    /// When a queued Render began waiting for its 3D mesh.
    capture_wait: HashMap<NodeId, Instant>,
    /// Models this generator unlocked so its focused drag can fly them.
    steering: HashMap<NodeId, NodeId>,
    /// Image id whose aspect ratio the card was last fitted to.
    fitted_image: HashMap<NodeId, String>,
    generator_views: std::cell::RefCell<HashMap<NodeId, CachedGenerator>>,
    /// Live signature waiting for typing to pause, and when it first appeared.
    live_settle: HashMap<NodeId, (u64, Instant)>,
    /// Newest sampling frame decoded for display: request id, frame number,
    /// step of steps, texture.
    previews: HashMap<NodeId, LivePreview>,
    #[cfg(test)]
    pub(crate) dispatched: Vec<(NodeId, atlas_agent::AgentRequest)>,
    /// Contents focus — selection is not this (P1.portal.contents-focus).
    pub focused: Option<NodeId>,
    ide: CursorIdeStatus,
    ide_rx: Option<Receiver<CursorIdeStatus>>,
    ide_inflight: bool,
    ide_next: Option<Instant>,
    chats: HashMap<(String, PathBuf), Vec<CursorChat>>,
    chats_rx: Option<Receiver<(String, PathBuf, Result<Vec<CursorChat>, String>)>>,
    chats_inflight: HashMap<PathBuf, Instant>,
    pending_chat_pick: Option<NodeId>,
    pub chat_picker: Option<ChatPicker>,
    local_turns: HashMap<NodeId, Vec<AgentTurn>>,
    sidecar_spawned: HashSet<String>,
    sidecar_child: HashMap<String, std::process::Child>,
    sidecar_booting: HashSet<String>,
    sidecar_boot_tx: Option<Sender<SidecarBoot>>,
    sidecar_boot_rx: Option<Receiver<SidecarBoot>>,
    /// Document ownership, rejoin, "Connecting…" sends and sidecar stops.
    pub(crate) life: life::AgentLife,
    awaiting: HashMap<NodeId, AgentAwait>,
    pub key_draft: String,
    key_entry: Option<NodeId>,
    /// Artifact id → the portal placed for it, so a second click retracts.
    spawned: HashMap<(NodeId, String), NodeId>,
    /// Portals animating back into the agent dot after retract.
    retract_ghosts: Vec<(Node, Pos2, Instant)>,
    /// Output-port menu and text block drafts of generators and text blocks.
    pub(crate) flow: super::board_flow::FlowUi,
    /// The capsule stack on chat cards' output circles.
    pub(crate) outputs: outputs::OutputsUi,
}

/// In-flight send. Failure is a named state — never a blank transcript.
#[derive(Clone)]
enum AgentAwait {
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
enum AgentRecover {
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
const AWAIT_TIMEOUT: Duration = Duration::from_secs(20);
/// Presses accepted while a generator is busy.
const GENERATION_QUEUE: usize = 8;
/// How long a Render waits for its 3D mesh before naming the failure.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);
/// Pause in typing before a live generator renders the new prompt.
const LIVE_TYPING_SETTLE: Duration = Duration::from_millis(300);
/// Designed type size of generator hover chips and buttons (scales with zoom).
pub(crate) const GENERATOR_CHIP_PX: f32 = 12.0;

/// One seed per generator session keeps live frames and Keep coherent.
fn stable_seed(session: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    session.hash(&mut hasher);
    hasher.finish() >> 1
}

/// Worker-thread result of finding Node, installing `@cursor/sdk`, and spawning.
struct SidecarBoot {
    /// Tab that sent, so a failure lands on its document after a tab switch.
    doc: u64,
    portal: NodeId,
    session: String,
    result: Result<std::process::Child, String>,
}

/// Palette of saved Cursor chats for one bound folder.
pub struct ChatPicker {
    pub portal: NodeId,
    pub folder: PathBuf,
    pub chats: Vec<CursorChat>,
}

impl AgentRuntime {
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

    /// A model list is open at `rect` this pass; the wheel over it scrolls it.
    pub fn note_menu_popup(&self, ctx: &egui::Context, rect: Rect) {
        let pass = ctx.cumulative_pass_nr();
        let rect = match self.menu_popup.get() {
            Some((old, at)) if at == pass => old.union(rect),
            _ => rect,
        };
        self.menu_popup.set(Some((rect, pass)));
    }

    /// The pointer is over a model list that was open last pass or this one.
    pub fn over_menu_popup(&self, ctx: &egui::Context, pointer: Option<Pos2>) -> bool {
        let pass = ctx.cumulative_pass_nr();
        self.menu_popup
            .get()
            .is_some_and(|(rect, at)| at + 1 >= pass && pointer.is_some_and(|p| rect.contains(p)))
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

    fn remove_pending(&mut self, id: &str) {
        self.pending.retain(|p| p.id != id);
    }
}

pub(crate) fn paint_agent_spinner(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    time: f32,
    ink: Color32,
) {
    for i in 0..8 {
        let angle = time * 5.0 + i as f32 * std::f32::consts::TAU / 8.0;
        let p = center + egui::vec2(angle.cos(), angle.sin()) * radius;
        let alpha = 36 + i * 26;
        painter.circle_filled(p, radius * 0.18, ink.gamma_multiply(alpha as f32 / 255.0));
    }
}

fn tracked_galley(
    ui: &egui::Ui,
    text: &str,
    size: f32,
    color: Color32,
    tracking: f32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.wrap = egui::text::TextWrapping {
        max_width,
        max_rows: 1,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: FontId::proportional(size),
            color,
            extra_letter_spacing: tracking,
            ..Default::default()
        },
    );
    ui.fonts(|fonts| fonts.layout_job(job))
}

/// Title band above the message being typed, in world units.
const COMPOSER_TOP: f32 = 32.0;
/// Padding under the message, in world units.
const COMPOSER_BOTTOM: f32 = 14.0;
/// Gap between a transcript and the composer, in world units.
const COMPOSER_GAP: f32 = 8.0;
/// Extra room so a measured transcript is not clipped by spacing.
const TRANSCRIPT_SLACK: f32 = 4.0;
/// About 1.3 lines under the last line of a sent message.
const CARD_TEXT_PAD: f32 = 18.0;
/// Top of a sent card's text, in world units.
const SUMMARY_TEXT_TOP: f32 = 28.0;
/// Lines a collapsed card keeps.
const COLLAPSED_ROWS: usize = 3;
/// Card text size, in world units.
const CARD_TEXT_PX: f32 = 13.0;

/// A person's resize of a chat card is authored size, recorded in the same
/// patch as the rect. It also opens a collapsed card. Drafts and bundles still fit.
pub(crate) fn record_agent_resize(node: &mut Node) -> bool {
    let size = [node.rect.w, node.rect.h];
    let NodeKind::Portal(p) = &mut node.kind else {
        return false;
    };
    let Some(a) = p.agent.as_mut() else {
        return false;
    };
    if a.provider.is_empty()
        || a.view != atlas_ai::agent::PortalView::Chat
        || a.chat.draft
        || (a.chat.train && !a.chat.bundled.is_empty())
    {
        return false;
    }
    a.chat.size = Some(size);
    a.chat.collapsed = false;
    true
}

/// Bundling a run keeps it on the terminal card, one level deeper; the rest
/// of the run hides (D25).
fn bundle_view(node: &mut Node, run: &[NodeId]) {
    let Some((last, members)) = run.split_last() else {
        return;
    };
    if node.id != *last {
        node.hidden = true;
        return;
    }
    if let NodeKind::Portal(p) = &mut node.kind {
        if let Some(a) = &mut p.agent {
            a.chat.bundle_layers.push(a.chat.bundled.clone());
            a.chat.bundled = members.to_vec();
        }
    }
}

/// One line naming a wired context source.
fn context_label(n: &Node) -> String {
    match &n.kind {
        NodeKind::Text(t) => {
            let line = t.text.lines().next().unwrap_or("").trim();
            if line.is_empty() {
                "Text".into()
            } else {
                line.chars().take(64).collect()
            }
        }
        NodeKind::Portal(p) => p.title.clone(),
        NodeKind::Image(_) => "Image".into(),
        _ => "Linked context".into(),
    }
}

/// Header, the first lines of `text` at the card's wrap, and the text pad.
fn collapsed_card_height(ctx: &egui::Context, text: String, card_w: f32) -> f32 {
    let mut job = egui::text::LayoutJob::simple(
        text,
        FontId::proportional(CARD_TEXT_PX),
        Color32::WHITE,
        (card_w - 24.0).max(1.0),
    );
    job.wrap.max_rows = COLLAPSED_ROWS;
    let lines = ctx.fonts(|fonts| fonts.layout_job(job)).size().y;
    COMPOSER_TOP + lines + CARD_TEXT_PAD
}

fn composer_wrap(card_w: f32) -> f32 {
    (card_w - slate_doc::agent_chat::PORT_INSET - 10.0 - 12.0).max(1.0)
}

/// Wrapped height of the text being typed. An empty draft is one line.
fn composer_text_height(ctx: &egui::Context, text: &str, wrap: f32, font_px: f32) -> f32 {
    let font = FontId::proportional(font_px);
    ctx.fonts(|fonts| {
        let row = fonts.row_height(&font);
        if text.is_empty() {
            row
        } else {
            fonts
                .layout(text.to_owned(), font, Color32::TRANSPARENT, wrap.max(1.0))
                .size()
                .y
                .max(row)
        }
    })
}

fn hugging_composer_card(prompt_h: f32) -> f32 {
    (COMPOSER_TOP + prompt_h + COMPOSER_BOTTOM).max(slate_doc::agent_chat::DRAFT_HEIGHT)
}

fn conversation_card_height(transcript_h: f32, prompt_h: f32) -> f32 {
    if transcript_h <= 1.0 {
        hugging_composer_card(prompt_h)
    } else {
        COMPOSER_TOP + transcript_h + TRANSCRIPT_SLACK + COMPOSER_GAP + prompt_h + COMPOSER_BOTTOM
    }
}

/// What [`SlateApp::build_artifact_node`] made.
#[allow(clippy::large_enum_variant)]
pub(crate) enum ArtifactBuild {
    Node(Node),
    /// More than one face and none chosen: the caller asks.
    Ask,
    Unavailable,
}

/// `[portal, artifact]`, or `{"portal","artifact","face"}` where `face` is a
/// `return.json` `as` value (`PreviewFace` ids are the same words).
fn parse_artifact_open(detail: &str) -> Option<(u64, String, atlas_agent::Face)> {
    let value: serde_json::Value = serde_json::from_str(detail).ok()?;
    if let Some(pair) = value.as_array() {
        let id = pair.first()?.as_u64()?;
        let artifact = pair.get(1)?.as_str()?.to_string();
        return Some((id, artifact, atlas_agent::Face::Auto));
    }
    let obj = value.as_object()?;
    let id = obj.get("portal")?.as_u64()?;
    let artifact = obj.get("artifact")?.as_str()?.to_string();
    let face = obj
        .get("face")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    Some((id, artifact, face))
}

impl SlateApp {
    pub(crate) fn stop_pruned_agent_runs(&self, ids: &[NodeId]) {
        for id in ids {
            if let Some((session, _)) = self.agent_session_for(*id) {
                if let Some(link) = self.agents.codex.get(&session) {
                    link.stop();
                }
            }
        }
    }
    fn agent_draft_width(&self, id: NodeId) -> f32 {
        self.agents
            .title_widths
            .get(&id)
            .copied()
            .unwrap_or(slate_doc::agent_chat::MIN_CARD_WIDTH)
    }

    /// Content measurements are cached on change; geometry uses the existing
    /// coalescing journal path, so typing/streaming cannot bypass undo.
    pub(crate) fn fit_agent_cards(&mut self, ctx: &egui::Context) {
        use std::hash::{Hash, Hasher};
        if self.tab().read_only || self.board_drag.is_some() {
            return;
        }
        // Respect Undo until new typing or provider output actually changes the content.
        if self.tab().journal.can_redo()
            && self
                .agents
                .fit_revision
                .is_none_or(|r| r.1 == self.agents.output_epoch && r.2 == self.agents.prompt_epoch)
        {
            return;
        }
        let revision = (
            self.scene_gen,
            self.agents.output_epoch,
            self.agents.prompt_epoch,
            self.tab().id,
            self.agents.project_picker,
            self.agents.chat_picker.as_ref().map(|p| p.portal),
            self.agents.pending_chat_pick,
            self.agents.measure_epoch,
        );
        if self.agents.fit_revision == Some(revision) {
            return;
        }
        let nodes: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && !n.locked)
            .filter(|n| {
                slate_doc::agent_chat::agent(n).is_some_and(|a| {
                    !a.provider.is_empty() && a.view == atlas_ai::agent::PortalView::Chat
                })
            })
            .cloned()
            .collect();
        let mut updates = Vec::new();
        let mut measured = 0;
        let mut more = false;
        for node in nodes {
            if self.agents.project_picker == Some(node.id)
                || self
                    .agents
                    .chat_picker
                    .as_ref()
                    .is_some_and(|p| p.portal == node.id)
                || self.agents.pending_chat_pick == Some(node.id)
            {
                continue;
            }
            let NodeKind::Portal(p) = &node.kind else {
                continue;
            };
            let a = p.agent.as_ref().unwrap();
            let turns = self.visible_agent_turns(node.id);
            let prompt = self
                .agents
                .prompts
                .get(&node.id)
                .cloned()
                .unwrap_or_default();
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            (
                self.tab().id,
                &p.title,
                &a.model,
                a.chat.train,
                a.chat.draft,
                a.chat.start,
                a.chat.end,
                &a.chat.bundled,
                &a.chat.bundle_layers,
                a.chat.window_height,
                format!("{:?}", a.chat.detail),
                &prompt,
            )
                .hash(&mut hash);
            (
                a.chat.collapsed,
                a.chat.size.map(|s| [s[0].to_bits(), s[1].to_bits()]),
                self.agent_has_child(node.id),
            )
                .hash(&mut hash);
            (self.agents.key_entry == Some(node.id)).hash(&mut hash);
            self.agents
                .content_heights
                .get(&node.id)
                .map(|(w, h, s)| (w.to_bits(), h.to_bits(), *s))
                .hash(&mut hash);
            node.rect.w.to_bits().hash(&mut hash);
            node.rect.h.to_bits().hash(&mut hash);
            for t in &turns {
                t.text.hash(&mut hash);
                t.role.hash(&mut hash);
            }
            let key = hash.finish();
            if self.agents.fit_keys.get(&node.id) == Some(&key) {
                continue;
            }
            if measured >= 8 {
                more = true;
                break;
            }
            measured += 1;
            self.agents.fit_keys.insert(node.id, key);
            let after = self.fitted_agent_card(ctx, &node);
            if (after.rect.w - node.rect.w).abs() > 0.5
                || (after.rect.h - node.rect.h).abs() > 0.5
                || after.kind != node.kind
            {
                updates.push(after);
            }
        }
        for after in updates {
            self.patch_nodes(&[after.id], |n| {
                *n = after.clone();
            });
        }
        self.agents.fit_revision = if more {
            None
        } else {
            Some((
                self.scene_gen,
                self.agents.output_epoch,
                self.agents.prompt_epoch,
                self.tab().id,
                self.agents.project_picker,
                self.agents.chat_picker.as_ref().map(|p| p.portal),
                self.agents.pending_chat_pick,
                self.agents.measure_epoch,
            ))
        };
        if more {
            ctx.request_repaint();
        }
    }

    /// What `paint_agent_bound` stacks in a card's transcript. A painted height
    /// only describes the card while this is unchanged.
    fn transcript_signature(&self, id: NodeId, turns: &[AgentTurn]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for t in turns {
            t.text.hash(&mut hash);
            t.role.hash(&mut hash);
        }
        self.agents
            .awaiting
            .get(&id)
            .map(std::mem::discriminant)
            .hash(&mut hash);
        self.agents
            .session(id)
            .is_some_and(|s| s.approval.is_some())
            .hash(&mut hash);
        self.agent_has_child(id).hash(&mut hash);
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| std::mem::discriminant(&a.chat.detail))
            .hash(&mut hash);
        hash.finish()
    }

    /// The transcript height paint laid out for this card at `width`, while it
    /// still shows the same content.
    fn measured_transcript(&self, id: NodeId, width: f32, turns: &[AgentTurn]) -> Option<f32> {
        let (w, h, signature) = *self.agents.content_heights.get(&id)?;
        ((w - width).abs() <= 0.5 && signature == self.transcript_signature(id, turns)).then_some(h)
    }

    /// `node` with the rect its content, presentation and authored size give it.
    fn fitted_agent_card(&mut self, ctx: &egui::Context, node: &Node) -> Node {
        use slate_doc::agent_chat::{CARD_WIDTH, DRAFT_HEIGHT, MIN_CARD_WIDTH};
        let NodeKind::Portal(p) = &node.kind else {
            return node.clone();
        };
        let Some(a) = p.agent.as_ref() else {
            return node.clone();
        };
        let turns = self.visible_agent_turns(node.id);
        let prompt = self
            .agents
            .prompts
            .get(&node.id)
            .cloned()
            .unwrap_or_default();
        {
            let measure = |text: String, width: f32, size: f32| {
                ctx.fonts(|f| {
                    f.layout(
                        text,
                        FontId::proportional(size),
                        Color32::WHITE,
                        width.max(1.0),
                    )
                    .size()
                })
            };
            let title = if a.provider == "codex"
                || a.provider == "cursor"
                || a.provider.starts_with("ollama")
            {
                format!(
                    "{} · {} ▾",
                    p.title,
                    atlas_ai::agent::model_label(
                        a.model
                            .as_deref()
                            .or_else(|| a.provider.strip_prefix("ollama/"))
                            .unwrap_or(model_fallback(&a.provider))
                    )
                )
            } else {
                p.title.clone()
            };
            let title_w = (measure(title, f32::INFINITY, 13.0).x + 88.0).max(MIN_CARD_WIDTH);
            self.agents.title_widths.insert(node.id, title_w);
            let mut after = node.clone();
            if a.chat.train && !a.chat.bundled.is_empty() {
                let count = a.chat.bundled.len() + 1;
                after.rect.w = (count.min(6) as f32 * 76.0 + 24.0).max(title_w);
                after.rect.h = 36.0 + count.div_ceil(6) as f32 * 88.0;
            } else if a.chat.draft {
                after.rect.w = title_w
                    .max(measure(prompt.clone(), f32::INFINITY, 14.0).x + 24.0)
                    .min(CARD_WIDTH.max(title_w));
                let prompt_h =
                    composer_text_height(ctx, &prompt, composer_wrap(after.rect.w), 14.0);
                after.rect.h = hugging_composer_card(prompt_h);
            } else if a.chat.collapsed || a.chat.size.is_some() {
                // Collapse owns the height; a person's size owns the rest.
                after.rect.w = a.chat.size.map_or(node.rect.w, |s| s[0]);
                after.rect.h = match a.chat.size {
                    Some(size) if !a.chat.collapsed => size[1],
                    _ => {
                        let text = turns
                            .iter()
                            .map(|t| t.text.as_str())
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        let mut h = collapsed_card_height(ctx, text, after.rect.w);
                        if !self.agent_has_child(node.id) {
                            h += composer_text_height(
                                ctx,
                                &prompt,
                                composer_wrap(after.rect.w),
                                14.0,
                            ) + COMPOSER_BOTTOM;
                        }
                        h
                    }
                };
            } else if a.chat.train {
                let text = turns
                    .iter()
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                after.rect.w = title_w
                    .max(measure(text.clone(), f32::INFINITY, 13.0).x + 24.0)
                    .min(CARD_WIDTH.max(title_w));
                let tail = !self.agent_has_child(node.id);
                // Sent Summary cards use the summary painter; `paint_agent_bound`
                // draws every other train card, composer only on the tail.
                after.rect.h = if tail || a.chat.detail != slate_doc::agent_chat::Detail::Summary {
                    let transcript_h = self
                        .measured_transcript(node.id, after.rect.w, &turns)
                        .unwrap_or_else(|| {
                            let pairs = a.chat.detail == slate_doc::agent_chat::Detail::Pair;
                            let mut h = 0.0;
                            for (i, t) in turns.iter().enumerate() {
                                if pairs
                                    && i > 0
                                    && turns[i - 1].role == "user"
                                    && t.role == "assistant"
                                {
                                    h += 22.0;
                                }
                                let ratio = if t.role == "user" { 0.78 } else { 0.94 };
                                h += measure(
                                    t.text.clone(),
                                    (after.rect.w - 24.0) * ratio - 20.0,
                                    14.0,
                                )
                                .y + 32.0;
                            }
                            if self.agents.awaiting.contains_key(&node.id) {
                                h += 30.0;
                            }
                            h
                        });
                    if tail {
                        let prompt_h =
                            composer_text_height(ctx, &prompt, composer_wrap(after.rect.w), 14.0);
                        conversation_card_height(transcript_h, prompt_h)
                    } else {
                        (COMPOSER_TOP + transcript_h + COMPOSER_BOTTOM).max(DRAFT_HEIGHT)
                    }
                } else {
                    let text_h = measure(text, after.rect.w - 24.0, 13.0).y;
                    (COMPOSER_TOP + text_h + CARD_TEXT_PAD).max(DRAFT_HEIGHT)
                };
            } else {
                after.rect.w = node.rect.w.max(title_w);
                let cap = a
                    .chat
                    .window_height
                    .unwrap_or(node.rect.h.max(112.0) as u32)
                    .max(112);
                let text_h: f32 = turns
                    .iter()
                    .map(|t| {
                        measure(
                            t.text.clone(),
                            (after.rect.w - 24.0) * if t.role == "user" { 0.78 } else { 0.94 }
                                - 20.0,
                            14.0,
                        )
                        .y + 32.0
                    })
                    .sum();
                let prompt_h =
                    composer_text_height(ctx, &prompt, composer_wrap(after.rect.w), 14.0);
                after.rect.h = conversation_card_height(text_h, prompt_h)
                    .min(cap as f32)
                    .max(112.0);
                if let NodeKind::Portal(p) = &mut after.kind {
                    p.agent.as_mut().unwrap().chat.window_height = Some(cap);
                }
            }
            if self.agents.key_entry == Some(node.id)
                && !a.chat.collapsed
                && a.chat.size.is_none()
                && (a.chat.draft
                    || !a.chat.train
                    || a.chat.detail == slate_doc::agent_chat::Detail::Full
                    || self.agent_linear_terminal(node.id))
            {
                after.rect.h += 72.0;
            }
            after
        }
    }

    pub(crate) fn agent_spawn_command(&mut self, detail: Option<&str>) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some(original) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let Some(binding) = slate_doc::agent_chat::agent(&original).cloned() else {
            return false;
        };
        if (self.agent_linear(id) && !self.agent_linear_terminal(id))
            || self.agent_is_running(id)
            || binding.chat.draft
        {
            return false;
        }
        let turns = self.agent_all_turns(id);
        let end = binding.chat.end.unwrap_or(turns.len());
        if atlas_ai::agent::checkpoint(&turns, Some(end)).is_err() {
            self.toast("Wait for this history to load.");
            return true;
        }
        let position = detail
            .and_then(|s| serde_json::from_str::<[f32; 2]>(s).ok())
            .filter(|p| p.iter().all(|v| v.is_finite()))
            .unwrap_or([
                original.rect.x + original.rect.w + slate_doc::agent_chat::CARD_GAP,
                original.rect.y,
            ]);
        let mut next = self.doc_mut().scene.build_duplicate(&original, 0.0, 0.0);
        next.rect = slate_doc::WorldRect::new(
            position[0],
            position[1],
            self.agent_draft_width(id),
            slate_doc::agent_chat::DRAFT_HEIGHT,
        );
        if let NodeKind::Portal(p) = &mut next.kind {
            if let Some(a) = &mut p.agent {
                a.chat = slate_doc::agent_chat::ChatView {
                    linear: self.agent_linear(id),
                    train: binding.chat.train,
                    parent: Some(id),
                    start: end,
                    end: Some(end),
                    detail: binding.chat.detail,
                    custom_fill: binding.chat.custom_fill,
                    stroke: binding.chat.stroke,
                    draft: true,
                    ..Default::default()
                };
            }
        }
        let next_id = next.id;
        let fork = !self.agent_linear(id) && binding.chat.forks_on_output();
        let mut nodes = vec![next];
        if fork {
            let mut sibling = self.doc_mut().scene.build_duplicate(
                &nodes[0],
                0.0,
                slate_doc::agent_chat::DRAFT_HEIGHT + slate_doc::agent_chat::LANE_GAP,
            );
            if let NodeKind::Portal(p) = &mut sibling.kind {
                p.agent.as_mut().unwrap().chat.parent = Some(id);
            }
            nodes.push(sibling);
        }
        if self.agent_linear(id) {
            let mut after = original.clone();
            if let NodeKind::Portal(p) = &mut after.kind {
                p.agent.as_mut().unwrap().chat.end = Some(end);
            }
            let base = self.doc().scene.nodes.len();
            let mut commands = vec![slate_doc::SceneCmd::Patch {
                before: Box::new(original),
                after: Box::new(after),
            }];
            commands.extend(nodes.into_iter().enumerate().map(|(i, node)| {
                slate_doc::SceneCmd::Add {
                    index: base + i,
                    node,
                }
            }));
            if !self.commit_scene(commands) {
                return false;
            }
        } else {
            self.add_nodes(nodes);
        }
        self.board_sel.clear();
        self.board_sel.insert(next_id);
        self.agent_focus(next_id);
        self.agents.composer_focus = Some(next_id);
        true
    }

    /// Where a card spawned from `origin` lands: beside it for a click, at
    /// `drop` for a drag, then snapped to the board and the agent datum.
    pub(crate) fn agent_spawn_rect(
        &mut self,
        origin: NodeId,
        drop: Option<Pos2>,
        size: egui::Vec2,
        zoom: f32,
    ) -> Option<slate_doc::WorldRect> {
        let from = self.doc().scene.node(origin)?.rect;
        let world = drop.unwrap_or(Pos2::new(
            from.x + from.w + slate_doc::agent_chat::CARD_GAP,
            from.y,
        ));
        let resolved = self.resolve_point_snap(world, &[origin], None, false, false);
        let rect = slate_doc::WorldRect::new(resolved.x, resolved.y, size.x, size.y);
        Some(if self.alt_down {
            rect
        } else {
            super::board_snap::agent_datum(rect, &[origin], &self.doc().scene, zoom)
        })
    }

    pub(crate) fn agent_output_at(&self, screen: Pos2, xf: &BoardXf) -> Option<NodeId> {
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find(|n| {
                !n.hidden
                    && !n.locked
                    && slate_doc::agent_chat::agent(n).is_some_and(|a| {
                        !a.chat.draft && !a.provider.is_empty() && a.chat.parent.is_some()
                    })
                    && !self.agent_in_choose_phase(n.id)
                    && screen.distance(
                        xf.rect_w2s(n.rect).right_top()
                            + egui::vec2(
                                -slate_doc::agent_chat::PORT_INSET,
                                slate_doc::agent_chat::RAIL_INSET,
                            ) * xf.z,
                    ) <= 7.0 * xf.z
            })
            .map(|n| n.id)
    }

    /// Runs before canvas gestures; output handles own their press until release.
    pub(crate) fn agent_spawn_input(&mut self, ui: &egui::Ui, xf: &BoardXf) -> bool {
        if self.flow_input(ui, xf) {
            self.board_align_eat_press = true;
            return true;
        }
        if self.agent_output_drag_input(ui, xf) {
            self.board_align_eat_press = true;
            return true;
        }
        if let Some(pointer) = ui.ctx().pointer_latest_pos() {
            if self.board_drag.is_none() && self.board_tool == super::board::BoardTool::Select {
                let picker = self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .rev()
                    .find(|n| {
                        !n.hidden
                            && !n.locked
                            && slate_doc::agent_chat::agent(n).is_some_and(|_a| {
                                self.agents.project_picker == Some(n.id)
                                    || self
                                        .agents
                                        .chat_picker
                                        .as_ref()
                                        .is_some_and(|p| p.portal == n.id)
                            })
                            && xf.rect_w2s(n.rect).shrink(8.0 * xf.z).contains(pointer)
                    })
                    .map(|n| n.id);
                if let Some(id) = picker {
                    if ui.input(|i| i.pointer.primary_pressed()) {
                        self.board_sel = std::iter::once(id).collect();
                        self.agent_focus(id);
                        self.board_align_eat_press = true;
                    }
                    return true;
                }
            }
            if self
                .agents
                .artifact_popup_rect
                .is_some_and(|r| r.contains(pointer))
            {
                self.board_align_eat_press = true;
                return true;
            }
            let on_dot = self.context_auto_under(pointer, xf);
            let on_pocket = self.agent_manual_context_at(pointer, xf).filter(|id| {
                self.agent_has_pocket(*id)
                    && self.doc().scene.node(*id).is_some_and(|n| {
                        xf.rect_w2s(n.rect)
                            .expand(canvas_scale::px(12.0, xf.z))
                            .contains(pointer)
                    })
            });
            if ui.input(|i| i.pointer.primary_pressed()) {
                self.agents.context_press = on_dot.map(|id| (id, pointer));
                self.agents.pocket_press = on_pocket.map(|id| (id, pointer));
                if on_dot.is_none() && on_pocket.is_none() {
                    if let Some(id) = self.composer_under(pointer, xf) {
                        self.board_sel = std::iter::once(id).collect();
                        self.agents.composer_focus = Some(id);
                        self.board_align_eat_press = true;
                        return true;
                    }
                }
            }
            if ui.input(|i| i.pointer.primary_released()) {
                if let Some((id, origin)) = self.agents.context_press.take() {
                    if pointer.distance(origin) < 4.0 && on_dot == Some(id) {
                        self.pin_agent_context(id);
                        self.agents.context_preview = None;
                        self.agents.context_preview_rect = None;
                    }
                }
                if let Some((id, origin)) = self.agents.pocket_press.take() {
                    if pointer.distance(origin) < 4.0 && on_pocket == Some(id) {
                        self.dispatch(
                            ui.ctx(),
                            atlas_commands::CommandId("portal.agent.pocket"),
                            Some(id.0.to_string()),
                        );
                    }
                }
            }
        }
        if self.tab().read_only {
            return false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.agents.spawn_drag = None;
            self.agents.artifact_drag = None;
            self.agents.preview_ask = None;
            self.agents.preview_ask_ready = false;
            return false;
        }
        let pointer = ui.ctx().pointer_latest_pos();
        if let Some((id, press)) = self.agents.artifact_drag {
            if let Some(p) = pointer {
                if ui.input(|i| i.pointer.any_released()) {
                    self.agents.artifact_drag = None;
                    let existing = self.provenance_portals(id, true);
                    if (p - press).length() > 4.0 {
                        let shift = ui.input(|i| i.modifiers.shift);
                        let at = self.resolve_point_snap(xf.s2w(p), &[], None, shift, false);
                        if let Some(node) = existing.last().copied() {
                            self.move_node_to(node, at);
                        } else {
                            self.dispatch(
                                ui.ctx(),
                                atlas_commands::CommandId("portal.agent.spawn_outputs"),
                                Some(
                                    serde_json::json!({ "portal": id.0, "at": [at.x, at.y] })
                                        .to_string(),
                                ),
                            );
                        }
                    } else if self.agent_has_outputs(id) {
                        self.dispatch(
                            ui.ctx(),
                            atlas_commands::CommandId("portal.agent.artifacts"),
                            Some(serde_json::to_string(&(id.0, true)).unwrap()),
                        );
                    }
                }
            }
            self.board_align_eat_press = true;
            return true;
        }
        let pointer = ui.ctx().pointer_latest_pos();
        if let Some((id, press)) = self.agents.spawn_drag {
            if let Some(p) = pointer {
                let moving = (p - press).length() > 4.0;
                let size = egui::vec2(
                    self.agent_draft_width(id),
                    slate_doc::agent_chat::DRAFT_HEIGHT,
                );
                let drop =
                    moving.then(|| xf.s2w(p) - egui::vec2(0.0, slate_doc::agent_chat::RAIL_INSET));
                if let Some(rect) = self.agent_spawn_rect(id, drop, size, xf.z) {
                    self.board_point_snap = Some(Pos2::new(rect.x, rect.y));
                    if ui.input(|i| i.pointer.any_released()) {
                        self.agents.spawn_drag = None;
                        self.board_sel.clear();
                        self.board_sel.insert(id);
                        self.dispatch(
                            ui.ctx(),
                            atlas_commands::CommandId("portal.agent.continue"),
                            Some(serde_json::to_string(&[rect.x, rect.y]).unwrap()),
                        );
                    }
                }
            }
            self.board_align_eat_press = true;
            return true;
        }
        if self.board_drag.is_some() || self.board_tool != super::board::BoardTool::Select {
            return false;
        }
        let ids: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && !n.locked)
            .filter(|n| {
                slate_doc::agent_chat::agent(n).is_some_and(|a| {
                    !a.chat.draft && !a.provider.is_empty() && a.chat.parent.is_some()
                })
            })
            .map(|n| n.id)
            .collect();
        for id in ids {
            if self.agent_in_choose_phase(id) {
                continue;
            }
            let n = self.doc().scene.node(id).unwrap();
            let r = xf.rect_w2s(n.rect);
            let handle = Pos2::new(
                r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                r.top() + slate_doc::agent_chat::RAIL_INSET * xf.z,
            );
            if self.agent_has_outputs(id) {
                let docs = Pos2::new(
                    r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                    r.center().y,
                );
                if pointer.is_some_and(|p| p.distance(docs) <= 10.0 * xf.z)
                    && ui.input(|i| i.pointer.primary_pressed())
                {
                    self.agents.artifact_drag = Some((id, pointer.unwrap()));
                    self.board_align_eat_press = true;
                    return true;
                }
            }
            let hit = Rect::from_center_size(handle, egui::vec2(8.0, 8.0) * xf.z);
            if pointer.is_some_and(|p| p.distance(handle) <= 7.0 * xf.z) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
            }
            if pointer.is_some_and(|p| hit.contains(p)) && ui.input(|i| i.pointer.primary_pressed())
            {
                self.agents.spawn_drag = Some((id, pointer.unwrap()));
                self.board_align_eat_press = true;
                return true;
            }
        }
        false
    }

    pub(crate) fn paint_agent_spawn_preview(&self, painter: &egui::Painter, xf: &BoardXf) {
        self.paint_flow_ports(painter, xf);
        self.paint_agent_output_drag(painter, xf);
        let palette = self.palette();
        let pointer = painter.ctx().pointer_latest_pos();
        if let (Some((id, _)), Some(p)) = (self.agents.artifact_drag, pointer) {
            if let Some(n) = self.doc().scene.node(id) {
                let r = xf.rect_w2s(n.rect);
                let from = Pos2::new(
                    r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                    r.center().y,
                );
                painter.line_segment(
                    [from, p],
                    egui::Stroke::new((1.25 * xf.z).max(1.0), palette.sub.gamma_multiply(0.7)),
                );
            }
        }
        for n in &self.doc().scene.nodes {
            let Some(a) = slate_doc::agent_chat::agent(n) else {
                continue;
            };
            if n.hidden
                || n.locked
                || a.chat.draft
                || a.provider.is_empty()
                || a.chat.parent.is_none()
                || self.agent_in_choose_phase(n.id)
            {
                continue;
            }
            let r = xf.rect_w2s(n.rect);
            let p = Pos2::new(
                r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                r.top() + slate_doc::agent_chat::RAIL_INSET * xf.z,
            );
            let color = palette.sub.gamma_multiply(
                if pointer.is_some_and(|q| q.distance(p) < 16.0 * xf.z) {
                    0.9
                } else {
                    0.35
                },
            );
            let near = pointer.is_some_and(|q| q.distance(p) <= 10.0 * xf.z);
            painter.circle_filled(p, 3.5 * xf.z * if near { 1.12 } else { 1.0 }, color);
        }
        if let (Some((id, _)), Some(pos)) = (self.agents.spawn_drag, self.board_point_snap) {
            let count = if self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| {
                    !atlas_ai::runtime::linear_provider(&a.provider) && a.chat.forks_on_output()
                }) {
                2
            } else {
                1
            };
            for lane in 0..count {
                let rect = slate_doc::WorldRect::new(
                    pos.x,
                    pos.y
                        + lane as f32
                            * (slate_doc::agent_chat::DRAFT_HEIGHT
                                + slate_doc::agent_chat::LANE_GAP),
                    self.agent_draft_width(id),
                    slate_doc::agent_chat::DRAFT_HEIGHT,
                );
                let r = xf.rect_w2s(rect);
                painter.rect_filled(r, 8.0 * xf.z, palette.card.gamma_multiply(0.45));
                painter.rect_stroke(
                    r,
                    8.0 * xf.z,
                    egui::Stroke::new(xf.z, palette.sub.gamma_multiply(0.5)),
                    egui::StrokeKind::Inside,
                );
                if let Some(parent) = self.doc().scene.node(id) {
                    if let NodeKind::Portal(p) = &parent.kind {
                        canvas_text::text(
                            painter,
                            r.min + egui::vec2(16.0, 10.0) * xf.z,
                            Align2::LEFT_CENTER,
                            &p.title,
                            FontId::proportional(13.0 * xf.z),
                            palette.sub.gamma_multiply(0.55),
                        );
                        painter.line_segment(
                            [
                                r.min + egui::vec2(12.0, 30.0) * xf.z,
                                r.min + egui::vec2(12.0, 45.0) * xf.z,
                            ],
                            egui::Stroke::new(xf.z, palette.sub.gamma_multiply(0.5)),
                        );
                    }
                    let b = slate_doc::agent_chat::rail(parent.rect, rect);
                    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                        [b.p0, b.c1, b.c2, b.p3].map(|p| xf.w2s(Pos2::new(p[0], p[1]))),
                        false,
                        Color32::TRANSPARENT,
                        egui::Stroke::new(xf.z, palette.sub.gamma_multiply(0.5)),
                    ));
                }
            }
        }
    }

    pub(crate) fn agent_rename(&mut self, title: &str) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let title = title.trim();
        if title.is_empty() {
            return false;
        }
        let ids = slate_doc::agent_chat::segments(&self.doc().scene, id)
            .into_iter()
            .find(|path| path.contains(&id))
            .unwrap_or_else(|| vec![id]);
        self.patch_nodes(&ids, |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                p.title = title.into();
            }
        });
        true
    }

    /// Selected chat cards that can collapse or be sized: not drafts, not bundles.
    fn selected_chat_cards(&self) -> Vec<NodeId> {
        self.board_sel
            .iter()
            .copied()
            .filter(|id| {
                self.doc()
                    .scene
                    .node(*id)
                    .and_then(slate_doc::agent_chat::agent)
                    .is_some_and(|a| {
                        a.view == atlas_ai::agent::PortalView::Chat
                            && !a.provider.is_empty()
                            && !a.chat.draft
                            && !(a.chat.train && !a.chat.bundled.is_empty())
                    })
            })
            .collect()
    }

    /// Collapse every selected card to three lines, or expand them all when
    /// every one is already collapsed.
    pub(crate) fn agent_toggle_collapse(&mut self, ctx: &egui::Context) -> bool {
        let ids = self.selected_chat_cards();
        let collapse = ids.iter().any(|id| {
            self.doc()
                .scene
                .node(*id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| !a.chat.collapsed)
        });
        self.refit_agent_cards(ctx, &ids, |chat| chat.collapsed = collapse)
    }

    /// Per-user grants that must survive a relaunch and never ride in a
    /// workbook: agent full access here, web origin consent in `board_web`.
    pub(crate) fn install_local_grants(&mut self) {
        let path = atlas_ai::access::store_path();
        self.agents.full_access = atlas_ai::access::load_in(&path);
        self.agents.access_path = Some(path);
        self.web
            .use_consent_file(atlas_core::index::data_dir().join("web-consent.json"));
    }

    pub(crate) fn agent_full_access(&self, session: &str) -> bool {
        self.agents.full_access.contains(session)
    }

    /// The person's explicit grant for one conversation: the provider runs
    /// commands and edits files without asking. Applies from the next message.
    pub(crate) fn agent_toggle_full_access(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some((session, provider)) = self.agent_session_for(id) else {
            return false;
        };
        if !atlas_ai::runtime::linear_provider(&provider) || self.agent_is_running(id) {
            return false;
        }
        let on = !self.agent_full_access(&session);
        if let Some(path) = self.agents.access_path.clone() {
            if let Err(error) = atlas_ai::access::set_in(&path, &session, on) {
                self.toast(error);
                return false;
            }
        }
        if on {
            self.agents.full_access.insert(session.clone());
        } else {
            self.agents.full_access.remove(&session);
        }
        // Cursor reads the grant when its sidecar starts; the next Send restarts it.
        if provider == "cursor" {
            self.detach_cursor_sidecar(&session);
        }
        self.toast(if on {
            "Full access on. This conversation runs commands and edits files without asking."
        } else {
            "Full access off. The agent asks before risky actions."
        });
        true
    }

    /// Forget the size a person gave these cards; they hug their text again.
    pub(crate) fn agent_fit_to_text(&mut self, ctx: &egui::Context) -> bool {
        let ids = self.selected_chat_cards();
        self.refit_agent_cards(ctx, &ids, |chat| chat.size = None)
    }

    /// Edit each card's view and refit its rect in the same journal step, so
    /// one Undo returns both.
    fn refit_agent_cards(
        &mut self,
        ctx: &egui::Context,
        ids: &[NodeId],
        edit: impl Fn(&mut slate_doc::agent_chat::ChatView),
    ) -> bool {
        let mut commands = Vec::new();
        for id in ids {
            let Some(before) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let mut edited = before.clone();
            if let NodeKind::Portal(p) = &mut edited.kind {
                if let Some(a) = &mut p.agent {
                    edit(&mut a.chat);
                }
            }
            if edited == before {
                continue;
            }
            let after = self.fitted_agent_card(ctx, &edited);
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        !commands.is_empty() && self.commit_scene(commands)
    }

    /// Unsent text and focus belong to the card; they leave with it.
    pub(crate) fn forget_deleted_agent_cards(&mut self, ids: &[NodeId]) {
        for id in ids {
            self.agents.prompts.remove(id);
            self.agents.pocket_open.remove(id);
            self.agents.composer_rects.remove(id);
            if self.agents.composer_editing == Some(*id) {
                self.agents.composer_editing = None;
            }
            if self.agents.composer_focus == Some(*id) {
                self.agents.composer_focus = None;
            }
        }
        self.agents.prompt_epoch = self.agents.prompt_epoch.wrapping_add(1);
    }

    /// Which Delete keys remove the selected chat cards while a card holds the
    /// keyboard: `(Delete, Backspace)`. Typed text keeps both keys. An empty
    /// composer lets Delete through; Backspace stays with the field.
    pub(crate) fn agent_delete_keys(&self, wants_kb: bool) -> (bool, bool) {
        let cards = !self.board_sel.is_empty()
            && self
                .board_sel
                .iter()
                .all(|id| slate_doc::agent_inputs::is_chat_card(&self.doc().scene, *id));
        if !cards || self.agents.title_edit.is_some() || self.agents.key_focus.is_some() {
            return (false, false);
        }
        match self.agents.composer_editing {
            Some(id) => {
                let empty = self.agents.prompts.get(&id).is_none_or(|t| t.is_empty());
                (empty && self.board_sel.contains(&id), false)
            }
            None if wants_kb => (false, false),
            None => (true, true),
        }
    }

    /// Materialize references to a linked transcript; never copy text into .slate.
    pub(crate) fn agent_show_train(&mut self) -> bool {
        self.agent_project_cards(1)
    }

    pub(crate) fn agent_show_pairs(&mut self) -> bool {
        self.agent_project_cards(2)
    }

    /// `stride` 1 is one turn per card. `stride` 2 keeps a user line with its reply.
    fn agent_project_cards(&mut self, stride: usize) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let ids = slate_doc::agent_chat::conversation(&self.doc().scene, id);
        if ids.iter().any(|id| self.agent_is_running(*id)) {
            self.toast("Wait for the response before changing its presentation.");
            return true;
        }
        let mut commands = Vec::new();
        let mut add_index = self.doc().scene.nodes.len();
        for member in ids {
            let original = self.doc().scene.node(member).unwrap().clone();
            let binding = slate_doc::agent_chat::agent(&original).unwrap();
            let turns = self.agent_all_turns(member);
            let end = binding
                .chat
                .end
                .unwrap_or(turns.len())
                .max(binding.chat.start + 1);
            self.agent_projection_views(&original, stride, end, &mut add_index, &mut commands);
        }
        self.arrange_agent_projection(id, &mut commands);
        self.commit_scene(commands);
        true
    }

    /// Views of `original`'s turns from its start to `end`, `stride` turns
    /// each, chained by parent. The original keeps the last range and its end.
    fn agent_projection_views(
        &mut self,
        original: &Node,
        stride: usize,
        end: usize,
        add_index: &mut usize,
        commands: &mut Vec<slate_doc::SceneCmd>,
    ) {
        let binding = slate_doc::agent_chat::agent(original).unwrap();
        let mut parent = binding.chat.parent;
        let mut i = binding.chat.start;
        while i < end {
            let chunk_end = (i + stride).min(end);
            let last = chunk_end == end;
            let mut node = if last {
                original.clone()
            } else {
                self.doc_mut().scene.build_duplicate(original, 0.0, 0.0)
            };
            node.hidden = false;
            node.rect.w = slate_doc::agent_chat::CARD_WIDTH;
            node.rect.h = if stride > 1 {
                slate_doc::agent_chat::PAIR_HEIGHT
            } else {
                slate_doc::agent_chat::CARD_HEIGHT
            };
            if let NodeKind::Portal(p) = &mut node.kind {
                if let Some(a) = &mut p.agent {
                    a.chat.train = true;
                    a.chat.bundled.clear();
                    a.chat.bundle_layers.clear();
                    a.chat.detail = if stride > 1 {
                        slate_doc::agent_chat::Detail::Pair
                    } else {
                        slate_doc::agent_chat::Detail::Summary
                    };
                    a.chat.parent = parent;
                    a.chat.start = i;
                    if !last {
                        a.chat.end = Some(chunk_end);
                    }
                }
            }
            parent = Some(node.id);
            i = chunk_end;
            if last {
                commands.push(slate_doc::SceneCmd::Patch {
                    before: Box::new(original.clone()),
                    after: Box::new(node),
                });
            } else {
                commands.push(slate_doc::SceneCmd::Add {
                    index: *add_index,
                    node,
                });
                *add_index += 1;
            }
        }
    }

    fn arrange_agent_projection(&self, id: NodeId, commands: &mut Vec<slate_doc::SceneCmd>) {
        let mut projected = self.doc().scene.clone();
        for command in commands.iter() {
            projected.apply(command);
        }
        let positions = slate_doc::agent_chat::projection_positions(&projected, id);
        let touched: HashSet<_> = commands
            .iter()
            .filter_map(|c| match c {
                slate_doc::SceneCmd::Patch { after, .. } => Some(after.id),
                slate_doc::SceneCmd::Add { node, .. } => Some(node.id),
                _ => None,
            })
            .collect();
        for id in positions.keys().filter(|id| !touched.contains(id)) {
            let before = self.doc().scene.node(*id).unwrap().clone();
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before.clone()),
                after: Box::new(before),
            });
        }
        for command in commands {
            let node = match command {
                slate_doc::SceneCmd::Patch { after, .. } => after.as_mut(),
                slate_doc::SceneCmd::Add { node, .. } => node,
                _ => continue,
            };
            if let Some(p) = positions.get(&node.id) {
                node.rect.x = p[0];
                node.rect.y = p[1];
            }
        }
    }

    fn agent_all_turns(&self, id: NodeId) -> Vec<AgentTurn> {
        self.agents
            .local_turns
            .get(&id)
            .or_else(|| self.agents.sessions.get(&id).map(|s| &s.turns))
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn agent_bundle_selection(&mut self) -> bool {
        let ids: Vec<_> = self.board_sel.iter().copied().collect();
        let Some(run) = slate_doc::agent_chat::bundle_run(&self.doc().scene, &ids) else {
            self.toast("Select a consecutive run on one branch, without an interior fork.");
            return true;
        };
        let last = *run.last().unwrap();
        if run.iter().any(|id| self.agent_is_running(*id)) {
            self.toast("Wait for the response before bundling.");
            return true;
        }
        self.patch_nodes(&run, |n| bundle_view(n, &run));
        self.board_sel.clear();
        self.board_sel.insert(last);
        true
    }

    pub(crate) fn agent_expand_bundle(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some(a) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return false;
        };
        if !a.chat.train || a.chat.bundled.is_empty() {
            return false;
        }
        let mut ids = a.chat.bundled.clone();
        ids.push(id);
        let origin = self.doc().scene.node(id).unwrap().rect;
        let mut x = origin.x;
        let mut positions = HashMap::new();
        for member in &ids {
            let n = self.doc().scene.node(*member).unwrap();
            positions.insert(*member, x);
            x += n.rect.w + slate_doc::agent_chat::CARD_GAP;
        }
        let mut commands = Vec::new();
        for member in &ids {
            let before = self.doc().scene.node(*member).unwrap().clone();
            let mut after = before.clone();
            let n = &mut after;
            n.hidden = false;
            n.rect.x = positions[&n.id];
            n.rect.y = origin.y;
            if n.id == id {
                if let NodeKind::Portal(p) = &mut n.kind {
                    if let Some(a) = &mut p.agent {
                        a.chat.bundled = a.chat.bundle_layers.pop().unwrap_or_default();
                        a.chat.train = true;
                    }
                }
            }
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        self.arrange_agent_projection(id, &mut commands);
        self.commit_scene(commands);
        true
    }

    pub(crate) fn agent_show_chat(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let segments = slate_doc::agent_chat::segments(&self.doc().scene, id);
        if segments
            .iter()
            .flatten()
            .any(|id| self.agent_is_running(*id))
        {
            self.toast("Wait for the response before changing its presentation.");
            return true;
        }
        let mut commands = Vec::new();
        let mut selected = None;
        for path in segments {
            let Some(last) = path.last().copied() else {
                continue;
            };
            if path.contains(&id) {
                selected = Some(last);
            }
            for member in &path {
                let before = self.doc().scene.node(*member).unwrap().clone();
                let mut after = before.clone();
                after.hidden = *member != last;
                if *member == last {
                    if let NodeKind::Portal(p) = &mut after.kind {
                        if let Some(a) = &mut p.agent {
                            a.chat.bundled = path[..path.len() - 1].to_vec();
                            a.chat.train = false;
                            a.chat.detail = slate_doc::agent_chat::Detail::Full;
                            a.chat.window_height =
                                Some(before.rect.h.max(slate_doc::agent_chat::DRAFT_HEIGHT) as u32);
                        }
                    }
                }
                commands.push(slate_doc::SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                });
            }
        }
        self.arrange_agent_projection(id, &mut commands);
        self.commit_scene(commands);
        if let Some(id) = selected {
            self.board_sel.clear();
            self.board_sel.insert(id);
        }
        true
    }

    pub(crate) fn agent_set_detail(&mut self, detail: slate_doc::agent_chat::Detail) -> bool {
        let ids: Vec<_> = self.board_sel.iter().copied().collect();
        self.patch_nodes(&ids, |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                if let Some(a) = &mut p.agent {
                    if !a.chat.train && detail != slate_doc::agent_chat::Detail::Full {
                        return;
                    }
                    a.chat.detail = detail;
                    n.rect.h = match detail {
                        slate_doc::agent_chat::Detail::Identity => 48.0,
                        slate_doc::agent_chat::Detail::Summary => {
                            slate_doc::agent_chat::CARD_HEIGHT
                        }
                        slate_doc::agent_chat::Detail::Pair => slate_doc::agent_chat::PAIR_HEIGHT,
                        slate_doc::agent_chat::Detail::Full => 540.0,
                    };
                }
            }
        });
        true
    }

    /// Opacity of a train's history rails over `palette.sub`, per theme.
    fn rail_opacity(&self) -> f32 {
        if self.palette().dark_mode {
            slate_doc::agent_chat::RAIL_OPACITY
        } else {
            slate_doc::agent_chat::RAIL_LIGHT_OPACITY
        }
    }

    /// The calm gray of a train's history rails as a stored wire color. A wire
    /// into a chat card starts with it; a color the person picks later stays.
    pub(crate) fn chat_wire_color(&self) -> slate_doc::scene::Rgba {
        let sub = self.palette().sub;
        slate_doc::scene::Rgba([
            sub.r(),
            sub.g(),
            sub.b(),
            (self.rail_opacity() * 255.0).round() as u8,
        ])
    }

    pub(crate) fn paint_agent_history_rails(&self, painter: &egui::Painter, xf: &BoardXf) {
        let palette = self.palette();
        let rail_color = palette.sub.gamma_multiply(self.rail_opacity());
        let mut cache = self.agents.rail_cache.borrow_mut();
        let revision = (self.scene_gen, self.doc().scene.scene_gen());
        if cache.0 != revision || cache.1.is_empty() {
            *cache = (
                revision,
                slate_doc::agent_chat::history_rails(&self.doc().scene),
            );
        }
        for rail in &cache.1 {
            let b = &rail.curve;
            if !xf
                .rect_w2s(b.aabb())
                .expand(2.0 * xf.z)
                .intersects(painter.clip_rect())
            {
                continue;
            }
            let point = |p: [f32; 2]| xf.w2s(egui::pos2(p[0], p[1]));
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [point(b.p0), point(b.c1), point(b.c2), point(b.p3)],
                false,
                Color32::TRANSPARENT,
                egui::Stroke::new(
                    canvas_scale::px(slate_doc::agent_chat::RAIL_WIDTH, xf.z),
                    rail_color,
                ),
            ));
        }
    }

    fn agent_text_key(&self, painter: &egui::Painter, z: f32) -> (u64, u64, u64) {
        let palette = self.palette();
        (
            self.agents.output_epoch,
            self.doc().scene.scene_gen(),
            ((z * painter.ctx().pixels_per_point() * 8.0)
                .ceil()
                .to_bits() as u64)
                << 32
                | u32::from_le_bytes(palette.ink.to_array()) as u64,
        )
    }

    fn paint_agent_bundle(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let Some(a) = portal.agent.as_ref() else {
            return;
        };
        let palette = self.palette();
        let rect = xf.rect_w2s(node.rect);
        self.paint_agent_bundle_count(painter, rect, node, a, xf.z);
        for (index, id) in a
            .chat
            .bundled
            .iter()
            .chain(std::iter::once(&node.id))
            .enumerate()
        {
            let origin = rect.min
                + egui::vec2(
                    12.0 + (index % 6) as f32 * 76.0,
                    32.0 + (index / 6) as f32 * 88.0,
                ) * xf.z;
            let mini = Rect::from_min_size(origin, egui::vec2(64.0, 72.0) * xf.z);
            atlas_shell::selection_tools::agent_card(
                painter,
                mini,
                5.0 * xf.z,
                xf.z * 0.5,
                palette,
            );
            painter.circle_filled(
                origin + egui::vec2(6.0, 8.0) * xf.z,
                2.0 * xf.z,
                palette.sub.gamma_multiply(0.5),
            );
            let Some(member) = self.doc().scene.node(*id) else {
                continue;
            };
            let Some(binding) = slate_doc::agent_chat::agent(member) else {
                continue;
            };
            let key = self.agent_text_key(painter, xf.z * 5.0 / 13.0);
            let mut cache = self.agents.summary_cache.borrow_mut();
            if cache.get(id).is_none_or(|e| (e.0, e.1, e.2) != key) {
                let turns = self.agent_all_turns(*id);
                let text = turns
                    .iter()
                    .skip(binding.chat.start)
                    .take(
                        binding
                            .chat
                            .end
                            .unwrap_or(turns.len())
                            .saturating_sub(binding.chat.start),
                    )
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let excerpt: String = text.chars().take(100).collect();
                let miniature = canvas_text::layout(
                    painter,
                    excerpt,
                    FontId::proportional(5.0 * xf.z),
                    palette.ink,
                    52.0 * xf.z,
                );
                cache.insert(
                    *id,
                    (
                        key.0,
                        key.1,
                        key.2,
                        miniature.galley(),
                        miniature.scale() / xf.z,
                    ),
                );
            }
            let entry = &cache[id];
            let miniature = canvas_text::Scaled::from_galley(entry.3.clone(), entry.4 * xf.z);
            miniature.paint(
                &painter.with_clip_rect(mini.shrink(4.0 * xf.z)),
                origin + egui::vec2(6.0, 17.0) * xf.z,
                palette.ink,
            );
        }
    }

    /// `max_rows` paints a collapsed capsule. A user-sized card scrolls what overflows.
    fn paint_agent_summary(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
        max_rows: Option<usize>,
    ) {
        let Some(agent) = portal.agent.as_ref() else {
            return;
        };
        let rect = xf.rect_w2s(node.rect);
        let palette = self.palette();
        let z = xf.z;
        let clip = rect.shrink(10.0 * z).intersect(painter.clip_rect());
        let painter = painter.with_clip_rect(clip);
        if !canvas_text::legible(canvas_text::authored_px(13.0, z)) {
            return;
        }
        let key = self.agent_text_key(&painter, z);
        let mut cache = self.agents.summary_cache.borrow_mut();
        if cache
            .get(&node.id)
            .is_none_or(|entry| (entry.0, entry.1, entry.2) != key)
        {
            let turns = self.visible_agent_turns(node.id);
            let excerpt = turns
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let font = FontId::proportional(13.0 * z);
            let wrap = ((node.rect.w - 24.0) * z).max(1.0);
            let laid = match max_rows {
                Some(rows) => {
                    canvas_text::layout_rows(&painter, excerpt, font, palette.ink, wrap, rows)
                }
                None => canvas_text::layout(&painter, excerpt, font, palette.ink, wrap),
            };
            cache.insert(
                node.id,
                (key.0, key.1, key.2, laid.galley(), laid.scale() / z),
            );
        }
        let entry = &cache[&node.id];
        let laid = canvas_text::Scaled::from_galley(entry.3.clone(), entry.4 * z);
        drop(cache);
        let mut offset = 0.0;
        if max_rows.is_none() && agent.chat.size.is_some() {
            let room = node.rect.h - SUMMARY_TEXT_TOP - 10.0;
            let max = (laid.size().y / z - room).max(0.0);
            self.agents.card_overflow.insert(node.id, max);
            offset = self
                .agents
                .transcript_scroll
                .get(&node.id)
                .copied()
                .unwrap_or(0.0);
            if max > 0.0
                && ui
                    .ctx()
                    .pointer_hover_pos()
                    .is_some_and(|p| clip.contains(p))
            {
                offset -= ui.input(|i| i.smooth_scroll_delta.y) / z;
            }
            offset = offset.clamp(0.0, max);
            self.agents.transcript_scroll.insert(node.id, offset);
        }
        let mut text_ui = egui::Ui::new(
            ui.ctx().clone(),
            Id::new(("agent-summary", node.id.0)),
            egui::UiBuilder::new()
                .layer_id(ui.layer_id())
                .max_rect(clip),
        );
        text_ui.set_clip_rect(clip);
        laid.selectable(
            &text_ui,
            Id::new(("agent-summary-selection", node.id.0)),
            rect.min + egui::vec2(12.0, SUMMARY_TEXT_TOP - offset) * z,
            palette.ink,
        );
    }

    /// A collapsed tail keeps its composer under the three lines.
    fn paint_collapsed_composer(
        &mut self,
        ui: &egui::Ui,
        layout: &super::board_portal_chrome::PortalChromeLayout,
        node: &Node,
        z: f32,
    ) {
        if self.agent_has_child(node.id) {
            return;
        }
        let body = layout.body;
        let pad = 12.0 * z;
        let text_x = body.left() + (slate_doc::agent_chat::PORT_INSET + 10.0) * z;
        let prompt = self
            .agents
            .prompts
            .get(&node.id)
            .cloned()
            .unwrap_or_default();
        let wrap = (body.right() - pad - text_x).max(1.0);
        let prompt_h = composer_text_height(ui.ctx(), &prompt, wrap, 14.0 * z);
        let bottom = body.bottom() - COMPOSER_BOTTOM * z;
        let field = Rect::from_min_max(
            Pos2::new(text_x, bottom - prompt_h - 2.0 * z),
            Pos2::new(body.right() - pad, bottom),
        );
        self.paint_agent_composer(ui, node.id, field, z, false);
    }

    /// Reserve both message views once. Source history is replayed into a fresh
    /// branch so sending from an old card cannot contaminate its siblings.
    pub(super) fn prepare_agent_train_send(
        &mut self,
        id: NodeId,
        ws: &std::path::Path,
    ) -> Option<(NodeId, Vec<AgentTurn>)> {
        use slate_doc::agent_chat::{
            ChatView, Detail, CARD_GAP, CARD_HEIGHT, CARD_WIDTH, LANE_GAP,
        };
        let original = self.doc().scene.node(id)?.clone();
        let a = slate_doc::agent_chat::agent(&original)?;
        if !a.chat.train && !a.chat.draft && a.chat.end.is_none() && a.chat.bundled.is_empty() {
            return Some((id, Vec::new()));
        }
        let linear = self.agent_linear(id);
        if linear
            && self.doc().scene.nodes.iter().any(|n| {
                slate_doc::agent_chat::agent(n).is_some_and(|other| other.chat.parent == Some(id))
            })
        {
            self.toast("Continue at the end of this conversation; coding agents have one stream.");
            return None;
        }
        let source = self.agent_all_turns(id);
        let through = a
            .chat
            .end
            .or_else(|| (source.len() < a.chat.start).then_some(a.chat.start));
        let history = match atlas_ai::agent::checkpoint(&source, through) {
            Ok(history) => history.to_vec(),
            Err(error) => {
                self.toast(error);
                return None;
            }
        };
        let count = history.len();
        let draft = a.chat.draft || (linear && source.is_empty());
        let children: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
            .map(|n| n.id)
            .collect();
        let branches = children.len();
        let lane_height = slate_doc::agent_chat::subtree(&self.doc().scene, &children)
            .iter()
            .filter_map(|id| self.doc().scene.node(*id))
            .map(|n| n.rect.h)
            .fold(CARD_HEIGHT, f32::max)
            + LANE_GAP;
        let mut user = if draft {
            original.clone()
        } else {
            self.doc_mut().scene.build_duplicate(&original, 0.0, 0.0)
        };
        user.rect = slate_doc::WorldRect::new(
            original.rect.x + original.rect.w + CARD_GAP,
            original.rect.y
                + if branches == 0 {
                    0.0
                } else {
                    (branches as f32 - 0.5) * lane_height
                },
            CARD_WIDTH,
            CARD_HEIGHT,
        );
        if draft {
            user.rect.x = original.rect.x;
            user.rect.y = original.rect.y;
        }
        let session = if linear {
            a.session.clone()
        } else {
            slate_doc::scene::new_agent_session_id()
        };
        let manifest = slate_doc::SourceUri {
            locator: super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &atlas_ai::agent::agent_dir(ws, &session).join("session.json"),
            ),
        };
        let pair = a.chat.detail == Detail::Pair;
        if !a.chat.train || (!linear && a.chat.detail == Detail::Full) || (pair && draft) {
            let mut after = original.clone();
            if let NodeKind::Portal(p) = &mut after.kind {
                if let Some(child) = &mut p.agent {
                    child.session = session;
                    child.bundle = Some(manifest);
                    if !linear {
                        child.channel = None;
                    }
                    child.chat.linear = linear;
                    child.chat.end = None;
                    child.chat.draft = false;
                }
            }
            if !self.commit_scene(vec![slate_doc::SceneCmd::Patch {
                before: Box::new(original),
                after: Box::new(after),
            }]) {
                return None;
            }
            return Some((id, if linear { Vec::new() } else { history }));
        }
        if let NodeKind::Portal(p) = &mut user.kind {
            if let Some(a) = &mut p.agent {
                a.session = session;
                a.bundle = Some(manifest);
                if !linear {
                    a.channel = None;
                }
                a.chat = ChatView {
                    linear,
                    train: true,
                    parent: if draft { a.chat.parent } else { Some(id) },
                    start: count,
                    end: if pair { None } else { Some(count + 1) },
                    detail: if pair { Detail::Pair } else { Detail::Summary },
                    custom_fill: a.chat.custom_fill,
                    stroke: a.chat.stroke,
                    ..Default::default()
                };
            }
        }
        if pair {
            user.rect.h = slate_doc::agent_chat::PAIR_HEIGHT;
        }
        let mut before_end = original.clone();
        if let NodeKind::Portal(p) = &mut before_end.kind {
            if let Some(a) = &mut p.agent {
                a.chat.end = Some(count);
            }
        }
        let base = self.doc().scene.nodes.len();
        let (reply_id, mut commands) = if pair {
            let focus = user.id;
            let commands = if draft {
                vec![slate_doc::SceneCmd::Patch {
                    before: Box::new(original),
                    after: Box::new(user),
                }]
            } else {
                vec![
                    slate_doc::SceneCmd::Patch {
                        before: Box::new(original),
                        after: Box::new(before_end),
                    },
                    slate_doc::SceneCmd::Add {
                        index: base,
                        node: user,
                    },
                ]
            };
            (focus, commands)
        } else {
            let mut reply = self
                .doc_mut()
                .scene
                .build_duplicate(&user, CARD_WIDTH + CARD_GAP, 0.0);
            if let NodeKind::Portal(p) = &mut reply.kind {
                if let Some(a) = &mut p.agent {
                    a.chat.parent = Some(user.id);
                    a.chat.start = count + 1;
                    a.chat.end = None;
                    a.chat.detail = Detail::Summary;
                }
            }
            let focus = reply.id;
            let commands = if draft {
                vec![
                    slate_doc::SceneCmd::Patch {
                        before: Box::new(original),
                        after: Box::new(user),
                    },
                    slate_doc::SceneCmd::Add {
                        index: base,
                        node: reply,
                    },
                ]
            } else {
                vec![
                    slate_doc::SceneCmd::Patch {
                        before: Box::new(original),
                        after: Box::new(before_end),
                    },
                    slate_doc::SceneCmd::Add {
                        index: base,
                        node: user,
                    },
                    slate_doc::SceneCmd::Add {
                        index: base + 1,
                        node: reply,
                    },
                ]
            };
            (focus, commands)
        };
        if !draft && branches == 1 {
            for child in slate_doc::agent_chat::subtree(&self.doc().scene, &children) {
                if let Some(before) = self.doc().scene.node(child) {
                    let mut after = before.clone();
                    after.rect.y -= lane_height * 0.5;
                    commands.push(slate_doc::SceneCmd::Patch {
                        before: Box::new(before.clone()),
                        after: Box::new(after),
                    });
                }
            }
        }
        if !self.commit_scene(commands) {
            return None;
        }
        self.board_sel.clear();
        self.board_sel.insert(reply_id);
        Some((reply_id, if linear { Vec::new() } else { history }))
    }

    /// Project, conversation, and first-load phases have no train handles yet.
    fn agent_in_choose_phase(&self, id: NodeId) -> bool {
        self.agents.project_picker == Some(id)
            || self.agents.pending_chat_pick == Some(id)
            || self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|p| p.portal == id)
            || (self.agents.connection_pending == Some(id) && !self.agents.connection_background)
    }

    /// Train presentation is the default place context and document handles live.
    fn agent_context_handles_visible(&self, id: NodeId) -> bool {
        if self.agent_in_choose_phase(id) {
            return false;
        }
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| {
                !a.provider.is_empty()
                    && a.chat.train
                    && a.chat.parent.is_some()
                    && a.view == atlas_ai::agent::PortalView::Chat
            })
    }

    pub(crate) fn agent_manual_context_at(&self, screen: Pos2, xf: &BoardXf) -> Option<NodeId> {
        self.agents
            .context_human
            .iter()
            .find(|(_, p)| screen.distance(**p) <= 10.0 * xf.z)
            .map(|(id, _)| *id)
    }

    pub(crate) fn context_auto_under(&self, screen: Pos2, xf: &BoardXf) -> Option<NodeId> {
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find(|n| {
                if n.hidden || n.locked || !self.agent_context_handles_visible(n.id) {
                    return false;
                }
                let Some(p) = self.agents.context_auto.get(&n.id).copied() else {
                    return false;
                };
                screen.distance(p) <= 10.0 * xf.z
            })
            .map(|n| n.id)
    }

    fn agent_context_blurb(&self, id: NodeId) -> String {
        let mut lines = Vec::new();
        let refs = self.agent_artifacts(id, false);
        if refs.is_empty() {
            lines.push("No linked references.".into());
        } else {
            lines.extend(
                refs.iter()
                    .take(6)
                    .map(|a| format!("{} · {}", a.kind.label(), a.title)),
            );
        }
        for node in &self.doc().scene.nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            let Some(binding) = &conn.binding else {
                continue;
            };
            let (source, target) = if binding.input_b {
                (&conn.a, &conn.b)
            } else {
                (&conn.b, &conn.a)
            };
            if slate_doc::agent_inputs::endpoint_node(target) != Some(id) {
                continue;
            }
            let Some(src) = slate_doc::agent_inputs::endpoint_node(source) else {
                continue;
            };
            let Some(label) = self.doc().scene.node(src).map(context_label) else {
                continue;
            };
            lines.push(label);
        }
        lines.join("\n")
    }

    /// What the human context handle holds: the card's pocketed context.
    fn agent_pocket_blurb(&self, id: NodeId) -> String {
        let hidden = slate_doc::agent_inputs::pocketed(&self.doc().scene, id);
        if hidden.is_empty() {
            return "Click to pocket this context again".into();
        }
        let mut lines = vec![format!(
            "Pocketed context · {} · click to show",
            hidden.len()
        )];
        lines.extend(
            hidden
                .iter()
                .filter_map(|n| self.doc().scene.node(*n))
                .take(8)
                .map(context_label),
        );
        lines.join("\n")
    }

    pub(crate) fn agent_artifact_at(&self, point: Pos2, xf: &BoardXf) -> Option<(NodeId, bool)> {
        for n in self
            .doc()
            .scene
            .nodes
            .iter()
            .rev()
            .filter(|n| !n.hidden && !n.locked && self.agent_context_handles_visible(n.id))
        {
            let r = xf.rect_w2s(n.rect);
            let left = self.agents.context_auto.get(&n.id);
            if let Some(left) = left {
                if point.distance(*left) <= 10.0 * xf.z {
                    return Some((n.id, false));
                }
            }
            if self.agent_has_outputs(n.id) {
                let right = Pos2::new(
                    r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                    r.center().y,
                );
                if point.distance(right) <= 10.0 * xf.z {
                    return Some((n.id, true));
                }
            }
        }
        None
    }

    pub(crate) fn agent_artifacts(
        &self,
        id: NodeId,
        output: bool,
    ) -> Vec<atlas_ai::agent::AgentArtifact> {
        let Some(a) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return vec![];
        };
        let start = self
            .doc()
            .scene
            .node(id)
            .and_then(|n| slate_doc::agent_chat::bundle_entry(&self.doc().scene, n))
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.chat.start)
            .unwrap_or(a.chat.start);
        self.agents
            .session(id)
            .map(|s| {
                s.artifacts
                    .iter()
                    .filter(|artifact| {
                        artifact.kind.is_output() == output
                            && artifact.turn >= start
                            && a.chat.end.is_none_or(|end| artifact.turn < end)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn toggle_agent_artifacts(&mut self, detail: Option<&str>) -> bool {
        let Some((id, output)) = detail.and_then(|s| serde_json::from_str::<(u64, bool)>(s).ok())
        else {
            return false;
        };
        let key = (NodeId(id), output);
        self.agents.artifact_popup = if self.agents.artifact_popup == Some(key) {
            None
        } else {
            Some(key)
        };
        self.agents.artifact_popup_rect = None;
        true
    }

    fn paint_agent_artifacts(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
    ) {
        let r = xf.rect_w2s(node.rect);
        let z = xf.z;
        let palette = self.palette();
        let radius = 3.5 * z;
        let rest = Pos2::new(
            r.left() + slate_doc::agent_chat::PORT_INSET * z,
            r.center().y,
        );
        let lift = canvas_scale::px(14.0, z);
        let drop = canvas_scale::px(12.0, z);
        let reach = canvas_scale::px(11.0, z);
        let pointer = ui.ctx().pointer_hover_pos();
        let refs = self.agent_artifacts(node.id, false);
        let has_refs = !refs.is_empty();
        let raised = rest + egui::vec2(0.0, -lift);
        let lowered = rest + egui::vec2(0.0, drop);
        let near = |p: Pos2| pointer.is_some_and(|q| q.distance(p) <= reach);
        let on_rest = near(rest);
        let outward = pointer.is_some_and(|p| {
            let d = p - rest;
            d.x < -canvas_scale::px(2.0, z)
                && d.length() < canvas_scale::px(42.0, z)
                && !near(rest)
                && !near(raised)
        });
        let split_on = if has_refs {
            on_rest || near(raised) || near(lowered)
        } else {
            outward || on_rest
        };
        let split = ui.ctx().animate_bool_with_time(
            Id::new(("agent-context-split", node.id.0)),
            split_on,
            0.18,
        );
        let auto = Pos2::new(rest.x, rest.y - if has_refs { split * lift } else { 0.0 });
        let human = Pos2::new(rest.x, rest.y + if has_refs { split * drop } else { 0.0 });
        let pocket = self.agent_has_pocket(node.id);
        self.agents.context_human.remove(&node.id);
        let gray = palette.sub;
        if has_refs {
            self.agents.context_auto.insert(node.id, auto);
            if split > 0.02 {
                self.paint_context_wire_slide(painter, node.id, rest, auto, human, gray, z);
                painter.circle_filled(
                    human,
                    radius * if near(human) { 1.12 } else { 1.0 },
                    gray.gamma_multiply(0.8 * split.min(1.0)),
                );
                self.agents.context_human.insert(node.id, human);
            }
            painter.circle_filled(
                auto,
                radius * if near(auto) { 1.12 } else { 1.0 },
                gray.gamma_multiply(0.8),
            );
        } else {
            self.agents.context_auto.remove(&node.id);
            let fade = ui.ctx().animate_bool_with_time(
                Id::new(("agent-context-fade", node.id.0)),
                split_on,
                0.12,
            );
            // A pocket keeps its handle findable at rest, a shade quieter.
            let fade = if pocket { fade.max(0.6) } else { fade };
            if fade > 0.04 {
                painter.circle_filled(
                    rest,
                    radius * if on_rest { 1.12 } else { 1.0 },
                    gray.gamma_multiply(0.8 * fade),
                );
                self.agents.context_human.insert(node.id, rest);
            }
        }
        if let Some(handle) = self
            .agents
            .context_human
            .get(&node.id)
            .copied()
            .filter(|_| pocket)
        {
            let blurb = self.agent_pocket_blurb(node.id);
            ui.interact(
                Rect::from_center_size(handle, egui::Vec2::splat(canvas_scale::px(16.0, z))),
                Id::new(("agent-pocket", node.id.0)),
                Sense::hover(),
            )
            .on_hover_text(blurb);
        }
        if self.agents.context_preview == Some(node.id) {
            let over = pointer.is_some_and(|p| {
                p.distance(auto) <= reach * 1.6
                    || self
                        .agents
                        .context_preview_rect
                        .is_some_and(|rect| rect.expand(canvas_scale::px(12.0, z)).contains(p))
            });
            if self.agents.context_preview_rect.is_some() && !over {
                self.agents.context_preview = None;
                self.agents.context_preview_rect = None;
            } else {
                self.paint_context_preview(painter, xf, node, auto);
            }
        }
        let mut open = None;
        for output in [false, true] {
            if !output && !has_refs {
                continue;
            }
            if output && !self.agent_has_outputs(node.id) {
                continue;
            }
            let artifacts = if output {
                Vec::new()
            } else {
                self.agent_artifacts(node.id, false)
            };
            let anchor = if output {
                Pos2::new(
                    r.right() - slate_doc::agent_chat::PORT_INSET * z,
                    r.center().y,
                )
            } else {
                auto
            };
            if output {
                let near = pointer.is_some_and(|p| p.distance(anchor) <= canvas_scale::px(12.0, z));
                painter.circle_filled(
                    anchor,
                    radius * if near { 1.12 } else { 1.0 },
                    gray.gamma_multiply(0.8),
                );
            }
            let response = ui.interact(
                Rect::from_center_size(anchor, egui::vec2(16.0, 16.0) * z),
                Id::new(("agent-artifact", node.id.0, output)),
                Sense::hover(),
            );
            if output {
                if self.agents.artifact_popup == Some((node.id, true)) {
                    self.paint_agent_output_stack(ui, xf, node.id, anchor);
                } else {
                    let count = self.agent_outputs(node.id).list.items.len();
                    response.on_hover_text(format!("Outputs · {count}"));
                }
                continue;
            }
            let blurb = self.agent_context_blurb(node.id);
            response.on_hover_text(blurb);
            if self.agents.artifact_popup == Some((node.id, output)) {
                let title = "Linked context";
                let dark = palette.dark_mode;
                let shown = atlas_shell::menu::anchored(
                    ui.ctx(),
                    Id::new(("agent-artifact-list", node.id.0, output)),
                    anchor + egui::vec2(0.0, 14.0 * z),
                    dark,
                    360.0,
                    None,
                    |ui| {
                        atlas_shell::menu::heading(ui, title, dark);
                        if artifacts.is_empty() {
                            atlas_shell::menu::note(
                                ui,
                                "No reported artifacts for this exchange.",
                                dark,
                            );
                        }
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                for a in &artifacts {
                                    let label = format!("{} · {}", a.kind.label(), a.title);
                                    if ui
                                        .add_enabled(
                                            a.kind != atlas_ai::agent::ArtifactKind::Deleted,
                                            egui::Button::new(label),
                                        )
                                        .on_hover_text(&a.source)
                                        .clicked()
                                    {
                                        open = Some(a.id.clone());
                                    }
                                }
                            });
                        atlas_shell::menu::item(
                            ui,
                            atlas_shell::menu::MenuIcon::None,
                            "Close",
                            dark,
                        )
                        .clicked()
                    },
                );
                if shown.inner {
                    self.agents.artifact_popup = None;
                    self.agents.artifact_popup_rect = None;
                }
                if self.agents.artifact_popup.is_some() {
                    self.agents.artifact_popup_rect = Some(shown.rect);
                }
            }
        }
        if let Some(id) = open {
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("portal.agent.open_artifact"),
                Some(serde_json::to_string(&(node.id.0, id)).unwrap()),
            );
            self.agents.artifact_popup = None;
            self.agents.artifact_popup_rect = None;
        }
        self.paint_preview_ask(ui, node, r);
    }

    fn paint_preview_ask(&mut self, ui: &egui::Ui, node: &Node, rect: Rect) {
        let Some((agent, artifact, at)) = self.agents.preview_ask.clone() else {
            return;
        };
        if agent != node.id {
            return;
        }
        let dot = Pos2::new(
            rect.right() - slate_doc::agent_chat::PORT_INSET * 2.0,
            rect.center().y,
        );
        let dark = self.palette().dark_mode;
        let mut chosen = None;
        let shown = atlas_shell::menu::anchored(
            ui.ctx(),
            Id::new(("agent-preview-ask", node.id.0)),
            dot + egui::vec2(8.0, 12.0),
            dark,
            260.0,
            Some(&mut self.agents.preview_ask_ready),
            |ui| {
                atlas_shell::menu::heading(ui, "Open this file", dark);
                atlas_shell::menu::note(
                    ui,
                    "This file can be read as source or drawn as a page.",
                    dark,
                );
                for face in [
                    slate_doc::media::PreviewFace::Text,
                    slate_doc::media::PreviewFace::Graphic,
                ] {
                    if atlas_shell::menu::item(
                        ui,
                        atlas_shell::menu::MenuIcon::None,
                        face.label(),
                        dark,
                    )
                    .clicked()
                    {
                        chosen = Some(face);
                    }
                }
            },
        );
        if let Some(face) = chosen {
            self.agents.preview_ask = None;
            self.agents.preview_ask_ready = false;
            let _ = self.open_agent_artifact_slot(
                Some(
                    &serde_json::json!({
                        "portal": agent.0,
                        "artifact": artifact,
                        "face": face.id(),
                    })
                    .to_string(),
                ),
                0,
                at,
            );
        } else if shown.dismissed {
            self.agents.preview_ask = None;
            self.agents.preview_ask_ready = false;
        }
    }

    fn context_preview_rows(&self, id: NodeId) -> Vec<(String, Option<String>)> {
        let mut rows = Vec::new();
        for artifact in self.agent_artifacts(id, false) {
            rows.push((
                format!("{} · {}", artifact.kind.label(), artifact.title),
                Some(artifact.id),
            ));
        }
        for node in &self.doc().scene.nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            let Some(binding) = &conn.binding else {
                continue;
            };
            let (source, target) = if binding.input_b {
                (&conn.a, &conn.b)
            } else {
                (&conn.b, &conn.a)
            };
            if slate_doc::agent_inputs::endpoint_node(target) != Some(id) {
                continue;
            }
            let Some(src) = slate_doc::agent_inputs::endpoint_node(source) else {
                continue;
            };
            let Some(label) = self.doc().scene.node(src).map(|n| match &n.kind {
                NodeKind::Text(t) => t
                    .text
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(64)
                    .collect::<String>(),
                NodeKind::Portal(p) => p.title.clone(),
                NodeKind::Image(_) => "Image".into(),
                _ => "Linked context".into(),
            }) else {
                continue;
            };
            if label.is_empty() || rows.iter().any(|(title, _)| title == &label) {
                continue;
            }
            rows.push((label, None));
        }
        if rows.is_empty() {
            rows.push(("No linked context".into(), None));
        }
        rows
    }

    /// Unjournaled cards. They last only while the pointer stays with them.
    fn paint_context_preview(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        anchor: Pos2,
    ) {
        use atlas_shell::selection_tools::{self as tools, Capsule, StackSide};
        let z = xf.z;
        let rows = self.context_preview_rows(node.id);
        let palette = self.palette();
        let mut bounds: Option<Rect> = None;
        let rects = tools::capsule_stack_rects(anchor, StackSide::Left, rows.len(), z);
        for ((label, _), rect) in rows.iter().zip(rects) {
            bounds = Some(bounds.map_or(rect, |b| b.union(rect)));
            let row = Capsule {
                label,
                ..Default::default()
            };
            tools::paint_capsule_row(painter, rect, &row, false, false, z, palette);
        }
        self.agents.context_preview_rect = bounds;
    }

    fn pin_agent_context(&mut self, id: NodeId) {
        let existing = self.provenance_portals(id, false);
        if !existing.is_empty() {
            self.begin_context_retract(&existing);
            return;
        }
        let ids: Vec<_> = self
            .agent_artifacts(id, false)
            .into_iter()
            .filter(|a| a.kind != atlas_ai::agent::ArtifactKind::Deleted)
            .map(|a| a.id)
            .collect();
        if ids.is_empty() {
            self.toast("No linked context to pin.");
            return;
        }
        for (slot, artifact_id) in ids.into_iter().enumerate() {
            let _ = self.open_agent_artifact_slot(
                Some(&serde_json::to_string(&(id.0, artifact_id)).unwrap()),
                slot,
                None,
            );
        }
    }

    /// Short screen stub so a provenance wire follows the raised context mark,
    /// and a human context wire follows the lower mark. The cached curve stays put.
    fn paint_context_wire_slide(
        &self,
        painter: &egui::Painter,
        node: NodeId,
        rest: Pos2,
        auto: Pos2,
        human: Pos2,
        color: Color32,
        z: f32,
    ) {
        use slate_doc::scene::{ConnectorEnd, Side};
        let scene = &self.doc().scene;
        for n in &scene.nodes {
            let NodeKind::Connector(conn) = &n.kind else {
                continue;
            };
            // A wire to pocketed context is not painted, so neither is its stub.
            if [&conn.a, &conn.b].into_iter().any(|end| {
                slate_doc::agent_inputs::endpoint_node(end)
                    .and_then(|id| scene.node(id))
                    .is_some_and(|other| other.hidden)
            }) {
                continue;
            }
            for end in [&conn.a, &conn.b] {
                let ConnectorEnd::Anchored {
                    node: id,
                    side: Side::Left,
                    t,
                } = end
                else {
                    continue;
                };
                if *id != node || (*t - 0.5).abs() > 0.08 {
                    continue;
                }
                let tip = if conn.binding.is_some() { human } else { auto };
                painter.line_segment(
                    [rest, tip],
                    egui::Stroke::new((1.25 * z).max(1.0), color.gamma_multiply(0.55)),
                );
            }
        }
    }

    pub(crate) fn open_agent_artifact(&mut self, detail: Option<&str>) -> bool {
        self.open_agent_artifact_slot(detail, 0, None)
    }

    fn open_agent_artifact_slot(
        &mut self,
        detail: Option<&str>,
        slot: usize,
        at: Option<Pos2>,
    ) -> bool {
        let Some((id, artifact_id, face)) = detail.and_then(parse_artifact_open) else {
            return false;
        };
        let id = NodeId(id);
        let Some(original) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let Some(artifact) = self
            .agents
            .session(id)
            .and_then(|s| s.artifacts.iter().find(|a| a.id == artifact_id))
            .cloned()
        else {
            // An output capsule, answering the text-or-graphic chooser.
            let face = (face != atlas_agent::Face::Auto).then_some(face);
            return self.spawn_agent_output(id, &artifact_id, face, at);
        };
        if artifact.kind == atlas_ai::agent::ArtifactKind::Deleted {
            self.toast("This file was deleted in this exchange.");
            return true;
        }
        if let Some(existing) = self.live_spawn(id, &artifact_id) {
            if let Some(at) = at {
                self.move_node_to(existing, at);
            } else {
                self.begin_context_retract(&[existing]);
            }
            return true;
        }
        let output = artifact.kind.is_output();
        let rect = if let Some(at) = at {
            slate_doc::WorldRect::new(at.x, at.y, 480.0, 320.0)
        } else {
            self.clear_spawn_rect(slate_doc::WorldRect::new(
                if output {
                    original.rect.x + original.rect.w + 72.0
                } else {
                    original.rect.x - 480.0 - 72.0
                },
                original.rect.y + slot as f32 * 376.0,
                480.0,
                320.0,
            ))
        };
        let source = if artifact.kind == atlas_ai::agent::ArtifactKind::Web {
            if !(artifact.source.starts_with("https://") || artifact.source.starts_with("http://"))
            {
                return false;
            }
            artifact.source.clone()
        } else {
            let path = PathBuf::from(&artifact.source);
            if path.is_absolute() {
                artifact.source.clone()
            } else {
                self.agent_folder_for(id)
                    .unwrap_or_default()
                    .join(path)
                    .to_string_lossy()
                    .into_owned()
            }
        };
        let portal = match self.build_artifact_node(&source, face, rect) {
            ArtifactBuild::Node(node) => node,
            ArtifactBuild::Ask => {
                self.agents.preview_ask = Some((id, artifact_id, at));
                self.agents.preview_ask_ready = false;
                return true;
            }
            ArtifactBuild::Unavailable => return false,
        };
        let portal_id = portal.id;
        let wire = self.provenance_wire(id, &portal, output);
        self.add_nodes(vec![portal, wire]);
        self.agents.spawned.insert((id, artifact_id), portal_id);
        self.board_sel = std::iter::once(portal_id).collect();
        true
    }

    /// The node an artifact becomes, and nothing else: a web portal for a URL
    /// or a page shown as a graphic, a File Atlas portal for a folder, the
    /// placed file otherwise, and File Atlas on the parent folder, filtered to
    /// the name, for a file that is not there. A file with more than one face
    /// and none chosen asks. Wires, selection and tracking are the caller's.
    pub(crate) fn build_artifact_node(
        &mut self,
        source: &str,
        face: atlas_agent::Face,
        rect: slate_doc::WorldRect,
    ) -> ArtifactBuild {
        use atlas_agent::Face;
        use slate_doc::media::PreviewFace;
        if source.starts_with("https://") || source.starts_with("http://") {
            return ArtifactBuild::Node(self.build_web_portal(rect, Some(source.into()), None));
        }
        let path = std::path::Path::new(source);
        let chosen = match face {
            Face::Graphic => Some(PreviewFace::Graphic),
            Face::Text => Some(PreviewFace::Text),
            Face::Auto | Face::Images | Face::Folder => None,
        };
        let choices = slate_doc::media::preview_choices(path);
        if choices.len() > 1 && chosen.is_none() && face != Face::Folder {
            return ArtifactBuild::Ask;
        }
        let graphic = chosen == Some(PreviewFace::Graphic)
            || (chosen.is_none() && choices == [PreviewFace::Graphic]);
        let node = if graphic && path.is_file() {
            self.build_web_portal(rect, Some(path.to_string_lossy().into_owned()), None)
        } else if path.is_dir() {
            self.build_bound_atlas(rect, path, Vec::new())
        } else if path.is_file() && face != Face::Folder {
            let Some(item) = self.item_for_path(path) else {
                return ArtifactBuild::Unavailable;
            };
            self.doc_mut().scene.build_node(
                rect,
                NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
            )
        } else {
            let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
                return ArtifactBuild::Unavailable;
            };
            self.build_bound_atlas(rect, parent, vec![name.to_string_lossy().into_owned()])
        };
        ArtifactBuild::Node(node)
    }

    fn live_spawn(&self, agent: NodeId, artifact: &str) -> Option<NodeId> {
        let id = *self.agents.spawned.get(&(agent, artifact.to_string()))?;
        self.doc().scene.node(id).is_some().then_some(id)
    }

    /// Context cards tied to this agent by a faint provenance wire.
    /// `output` selects the right-hand (created) side; otherwise the left.
    fn provenance_portals(&self, agent: NodeId, output: bool) -> Vec<NodeId> {
        use slate_doc::scene::{ConnectorEnd, Side, WireDisplay};
        let want = if output { Side::Right } else { Side::Left };
        let mut found = Vec::new();
        for n in &self.doc().scene.nodes {
            let NodeKind::Connector(c) = &n.kind else {
                continue;
            };
            if c.binding.is_some() || c.display != WireDisplay::Faint {
                continue;
            }
            let (
                ConnectorEnd::Anchored {
                    node: a, side: sa, ..
                },
                ConnectorEnd::Anchored { node: b, .. },
            ) = (&c.a, &c.b)
            else {
                continue;
            };
            let portal = if *a == agent && *sa == want {
                *b
            } else if *b == agent {
                if let ConnectorEnd::Anchored { side, .. } = &c.b {
                    if *side == want {
                        *a
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            } else {
                continue;
            };
            if self.doc().scene.node(portal).is_some() {
                found.push(portal);
            }
        }
        found
    }

    /// Provenance wire and the agent-dot position a spawned card retracts toward.
    pub(crate) fn machine_context_link(&self, portal: NodeId) -> Option<(NodeId, Pos2)> {
        use slate_doc::scene::{ConnectorEnd, WireDisplay};
        for n in &self.doc().scene.nodes {
            let NodeKind::Connector(c) = &n.kind else {
                continue;
            };
            if c.binding.is_some() || c.display != WireDisplay::Faint {
                continue;
            }
            let (a, b) = match (&c.a, &c.b) {
                (
                    ConnectorEnd::Anchored {
                        node: na,
                        side: sa,
                        t: ta,
                    },
                    ConnectorEnd::Anchored {
                        node: nb,
                        side: sb,
                        t: tb,
                    },
                ) => ((*na, *sa, *ta), (*nb, *sb, *tb)),
                _ => continue,
            };
            let agent = if a.0 == portal {
                b
            } else if b.0 == portal {
                a
            } else {
                continue;
            };
            if !self.is_agent_portal(agent.0) {
                continue;
            }
            let anchor = self
                .doc()
                .scene
                .node(agent.0)
                .map(|nn| slate_doc::connector_anchor_on(nn, agent.1, agent.2))
                .unwrap_or([0.0, 0.0]);
            return Some((n.id, Pos2::new(anchor[0], anchor[1])));
        }
        None
    }

    /// The faint wire [`Self::machine_context_link`] follows: from the card's
    /// right midpoint to a spawned output, or from spawned context into the
    /// card's left midpoint. Provenance is never model input, so it has no
    /// binding. `node` may still be waiting for the same `add_nodes` call.
    pub(crate) fn provenance_wire(&mut self, agent: NodeId, node: &Node, output: bool) -> Node {
        use slate_doc::scene::{ConnectorEnd, Side};
        let agent_end = ConnectorEnd::Anchored {
            node: agent,
            side: if output { Side::Right } else { Side::Left },
            t: 0.5,
        };
        let node_end = ConnectorEnd::Anchored {
            node: node.id,
            side: if output { Side::Left } else { Side::Right },
            t: 0.5,
        };
        let (a, b) = if output {
            (agent_end, node_end)
        } else {
            (node_end, agent_end)
        };
        let mut wire = self.build_connector_with(a, b, std::slice::from_ref(node));
        if let NodeKind::Connector(c) = &mut wire.kind {
            c.binding = None;
            c.display = slate_doc::scene::WireDisplay::Faint;
        }
        wire
    }

    fn clear_spawn_rect(&self, mut rect: slate_doc::WorldRect) -> slate_doc::WorldRect {
        let hits = |a: slate_doc::WorldRect, b: slate_doc::WorldRect| {
            a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
        };
        for _ in 0..16 {
            let blocked = self
                .doc()
                .scene
                .nodes
                .iter()
                .any(|n| !n.hidden && n.rect.w > 2.0 && n.rect.h > 2.0 && hits(n.rect, rect));
            if !blocked {
                break;
            }
            rect.y += rect.h + 48.0;
        }
        rect
    }

    fn move_node_to(&mut self, id: NodeId, at: Pos2) {
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return;
        };
        let mut after = before.clone();
        after.rect.x = at.x;
        after.rect.y = at.y;
        let _ = self.commit_scene(vec![slate_doc::SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]);
    }

    pub(crate) fn begin_context_retract(&mut self, portals: &[NodeId]) {
        self.retract_context(portals, &HashSet::new());
    }

    /// Context shrinks back into the agent it belongs to. A provenance card is
    /// removed. Context a chat card already sent is pocketed (hidden, wires kept)
    /// into every card that used it. `leaving` cards are deleted by the same action.
    pub(crate) fn retract_context(&mut self, ids: &[NodeId], leaving: &HashSet<NodeId>) {
        let mut drop = Vec::new();
        let mut rest = Vec::new();
        for id in ids {
            let Some(node) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let Some((wire, anchor)) = self.machine_context_link(*id) else {
                rest.push(*id);
                continue;
            };
            let now = Instant::now();
            if matches!(node.kind, NodeKind::Frame(_)) {
                let members = self.doc().scene.members_of(*id);
                let ghosts: Vec<_> = members
                    .iter()
                    .filter_map(|m| self.doc().scene.node(*m))
                    .filter(|m| !matches!(m.kind, NodeKind::Connector(_)))
                    .map(|m| (m.clone(), anchor, now))
                    .collect();
                self.agents.retract_ghosts.push((node, anchor, now));
                self.agents.retract_ghosts.extend(ghosts);
                drop.extend(members);
            } else {
                self.agents.retract_ghosts.push((node, anchor, now));
            }
            drop.push(wire);
            drop.push(*id);
            self.agents.spawned.retain(|_, n| n != id);
        }
        for id in self.pocket_context(&rest, leaving) {
            if self
                .doc()
                .scene
                .node(id)
                .is_some_and(|n| matches!(n.kind, NodeKind::Frame(_)))
            {
                drop.extend(self.doc().scene.members_of(id));
            }
            drop.push(id);
            self.agents.spawned.retain(|_, n| *n != id);
        }
        drop.sort();
        drop.dedup();
        if !drop.is_empty() {
            self.delete_board_nodes_now(&drop);
        }
    }

    /// Cards other than `leaving` that already sent `id` as context.
    fn context_consumers(&self, id: NodeId, leaving: &HashSet<NodeId>) -> Vec<NodeId> {
        let mut cards: Vec<_> = slate_doc::agent_inputs::consumed_by(&self.doc().scene, id)
            .into_iter()
            .map(|(_, card)| card)
            .filter(|card| !leaving.contains(card))
            .collect();
        cards.sort();
        cards.dedup();
        cards
    }

    /// Hide each used context in one journal step, shrinking a ghost into every
    /// visible consuming card's input. Returns the ids no card had used.
    fn pocket_context(&mut self, ids: &[NodeId], leaving: &HashSet<NodeId>) -> Vec<NodeId> {
        use slate_doc::scene::Side;
        let mut unused = Vec::new();
        let mut commands = Vec::new();
        let now = Instant::now();
        for id in ids {
            let cards = self.context_consumers(*id, leaving);
            let Some(before) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            if cards.is_empty() {
                unused.push(*id);
                continue;
            }
            if before.hidden {
                continue;
            }
            for card in cards {
                if let Some(host) = self.doc().scene.node(card).filter(|n| !n.hidden) {
                    let anchor = slate_doc::connector_anchor_on(host, Side::Left, 0.5);
                    self.agents.retract_ghosts.push((
                        before.clone(),
                        Pos2::new(anchor[0], anchor[1]),
                        now,
                    ));
                }
            }
            let mut after = before.clone();
            after.hidden = true;
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        if !commands.is_empty() && self.commit_scene(commands) {
            for id in ids {
                self.board_sel.remove(id);
            }
            for open in self.agents.pocket_open.values_mut() {
                open.retain(|id| !ids.contains(id));
            }
        }
        unused
    }

    /// Hidden context, or context a handle click brought out of this card.
    pub(crate) fn agent_has_pocket(&self, card: NodeId) -> bool {
        if self
            .agents
            .pocket_open
            .get(&card)
            .is_some_and(|open| !open.is_empty())
        {
            return true;
        }
        let revision = (self.scene_gen, self.doc().scene.scene_gen());
        let mut cache = self.agents.pocket_cache.borrow_mut();
        if cache.0 != revision {
            *cache = (
                revision,
                slate_doc::agent_inputs::pocket_holders(&self.doc().scene)
                    .into_iter()
                    .collect(),
            );
        }
        cache.1.contains(&card)
    }

    /// The human context handle: bring the card's pocketed context out beside
    /// it, or pocket what that click brought out. One journal step each way.
    pub(crate) fn agent_toggle_pocket(&mut self, detail: Option<&str>) -> bool {
        let card = detail
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(NodeId)
            .or_else(|| self.selected_agent_portal())
            .filter(|id| slate_doc::agent_inputs::is_chat_card(&self.doc().scene, *id));
        let Some(card) = card else {
            return false;
        };
        let Some(host) = self.doc().scene.node(card).map(|n| n.rect) else {
            return false;
        };
        let hidden = slate_doc::agent_inputs::pocketed(&self.doc().scene, card);
        if hidden.is_empty() {
            let open: Vec<_> = self
                .agents
                .pocket_open
                .remove(&card)
                .unwrap_or_default()
                .into_iter()
                .filter(|id| {
                    self.doc().scene.node(*id).is_some_and(|n| !n.hidden)
                        && self.context_consumers(*id, &HashSet::new()).contains(&card)
                })
                .collect();
            if open.is_empty() {
                self.toast("Nothing is pocketed in this card.");
                return false;
            }
            self.pocket_context(&open, &HashSet::new());
            return true;
        }
        let mut y = host.y;
        let mut commands = Vec::new();
        for id in &hidden {
            let Some(before) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let placed = self.clear_spawn_rect(slate_doc::WorldRect::new(
                host.x - before.rect.w - 72.0,
                y,
                before.rect.w,
                before.rect.h,
            ));
            y = placed.y + placed.h + 48.0;
            let mut after = before.clone();
            after.hidden = false;
            after.rect.x = placed.x;
            after.rect.y = placed.y;
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        if !self.commit_scene(commands) {
            return false;
        }
        self.agents.pocket_open.insert(card, hidden);
        true
    }

    pub(crate) fn paint_context_retract(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
    ) {
        // 75% faster than the previous 0.42s suck.
        const SECS: f32 = 0.24;
        if self.agents.retract_ghosts.is_empty() {
            return;
        }
        self.agents
            .retract_ghosts
            .retain(|(_, _, at)| at.elapsed().as_secs_f32() < SECS);
        let ghosts = self.agents.retract_ghosts.clone();
        for (node, anchor, at) in ghosts {
            let t = (at.elapsed().as_secs_f32() / SECS).clamp(0.0, 1.0);
            let ease = t * t;
            let mut ghost = node;
            let cx = ghost.rect.x + ghost.rect.w * 0.5;
            let cy = ghost.rect.y + ghost.rect.h * 0.5;
            let scale = (1.0 - ease * 0.92).max(0.04);
            let nx = cx + (anchor.x - cx) * ease;
            let ny = cy + (anchor.y - cy) * ease;
            ghost.rect.w *= scale;
            ghost.rect.h *= scale;
            ghost.rect.x = nx - ghost.rect.w * 0.5;
            ghost.rect.y = ny - ghost.rect.h * 0.5;
            ghost.opacity *= 1.0 - ease;
            ghost.hidden = false;
            self.paint_board_node(ui, painter, xf, &ghost, false);
        }
        if !self.agents.retract_ghosts.is_empty() {
            ui.ctx().request_repaint();
        }
    }

    fn agent_has_child(&self, id: NodeId) -> bool {
        self.doc()
            .scene
            .nodes
            .iter()
            .any(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
    }

    fn agent_linear_terminal(&self, id: NodeId) -> bool {
        self.agent_linear(id)
            && !self
                .doc()
                .scene
                .nodes
                .iter()
                .any(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
    }
    fn agent_linear(&self, id: NodeId) -> bool {
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.chat.linear || atlas_ai::runtime::linear_provider(&a.provider))
    }
    pub(crate) fn agent_fork_selected(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        if self.agent_linear(id) {
            self.toast("Coding agents use one conversation stream.");
            return true;
        }
        if self.agent_is_running(id) {
            self.toast("Wait for a completed checkpoint before forking.");
            return true;
        }
        if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| !a.chat.train)
        {
            self.toast("Choose Chat train, then select the message to fork.");
            return true;
        }
        self.agent_focus(id);
        self.agent_set_detail(slate_doc::agent_chat::Detail::Full);
        self.toast(
            "Write the alternative message and Send. A new branch will start at this checkpoint.",
        );
        true
    }

    fn ensure_agent_programs(&mut self) {
        if !self.agents.programs_started {
            self.agents.programs_started = true;
            let ws = self.ai.config.workspace_dir.clone();
            let (tx, rx) = unbounded();
            self.agents.programs_rx = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(atlas_ai::runtime::discover_programs(ws.as_deref()));
            });
        }
        if let Some(programs) = self
            .agents
            .programs_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.agents.programs = programs;
            self.agents.programs_rx = None;
        }
    }

    pub(crate) fn set_agent_program(&mut self, id: NodeId, provider: &str) {
        if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.chat.parent.is_some() || a.chat.end.is_some())
        {
            self.toast(
                "Create a new agent portal to change programs; this card is a history checkpoint.",
            );
            return;
        }
        if self.agent_is_running(id) {
            self.toast("Stop the current response before changing programs.");
            return;
        }
        if let Some((old, _)) = self.agent_session_for(id) {
            self.agents.codex.remove(&old);
        }

        let program = self
            .agents
            .programs
            .iter()
            .find(|p| p.id == provider)
            .cloned()
            .unwrap_or_else(|| atlas_ai::agent::provider_by_id(provider));
        let binding = self.program_binding(Some(id));
        self.patch_nodes(&[id], |node| bind_program(node, &program, &binding));

        self.agents.project_picker = atlas_ai::runtime::linear_provider(provider).then_some(id);
        self.agents.sessions.remove(&id);
        self.agents.local_turns.remove(&id);
        self.agents.awaiting.remove(&id);
        self.agent_focus(id);
    }

    /// A fresh session and the locators a newly bound program writes through.
    pub(crate) fn program_binding(&self, id: Option<NodeId>) -> ProgramBinding {
        let source = id
            .and_then(|id| self.agent_folder_for(id))
            .or_else(|| self.ai.config.workspace_dir.clone())
            .or_else(|| {
                self.tab()
                    .path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_path_buf())
            });
        let locator = source
            .as_deref()
            .map(|path| super::board_portal::source_locator(self.tab().path.as_deref(), path));
        let session = slate_doc::scene::new_agent_session_id();
        let bundle = self
            .ai
            .config
            .workspace_dir
            .as_ref()
            .map(|ws| slate_doc::SourceUri {
                locator: super::board_portal::source_locator(
                    self.tab().path.as_deref(),
                    &atlas_ai::agent::agent_dir(ws, &session).join("session.json"),
                ),
            });
        ProgramBinding {
            session,
            bundle,
            locator,
        }
    }

    pub(crate) fn agent_images(
        &self,
        id: NodeId,
    ) -> std::sync::Arc<Vec<atlas_ai::agent::ImageOutput>> {
        let key = (self.scene_gen, self.agents.output_epoch);
        if let Some((_, images)) = self
            .agents
            .image_cache
            .borrow()
            .get(&id)
            .filter(|(k, _)| *k == key)
        {
            return images.clone();
        }
        let mut images = Vec::new();
        if let Some(seed) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|a| a.seed.clone())
        {
            images.push(seed);
        }
        if let Some(session) = self.agents.sessions.get(&id) {
            for image in &session.bundle.images {
                if !image.id.is_empty()
                    && !image.source.is_empty()
                    && !images.iter().any(|v| v.id == image.id)
                {
                    images.push(image.clone());
                }
            }
        }
        let images = std::sync::Arc::new(images);
        self.agents
            .image_cache
            .borrow_mut()
            .insert(id, (key, images.clone()));
        images
    }

    pub(crate) fn agent_active_output(&self, id: NodeId) -> Option<String> {
        let images = self.agent_images(id);
        images
            .get(self.agent_shown_index(id)?)
            .map(|i| i.id.clone())
    }

    /// Which result an agent's card shows. A picture shows the file a person
    /// picked, else the newest result; a chat card's album shows its focus.
    pub(crate) fn agent_shown_index(&self, id: NodeId) -> Option<usize> {
        let images = self.agent_images(id);
        if images.is_empty() {
            return None;
        }
        if let Some(NodeKind::Image(img)) = self.doc().scene.node(id).map(|n| &n.kind) {
            let base = self.tab().path.as_deref();
            if let Some(item) = self.doc().item(img.item) {
                if let Some(i) = images
                    .iter()
                    .position(|o| resolve_source(base, &o.source) == item.path)
                {
                    return Some(i);
                }
            }
            let newest = slate_doc::agent_inputs::newest_image(&images)?;
            return images.iter().position(|o| o.id == newest.id);
        }
        Some(
            self.agents
                .cover_focus
                .get(&id)
                .copied()
                .unwrap_or(0)
                .min(images.len() - 1),
        )
    }

    pub(crate) fn toggle_agent_wire_output(&mut self) -> bool {
        let Some(id)=self.board_sel.iter().copied().find(|id|matches!(self.doc().scene.node(*id).map(|n|&n.kind),Some(NodeKind::Connector(c)) if c.binding.as_ref().is_some_and(|b|b.kind==slate_doc::agent_inputs::InputKind::Images))) else{return false;};
        let NodeKind::Connector(c) = &self.doc().scene.node(id).unwrap().kind else {
            return false;
        };
        let binding = c.binding.as_ref().unwrap();
        let all = !binding.all_images;
        let source = if binding.input_b { &c.a } else { &c.b };
        let pin = if all {
            None
        } else {
            slate_doc::agent_inputs::endpoint_node(source)
                .and_then(|id| self.agent_active_output(id))
        };
        self.patch_nodes(&[id], |n| {
            if let NodeKind::Connector(c) = &mut n.kind {
                if let Some(b) = &mut c.binding {
                    b.all_images = all;
                    b.output = pin.clone();
                }
            }
        });
        true
    }

    /// The inputs an image generator reads now, in wire order. Cached until the
    /// scene or an upstream output changes.
    pub(crate) fn generator_view(&self, id: NodeId) -> std::rc::Rc<GeneratorView> {
        let editing = self.text_edit.as_ref().map(|(node, text)| {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (node, text).hash(&mut hasher);
            hasher.finish()
        });
        let key = (self.scene_gen, self.agents.output_epoch, editing);
        if let Some((cached, view)) = self.agents.generator_views.borrow().get(&id) {
            if *cached == key {
                return view.clone();
            }
        }
        let view = std::rc::Rc::new(self.resolve_generator(id));
        self.agents
            .generator_views
            .borrow_mut()
            .insert(id, (key, view.clone()));
        view
    }

    fn resolve_generator(&self, id: NodeId) -> GeneratorView {
        use std::hash::{Hash, Hasher};
        let inputs = match self.agent_input_snapshot(id) {
            Ok(inputs) => inputs,
            Err(error) => {
                return GeneratorView {
                    error: Some(error),
                    ..Default::default()
                }
            }
        };
        let mut view = GeneratorView::default();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let mut prompts = Vec::new();
        let short = |text: &str| -> String {
            let line = text.lines().next().unwrap_or("").trim();
            let mut out: String = line.chars().take(36).collect();
            if line.chars().count() > 36 {
                out.push('…');
            }
            out
        };
        for item in &inputs.wired {
            let node = NodeId(item.node);
            let slot = item.port();
            if let Some(info) = self
                .model_node_info(node)
                .filter(|_| slot == atlas_agent::InputSlot::Media)
            {
                node.0.hash(&mut hasher);
                view.inputs.push(GeneratorInput {
                    node,
                    role: InputRole::Geometry,
                    label: info
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "3D model".into()),
                });
                continue;
            }
            // A web page's picture is captured when the run starts.
            if item.images.is_empty()
                && !slot.takes_text()
                && slate_doc::agent_inputs::is_web_page(&self.doc().scene, node)
            {
                node.0.hash(&mut hasher);
                view.inputs.push(GeneratorInput {
                    node,
                    role: InputRole::of_slot(slot),
                    label: "Web page".into(),
                });
                continue;
            }
            if let Some(first) = item.images.first().filter(|_| !slot.takes_text()) {
                (slot, &item.images).hash(&mut hasher);
                let label = if matches!(
                    self.doc().scene.node(node).map(|n| &n.kind),
                    Some(NodeKind::Portal(_))
                ) {
                    "Generator output".to_string()
                } else {
                    std::path::Path::new(first)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Image".into())
                };
                view.inputs.push(GeneratorInput {
                    node,
                    role: InputRole::of_slot(slot),
                    label,
                });
                continue;
            }
            let text = item.text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            text.hash(&mut hasher);
            view.inputs.push(GeneratorInput {
                node,
                role: InputRole::Prompt,
                label: short(&text),
            });
            prompts.push(text);
        }
        view.prompt = prompts.join(", ");
        view.signature = hasher.finish();
        view
    }

    /// Wired notes are the prompt. Typed text is added after them.
    fn generator_prompt(&mut self, id: NodeId) -> String {
        let view = self.generator_view(id);
        // The journaled prompt an image agent was submitted with, else a draft.
        let authored = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.instruction.trim().to_string())
            .unwrap_or_default();
        let typed = if authored.is_empty() {
            self.agents.prompt_mut(id).trim().to_string()
        } else {
            authored
        };
        let prompt = [view.prompt.as_str(), typed.as_str()]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        if !prompt.is_empty() {
            return prompt;
        }
        self.agent_images(id)
            .last()
            .map(|i| i.prompt.clone())
            .unwrap_or_default()
    }

    fn generation_request(&mut self, id: NodeId, live: bool) -> Result<AgentRequest, String> {
        if let Some(error) = self.generator_view(id).error.clone() {
            return Err(error);
        }
        let prompt = self.generator_prompt(id);
        if prompt.is_empty() {
            return Err("Connect a note with a prompt to this generator.".into());
        }
        let inputs = self.agent_input_snapshot(id)?;
        let (model, session, settings) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| (a.model.clone(), a.session.clone(), a.image))
            .unwrap_or_default();
        let live_run = live.then(|| {
            self.agents
                .live_run
                .entry(id)
                .or_insert_with(atlas_ai::agent::request_id)
                .clone()
        });
        // Live repeats one seed per generator; otherwise a locked seed repeats.
        let image = Some(atlas_ai::agent::ImageParams {
            seed: if live {
                Some(settings.seed.unwrap_or_else(|| stable_seed(&session)))
            } else {
                settings.seed
            },
            live: live_run,
            aspect: settings.aspect,
            count: settings.count,
        });
        Ok(AgentRequest {
            id: atlas_ai::agent::request_id(),
            prompt,
            model,
            at: atlas_ai::context::now_secs(),
            inputs,
            history: vec![],
            image,
            output_dir: None,
            oneshot: false,
        })
    }

    /// Every press is accepted. One generation runs per note at a time.
    pub(crate) fn queue_generation(&mut self, id: NodeId) {
        match self.generation_request(id, false) {
            Ok(request) => self.enqueue_generation(id, request),
            Err(error) => {
                self.toast(error.clone());
                self.fail_agent_await(id, error);
            }
        }
    }

    pub(crate) fn enqueue_generation(&mut self, id: NodeId, request: AgentRequest) {
        if self.ai.config.valid_workspace().is_none() {
            self.fail_agent_await(
                id,
                "Set an AI workspace before generating — the agent link has nowhere to write."
                    .into(),
            );
            self.toast("Set an AI workspace before generating.");
            #[cfg(not(test))]
            self.ai.pick_workspace();
            return;
        }
        let queue = self.agents.comfy_queue.entry(id).or_default();
        if queue.len() >= GENERATION_QUEUE {
            self.toast("Eight generations are already waiting on this generator.");
            return;
        }
        queue.push_back(request);
        if matches!(
            self.agents.awaiting.get(&id),
            Some(AgentAwait::Failed { .. })
        ) {
            self.agents.awaiting.remove(&id);
        }
        self.pump_comfy_queue();
    }

    pub(crate) fn generations_waiting(&self, id: NodeId) -> usize {
        self.agents.comfy_queue.get(&id).map_or(0, |q| q.len())
    }

    pub(crate) fn pump_comfy_queue(&mut self) {
        let ready: Vec<NodeId> = self
            .agents
            .comfy_queue
            .iter()
            .filter(|(id, queue)| !queue.is_empty() && !self.agent_is_running(**id))
            .map(|(id, _)| *id)
            .collect();
        for id in ready {
            let Some(mut request) = self
                .agents
                .comfy_queue
                .get(&id)
                .and_then(|queue| queue.front().cloned())
            else {
                continue;
            };
            match self.capture_generator_inputs(id, &mut request) {
                Ok(true) => {}
                Ok(false) => {
                    let since = *self
                        .agents
                        .capture_wait
                        .entry(id)
                        .or_insert_with(Instant::now);
                    if since.elapsed() > CAPTURE_TIMEOUT {
                        self.agents.capture_wait.remove(&id);
                        self.agents.comfy_queue.remove(&id);
                        self.fail_agent_await(
                            id,
                            "The wired 3D model, video or web page did not finish loading.".into(),
                        );
                    }
                    continue;
                }
                Err(error) => {
                    self.agents.capture_wait.remove(&id);
                    self.agents.comfy_queue.remove(&id);
                    self.fail_agent_await(id, error);
                    continue;
                }
            }
            self.agents.capture_wait.remove(&id);
            if let Some(queue) = self.agents.comfy_queue.get_mut(&id) {
                queue.pop_front();
            }
            self.dispatch_generation(id, request);
        }
    }

    /// A wired 3D model is captured when its run starts, from its current camera.
    fn capture_generator_inputs(
        &mut self,
        id: NodeId,
        request: &mut AgentRequest,
    ) -> Result<bool, String> {
        let session = self
            .agent_session_for(id)
            .map(|(s, _)| s)
            .unwrap_or_default();
        let dir = atlas_core::index::data_dir()
            .join("comfy-inputs")
            .join(&session);
        for item in &mut request.inputs.wired {
            let node = NodeId(item.node);
            // A web page on Prompt feeds its visible text, read once as the
            // run starts (D15 / D27 amendment).
            if item.port().takes_text()
                && slate_doc::agent_inputs::is_web_page(&self.doc().scene, node)
            {
                match self.read_web_text(node)? {
                    Some(text) => item.text = text,
                    None => return Ok(false),
                }
                continue;
            }
            // A web page on a picture port feeds what it shows, as pixels.
            if item.images.is_empty()
                && !item.port().takes_text()
                && slate_doc::agent_inputs::is_web_page(&self.doc().scene, node)
            {
                match self.capture_web_page(node, &dir)? {
                    Some(page) => item.images = vec![page.to_string_lossy().into_owned()],
                    None => return Ok(false),
                }
                continue;
            }
            // A video feeds the frame it shows, the way a model feeds its view.
            if self.node_is_video(node) {
                match self.capture_video_frame(node, &dir)? {
                    Some(frame) => item.images = vec![frame.to_string_lossy().into_owned()],
                    None => return Ok(false),
                }
                continue;
            }
            if self.model_node_info(node).is_none() {
                continue;
            }
            match self.capture_model_inputs(node, &dir)? {
                Some(capture) => {
                    item.images = vec![capture.view.to_string_lossy().into_owned()];
                    item.depth = capture
                        .depth
                        .map(|path| path.to_string_lossy().into_owned());
                }
                None => return Ok(false),
            }
        }
        // A text block reads what was captured (a page's text), not what its
        // wires held when it was queued.
        if request.oneshot {
            request.prompt = self.text_block_prompt(id, &request.inputs);
        }
        Ok(true)
    }

    fn dispatch_generation(&mut self, id: NodeId, request: AgentRequest) {
        let Some((session, provider)) = self.agent_session_for(id) else {
            return;
        };
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            return;
        };
        let dir = self
            .agent_link_dir(id, &ws)
            .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &session));
        let manifest = slate_doc::SourceUri {
            locator: super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &dir.join("session.json"),
            ),
        };
        if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.bundle.is_none())
        {
            self.patch_nodes(&[id], |n| {
                if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                    a.bundle.get_or_insert_with(|| manifest.clone());
                }
            });
        }
        self.agents.requests.insert(id, request.id.clone());
        self.agents.bindings.insert(id, session.clone());
        self.agents.awaiting.insert(
            id,
            AgentAwait::Sent {
                at: Instant::now(),
                req_at: request.at,
            },
        );
        self.agents.sources.expect(&dir, &request.id);
        #[cfg(test)]
        {
            let _ = (provider, ws);
            self.agents.dispatched.push((id, request));
        }
        #[cfg(not(test))]
        {
            let cwd = self.agent_folder_for(id).unwrap_or_else(|| ws.clone());
            let runtime = self.agents.codex.entry(session).or_insert_with(|| {
                atlas_ai::runtime::CodexLink::start_provider(dir, cwd, provider)
            });
            if let Err(error) = runtime.send(request) {
                self.fail_agent_await(id, error);
            }
        }
    }

    fn live_generators(&self) -> Vec<NodeId> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter(|n| {
                slate_doc::agent_chat::agent(n)
                    .is_some_and(|a| atlas_ai::agent::local_image_engine(&a.provider) && a.live)
            })
            .map(|n| n.id)
            .collect()
    }

    /// Live re-renders whenever a wired input changes: the camera of a wired
    /// model, a prompt, a source image, or the checkpoint. Latest wins.
    fn pump_live_generators(&mut self) {
        use std::hash::{Hash, Hasher};
        self.release_steering();
        for id in self.live_generators() {
            if self.agent_is_running(id) || self.generations_waiting(id) > 0 {
                continue;
            }
            let view = self.generator_view(id);
            if view.inputs.is_empty() {
                continue;
            }
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            view.signature.hash(&mut hasher);
            self.agents.prompt_mut(id).trim().hash(&mut hasher);
            self.doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .map(|a| a.model.clone())
                .hash(&mut hasher);
            if let Some(model) = view.geometry() {
                let Some(pose) = self.model_pose_hash(model) else {
                    continue;
                };
                pose.hash(&mut hasher);
            }
            let signature = hasher.finish();
            if self.agents.live_sent.get(&id) == Some(&signature) {
                self.agents.live_settle.remove(&id);
                continue;
            }
            let typing = self.text_edit.as_ref().is_some_and(|(node, _)| {
                view.inputs
                    .iter()
                    .any(|i| i.node == *node && i.role == InputRole::Prompt)
            });
            if typing {
                let since = match self.agents.live_settle.get(&id) {
                    Some((pending, at)) if *pending == signature => *at,
                    _ => {
                        self.agents
                            .live_settle
                            .insert(id, (signature, Instant::now()));
                        continue;
                    }
                };
                if since.elapsed() < LIVE_TYPING_SETTLE {
                    continue;
                }
            }
            self.agents.live_settle.remove(&id);
            self.agents.live_sent.insert(id, signature);
            match self.generation_request(id, true) {
                Ok(request) => self.enqueue_generation(id, request),
                Err(error) => self.fail_agent_await(id, error),
            }
        }
    }

    fn model_pose_hash(&self, id: NodeId) -> Option<u64> {
        let info = self.model_node_info(id)?;
        if self.model3d.external.contains(&info.cache_key) {
            let stamp = self
                .model3d
                .enscape_stamps
                .get(&info.cache_key)
                .copied()
                .unwrap_or(0);
            return Some(
                stamp
                    ^ (info.rect.w.to_bits() as u64).rotate_left(17)
                    ^ (info.rect.h.to_bits() as u64).rotate_left(41),
            );
        }
        let cam = self
            .model3d
            .live
            .get(&id)
            .map(|vp| vp.cam)
            .unwrap_or(info.cam);
        Some(
            cam.cache_hash()
                ^ (info.rect.w.to_bits() as u64).rotate_left(17)
                ^ (info.rect.h.to_bits() as u64).rotate_left(41),
        )
    }

    /// A focused live generator flies its wired model. Leaving focus locks the
    /// model at that pose, one journaled camera change.
    fn release_steering(&mut self) {
        let done: Vec<(NodeId, NodeId)> = self
            .agents
            .steering
            .iter()
            .filter(|(generator, _)| self.contents_focused() != Some(**generator))
            .map(|(g, m)| (*g, *m))
            .collect();
        for (generator, model) in done {
            self.agents.steering.remove(&generator);
            if self.model3d.live.contains_key(&model) {
                self.lock_model(model);
            }
        }
    }

    fn steer_generator_model(&mut self, ui: &egui::Ui, generator: NodeId, body: Rect) {
        let Some(model) = self.generator_view(generator).geometry() else {
            return;
        };
        let response = ui.interact(
            body,
            Id::new(("generator-steer", generator.0)),
            Sense::drag(),
        );
        let scroll = if response.hovered() {
            ui.input(|i| i.smooth_scroll_delta.y)
        } else {
            0.0
        };
        if !(response.dragged() || scroll != 0.0) {
            return;
        }
        if self
            .model_node_info(model)
            .is_some_and(|info| self.model3d.external.contains(&info.cache_key))
        {
            if self.enscape_shown_node() != Some(model) {
                self.open_enscape_node(model);
            }
            return;
        }
        if !self.model3d.live.contains_key(&model) {
            self.unlock_model(model);
            self.agents.steering.insert(generator, model);
        }
        if response.dragged() {
            let delta = response.drag_delta();
            let pan = ui.input(|i| i.modifiers.shift);
            self.model_drag(model, delta.x, delta.y, pan, body.height());
        }
        if scroll != 0.0 {
            self.model_scroll(model, scroll);
        }
        ui.ctx().request_repaint();
    }

    /// Fit the card to the newest image once, so browsing never resizes it.
    fn fit_generator_frame(&mut self, id: NodeId, image: &str, size: egui::Vec2) {
        if self.agents.fitted_image.get(&id).map(String::as_str) == Some(image)
            || size.x < 2.0
            || size.y < 2.0
            || self.tab().read_only
            || self.board_drag.is_some()
        {
            return;
        }
        self.agents.fitted_image.insert(id, image.to_string());
        let Some(width) = self.doc().scene.node(id).map(|n| n.rect.w) else {
            return;
        };
        let height = fit_frame_height(width, size.x as u32, size.y as u32);
        if self
            .doc()
            .scene
            .node(id)
            .is_some_and(|n| (n.rect.h - height).abs() > 2.0)
        {
            self.patch_nodes(&[id], |n| n.rect.h = height);
        }
    }

    pub(crate) fn agent_input_snapshot(
        &self,
        id: NodeId,
    ) -> Result<atlas_ai::agent::InputSnapshot, String> {
        let mut outputs = std::collections::BTreeMap::new();
        for node in &self.doc().scene.nodes {
            if slate_doc::agent_chat::is_agent_node(node) {
                let images = self
                    .agent_images(node.id)
                    .iter()
                    .map(|i| {
                        resolve_source(self.tab().path.as_deref(), &i.source)
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect();
                let text = self
                    .agents
                    .sessions
                    .get(&node.id)
                    .filter(|s| s.status == AgentStatus::Idle)
                    .and_then(|s| s.turns.iter().rev().find(|t| t.role == "assistant"))
                    .map(|t| t.text.clone())
                    .unwrap_or_default();
                let image_outputs = self
                    .agent_images(node.id)
                    .iter()
                    .map(|i| {
                        (
                            i.id.clone(),
                            resolve_source(self.tab().path.as_deref(), &i.source)
                                .to_string_lossy()
                                .into_owned(),
                        )
                    })
                    .collect();
                let active = self.agent_active_output(node.id);
                outputs.insert(
                    node.id,
                    atlas_ai::agent::ContextItem {
                        node: node.id.0,
                        text,
                        images,
                        outputs: image_outputs,
                        active,
                        depth: None,
                        slot: None,
                    },
                );
            }
            // A placed text document supplies the words its card shows. Read
            // once by the card or a queued run (`snippet_for`), never here.
            if let NodeKind::Image(image) = &node.kind {
                let text_doc = self.doc().item(image.item).is_some_and(|item| {
                    slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Text
                });
                if text_doc {
                    let text = self
                        .snippets
                        .get(&image.item)
                        .cloned()
                        .flatten()
                        .unwrap_or_default();
                    outputs.insert(
                        node.id,
                        atlas_ai::agent::ContextItem {
                            node: node.id.0,
                            text,
                            images: vec![],
                            outputs: Default::default(),
                            active: None,
                            depth: None,
                            slot: None,
                        },
                    );
                }
            }
        }
        let mut inputs = slate_doc::agent_inputs::snapshot(
            self.doc(),
            id,
            AgentContextScope::Selection,
            &[],
            &outputs,
        )?;
        self.clip_agent_images(&mut inputs);
        // A prompt being typed steers before the edit commits.
        if let Some((editing, text)) = &self.text_edit {
            for item in &mut inputs.wired {
                if item.node == editing.0 && item.images.is_empty() {
                    item.text = text.clone();
                }
            }
        }
        Ok(inputs)
    }

    /// Replace a wired source path with a file of the visible crop. Generated
    /// outputs and a full crop stay as they are.
    fn clip_agent_images(&self, inputs: &mut atlas_ai::agent::InputSnapshot) {
        for item in inputs.context.iter_mut().chain(inputs.wired.iter_mut()) {
            let Some(node) = self.doc().scene.node(NodeId(item.node)) else {
                continue;
            };
            let NodeKind::Image(img) = &node.kind else {
                continue;
            };
            if img.crop.is_full() {
                continue;
            }
            let Some(source) = self.doc().item(img.item).map(|i| i.path.clone()) else {
                continue;
            };
            let Some(clipped) = super::imagefx::visible_crop_file(&source, img.crop) else {
                continue;
            };
            let clipped = clipped.to_string_lossy().into_owned();
            for slot in item.images.iter_mut().chain(item.depth.as_mut()) {
                if std::path::Path::new(&*slot) == source {
                    *slot = clipped.clone();
                }
            }
        }
    }

    pub(crate) fn export_agent_images(&self) -> std::collections::BTreeMap<NodeId, Vec<PathBuf>> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter_map(|node| {
                let mut images = self.agent_images(node.id).as_ref().clone();
                let active = self.agent_shown_index(node.id)?;
                images.rotate_left(active);
                Some((
                    node.id,
                    images
                        .iter()
                        .map(|i| resolve_source(self.tab().path.as_deref(), &i.source))
                        .collect(),
                ))
            })
            .collect()
    }

    pub(crate) fn agent_is_running(&self, id: NodeId) -> bool {
        matches!(
            self.agents.awaiting.get(&id),
            Some(
                AgentAwait::Sent { .. }
                    | AgentAwait::Thinking { .. }
                    | AgentAwait::Responding { .. }
            )
        ) || self
            .agents
            .sessions
            .get(&id)
            .is_some_and(|s| s.status == AgentStatus::Thinking)
    }

    pub(crate) fn unbundle_selected_agent(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        if self.agent_is_running(id) {
            self.toast("Wait for the current generation or stop it before unbundling.");
            return false;
        }
        let images = self.agent_images(id);
        if matches!(
            self.doc().scene.node(id).map(|n| &n.kind),
            Some(NodeKind::Image(_))
        ) {
            return self.place_agent_results(id, &images);
        }
        let active = self.agents.cover_focus.get(&id).copied().unwrap_or(0);
        match slate_doc::agent_inputs::unbundle(&self.doc().scene, id, &images, active) {
            Ok((commands, ids)) => {
                if self.commit_scene(commands) {
                    self.board_sel = ids.into_iter().collect();
                    self.agents.sessions.remove(&id);

                    self.agents.cover_focus.remove(&id);
                    true
                } else {
                    false
                }
            }
            Err(error) => {
                self.toast(error);
                false
            }
        }
    }

    /// Unbundle on a picture an agent makes: every other result becomes an
    /// ordinary placed picture in a row beside it, one undo step. The card
    /// keeps its agent and its shown result.
    fn place_agent_results(&mut self, id: NodeId, images: &[atlas_ai::agent::ImageOutput]) -> bool {
        let Some(rect) = self.doc().scene.node(id).map(|n| n.rect) else {
            return false;
        };
        let shown = self.agent_shown_index(id);
        let base = self.tab().path.clone();
        let mut nodes = Vec::new();
        for (i, image) in images.iter().enumerate() {
            if Some(i) == shown {
                continue;
            }
            let Some(item) = self.item_for_path(&resolve_source(base.as_deref(), &image.source))
            else {
                continue;
            };
            let at = rect.translated((nodes.len() as f32 + 1.0) * (rect.w + 24.0), 0.0);
            nodes.push(
                self.doc_mut()
                    .scene
                    .build_node(at, NodeKind::Image(slate_doc::scene::ImageNode::new(item))),
            );
        }
        if nodes.is_empty() {
            self.toast("This picture has no other results to place.");
            return false;
        }
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.into_iter().collect();
        true
    }

    pub(crate) fn stop_selected_agent(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some((session, _)) = self.agent_session_for(id) else {
            return false;
        };
        let generator = slate_doc::agent_inputs::is_flow_node(&self.doc().scene, id)
            || self
                .agent_session_for(id)
                .is_some_and(|(_, p)| atlas_ai::agent::local_image_engine(&p));
        if generator {
            self.agents.comfy_queue.remove(&id);
            self.agents.capture_wait.remove(&id);
            if self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.live)
            {
                self.set_generator_live(id, false);
            }
        }
        if let Some(link) = self.agents.codex.get(&session) {
            link.stop();
            return true;
        }
        if generator {
            return true;
        }
        let Some((_, provider)) = self.agent_session_for(id) else {
            return false;
        };
        if provider != "cursor" {
            self.toast("Stop this run in its source program.");
            return false;
        }
        let Some(request) = self.agents.requests.get(&id).cloned() else {
            self.toast("This Cursor response has not started yet.");
            return false;
        };
        let Some(ws) = self.ai.config.workspace_dir.clone() else {
            return false;
        };
        let Some(dir) = self.agent_link_dir(id, &ws) else {
            return false;
        };
        std::thread::spawn(move || {
            let _ = atlas_ai::agent::atomic_write_json(
                &dir.join("cancel.json"),
                &serde_json::json!({"id": request}),
            );
        });
        true
    }

    fn detach_cursor_sidecar(&mut self, session: &str) {
        if let Some(mut child) = self.agents.sidecar_child.remove(session) {
            let _ = child.kill();
            let _ = child.try_wait();
        }
        self.agents.sidecar_spawned.remove(session);
        self.agents.sidecar_booting.remove(session);
    }

    /// Drop the portal's owned provider thread pointer. The provider keeps the
    /// conversation; the next send starts a new one.
    fn forget_owned_thread(&mut self, portal: NodeId, file: &str) {
        if let Some(old) = self.agents.sessions.get(&portal).cloned() {
            let mut state = (*old).clone();
            state.conversation.clear();
            self.agents
                .sessions
                .insert(portal, std::sync::Arc::new(state));
        }
        let Some(ws) = self.ai.config.workspace_dir.clone() else {
            return;
        };
        let Some(dir) = self.agent_link_dir(portal, &ws) else {
            return;
        };
        let file = file.to_string();
        std::thread::spawn(move || {
            let _ = std::fs::remove_file(dir.join(&file));
            let session_path = dir.join("session.json");
            if let Ok(bytes) = std::fs::read(&session_path) {
                if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    value["conversation"] = serde_json::Value::String(String::new());
                    let _ = atlas_ai::agent::atomic_write_json(&session_path, &value);
                }
            }
        });
    }

    /// One rounded hover control over a generator image. Returns its click.
    pub(crate) fn generator_button(
        ui: &egui::Ui,
        painter: &egui::Painter,
        at: Pos2,
        label: &str,
        id: Id,
        fill: Color32,
        z: f32,
    ) -> (egui::Response, Rect) {
        let font = canvas_scale::font(GENERATOR_CHIP_PX, z);
        let text = canvas_text::layout_no_wrap(painter, label.to_string(), font, Color32::WHITE);
        let pad = canvas_scale::px(12.0, z);
        let rect = Rect::from_min_size(
            at,
            egui::vec2(text.size().x + pad * 2.0, canvas_scale::px(28.0, z)),
        );
        painter.rect_filled(rect, canvas_scale::px(8.0, z), fill);
        text.paint_anchored(
            painter,
            rect.center(),
            Align2::CENTER_CENTER,
            Color32::WHITE,
        );
        (ui.interact(rect, id, Sense::click()), rect)
    }

    /// The generator's inputs as the run will read them. A click selects the source.
    fn paint_generator_chips(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        id: NodeId,
        body: Rect,
        view: &GeneratorView,
        z: f32,
    ) -> f32 {
        let font = canvas_scale::font(GENERATOR_CHIP_PX, z);
        let mut y = body.top() + canvas_scale::px(12.0, z);
        if !canvas_text::legible(font.size) {
            return y;
        }
        let x = body.left() + canvas_scale::px(12.0, z);
        let pad = canvas_scale::px(6.0, z);
        let dot = canvas_scale::px(3.5, z);
        let mut picked = None;
        for (i, input) in view.inputs.iter().enumerate().take(6) {
            let (role, color) = input.role.look();
            let text = canvas_text::layout_no_wrap(
                painter,
                format!("{role} · {}", input.label),
                font.clone(),
                Color32::WHITE,
            );
            let size = text.size();
            let rect = Rect::from_min_size(
                Pos2::new(x, y),
                egui::vec2(size.x + pad * 3.0 + dot * 2.0, size.y + pad * 1.4),
            );
            painter.rect_filled(
                rect,
                canvas_scale::px(7.0, z),
                Color32::from_black_alpha(175),
            );
            painter.circle_filled(
                Pos2::new(rect.left() + pad + dot, rect.center().y),
                dot,
                color,
            );
            text.paint_anchored(
                painter,
                Pos2::new(rect.left() + pad * 2.0 + dot * 2.0, rect.center().y),
                Align2::LEFT_CENTER,
                Color32::WHITE,
            );
            if ui
                .interact(rect, Id::new(("generator-input", id.0, i)), Sense::click())
                .clicked()
            {
                picked = Some(input.node);
            }
            y = rect.bottom() + canvas_scale::px(4.0, z);
        }
        if let Some(node) = picked {
            self.board_sel = std::iter::once(node).collect();
        }
        y
    }

    /// The model a portal will use next, as its menu button reads it.
    fn model_menu_label(&self, agent: &slate_doc::scene::AgentPortalRef) -> String {
        let catalog = atlas_ai::agent::model_catalog(&agent.provider);
        let name = agent
            .model
            .as_ref()
            .and_then(|id| {
                self.agents
                    .models
                    .get(catalog)?
                    .iter()
                    .find(|m| &m.id == id)
            })
            .map(|m| m.name.as_str())
            .or(agent.model.as_deref())
            .or_else(|| agent.provider.strip_prefix("ollama/"))
            .unwrap_or(model_default(agent));
        atlas_ai::agent::model_label(name).to_string()
    }

    /// The one model menu: a chat title and a generator's checkpoint chip open it.
    /// Returns the choice; empty means the provider's default.
    fn model_menu_items(
        &self,
        ui: &mut egui::Ui,
        id: NodeId,
        agent: &slate_doc::scene::AgentPortalRef,
        z: f32,
    ) -> Option<String> {
        atlas_shell::menu::prepare(ui, self.palette().dark_mode);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.style_mut().override_font_id =
            Some(FontId::proportional(canvas_text::authored_px(13.0, z)));
        let catalog = atlas_ai::agent::model_catalog(&agent.provider);
        let running = self.agent_is_running(id);
        let mut chosen = None;
        // The catalog is newest-first and longer than the window. An unscrolling
        // menu is shoved upward until its top (Grok 4.7, Opus 5, …) sits above
        // the screen, so only the older tail looks available.
        let max_h = (ui.ctx().screen_rect().height() - 48.0).max(160.0);
        egui::ScrollArea::vertical()
            .max_height(max_h)
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                // A local chat model is always an explicit choice; a text
                // block's Auto picks an installed model that reads pictures.
                if (catalog != "ollama" || agent.view == atlas_ai::agent::PortalView::Text)
                    && ui
                        .add_enabled(
                            !running,
                            egui::SelectableLabel::new(agent.model.is_none(), model_default(agent)),
                        )
                        .clicked()
                {
                    chosen = Some(String::new());
                    ui.close_menu();
                }
                for model in self.agents.models.get(catalog).into_iter().flatten() {
                    if ui
                        .add_enabled(
                            !running,
                            egui::SelectableLabel::new(
                                agent.model.as_ref() == Some(&model.id),
                                atlas_ai::agent::model_label(&model.name),
                            ),
                        )
                        .clicked()
                    {
                        chosen = Some(model.id.clone());
                        ui.close_menu();
                    }
                }
                if let Some(error) = self.agents.models_error.get(catalog) {
                    ui.label(error);
                }
                if self.agents.models_rx.is_some() {
                    ui.label("Loading models…");
                }
            });
        self.agents.note_menu_popup(ui.ctx(), ui.min_rect());
        chosen
    }

    /// A generator or text block switches engine or model. Its runtime link is
    /// dropped so the next run starts on the new engine, and the choice becomes
    /// the default for new agents of its kind.
    pub(crate) fn set_flow_model(&mut self, id: NodeId, image: bool, provider: &str, model: &str) {
        if self.agent_is_running(id) {
            self.toast("Stop this run before changing the model.");
            return;
        }
        if let Some((session, old)) = self.agent_session_for(id) {
            if old != provider {
                self.agents.codex.remove(&session);
            }
        }
        let chosen = (!model.is_empty()).then(|| model.to_string());
        self.patch_nodes(&[id], |n| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                a.provider = provider.to_string();
                a.model = chosen.clone();
            }
        });
        self.remember_model(image, provider, model);
    }

    /// Decode the newest sampling frame of each running generator once.
    fn pump_generation_previews(&mut self, ctx: &egui::Context) {
        let running: Vec<(NodeId, String, String)> = self
            .agents
            .requests
            .iter()
            .filter(|(id, _)| self.agent_is_running(**id))
            .filter_map(|(id, request)| {
                let session = self.agents.bindings.get(id)?;
                Some((*id, request.clone(), session.clone()))
            })
            .collect();
        self.agents
            .previews
            .retain(|id, _| running.iter().any(|(r, _, _)| r == id));
        for (id, request, session) in running {
            let Some(frame) = self
                .agents
                .codex
                .get(&session)
                .and_then(|link| link.preview())
                .filter(|p| p.request == request)
            else {
                continue;
            };
            if self
                .agents
                .previews
                .get(&id)
                .is_some_and(|p| p.request == request && p.frame == frame.frame)
            {
                continue;
            }
            let Ok(decoded) = image::load_from_memory(&frame.image) else {
                continue;
            };
            let rgba = decoded.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let pixels = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            let texture = match self.agents.previews.remove(&id) {
                Some(mut p) => {
                    p.texture.set(pixels, egui::TextureOptions::LINEAR);
                    p.texture
                }
                None => ctx.load_texture(
                    format!("generation-preview-{}", id.0),
                    pixels,
                    egui::TextureOptions::LINEAR,
                ),
            };
            self.agents.previews.insert(
                id,
                LivePreview {
                    request,
                    frame: frame.frame,
                    step: frame.step,
                    steps: frame.steps,
                    texture,
                },
            );
            ctx.request_repaint();
        }
    }

    /// The image coming into focus while it samples, with a thin step bar.
    fn paint_generation_preview(&self, painter: &egui::Painter, id: NodeId, body: Rect, z: f32) {
        let Some(preview) = self.agents.previews.get(&id) else {
            return;
        };
        if self.agents.requests.get(&id) != Some(&preview.request) {
            return;
        }
        let size = preview.texture.size_vec2();
        let scale = (body.width() / size.x).min(body.height() / size.y);
        let rect = Rect::from_center_size(body.center(), size * scale);
        painter.image(
            preview.texture.id(),
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if preview.steps > 0 {
            let t = (preview.step as f32 / preview.steps as f32).clamp(0.0, 1.0);
            let h = canvas_scale::px(3.0, z);
            let bar = Rect::from_min_size(
                egui::pos2(rect.left(), rect.bottom() - h),
                egui::vec2(rect.width() * t, h),
            );
            painter.rect_filled(bar, 0.0, Color32::from_rgb(92, 204, 170));
        }
    }

    /// What an agent adds to the picture it makes: the prompt docked along the
    /// bottom, progress, the run button, and squares under the card for every
    /// result. A click on a square picks that result; the newest shows until
    /// then. Model, count, aspect, seed and Live live on the Agent squircle.
    pub(crate) fn paint_agent_picture(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        body: Rect,
    ) {
        let Some(agent) = slate_doc::agent_chat::agent(node).cloned() else {
            return;
        };
        let id = node.id;
        let z = xf.z;
        let images = self.agent_images(id);
        let shown = self.agent_shown_index(id);
        let view = self.generator_view(id);
        let picked = matches!(&node.kind, NodeKind::Image(i) if !i.item.is_none());
        let hovered = ui
            .ctx()
            .pointer_latest_pos()
            .is_some_and(|p| body.contains(p));
        let selected = self.board_sel.len() == 1 && self.board_sel.contains(&id);
        let base = self.tab().path.clone();
        // A selected picture with several results cover-flows under the wheel.
        // Browsing hides its labels; where it settles becomes the pick.
        let album_id = Id::new(("agent-picture", id.0));
        let mut flowing = false;
        if !selected {
            self.agents.flow.set_hud_hidden(id, false);
        } else if self.image_wheel(id) == ImageWheel::Album {
            let was_browsing = atlas_shell::home::album_browsing(ui.ctx(), album_id) > 0.001;
            let focus = if was_browsing {
                self.agents.cover_focus.get(&id).copied()
            } else {
                shown
            }
            .unwrap_or(0)
            .min(images.len() - 1);
            if was_browsing {
                painter.rect_filled(body, 0.0, self.palette().thumb_bg);
            }
            let covers: Vec<AlbumImage> = images
                .iter()
                .enumerate()
                .map(|(i, image)| {
                    let texture = (i.abs_diff(focus) <= 3)
                        .then(|| {
                            self.linked_image_texture(
                                resolve_source(base.as_deref(), &image.source),
                                &image.id,
                                body.width().max(body.height()),
                            )
                        })
                        .flatten();
                    AlbumImage {
                        texture: texture.as_ref().map(|t| t.id()),
                        size: texture
                            .as_ref()
                            .map(|t| t.size_vec2())
                            .unwrap_or(egui::vec2(1.0, 1.0)),
                    }
                })
                .collect();
            let next = image_album(
                ui,
                album_id,
                body,
                &covers,
                focus,
                atlas_shell::home::AlbumInput {
                    drag: false,
                    wheel: true,
                    host_paints_rest: true,
                },
            );
            if next != focus {
                self.agents.flow.set_hud_hidden(id, true);
            }
            self.agents.cover_focus.insert(id, next);
            flowing = atlas_shell::home::album_browsing(ui.ctx(), album_id) > 0.001;
            if !flowing && next == focus && Some(next) != shown {
                self.pick_agent_result(id, next);
            }
        }
        // Clicking the picture again brings its labels back.
        if !flowing
            && self.agents.flow.hud_hidden(id)
            && hovered
            && ui.input(|i| i.pointer.primary_clicked())
        {
            self.agents.flow.set_hud_hidden(id, false);
        }
        let hud = !flowing && !self.agents.flow.hud_hidden(id);
        let reveal = hud && (hovered || selected || self.flow_capsule_open(id));
        // A new result reshapes the card to its aspect, once.
        if let (Some(i), false) = (shown, picked) {
            let image = &images[i];
            let path = resolve_source(base.as_deref(), &image.source);
            if let Some(tex) =
                self.linked_image_texture(path, "shown", body.width().max(body.height()))
            {
                self.fit_generator_frame(id, &image.id, tex.size_vec2());
            }
        }
        if self.image_wheel(id) == ImageWheel::Model {
            self.steer_generator_model(ui, id, body);
        }
        let running = self.agent_is_running(id);
        let waiting = self.generations_waiting(id);
        if running {
            self.paint_generation_preview(painter, id, body, z);
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let legible = canvas_text::legible(canvas_scale::px(GENERATOR_CHIP_PX, z));
        if agent.live && !reveal {
            painter.circle_filled(
                body.left_bottom()
                    + egui::vec2(canvas_scale::px(14.0, z), canvas_scale::px(-14.0, z)),
                canvas_scale::px(4.5, z),
                Color32::from_rgb(92, 204, 170),
            );
        }
        // Before its first result the picture is a prompt: type, or wire a note.
        if images.is_empty() && legible && !(running || waiting > 0) {
            let wired = view.inputs.iter().any(|i| i.role == InputRole::Prompt);
            let hint = if wired {
                "Add to the wired prompt, or press Generate"
            } else {
                "Wire a note to Prompt, or type here"
            };
            let width = (body.width() - canvas_scale::px(48.0, z)).min(canvas_scale::px(520.0, z));
            let field =
                Rect::from_center_size(body.center(), egui::vec2(width, canvas_scale::px(64.0, z)));
            painter.rect_filled(
                field.expand(canvas_scale::px(10.0, z)),
                canvas_scale::px(10.0, z),
                Color32::from_black_alpha(150),
            );
            self.paint_generator_prompt(ui, id, field, z, hint);
        }
        let previews = agent.provider == "comfy";
        if hud {
            self.paint_run_progress(ui, painter, id, body, z, view.task().working(), previews);
        }
        // Squares under the card: every result, grouped by run. While it
        // flows they follow the flow.
        if images.len() > 1 {
            let focus = if flowing {
                self.agents.cover_focus.get(&id).copied()
            } else {
                shown
            }
            .unwrap_or(0);
            let squares: Vec<AlbumImage> = images
                .iter()
                .enumerate()
                .map(|(i, image)| {
                    let near = i.abs_diff(focus) <= 12;
                    let texture = near
                        .then(|| {
                            self.linked_image_texture(
                                resolve_source(base.as_deref(), &image.source),
                                &image.id,
                                canvas_scale::px(44.0, z),
                            )
                        })
                        .flatten();
                    AlbumImage {
                        texture: texture.as_ref().map(|t| t.id()),
                        size: texture
                            .as_ref()
                            .map(|t| t.size_vec2())
                            .unwrap_or(egui::vec2(1.0, 1.0)),
                    }
                })
                .collect();
            let runs: Vec<&str> = images.iter().map(|i| i.request.as_str()).collect();
            if let Some(pick) = atlas_shell::home::album_index_strip(
                ui,
                Id::new(("agent-picture", id.0)),
                body,
                &squares,
                &runs,
                focus,
                reveal,
                z,
                self.palette(),
            ) {
                self.pick_agent_result(id, pick);
            }
        }
        if !reveal || !legible {
            return;
        }
        let text_top = self.paint_generator_chips(ui, painter, id, body, &view, z);
        let failure = match self.agents.awaiting.get(&id) {
            Some(AgentAwait::Failed { reason, .. }) => Some(reason.clone()),
            _ => view.error.clone(),
        };
        if let Some(reason) = failure {
            let text = canvas_text::layout(
                painter,
                reason,
                canvas_scale::font(GENERATOR_CHIP_PX, z),
                Color32::from_rgb(255, 190, 130),
                body.width() - canvas_scale::px(24.0, z),
            );
            let at = Pos2::new(body.left() + canvas_scale::px(12.0, z), text_top);
            let back = Rect::from_min_size(at, text.size()).expand(canvas_scale::px(5.0, z));
            painter.rect_filled(
                back,
                canvas_scale::px(6.0, z),
                Color32::from_black_alpha(175),
            );
            text.paint(painter, at, Color32::from_rgb(255, 190, 130));
        }
        // The dock: run, then the prompt that made the picture across the rest.
        let action = if agent.live {
            "Keep"
        } else if running {
            "Stop"
        } else {
            view.task().action()
        };
        let at =
            body.left_bottom() + egui::vec2(canvas_scale::px(12.0, z), canvas_scale::px(-40.0, z));
        let (primary, rect) = Self::generator_button(
            ui,
            painter,
            at,
            action,
            Id::new(("agent-generate", id.0)),
            Color32::from_black_alpha(175),
            z,
        );
        let primary = match action {
            "Keep" => primary.on_hover_text("Keep this frame. Live continues."),
            "Stop" => primary.on_hover_text("Stop this run and clear the waiting ones."),
            _ => primary.on_hover_text(view.task().explain()),
        };
        if primary.clicked() {
            self.board_sel = std::iter::once(id).collect();
            let command = match action {
                "Stop" => "portal.agent.stop",
                "Keep" => "portal.agent.keep",
                _ => "portal.agent.send",
            };
            self.dispatch(ui.ctx(), atlas_commands::CommandId(command), None);
        }
        if !images.is_empty() {
            let capsule = Rect::from_min_max(
                Pos2::new(rect.right() + canvas_scale::px(6.0, z), rect.top()),
                Pos2::new(body.right() - canvas_scale::px(12.0, z), rect.bottom()),
            );
            self.paint_prompt_capsule(ui, painter, id, capsule, body, z);
        }
    }

    /// Picking a result makes it the picture's file (journaled, undoable).
    pub(crate) fn pick_agent_result(&mut self, id: NodeId, index: usize) {
        let Some(image) = self.agent_images(id).get(index).cloned() else {
            return;
        };
        if self.tab().read_only {
            return;
        }
        let path = resolve_source(self.tab().path.as_deref(), &image.source);
        let Some(item) = self.item_for_path(&path) else {
            self.toast("That result is no longer on disk.");
            return;
        };
        self.patch_nodes(&[id], |n| {
            if let NodeKind::Image(i) = &mut n.kind {
                i.item = item;
            }
        });
    }

    /// The file an agent picture shows: the picked result, else the newest.
    pub(crate) fn agent_shown_path(&self, id: NodeId) -> Option<PathBuf> {
        let images = self.agent_images(id);
        let image = images.get(self.agent_shown_index(id)?)?;
        Some(resolve_source(self.tab().path.as_deref(), &image.source))
    }

    pub(crate) fn agent_pump(&mut self, ctx: &egui::Context) {
        self.sync_agent_doc();
        let mut existing: HashSet<String> = self
            .tabs
            .iter()
            .flat_map(|tab| tab.doc.scene.nodes.iter())
            .filter_map(|n| {
                slate_doc::agent_chat::agent(n)
                    .filter(|a| !a.chat.train)
                    .map(|a| a.session.clone())
            })
            .collect();
        existing.extend(
            self.agents
                .awaiting
                .iter()
                .filter(|(_, s)| !matches!(s, AgentAwait::Failed { .. }))
                .filter_map(|(id, _)| self.agent_session_for(*id).map(|(s, _)| s)),
        );
        existing.extend(self.parked_agent_sessions());
        self.agents
            .codex
            .retain(|session, _| existing.contains(session));
        self.ensure_agent_programs();
        ctx.request_repaint_after(Duration::from_millis(250));
        self.ensure_agent_recents(ctx);
        self.pump_cursor_ide(ctx);
        self.pump_agent_chats(ctx);
        self.paint_schedule_dialog(ctx);
        self.pump_agent_connection(ctx);
        self.pump_agent_connecting();
        self.rejoin_agent_chats();
        self.refresh_agent_connection();
        self.pump_agent_models();
        if let Some(id) = self.agents.focused {
            if !self.board_sel.contains(&id) && self.portal_chrome.maximized != Some(id) {
                self.agent_blur();
            }
        }
        let ws = self.ai.config.workspace_dir.clone().unwrap_or_default();

        let portals: Vec<(NodeId, Option<slate_doc::scene::AgentPortalRef>)> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| slate_doc::agent_chat::is_agent_node(n))
            .map(|n| (n.id, slate_doc::agent_chat::agent(n).cloned()))
            .collect();
        let live: std::collections::HashSet<NodeId> = portals.iter().map(|(id, _)| *id).collect();

        self.agents.sessions.retain(|id, _| live.contains(id));
        self.agents.prompts.retain(|id, _| live.contains(id));
        self.agents.awaiting.retain(|id, _| live.contains(id));
        self.agents.local_turns.retain(|id, _| live.contains(id));
        self.agents
            .summary_cache
            .borrow_mut()
            .retain(|id, _| live.contains(id));
        if self.agents.focused.is_some_and(|id| !live.contains(&id)) {
            self.agent_blur();
        }

        let publish = self
            .agents
            .context_tick
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(1));
        if publish {
            self.agents.context_tick = Some(Instant::now());
        }
        let mut published = HashSet::new();
        let mut live_dirs = HashSet::new();
        for (id, agent) in portals {
            let Some(agent) = agent else {
                continue;
            };
            if agent.provider.is_empty() {
                continue;
            }
            if self.agents.bindings.get(&id) != Some(&agent.session) {
                self.agents.bindings.insert(id, agent.session.clone());
                self.agents.image_cache.borrow_mut().remove(&id);

                self.agents.sessions.remove(&id);
                self.agents.local_turns.remove(&id);
                self.agents.awaiting.remove(&id);
                self.agents.cover_focus.remove(&id);
            }
            let dir = self
                .agent_link_dir(id, &ws)
                .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &agent.session));
            live_dirs.insert(dir.clone());
            if !self.agent_has_child(id) {
                self.consume_atlas_place(id, &dir);
                self.agent_output_roots(id, &dir);
            }
            let context = if publish && !ws.as_os_str().is_empty() && published.insert(dir.clone())
            {
                let mut context =
                    self.agent_context_for(&agent.session, &agent.provider, agent.context);
                if let Ok(inputs) = self.agent_input_snapshot(id) {
                    context.selection = inputs
                        .context
                        .iter()
                        .map(|item| format!("node:{}", item.node))
                        .collect();
                    context.board_summary = serde_json::to_string(&inputs).unwrap_or_default();
                }
                Some(context)
            } else {
                None
            };
            if let Some(session) = self.agents.sources.poll(&dir, context.as_ref()) {
                if self
                    .agents
                    .sessions
                    .get(&id)
                    .is_some_and(|previous| std::sync::Arc::ptr_eq(previous, &session))
                {
                    continue;
                }
                if let Some(local) = self.agents.local_turns.get(&id).cloned() {
                    let systems: Vec<AgentTurn> = local
                        .iter()
                        .filter(|t| t.role == "system")
                        .cloned()
                        .collect();
                    let authored = local.iter().filter(|t| t.role != "system").count();
                    let echoed =
                        local
                            .iter()
                            .rev()
                            .find(|t| t.role != "system")
                            .is_none_or(|tail| {
                                session
                                    .turns
                                    .iter()
                                    .any(|s| s.role == tail.role && s.text == tail.text)
                            });
                    if !echoed {
                        let mut merged = local.clone();
                        for turn in &session.turns {
                            if turn.role == "user" {
                                continue;
                            }
                            if !merged
                                .iter()
                                .any(|t| t.role == turn.role && t.text == turn.text)
                            {
                                merged.push(turn.clone());
                            }
                        }
                        if merged.len() != local.len() {
                            self.agents.local_turns.insert(id, merged);
                        }
                    } else if session.turns.len() >= authored {
                        if systems.is_empty() {
                            self.agents.local_turns.remove(&id);
                        } else {
                            let mut merged = session.turns.clone();
                            for turn in systems {
                                if !merged
                                    .iter()
                                    .any(|t| t.role == "system" && t.text == turn.text)
                                {
                                    merged.push(turn);
                                }
                            }
                            self.agents.local_turns.insert(id, merged);
                        }
                    }
                }
                self.agents.output_epoch = self.agents.output_epoch.wrapping_add(1);
                let previous = self
                    .agents
                    .sessions
                    .get(&id)
                    .map(|s| s.bundle.images.len())
                    .unwrap_or(0);
                let arrived = session.bundle.images.len() > previous;
                self.agents.sessions.insert(id, session);
                if arrived {
                    let count = self.agent_images(id).len();
                    self.agents.cover_focus.insert(id, count.saturating_sub(1));
                }
                ctx.request_repaint();
            }
        }
        self.agents.sources.retain(&live_dirs);
        self.pump_agent_awaits(ctx, &ws);
        self.pump_comfy_queue();
        self.pump_live_generators();
        if !self.agents.live_settle.is_empty() {
            ctx.request_repaint_after(LIVE_TYPING_SETTLE);
        }
        self.pump_generation_previews(ctx);

        let proposals = if ws.as_os_str().is_empty() {
            Vec::new()
        } else {
            self.agents.stage.tick_read(&ws)
        };
        if !proposals.is_empty() {
            for proposal in proposals {
                if let Some(existing) = self.agents.pending.iter_mut().find(|p| p.id == proposal.id)
                {
                    *existing = proposal;
                } else {
                    self.agents.pending.push(proposal);
                }
            }
            ctx.request_repaint();
        }
    }

    fn agent_context_for(
        &self,
        session: &str,
        provider: &str,
        _scope: AgentContextScope,
    ) -> AgentContext {
        let tab = self.tab();
        AgentContext {
            app: "slate",
            session: session.to_string(),
            provider: provider.to_string(),
            workbook: tab.path.clone(),
            format_version: tab.doc.format_version,
            scope: "wired".into(),
            selection: vec![],
            viewport: None,
            board_summary: String::new(),
            generated_at: atlas_ai::context::now_secs(),
        }
    }

    fn consume_agent_context(&mut self, portal: NodeId) {
        // A generator or text block rereads its wires on every run; they stay rewirable.
        if slate_doc::agent_inputs::is_flow_node(&self.doc().scene, portal) {
            return;
        }
        let ids: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| {
                let NodeKind::Connector(c) = &n.kind else {
                    return None;
                };
                let binding = c.binding.as_ref()?;
                if binding.consumed {
                    return None;
                }
                let target = if binding.input_b { &c.b } else { &c.a };
                slate_doc::agent_inputs::endpoint_node(target)
                    .filter(|id| *id == portal)
                    .map(|_| n.id)
            })
            .collect();
        if ids.is_empty() {
            return;
        }
        self.patch_nodes(&ids, |n| {
            if let NodeKind::Connector(c) = &mut n.kind {
                if let Some(binding) = &mut c.binding {
                    binding.consumed = true;
                }
            }
        });
    }

    pub(crate) fn send_agent_prompt(&mut self, portal: NodeId) {
        if (self.agents.connection_pending == Some(portal) && !self.agents.connection_background)
            || self.agents.project_picker == Some(portal)
            || self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|p| p.portal == portal)
        {
            self.toast("Choose a project and conversation first.");
            return;
        }
        if self.agents.connection_background && self.agents.connection_pending == Some(portal) {
            if !self.queue_agent_send(portal) {
                self.toast("Conversation is refreshing; send again in a moment.");
            }
            return;
        }
        let Some((_session, provider)) = self.agent_session_for(portal) else {
            self.toast("Select an agent portal first.");
            return;
        };
        if self.is_text_block(portal) {
            self.queue_text_block(portal);
            return;
        }
        if !atlas_ai::agent::local_image_engine(&provider)
            && matches!(
                self.agents.awaiting.get(&portal),
                Some(
                    AgentAwait::Sent { .. }
                        | AgentAwait::Thinking { .. }
                        | AgentAwait::Responding { .. }
                )
            )
        {
            self.toast("Wait for this response before sending another prompt.");
            return;
        }
        if provider.is_empty() {
            self.toast("Choose a program first.");
            return;
        }
        if atlas_ai::agent::local_image_engine(&provider) {
            self.queue_generation(portal);
            return;
        }
        if self.agent_linear(portal) {
            let selected = self
                .doc()
                .scene
                .node(portal)
                .and_then(slate_doc::agent_chat::agent)
                .and_then(|a| a.channel.as_deref());
            if selected.is_some_and(|id| {
                self.agents
                    .session(portal)
                    .is_none_or(|s| s.conversation != id)
            }) && self.ai.config.valid_workspace().is_some()
            {
                if !self.queue_agent_send(portal) {
                    self.toast(life::NOT_CONNECTED);
                }
                return;
            }
        }
        if provider == "ollama"
            && self
                .doc()
                .scene
                .node(portal)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.model.is_none())
        {
            self.toast("Choose an installed local model in the card title before sending.");
            return;
        }
        let inputs = match self.agent_input_snapshot(portal) {
            Ok(v) => v,
            Err(e) => {
                self.fail_agent_await(portal, e);
                return;
            }
        };
        let mut prompt = self.agents.prompt_mut(portal).trim().to_string();
        if prompt.is_empty() {
            prompt = inputs
                .wired
                .iter()
                .map(|v| v.text.as_str())
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
        }

        if prompt.is_empty() {
            prompt = self
                .agent_images(portal)
                .first()
                .map(|i| i.prompt.clone())
                .unwrap_or_default();
        }
        if prompt.is_empty() {
            self.toast("Type a prompt or connect a text node first.");
            return;
        }
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.fail_agent_await(
                portal,
                "Set an AI workspace before sending — the agent link has nowhere to write.".into(),
            );
            self.toast("Set an AI workspace before sending an agent prompt.");
            #[cfg(not(test))]
            self.ai.pick_workspace();
            return;
        };
        let composer = portal;
        let typing_here = self.agents.composing(composer);
        let Some((portal, history)) = self.prepare_agent_train_send(portal, &ws) else {
            return;
        };
        self.consume_agent_context(composer);
        // The next message is typed on the new tail. A send that waited to
        // connect leaves the caret wherever the person has moved since.
        if typing_here && portal != composer {
            self.agents.composer_editing = None;
            self.agents.composer_focus = Some(portal);
            self.agent_focus(portal);
        }
        let Some((session, provider)) = self.agent_session_for(portal) else {
            return;
        };
        let req = AgentRequest {
            model: self
                .doc()
                .scene
                .node(portal)
                .and_then(slate_doc::agent_chat::agent)
                .and_then(|a| a.model.clone()),
            history,
            inputs,
            id: atlas_ai::agent::request_id(),
            prompt,
            at: atlas_ai::context::now_secs(),
            image: None,
            output_dir: self.agent_output_dir(portal, &ws, &session),
            oneshot: false,
        };
        let dir = self
            .agent_link_dir(portal, &ws)
            .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &session));
        let manifest = slate_doc::SourceUri {
            locator: super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &dir.join("session.json"),
            ),
        };
        self.patch_nodes(&[portal], |n| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                if a.bundle.is_none() {
                    a.bundle = Some(manifest.clone());
                }
            }
        });
        self.agents.requests.insert(portal, req.id.clone());
        self.agents.bindings.insert(portal, session.clone());
        match self.agents.sources.send(&dir, &req) {
            Ok(()) => {
                let turn = AgentTurn {
                    role: "user".into(),
                    text: req.prompt.clone(),
                    at: req.at,
                };
                let turns = self.agents.local_turns.entry(portal).or_insert_with(|| {
                    self.agents
                        .sessions
                        .get(&portal)
                        .map(|s| s.turns.clone())
                        .unwrap_or_else(|| req.history.clone())
                });
                if turns
                    .last()
                    .is_none_or(|t| t.role != "user" || t.text != turn.text)
                {
                    turns.push(turn);
                }
                let pending_turns = turns.clone();
                let siblings: Vec<_> = self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .filter_map(|n| {
                        slate_doc::agent_chat::agent(n)
                            .filter(|a| a.session == session && n.id != portal)
                            .map(|_| n.id)
                    })
                    .collect();
                for id in siblings {
                    self.agents.bindings.insert(id, session.clone());
                    self.agents.local_turns.insert(id, pending_turns.clone());
                }
                self.agents.prompt_mut(portal).clear();
                self.agents.prompt_mut(composer).clear();
                self.agents.awaiting.insert(
                    portal,
                    AgentAwait::Sent {
                        at: Instant::now(),
                        req_at: req.at,
                    },
                );
                if provider == "codex" || provider == "ollama" || provider.starts_with("ollama/") {
                    let cwd = self.agent_folder_for(portal).unwrap_or_else(|| ws.clone());
                    let runtime = self.agents.codex.entry(session.clone()).or_insert_with(|| {
                        atlas_ai::runtime::CodexLink::start_provider(
                            dir.clone(),
                            cwd,
                            provider.clone(),
                        )
                    });
                    if let Err(error) = runtime.send(req.clone()) {
                        self.fail_agent_await(portal, error);
                    }
                } else {
                    self.ensure_agent_sidecar(portal, &session, &provider, &ws);
                }
            }
            Err(e) => {
                self.fail_agent_await(portal, format!("Could not write agent request: {e}"));
                self.toast(format!("Could not write agent request: {e}"));
            }
        }
    }

    fn ensure_agent_sidecar(
        &mut self,
        portal: NodeId,
        session: &str,
        provider: &str,
        ws: &std::path::Path,
    ) {
        if provider != "cursor" {
            return;
        } // configured external sidecars consume request.json
        if self.agents.sidecar_spawned.contains(session)
            || self.agents.sidecar_booting.contains(session)
        {
            return;
        }
        let dir = self
            .agent_link_dir(portal, ws)
            .unwrap_or_else(|| atlas_ai::agent::agent_dir(ws, session));
        #[cfg(test)]
        self.record_sidecar_dir(session, dir);
        #[cfg(not(test))]
        {
            self.record_sidecar_dir(session, dir.clone());
            let cwd = self
                .agent_folder_for(portal)
                .unwrap_or_else(|| ws.to_path_buf());
            let ws = ws.to_path_buf();
            let session = session.to_string();
            let doc = self.tab().id;
            self.agents.sidecar_booting.insert(session.clone());
            let tx = self.sidecar_boot_tx();
            std::thread::spawn(move || {
                // A sidecar left by an earlier run must not write the same link folder.
                let _ = atlas_ai::sidecar::stop(&dir);
                let result = atlas_ai::sidecar::spawn_cursor_sidecar_in(&ws, &session, &cwd, &dir);
                let _ = tx.send(SidecarBoot {
                    doc,
                    portal,
                    session,
                    result,
                });
            });
        }
    }

    #[cfg(not(test))]
    fn sidecar_boot_tx(&mut self) -> Sender<SidecarBoot> {
        if let Some(tx) = &self.agents.sidecar_boot_tx {
            return tx.clone();
        }
        let (tx, rx) = unbounded();
        self.agents.sidecar_boot_rx = Some(rx);
        self.agents.sidecar_boot_tx = Some(tx.clone());
        tx
    }

    fn pump_sidecar_boot(&mut self, ctx: &egui::Context) {
        loop {
            let msg = {
                let Some(rx) = &self.agents.sidecar_boot_rx else {
                    return;
                };
                match rx.try_recv() {
                    Ok(msg) => msg,
                    Err(crossbeam_channel::TryRecvError::Empty) => return,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        self.agents.sidecar_boot_rx = None;
                        self.agents.sidecar_boot_tx = None;
                        return;
                    }
                }
            };
            self.agents.sidecar_booting.remove(&msg.session);
            match msg.result {
                // Its document closed while it booted.
                Ok(mut child) if !self.agent_session_open(&msg.session) => {
                    let _ = child.kill();
                    let _ = child.try_wait();
                    self.forget_sidecar_dir(&msg.session);
                }
                Ok(child) => {
                    self.agents.sidecar_spawned.insert(msg.session.clone());
                    self.agents.sidecar_child.insert(msg.session, child);
                }
                Err(e) => self.fail_agent_await_in(msg.doc, msg.portal, e),
            }
            ctx.request_repaint();
        }
    }

    pub(crate) fn fail_agent_await(&mut self, portal: NodeId, reason: String) {
        let (reason, actions) = classify_agent_failure(reason);
        self.agents.awaiting.insert(
            portal,
            AgentAwait::Failed {
                reason: reason.clone(),
                actions,
            },
        );
        let turns = self.agents.local_turns.entry(portal).or_insert_with(|| {
            self.agents
                .sessions
                .get(&portal)
                .map(|s| s.turns.clone())
                .unwrap_or_default()
        });
        if turns
            .last()
            .is_none_or(|t| t.role != "system" || t.text != reason)
        {
            turns.push(AgentTurn {
                role: "system".into(),
                text: reason,
                at: atlas_ai::context::now_secs(),
            });
        }
    }

    fn apply_agent_recover(&mut self, portal: NodeId, action: &AgentRecover) {
        match action {
            AgentRecover::OpenUrl { url, .. } => self.open_url(url),
            AgentRecover::OpenPath { path, .. } => Self::open_path(path),
            AgentRecover::PasteKey => {
                self.agents.key_entry = Some(portal);
                self.agents.key_focus = Some(portal);
                self.agents.key_draft.clear();
            }
            AgentRecover::PickWorkspace => {
                #[cfg(not(test))]
                self.ai.pick_workspace();
            }
        }
    }

    pub(crate) fn open_cursor_api_key_page(&mut self) -> bool {
        self.open_url(atlas_ai::cursor_key::DASHBOARD_URL);
        true
    }

    pub(crate) fn save_cursor_api_key(&mut self, portal: Option<NodeId>) -> bool {
        let key = self.agents.key_draft.trim().to_string();
        match atlas_ai::cursor_key::save(&key) {
            Ok(()) => {
                self.agents.key_draft.clear();
                self.agents.key_entry = None;
                self.agents.key_focus = None;
                self.agents.key_rect = None;
                if let Some(id) = portal.or(self.selected_agent_portal()) {
                    if let Some((session, _)) = self.agent_session_for(id) {
                        self.agents.sidecar_spawned.remove(&session);
                        self.agents.sidecar_child.remove(&session);
                        self.agents.sidecar_booting.remove(&session);
                    }
                    self.mark_agent_operative(id);
                    let retry = self
                        .visible_agent_turns(id)
                        .iter()
                        .any(|t| t.role == "user");
                    if retry {
                        self.retry_last_agent_prompt(id);
                        self.toast("API key saved. Sending the last message.");
                    } else {
                        self.toast("API key saved. This agent is ready.");
                    }
                } else {
                    self.toast("API key saved on this machine.");
                }
                true
            }
            Err(e) => {
                self.toast(e);
                false
            }
        }
    }

    /// A saved key ends the failure. The card is idle and the composer is ready.
    fn mark_agent_operative(&mut self, id: NodeId) {
        self.agents.awaiting.remove(&id);
        if let Some(turns) = self.agents.local_turns.get_mut(&id) {
            turns.retain(|t| t.role != "system");
        }
        if self
            .agents
            .local_turns
            .get(&id)
            .is_some_and(|t| t.is_empty())
        {
            self.agents.local_turns.remove(&id);
        }
        if let Some(existing) = self.agents.sessions.get(&id).cloned() {
            if matches!(existing.status, AgentStatus::Error(_)) {
                let mut idle = (*existing).clone();
                idle.status = AgentStatus::Idle;
                if let Some(ws) = self.ai.config.workspace_dir.clone() {
                    if let Some(dir) = self.agent_link_dir(id, &ws) {
                        let _ =
                            atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &idle);
                    }
                }
                self.agents.sessions.insert(id, std::sync::Arc::new(idle));
            }
        }
        self.agents.composer_focus = Some(id);
        self.agent_focus(id);
    }

    fn retry_last_agent_prompt(&mut self, portal: NodeId) {
        let text = self
            .visible_agent_turns(portal)
            .iter()
            .rev()
            .find(|t| t.role == "user")
            .map(|t| t.text.clone());
        if let Some(text) = text {
            *self.agents.prompt_mut(portal) = text;
            self.send_agent_prompt(portal);
        }
    }

    fn pump_agent_awaits(&mut self, ctx: &egui::Context, ws: &std::path::Path) {
        self.pump_sidecar_boot(ctx);
        let ids: Vec<NodeId> = self.agents.awaiting.keys().copied().collect();
        let mut animating = false;
        for id in ids {
            let session_id = self
                .agent_session_for(id)
                .map(|(s, _)| s)
                .unwrap_or_default();
            if !session_id.is_empty() {
                if let Some(child) = self.agents.sidecar_child.get_mut(&session_id) {
                    if let Ok(Some(status)) = child.try_wait() {
                        self.agents.sidecar_child.remove(&session_id);
                        self.agents.sidecar_spawned.remove(&session_id);
                        if !status.success() {
                            if matches!(
                                self.agents.awaiting.get(&id),
                                Some(AgentAwait::Failed { .. })
                            ) {
                                continue;
                            }
                            let from_session =
                                self.agents.sessions.get(&id).and_then(|s| match &s.status {
                                    AgentStatus::Error(e) => Some(e.clone()),
                                    _ => None,
                                });
                            let reason = match from_session {
                                Some(e) => e,
                                None => {
                                    let tail =
                                        atlas_ai::sidecar::sidecar_log_tail(ws, &session_id, 4);
                                    match tail {
                                        Some(t) => format!("Agent sidecar exited: {t}"),
                                        None => {
                                            "Agent sidecar exited before it answered. Check Node, @cursor/sdk, and CURSOR_API_KEY."
                                                .into()
                                        }
                                    }
                                }
                            };
                            self.fail_agent_await(id, reason);
                            continue;
                        }
                    }
                }
            }

            let matching = self.agents.sessions.get(&id).filter(|s| {
                s.request.is_empty() || self.agents.requests.get(&id) == Some(&s.request)
            });
            let completed = matching
                .is_some_and(|s| !s.request.is_empty() && matches!(s.status, AgentStatus::Idle));
            let sidecar = matching.map(|s| s.status.clone());
            let Some(state) = self.agents.awaiting.get(&id).cloned() else {
                continue;
            };
            let req_at = match &state {
                AgentAwait::Sent { req_at, .. }
                | AgentAwait::Thinking { req_at }
                | AgentAwait::Responding { req_at } => Some(*req_at),
                AgentAwait::Failed { .. } => None,
            };
            let has_new = req_at.is_some_and(|at| self.portal_has_new_reply(id, at));
            let child_alive = !session_id.is_empty()
                && (self.agents.sidecar_child.contains_key(&session_id)
                    || self.agents.codex.contains_key(&session_id));
            let booting =
                !session_id.is_empty() && self.agents.sidecar_booting.contains(&session_id);
            if booting {
                animating = true;
            }
            match (&state, sidecar.as_ref()) {
                (AgentAwait::Failed { .. }, _) => {}
                (_, Some(AgentStatus::Error(e))) => {
                    self.fail_agent_await(id, e.clone());
                }
                (
                    AgentAwait::Sent { req_at, .. } | AgentAwait::Thinking { req_at },
                    Some(AgentStatus::Thinking),
                ) if has_new => {
                    self.agents
                        .awaiting
                        .insert(id, AgentAwait::Responding { req_at: *req_at });
                    animating = true;
                }
                (AgentAwait::Sent { req_at, .. }, Some(AgentStatus::Thinking)) => {
                    self.agents
                        .awaiting
                        .insert(id, AgentAwait::Thinking { req_at: *req_at });
                    animating = true;
                }
                (_, Some(AgentStatus::Idle)) if has_new || completed => {
                    self.agents.awaiting.remove(&id);
                }
                (AgentAwait::Sent { at, .. }, _)
                    if !child_alive && !booting && at.elapsed() >= AWAIT_TIMEOUT =>
                {
                    let tail = if !session_id.is_empty() {
                        atlas_ai::sidecar::sidecar_log_tail(ws, &session_id, 4)
                    } else {
                        None
                    };
                    let reason = match tail {
                        Some(t) => format!("Agent did not respond: {t}"),
                        None => {
                            "No response from this program. Open its link folder and start or reconnect the sidecar."
                                .into()
                        }
                    };
                    self.fail_agent_await(id, reason);
                }
                (
                    AgentAwait::Sent { .. }
                    | AgentAwait::Thinking { .. }
                    | AgentAwait::Responding { .. },
                    _,
                ) => {
                    animating = true;
                }
            }
        }
        if animating {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    pub(crate) fn send_selected_agent_prompt(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        self.send_agent_prompt(id);
        true
    }

    pub(crate) fn toggle_selected_agent_provider(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        if self.agent_is_running(id) {
            self.toast("Stop the current response before changing programs.");
            return false;
        }
        self.patch_nodes(&[id], |node| {
            if let NodeKind::Portal(p) = &mut node.kind {
                if let Some(a) = &mut p.agent {
                    a.provider.clear();
                }
            }
        });
        self.agents.programs_started = false;
        self.agent_focus(id);
        true
    }

    pub(crate) fn launch_agent_provider(&mut self, portal: NodeId) {
        let Some((_, provider)) = self.agent_session_for(portal) else {
            self.toast("Select an agent portal first.");
            return;
        };
        if provider == "cursor" {
            if let Some(folder) = self.agent_folder_for(portal) {
                if !atlas_ai::launch::cursor_available() {
                    self.toast("Cursor was not found — install it or add `cursor` to PATH.");
                    return;
                }
                match atlas_ai::launch::launch_cursor(&folder) {
                    Ok(()) => self.toast("Opening folder in Cursor."),
                    Err(e) => self.toast(e),
                }
                return;
            }
        }
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.toast("Set an AI workspace before launching the agent provider.");
            self.ai.pick_workspace();
            return;
        };
        if let Err(e) = launch_provider(&provider, &ws) {
            self.toast(e);
        }
    }

    pub(crate) fn launch_selected_agent_provider(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        self.launch_agent_provider(id);
        true
    }

    /// `place.json` beside `session.json` asks for a File Atlas portal.
    /// The folder path is relative to the AI workspace unless it is absolute.
    pub(crate) fn consume_atlas_place(
        &mut self,
        portal: NodeId,
        link_dir: &std::path::Path,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let request_path = link_dir.join("place.json");
        if !request_path.is_file() || atlas_core::cloud::is_dehydrated(&request_path) {
            return false;
        }
        let raw = match std::fs::read_to_string(&request_path) {
            Ok(text) => text,
            Err(_) => return false,
        };
        let value: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(_) => return false,
        };
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let path_raw = value
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if id.is_empty() || kind != "file_atlas" || path_raw.is_empty() {
            let _ = std::fs::write(
                link_dir.join("place.result.json"),
                r#"{"ok":false,"error":"place.json needs id, kind file_atlas, and path"}"#,
            );
            let _ = std::fs::remove_file(&request_path);
            return false;
        }
        let key = format!("{}:{id}", portal.0);
        if self.agents.placed_atlas.contains(&key) {
            let _ = std::fs::remove_file(&request_path);
            return false;
        }
        let folder = {
            let path = std::path::PathBuf::from(path_raw);
            if path.is_absolute() {
                path
            } else {
                self.ai
                    .config
                    .workspace_dir
                    .clone()
                    .unwrap_or_else(|| link_dir.to_path_buf())
                    .join(path)
            }
        };
        if atlas_core::cloud::is_dehydrated(&folder) || !folder.is_dir() {
            let _ = std::fs::write(
                link_dir.join("place.result.json"),
                format!(r#"{{"id":{id:?},"ok":false,"error":"folder not found"}}"#),
            );
            let _ = std::fs::remove_file(&request_path);
            self.toast("File Atlas folder was not found.");
            return false;
        }
        let Some(host) = self.doc().scene.node(portal).map(|n| n.rect) else {
            return false;
        };
        let rect = self.clear_spawn_rect(slate_doc::WorldRect::new(
            host.x + host.w + 72.0,
            host.y,
            super::board_atlas::ATLAS_DEFAULT_W,
            super::board_atlas::ATLAS_DEFAULT_H,
        ));
        let node = self.build_bound_atlas(rect, &folder, Vec::new());
        let placed = node.id;
        let author = self
            .doc()
            .scene
            .node(portal)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.provider.clone())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "agent".into());
        let cmd = slate_doc::SceneCmd::Add {
            index: self.doc().scene.nodes.len(),
            node,
        };
        if !self.commit_scene_as(vec![cmd], slate_doc::scene::CmdAuthor::Agent(author)) {
            return false;
        }
        self.agents.placed_atlas.insert(key);
        let _ = std::fs::write(
            link_dir.join("place.result.json"),
            format!(r#"{{"id":{id:?},"ok":true,"portal":{}}}"#, placed.0),
        );
        let _ = std::fs::remove_file(&request_path);
        self.toast("Placed a File Atlas portal for that folder.");
        true
    }

    pub(crate) fn reveal_agent_link(&mut self, portal: NodeId) {
        if let Some(dir) = self.agent_link_dir(
            portal,
            self.ai
                .config
                .workspace_dir
                .as_deref()
                .unwrap_or(std::path::Path::new("")),
        ) {
            atlas_ai::launch::reveal_dir(&dir);
        }
    }

    pub(crate) fn reveal_selected_agent_link(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        self.reveal_agent_link(id);
        true
    }

    pub(crate) fn accept_stage_proposal(&mut self, proposal_id: &str) {
        if self.refuse_read_only_edit() {
            return;
        }
        let Some(proposal) = self
            .agents
            .pending
            .iter()
            .find(|p| p.id == proposal_id)
            .cloned()
        else {
            return;
        };
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.toast("Could not accept the proposal: the AI workspace is unavailable.");
            return;
        };
        let outcome = {
            let tab = self.tab_mut();
            stage::accept_and_record(
                &proposal,
                tab.path.as_deref(),
                &mut tab.doc.scene,
                &mut tab.journal,
                &ws,
            )
        };
        match outcome {
            Ok(result) => {
                self.agents.remove_pending(&proposal.id);
                if result == ProposalResult::Accepted {
                    self.tab_mut().dirty = true;
                    self.note_scene_change();
                }
                match result {
                    ProposalResult::Accepted => self.toast("Accepted agent proposal."),
                    ProposalResult::Stale { reason } => {
                        self.toast(format!("Proposal not applied: {reason}."))
                    }
                    ProposalResult::Applying | ProposalResult::Rejected => unreachable!(),
                }
            }
            Err(failure) => {
                if failure.applied {
                    self.tab_mut().dirty = true;
                    self.note_scene_change();
                }
                match stage::read_result(&ws, &proposal.id) {
                    Ok(Some(ProposalResult::Applying)) | Err(_) => {
                        if let Some(p) =
                            self.agents.pending.iter_mut().find(|p| p.id == proposal.id)
                        {
                            p.status = slate_doc::ProposalStatus::RecoveryRequired;
                        }
                    }
                    Ok(Some(_)) => self.agents.remove_pending(&proposal.id),
                    Ok(None) => {}
                }
                let state = if failure.applied {
                    "Proposal applied, but its confirmation could not be saved. Review the board before dismissing it"
                } else {
                    "Proposal not applied"
                };
                self.toast(format!("{state}: {}.", failure.error));
            }
        }
    }

    pub(crate) fn reject_stage_proposal(&mut self, proposal_id: &str) {
        let Some(proposal) = self
            .agents
            .pending
            .iter()
            .find(|p| p.id == proposal_id)
            .cloned()
        else {
            return;
        };
        let Some(ws) = self.ai.config.valid_workspace() else {
            self.toast("Could not save the rejection: the AI workspace is unavailable.");
            return;
        };
        match stage::read_result(ws, &proposal.id) {
            Ok(Some(
                ProposalResult::Accepted | ProposalResult::Rejected | ProposalResult::Stale { .. },
            )) => {
                self.agents.remove_pending(&proposal.id);
                self.toast("This proposal already has a recorded decision. No board changes made.");
                return;
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {} // Human explicitly dismisses an unreadable decision.
            Err(e) => {
                self.toast(format!(
                    "Could not read the proposal decision: {e}. Rejection was not saved."
                ));
                return;
            }
        }
        if let Err(e) = stage::write_result(ws, &proposal.id, &reject(&proposal)) {
            self.toast(format!(
                "Could not save the rejection: {e}. The proposal remains available."
            ));
            return;
        }
        self.agents.remove_pending(&proposal.id);
        if proposal.status == slate_doc::ProposalStatus::RecoveryRequired {
            self.toast("Dismissed the recovery proposal. Previously applied board changes are unchanged; use Undo if needed.");
        } else {
            self.toast("Rejected agent proposal.");
        }
    }

    pub(crate) fn accept_selected_stage_proposal(&mut self) -> bool {
        let Some(session) = self
            .selected_agent_portal()
            .and_then(|id| self.agent_session_for(id).map(|(session, _)| session))
        else {
            self.toast("Select an agent portal first.");
            return false;
        };
        let Some(id) = self
            .agents
            .pending_for(&session)
            .next()
            .map(|p| p.id.clone())
        else {
            self.toast("No pending agent proposal for this portal.");
            return false;
        };
        self.accept_stage_proposal(&id);
        true
    }

    pub(crate) fn reject_selected_stage_proposal(&mut self) -> bool {
        let Some(session) = self
            .selected_agent_portal()
            .and_then(|id| self.agent_session_for(id).map(|(session, _)| session))
        else {
            self.toast("Select an agent portal first.");
            return false;
        };
        let Some(id) = self
            .agents
            .pending_for(&session)
            .next()
            .map(|p| p.id.clone())
        else {
            self.toast("No pending agent proposal for this portal.");
            return false;
        };
        self.reject_stage_proposal(&id);
        true
    }

    pub(crate) fn agent_session_for(&self, portal: NodeId) -> Option<(String, String)> {
        let agent = slate_doc::agent_chat::agent(self.doc().scene.node(portal)?)?;
        Some((agent.session.clone(), agent.provider.clone()))
    }

    /// The selected agent card: a chat, or media an agent makes.
    pub(crate) fn selected_agent_portal(&self) -> Option<NodeId> {
        self.board_sel.iter().copied().find(|id| {
            self.doc()
                .scene
                .node(*id)
                .is_some_and(slate_doc::agent_chat::is_agent_node)
        })
    }

    fn agent_portal_unbound(&self, id: NodeId) -> bool {
        self.doc().scene.node(id).is_some_and(|n| {
            matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent && p.source.is_none())
        })
    }

    /// The wheel scrolls the album once a generator note holds more than one image.
    pub(crate) fn pointer_over_image_album(&self, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        let world = xf.s2w(p);
        // The index strip under an album browses it as well.
        let on_strip = self.doc().scene.nodes.iter().any(|n| {
            let count = self.agent_images(n.id).len();
            !n.hidden
                && count > 1
                && self.image_wheel(n.id) == ImageWheel::Album
                && atlas_shell::home::album_strip_rect(xf.rect_w2s(n.rect), count, xf.z).contains(p)
        });
        on_strip
            || self
                .doc()
                .scene
                .nodes
                .iter()
                .rev()
                .find(|n| !n.hidden && n.rect.contains(world.x, world.y))
                .is_some_and(|n| self.image_wheel(n.id) != ImageWheel::Board)
    }

    /// Who takes the wheel over an image portal: its album, the model a focused
    /// live generator flies, or the board. Paint and the board camera ask here.
    pub(crate) fn image_wheel(&self, id: NodeId) -> ImageWheel {
        if !slate_doc::agent_inputs::is_image_generator(&self.doc().scene, id) {
            return ImageWheel::Board;
        }
        let live = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.live);
        let focused = self.contents_focused() == Some(id) || self.portal_is_maximized(id);
        if live {
            if focused && self.generator_view(id).geometry().is_some() {
                ImageWheel::Model
            } else {
                ImageWheel::Board
            }
        } else if self.agent_images(id).len() > 1
            && (!matches!(
                self.doc().scene.node(id).map(|n| &n.kind),
                Some(NodeKind::Image(_))
            ) || (self.board_sel.len() == 1 && self.board_sel.contains(&id)))
        {
            ImageWheel::Album
        } else {
            // Unselected, a picture zooms with the board; hover never flows.
            ImageWheel::Board
        }
    }

    /// The wheel zooms the board while the pointer is over an agent card.
    pub(crate) fn pointer_over_agent_card(&self, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        let world = xf.s2w(p);
        self.doc().scene.nodes.iter().rev().any(|n| {
            !n.hidden
                && n.rect.contains(world.x, world.y)
                && matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent)
        })
    }

    /// The folder list before a chat starts. The wheel scrolls that list.
    pub(crate) fn pointer_over_project_picker(&self, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        let Some(id) = self.agents.project_picker else {
            return false;
        };
        let world = xf.s2w(p);
        self.doc()
            .scene
            .node(id)
            .is_some_and(|n| !n.hidden && n.rect.contains(world.x, world.y))
    }

    /// Cover Flow / bound poster owns the pointer only in contents-focus
    /// (or when maximized and unbound). Selection alone leaves the wheel
    /// on the board — same rule as the web portal (P1.portal.contents-focus).
    pub(crate) fn agent_shelf_captures(&self, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        if let Some(id) = self.portal_chrome.maximized {
            return self.agent_portal_unbound(id);
        }
        let Some(id) = self.agents.focused else {
            return false;
        };
        if !self.is_agent_portal(id) {
            return false;
        }
        let Some(node) = self.doc().scene.node(id) else {
            return false;
        };
        let srect = xf.rect_w2s(node.rect);
        let layout = layout_for_portal(PortalKind::Agent, srect, false, false, xf.z);
        if layout.pointer_on_chrome(p) {
            return false;
        }
        srect.contains(p)
    }

    /// Text editing owns the pointer. The rest of a train card drags as a node.
    pub(crate) fn agent_text_editing_captures(&self, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        if let Some((id, _)) = &self.agents.title_edit {
            if let Some(node) = self.doc().scene.node(*id) {
                let r = xf.rect_w2s(node.rect);
                let band = Rect::from_min_max(r.min, Pos2::new(r.right(), r.top() + 32.0 * xf.z));
                if band.contains(p) {
                    return true;
                }
            }
        }
        if self
            .agents
            .key_rect
            .is_some_and(|(_, rect)| rect.contains(p))
        {
            return true;
        }
        self.agents.composer_editing.is_some_and(|id| {
            self.agents
                .composer_rects
                .get(&id)
                .is_some_and(|rect| rect.contains(p))
        })
    }

    /// The tail composer under the pointer, when its card is the topmost node there.
    fn composer_under(&self, p: Pos2, xf: &BoardXf) -> Option<NodeId> {
        let top = self.doc().scene.nodes.iter().rev().find(|n| {
            !n.hidden
                && !n.is_frame()
                && !matches!(n.kind, NodeKind::Connector(_))
                && xf.rect_w2s(n.rect).contains(p)
        })?;
        self.agents
            .composer_rects
            .get(&top.id)
            .is_some_and(|rect| rect.contains(p))
            .then_some(top.id)
    }

    /// Paint registers composer fields and scroll ranges again for what it draws.
    pub(crate) fn begin_agent_paint(&mut self) {
        self.agents.composer_rects.clear();
        self.agents.card_overflow.clear();
    }

    /// A user-sized card with more content than room scrolls instead of zooming.
    pub(crate) fn pointer_over_scrolling_agent_card(
        &self,
        xf: &BoardXf,
        pointer: Option<Pos2>,
    ) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        let world = xf.s2w(p);
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find(|n| !n.hidden && !n.is_frame() && n.rect.contains(world.x, world.y))
            .is_some_and(|n| {
                slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.size.is_some())
                    && self
                        .agents
                        .card_overflow
                        .get(&n.id)
                        .is_some_and(|max| *max > 0.5)
            })
    }

    pub(crate) fn agent_focus(&mut self, id: NodeId) {
        self.portal_enter_interactive(id);
    }

    pub(crate) fn agent_enter_contents(&mut self, id: NodeId) {
        if self.is_agent_portal(id) {
            if let Some((session, _)) = self.agent_session_for(id) {
                let selected = self
                    .board_sel
                    .iter()
                    .copied()
                    .filter(|n| *n != id)
                    .collect::<Vec<_>>();
                if !selected.is_empty() {
                    self.agents.context_selection.insert(session, selected);
                }
            }
            self.agents.focused = Some(id);
        }
    }

    pub(crate) fn agent_blur(&mut self) -> bool {
        if self.agents.focused.is_some() {
            self.contents_blur()
        } else {
            false
        }
    }

    pub(crate) fn agent_leave_contents(&mut self) -> bool {
        self.agents.focused.take().is_some()
    }

    pub(crate) fn agent_toggle_focus(&mut self) -> bool {
        if self.agents.focused.is_some() {
            return self.agent_blur();
        }
        match self.selected_agent_portal() {
            Some(id) => {
                self.agent_focus(id);
                true
            }
            None => false,
        }
    }

    fn is_agent_portal(&self, id: NodeId) -> bool {
        self.doc()
            .scene
            .node(id)
            .is_some_and(|n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent))
    }

    pub(crate) fn pick_agent_project(&mut self, portal: NodeId) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose project folder")
                .pick_folder();
            let _ = tx.send(PickerMsg::AgentPortalSource {
                portal,
                path: picked,
            });
        });
    }

    pub(crate) fn pick_selected_agent_project(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        self.pick_agent_project(id);
        true
    }

    pub(crate) fn bind_agent_project(&mut self, portal: NodeId, path: PathBuf) {
        if self.agent_linear(portal)
            && (self
                .agents
                .session(portal)
                .is_some_and(|s| !s.conversation.is_empty())
                || self
                    .doc()
                    .scene
                    .node(portal)
                    .and_then(slate_doc::agent_chat::agent)
                    .is_some_and(|a| {
                        a.channel.is_some() || a.chat.parent.is_some() || a.chat.end.is_some()
                    }))
        {
            self.toast("Create a new agent portal to choose another project; this conversation keeps its workspace.");
            return;
        }
        if self.agent_is_running(portal) {
            self.toast("Stop the current response before changing folders.");
            return;
        }
        if !path.is_dir() {
            self.toast("Choose a folder for this agent portal.");
            return;
        }
        self.agents.project_picker = None;
        self.bind_portal_source(portal, path.clone());
        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Project")
            .to_string();
        let mut recents = RecentList::load(AGENT_RECENTS_KEY);
        recents.record(path.clone(), title.clone());
        recents.save(AGENT_RECENTS_KEY);
        self.agents.recents.retain(|e| !paths_same(&e.path, &path));
        self.agents.recents.insert(
            0,
            RecentEntry {
                path: path.clone(),
                title,
                opened_at: atlas_ai::context::now_secs(),
                cover: None,
            },
        );
        self.agents.pending_chat_pick = Some(portal);
        self.agent_focus(portal);
        #[cfg(not(test))]
        {
            let provider = self
                .agent_session_for(portal)
                .map(|v| v.1)
                .unwrap_or_default();
            self.agents
                .chats
                .remove(&(provider.clone(), chat_key(&path)));
            self.request_agent_chats(path, &provider);
        }
    }

    fn ensure_agent_recents(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.agents.recents_rx {
            match rx.try_recv() {
                Ok(list) => {
                    self.agents.provider_recents = list;
                    self.agents.recents_rx = None;
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.agents.recents_rx = None;
                }
            }
        }
        if self.agents.recents_started {
            return;
        }
        self.agents.recents_started = true;
        let (tx, rx) = unbounded();
        self.agents.recents_rx = Some(rx);
        std::thread::spawn(move || {
            let list = ["codex", "cursor"]
                .into_iter()
                .map(|provider| {
                    (
                        provider.to_string(),
                        collect_agent_project_recents(provider),
                    )
                })
                .collect();
            let _ = tx.send(list);
        });
    }

    pub(crate) fn paint_agent_portal(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let rect = xf.rect_w2s(node.rect);
        let maximized = self.portal_is_maximized(node.id);
        let layout = super::board_portal_chrome::layout_for_portal(
            PortalKind::Agent,
            rect,
            false,
            maximized,
            xf.z,
        );
        atlas_shell::selection_tools::agent_card(painter, layout.frame, layout.radius, xf.z, {
            let mut palette = self.palette();
            if portal.agent.as_ref().is_some_and(|a| a.chat.custom_fill) {
                palette.card = super::board::rgba32(portal.fill);
            }
            palette
        });
        if let Some(stroke) = portal
            .agent
            .as_ref()
            .and_then(|a| a.chat.stroke)
            .filter(|s| s.width > 0.0)
        {
            painter.rect_stroke(
                layout.frame,
                layout.radius,
                egui::Stroke::new(stroke.width * xf.z, super::board::rgba32(stroke.color)),
                egui::StrokeKind::Inside,
            );
        }

        if portal.agent.as_ref().is_none_or(|a| a.provider.is_empty()) {
            self.paint_agent_unbound(ui, layout.body, node.id, maximized, xf.z);
        } else if portal
            .agent
            .as_ref()
            .is_some_and(|a| a.chat.train && !a.chat.bundled.is_empty())
        {
            self.paint_agent_bundle(painter, xf, node, portal);
        } else if !maximized
            && !self.agent_in_choose_phase(node.id)
            && portal
                .agent
                .as_ref()
                .is_some_and(|a| a.chat.collapsed && !a.chat.draft)
        {
            self.paint_agent_summary(ui, painter, xf, node, portal, Some(COLLAPSED_ROWS));
            self.paint_collapsed_composer(ui, &layout, node, xf.z);
        } else if !maximized
            && portal.agent.as_ref().is_some_and(|a| {
                a.chat.train
                    && !a.chat.draft
                    && a.chat.detail == slate_doc::agent_chat::Detail::Summary
                    && self.agent_has_child(node.id)
            })
        {
            self.paint_agent_summary(ui, painter, xf, node, portal, None);
        } else {
            self.paint_agent_bound(ui, painter, xf, &layout, node, portal, maximized);
        }

        if portal
            .agent
            .as_ref()
            .is_some_and(|a| !a.provider.is_empty() && a.view == atlas_ai::agent::PortalView::Chat)
        {
            self.paint_agent_chat_header(ui, painter, xf, node, portal);
        }

        if !maximized {
            self.paint_portal_shell_finish(
                ui,
                painter,
                &layout,
                node.id,
                portal,
                None,
                Color32::from_rgba_unmultiplied(150, 180, 230, 150),
                false,
                xf.z,
            );
        }
    }

    fn paint_agent_unbound(
        &mut self,
        ui: &egui::Ui,
        body: Rect,
        id: NodeId,
        _maximized: bool,
        zoom: f32,
    ) {
        self.ensure_agent_programs();
        let interactive = !self.tab().read_only;
        if interactive
            && !_maximized
            && self.board_drag.is_none()
            && !self.agents.programs.is_empty()
        {
            let count = self.agents.programs.len();
            let width = count.min(4) as f32 * 112.0 + 48.0;
            let height = count.div_ceil(4) as f32 * 96.0 + 72.0;
            if self
                .doc()
                .scene
                .node(id)
                .is_some_and(|n| (n.rect.w - width).abs() > 1.0 || (n.rect.h - height).abs() > 1.0)
            {
                self.patch_nodes(&[id], |n| {
                    n.rect.w = width;
                    n.rect.h = height;
                });
            }
        }
        if let Some(provider) = atlas_ai::ui::program_grid(
            ui,
            body,
            Id::new(("agent-programs", id.0)),
            &self.agents.programs,
            interactive,
            zoom,
        ) {
            self.set_agent_program(id, &provider);
        }
    }

    #[allow(clippy::too_many_arguments)] // Existing portal paint adapter.
    fn paint_agent_pick_list(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        layout: &super::board_portal_chrome::PortalChromeLayout,
        node: &Node,
        portal: &PortalNode,
        _maximized: bool,
    ) {
        let projects = self.agents.project_picker == Some(node.id);
        let provider = portal
            .agent
            .as_ref()
            .map(|a| a.provider.as_str())
            .unwrap_or_default();
        let projects_list = self
            .agents
            .provider_recents
            .get(provider)
            .unwrap_or(&self.agents.recents);
        let chats = if projects {
            projects_list
                .iter()
                .map(|e| CursorChat {
                    id: e.path.to_string_lossy().into_owned(),
                    title: e.title.clone(),
                    updated_at: e.opened_at,
                })
                .collect()
        } else {
            self.agents
                .chat_picker
                .as_ref()
                .map(|p| p.chats.clone())
                .unwrap_or_default()
        };
        let visible = chats.len().min(6) as f32;
        // Capsules are 70% of the previous 40px row. Chrome shrinks with the button.
        let desired_h = 156.0 + visible * 34.0;
        if !self.tab().read_only
            && self.board_drag.is_none()
            && ((node.rect.h - desired_h).abs() > 1.0 || node.rect.w < 380.0)
        {
            self.patch_nodes(&[node.id], |n| {
                n.rect.h = desired_h;
                n.rect.w = n.rect.w.max(380.0);
            });
        }
        let z = xf.z.max(0.01);
        let body = layout.body;
        let palette = self.palette();
        let interactive = !self.tab().read_only;
        let title_px = canvas_text::authored_px(15.0, z);
        let row_px = canvas_text::authored_px(13.5, z);
        let pad = canvas_scale::px(14.0, z);
        let row_h = canvas_scale::px(28.0, z);
        let gap = canvas_scale::px(6.0, z);
        let button_h = canvas_scale::px(28.0, z);
        let radius = canvas_scale::px(8.0, z);
        let div_y = body.top() + canvas_scale::px(34.0, z);
        painter.line_segment(
            [
                Pos2::new(body.left() + pad, div_y),
                Pos2::new(body.right() - pad, div_y),
            ],
            egui::Stroke::new((1.0 * z).max(1.0), palette.line.gamma_multiply(0.7)),
        );
        let heading = Rect::from_min_max(
            Pos2::new(body.left() + pad, div_y + canvas_scale::px(10.0, z)),
            Pos2::new(body.right() - pad, div_y + canvas_scale::px(36.0, z)),
        );
        let button = Rect::from_min_max(
            Pos2::new(body.left() + pad, body.bottom() - pad - button_h),
            Pos2::new(body.right() - pad, body.bottom() - pad),
        );
        let list = Rect::from_min_max(
            Pos2::new(heading.left(), heading.bottom() + canvas_scale::px(4.0, z)),
            Pos2::new(heading.right(), button.top() - canvas_scale::px(10.0, z)),
        );
        if list.height() < 8.0 {
            return;
        }
        let heading_label = if projects {
            "Choose project"
        } else {
            "Choose conversation"
        };
        let action = if projects {
            "Choose another folder"
        } else {
            "New conversation"
        };
        let mut chosen: Option<Option<String>> = None;
        if interactive && canvas_text::legible(row_px) {
            egui::Area::new(Id::new(("agent-pick-list", node.id.0)))
                .fixed_pos(heading.min)
                .constrain(false)
                .order(egui::Order::Middle)
                .show(ui.ctx(), |ui| {
                    let stack = Rect::from_min_max(heading.min, button.max);
                    ui.set_min_size(stack.size());
                    ui.set_max_size(stack.size());
                    ui.set_clip_rect(stack.intersect(ui.ctx().screen_rect()));
                    ui.style_mut().visuals = palette.visuals();
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if canvas_text::legible(title_px) {
                        let galley = tracked_galley(
                            ui,
                            heading_label,
                            title_px,
                            palette.ink.gamma_multiply(0.78),
                            canvas_scale::px(0.4, z),
                            heading.width(),
                        );
                        let pos = Pos2::new(
                            heading.left(),
                            heading.center().y - galley.size().y * 0.5,
                        );
                        ui.painter().galley(pos, galley, palette.ink);
                    }
                    ui.add_space(heading.height());
                    let row_fill = if palette.dark_mode {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                    } else {
                        Color32::from_rgba_unmultiplied(15, 23, 32, 14)
                    };
                    let row_hover = if palette.dark_mode {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 28)
                    } else {
                        Color32::from_rgba_unmultiplied(15, 23, 32, 24)
                    };
                    egui::ScrollArea::vertical()
                        .id_salt(("agent-pick-scroll", node.id.0))
                        .max_height(list.height())
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(list.width());
                            ui.spacing_mut().item_spacing.y = gap;
                            if let Some(error) = &self.agents.catalog_error {
                                ui.label(
                                    egui::RichText::new(error)
                                        .size(row_px)
                                        .color(palette.sub),
                                );
                            }
                            if chats.is_empty() {
                                let empty = if provider == "cursor" {
                                    "No agents for this folder yet. Chats in the Cursor window are a separate list."
                                } else if projects {
                                    "No recent projects yet."
                                } else {
                                    "No saved conversations for this project yet."
                                };
                                ui.label(
                                    egui::RichText::new(empty)
                                        .size(canvas_text::authored_px(12.5, z))
                                        .color(palette.sub),
                                );
                            }
                            let cols = if projects {
                                1
                            } else if chats.len() > 8 {
                                3
                            } else if chats.len() > 3 {
                                2
                            } else {
                                1
                            };
                            let col_w =
                                (list.width() - gap * (cols as f32 - 1.0)).max(40.0) / cols as f32;
                            ui.horizontal_wrapped(|ui| {
                                ui.set_width(list.width());
                                ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                                for chat in &chats {
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(col_w, row_h),
                                    Sense::click(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    radius,
                                    if resp.hovered() { row_hover } else { row_fill },
                                );
                                let text_w = (rect.width() - canvas_scale::px(44.0, z)).max(8.0);
                                let galley = tracked_galley(
                                    ui,
                                    &chat.title,
                                    row_px,
                                    palette.ink,
                                    canvas_scale::px(0.08, z),
                                    text_w,
                                );
                                let text_pos = Pos2::new(
                                    rect.left() + canvas_scale::px(14.0, z),
                                    rect.center().y - galley.size().y * 0.5,
                                );
                                ui.painter().galley(text_pos, galley, palette.ink);
                                let chev = Rect::from_center_size(
                                    Pos2::new(
                                        rect.right() - canvas_scale::px(16.0, z),
                                        rect.center().y,
                                    ),
                                    egui::vec2(canvas_scale::px(11.0, z), canvas_scale::px(11.0, z)),
                                );
                                atlas_shell::icons::paint(
                                    ui.painter(),
                                    chev,
                                    atlas_shell::icons::Icon::ChevronRight,
                                    palette.sub,
                                );
                                if resp.clicked() {
                                    chosen = Some(Some(chat.id.clone()));
                                }
                                }
                            });
                        });
                    let resp = ui.interact(
                        button,
                        Id::new(("agent-pick-new", node.id.0)),
                        Sense::click(),
                    );
                    let blue = if resp.hovered() {
                        Color32::from_rgb(55, 117, 250)
                    } else {
                        Color32::from_rgb(37, 99, 235)
                    };
                    ui.painter().rect_filled(button, radius, blue);
                    let label = format!("+  {action}");
                    let galley = tracked_galley(
                        ui,
                        &label,
                        canvas_text::authored_px(14.0, z),
                        Color32::WHITE,
                        canvas_scale::px(0.15, z),
                        button.width() - canvas_scale::px(24.0, z),
                    );
                    let pos = Pos2::new(
                        button.center().x - galley.size().x * 0.5,
                        button.center().y - galley.size().y * 0.5,
                    );
                    ui.painter().galley(pos, galley, Color32::WHITE);
                    if resp.clicked() {
                        chosen = Some(None);
                    }
                });
        } else if canvas_text::legible(row_px) {
            let preview = if chats.is_empty() {
                format!("{} — double-click to start", portal.title)
            } else {
                format!("{} agents — double-click to choose", chats.len())
            };
            canvas_text::text(
                painter,
                list.left_top(),
                Align2::LEFT_TOP,
                &preview,
                FontId::proportional(row_px),
                palette.sub,
            );
        }
        if let Some(channel) = chosen {
            if projects {
                if let Some(path) = channel {
                    self.bind_agent_project(node.id, PathBuf::from(path));
                } else {
                    self.pick_agent_project(node.id);
                }
            } else {
                self.set_agent_channel(node.id, channel);
                self.agents.chat_picker = None;
            }
        }
    }

    #[allow(clippy::too_many_arguments)] // Existing portal paint adapter.
    /// Agent content controls dispatch the same registered actions as the inspector.
    fn paint_agent_chat_header(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let Some(agent) = portal.agent.as_ref() else {
            return;
        };
        let z = xf.z;
        let rect = xf.rect_w2s(node.rect);
        let palette = self.palette();
        if self.agent_context_handles_visible(node.id) {
            self.paint_agent_artifacts(ui, painter, xf, node);
        } else {
            self.agents.context_auto.remove(&node.id);
            self.agents.context_human.remove(&node.id);
        }
        let anchor = rect.min
            + egui::vec2(
                slate_doc::agent_chat::PORT_INSET * z,
                slate_doc::agent_chat::RAIL_INSET * z,
            );
        let (_status_color, status_label, pulse) = agent_status_chip(
            &agent.provider,
            self.agents.ide,
            self.agents.session(node.id).map(|s| &s.status),
            self.agents.awaiting.get(&node.id),
        );
        {
            painter.circle_filled(anchor, 3.5 * z, palette.sub.gamma_multiply(0.55));
            ui.interact(
                Rect::from_center_size(anchor, egui::vec2(16.0 * z, 16.0 * z)),
                Id::new(("agent-status", node.id.0)),
                Sense::hover(),
            )
            .on_hover_text(status_label);
        }
        if pulse {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let title = portal.title.clone();
        let running = self.agent_is_running(node.id);
        let show_model = (agent.provider == "codex"
            || agent.provider == "cursor"
            || agent.provider.starts_with("ollama"))
            && self.agents.project_picker != Some(node.id)
            && !self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|p| p.portal == node.id);
        let title_rect = Rect::from_min_max(
            anchor + egui::vec2(10.0 * z, -10.0 * z),
            rect.right_top() + egui::vec2(-56.0 * z, 27.0 * z),
        );
        if self
            .agents
            .title_edit
            .as_ref()
            .is_some_and(|(id, _)| *id == node.id)
        {
            let mut draft = self.agents.title_edit.as_ref().unwrap().1.clone();
            let mut title_ui = egui::Ui::new(
                ui.ctx().clone(),
                Id::new(("agent-title-editor", node.id.0)),
                egui::UiBuilder::new()
                    .layer_id(ui.layer_id())
                    .max_rect(title_rect),
            );
            title_ui.set_clip_rect(title_rect);
            let edit = title_ui.add(
                egui::TextEdit::singleline(&mut draft)
                    .frame(false)
                    .font(FontId::proportional(13.0 * z))
                    .desired_width(title_rect.width()),
            );
            if !edit.has_focus() && !edit.lost_focus() {
                edit.request_focus();
            }
            self.agents.title_edit = Some((node.id, draft.clone()));
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.agents.title_edit = None;
            } else if edit.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.board_sel.clear();
                self.board_sel.insert(node.id);
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.agent.rename"),
                    Some(draft),
                );
                self.agents.title_edit = None;
            }
        } else if show_model && (agent.chat.draft || !self.agent_has_child(node.id)) {
            let model_name = self.model_menu_label(agent);
            let mut header = egui::Ui::new(
                ui.ctx().clone(),
                Id::new(("agent-model", node.id.0)),
                egui::UiBuilder::new()
                    .layer_id(ui.layer_id())
                    .max_rect(title_rect),
            );
            header.set_clip_rect(ui.clip_rect());
            header.style_mut().visuals = palette.visuals();
            header.style_mut().override_font_id = Some(FontId::proportional(13.0 * z));
            header.spacing_mut().button_padding = egui::vec2(0.0, 2.0 * z);
            header.spacing_mut().interact_size.y = 24.0 * z;
            header.spacing_mut().item_spacing.x = 4.0 * z;
            for widget in [&mut header.style_mut().visuals.widgets.inactive] {
                widget.bg_fill = Color32::TRANSPARENT;
                widget.weak_bg_fill = Color32::TRANSPARENT;
                widget.bg_stroke = egui::Stroke::NONE;
            }
            let mut chosen = None;
            let mut rename = false;
            header.horizontal(|header| {
                if header
                    .add(egui::Button::new(&title).frame(false))
                    .on_hover_text("Rename conversation")
                    .clicked()
                {
                    rename = true;
                }
                header.label("·");
                // Full access paints the model deep red, the moment it is granted.
                let full = self.agent_full_access(&agent.session);
                let model_text = if full {
                    egui::RichText::new(model_name).color(palette.danger)
                } else {
                    egui::RichText::new(model_name)
                };
                let model = header.menu_button(model_text, |ui| {
                    chosen = self.model_menu_items(ui, node.id, agent, z);
                });
                if full {
                    model.response.on_hover_text(
                        "Full access: this conversation runs commands and edits files without asking",
                    );
                }
            });
            if rename {
                self.agents.title_edit = Some((node.id, title));
            }
            if let Some(model) = chosen {
                self.board_sel = std::iter::once(node.id).collect();
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.agent.model"),
                    Some(model),
                );
            }
        } else {
            // Sent cards name the model that answered; only the tail chooses the next one.
            let font = FontId::proportional(canvas_text::authored_px(13.0, z));
            let clip = painter.with_clip_rect(title_rect);
            let at = anchor + egui::vec2(10.0 * z, 0.0);
            if show_model && self.agent_full_access(&agent.session) {
                let lead = format!("{title} · ");
                let lead_w =
                    canvas_text::layout_no_wrap(&clip, lead.clone(), font.clone(), palette.ink)
                        .size()
                        .x;
                canvas_text::text(
                    &clip,
                    at,
                    Align2::LEFT_CENTER,
                    lead,
                    font.clone(),
                    palette.ink,
                );
                canvas_text::text(
                    &clip,
                    at + egui::vec2(lead_w, 0.0),
                    Align2::LEFT_CENTER,
                    self.model_menu_label(agent),
                    font,
                    palette.danger,
                );
            } else {
                let label = if show_model {
                    format!("{title} · {}", self.model_menu_label(agent))
                } else {
                    title.clone()
                };
                canvas_text::text(&clip, at, Align2::LEFT_CENTER, label, font, palette.ink);
            }
            if ui
                .interact(
                    title_rect,
                    Id::new(("agent-title", node.id.0)),
                    Sense::click(),
                )
                .on_hover_text("Rename conversation")
                .clicked()
            {
                self.agents.title_edit = Some((node.id, title));
            }
        }
        if agent.chat.draft {
            return;
        }
        let leaf = !self.agent_has_child(node.id);
        // 60% of the previous cluster, pinned to its old top and right edge.
        let full_r = 3.5 * z;
        let full_pitch = full_r * 2.8;
        let full_top = rect.top() + slate_doc::agent_chat::RAIL_INSET * z - full_r;
        let full_right_edge =
            rect.right() - (slate_doc::agent_chat::PORT_INSET + 3.5) * z - 4.0 * z - full_r * 0.4;
        let dot_r = full_r * 0.6;
        let pitch = full_pitch * 0.6;
        let dot_cy = full_top + dot_r;
        let menu_w = pitch * 3.0;
        let menu_rect = Rect::from_min_size(
            Pos2::new(full_right_edge - (pitch * 2.5 + dot_r), full_top),
            egui::vec2(menu_w, dot_r * 2.0),
        );
        let mut menu_ui = egui::Ui::new(
            ui.ctx().clone(),
            Id::new(("agent-menu", node.id.0)),
            egui::UiBuilder::new()
                .layer_id(ui.layer_id())
                .max_rect(menu_rect),
        );
        menu_ui.set_clip_rect(ui.clip_rect());
        menu_ui.style_mut().visuals = palette.visuals();
        menu_ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
        menu_ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        menu_ui.spacing_mut().interact_size = egui::vec2(menu_w, dot_r * 2.0);
        let widgets = &mut menu_ui.style_mut().visuals.widgets;
        for widget in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
        ] {
            widget.bg_fill = Color32::TRANSPARENT;
            widget.weak_bg_fill = Color32::TRANSPARENT;
            widget.bg_stroke = egui::Stroke::NONE;
        }
        let full_on = self.agent_full_access(&agent.session);
        let schedulable = atlas_ai::runtime::linear_provider(&agent.provider) && !agent.chat.draft;
        let scheduled = schedulable && self.agent_schedule(node.id).is_some();
        let mut command: Option<&str> = None;
        let mut command_detail: Option<&str> = None;
        egui::menu::menu_custom_button(
            &mut menu_ui,
            egui::Button::new("")
                .min_size(menu_rect.size())
                .frame(false),
            |ui| {
                use atlas_shell::icons::Icon;
                use atlas_shell::menu::{MenuIcon, Row};
                use slate_doc::agent_chat::Detail;
                let dark = palette.dark_mode;
                atlas_shell::menu::prepare(ui, dark);
                ui.set_min_width(224.0);
                ui.set_max_width(260.0);
                let train = agent.chat.train;
                let full_access = if full_on {
                    Row::new(MenuIcon::Lock, "Full access")
                        .checked(true)
                        .danger()
                } else {
                    Row::new(MenuIcon::Lock, "Full access")
                };
                // Presentation, then the conversation, then this card.
                let rows = [
                    (
                        0,
                        Row::glyph(Icon::ChatWindow, "Single chat window"),
                        "portal.agent.chat",
                        train && !agent.chat.draft && !running,
                    ),
                    (
                        0,
                        Row::glyph(Icon::ChatTrain, "Chat train"),
                        "portal.agent.train",
                        (!train || agent.chat.detail == Detail::Pair) && !running,
                    ),
                    (
                        0,
                        Row::glyph(Icon::ChatPairs, "Message pairs"),
                        "portal.agent.pairs",
                        train && agent.chat.detail != Detail::Pair && !running,
                    ),
                    (
                        0,
                        Row::glyph(Icon::TextDoc, "Full conversation"),
                        "portal.agent.full",
                        train && !agent.chat.draft && agent.chat.detail != Detail::Full,
                    ),
                    (
                        1,
                        Row::glyph(Icon::Agent, "Choose program"),
                        "portal.agent.provider",
                        !running
                            && !train
                            && agent.chat.parent.is_none()
                            && agent.chat.end.is_none(),
                    ),
                    (
                        1,
                        Row::glyph(Icon::Stop, "Stop response"),
                        "portal.agent.stop",
                        running,
                    ),
                    (
                        1,
                        full_access.enabled(!running),
                        "portal.agent.full_access",
                        atlas_ai::runtime::linear_provider(&agent.provider) && !agent.chat.draft,
                    ),
                    (
                        1,
                        Row::glyph(
                            Icon::Clock,
                            if scheduled {
                                "Edit schedule…"
                            } else {
                                "Schedule…"
                            },
                        ),
                        "portal.agent.schedule:{\"open\":true}",
                        schedulable,
                    ),
                    (
                        1,
                        Row::glyph(Icon::Clock, "Stop schedule"),
                        "portal.agent.schedule:{\"stop\":true}",
                        scheduled,
                    ),
                    (
                        2,
                        Row::glyph(Icon::Fit, "Fit to text"),
                        "portal.agent.fit",
                        agent.chat.size.is_some(),
                    ),
                    (
                        2,
                        Row::new(MenuIcon::Trash, "Delete card").danger(),
                        "board.delete",
                        leaf,
                    ),
                ];
                let mut group = None;
                for (section, row, id, shown) in rows {
                    if !shown {
                        continue;
                    }
                    if group.is_some_and(|g| g != section) {
                        atlas_shell::menu::separator(ui, dark);
                    }
                    group = Some(section);
                    let response = atlas_shell::menu::row(ui, row, dark);
                    let response = if id == "portal.agent.full_access" {
                        response.on_hover_text(
                            "Run commands and edit files without asking, in this conversation",
                        )
                    } else {
                        response
                    };
                    if response.clicked() {
                        match id.split_once(':') {
                            Some((cmd, detail)) => {
                                command = Some(cmd);
                                command_detail = Some(detail);
                            }
                            None => command = Some(id),
                        }
                        ui.close_menu();
                    }
                }
            },
        );
        for i in 0..3 {
            ui.painter().circle_filled(
                Pos2::new(menu_rect.left() + pitch * (i as f32 + 0.5), dot_cy),
                dot_r,
                palette.ink.gamma_multiply(0.82),
            );
        }
        if !(agent.chat.train && !agent.chat.bundled.is_empty())
            && self.paint_agent_collapse_toggle(ui, node, rect, menu_rect.left(), dot_cy, z)
        {
            command = Some("portal.agent.collapse");
        }
        if schedulable && self.paint_agent_clock(ui, node.id, menu_rect.left(), dot_cy, z) {
            command = Some("portal.agent.schedule");
            command_detail = Some("{\"open\":true}");
        }
        if let Some(command) = command {
            if command != "portal.agent.bundle_chat" {
                self.board_sel.clear();
                self.board_sel.insert(node.id);
            }
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId(command),
                command_detail.map(str::to_string),
            );
        }
    }

    /// Chevron just left of the ellipsis. Quiet at rest; the card's hover reveals it.
    fn paint_agent_collapse_toggle(
        &self,
        ui: &egui::Ui,
        node: &Node,
        card_rect: Rect,
        menu_left: f32,
        cy: f32,
        z: f32,
    ) -> bool {
        let collapsed = slate_doc::agent_chat::agent(node).is_some_and(|a| a.chat.collapsed);
        let center = Pos2::new(menu_left - canvas_scale::px(8.0, z), cy);
        let response = ui
            .interact(
                Rect::from_center_size(center, egui::Vec2::splat(canvas_scale::px(12.0, z))),
                Id::new(("agent-collapse", node.id.0)),
                Sense::click(),
            )
            .on_hover_text(if collapsed {
                "Expand card"
            } else {
                "Collapse to three lines"
            });
        let card = self.board_sel.contains(&node.id)
            || ui
                .ctx()
                .pointer_hover_pos()
                .is_some_and(|p| card_rect.contains(p));
        let reveal = ui.ctx().animate_bool_with_time(
            Id::new(("agent-collapse-reveal", node.id.0)),
            card,
            0.12,
        );
        let alpha = if response.hovered() {
            0.95
        } else {
            0.18 + 0.64 * reveal
        };
        let half = canvas_scale::px(3.0, z);
        let rise = canvas_scale::px(1.6, z) * if collapsed { 1.0 } else { -1.0 };
        let stroke = egui::Stroke::new(
            canvas_scale::px(1.3, z),
            self.palette().ink.gamma_multiply(alpha),
        );
        ui.painter().line_segment(
            [
                center + egui::vec2(-half, -rise),
                center + egui::vec2(0.0, rise),
            ],
            stroke,
        );
        ui.painter().line_segment(
            [
                center + egui::vec2(0.0, rise),
                center + egui::vec2(half, -rise),
            ],
            stroke,
        );
        response.clicked()
    }

    fn paint_agent_bound(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        layout: &super::board_portal_chrome::PortalChromeLayout,
        node: &Node,
        portal: &PortalNode,
        maximized: bool,
    ) {
        let Some(agent) = portal.agent.as_ref() else {
            return;
        };

        if self.agents.project_picker == Some(node.id)
            || self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|p| p.portal == node.id)
        {
            self.paint_agent_pick_list(ui, painter, xf, layout, node, portal, maximized);
            return;
        }
        if self.agents.pending_chat_pick == Some(node.id)
            || (self.agents.connection_pending == Some(node.id)
                && !self.agents.connection_background)
        {
            canvas_text::text(
                painter,
                layout.body.center(),
                Align2::CENTER_CENTER,
                "Loading conversation…",
                FontId::proportional(13.0 * xf.z),
                self.palette().sub,
            );
            return;
        }
        let z = xf.z.max(0.01);
        let palette = self.palette();
        let body = layout.body;
        let font = FontId::proportional(canvas_text::authored_px(14.0, z));
        if !canvas_text::legible(font.size) {
            return;
        }
        let pad = 12.0 * z;
        let text_x = body.left() + (slate_doc::agent_chat::PORT_INSET + 10.0) * z;
        let prompt = self
            .agents
            .prompts
            .get(&node.id)
            .cloned()
            .unwrap_or_default();
        let wrap = (body.right() - pad - text_x).max(1.0);
        let prompt_h = composer_text_height(ui.ctx(), &prompt, wrap, 14.0 * z);
        let bottom_limit = body.bottom() - COMPOSER_BOTTOM * z;
        let title_floor = body.top() + COMPOSER_TOP * z;
        if agent.chat.draft {
            let field = Rect::from_min_max(
                Pos2::new(text_x, title_floor),
                Pos2::new(
                    body.right() - pad,
                    (title_floor + prompt_h + 2.0 * z).min(body.bottom() - 8.0 * z),
                ),
            );
            self.paint_agent_composer(ui, node.id, field, z, true);
            return;
        }
        let turns = self.visible_agent_turns(node.id);
        let awaiting = self.agents.awaiting.get(&node.id).cloned();
        let approval = self
            .agents
            .session(node.id)
            .and_then(|s| s.approval.clone());
        let key_open = self.agents.key_entry == Some(node.id);
        let transcript_open = !turns.is_empty() || awaiting.is_some() || approval.is_some();
        let key_shift = if key_open && !transcript_open {
            72.0 * z
        } else {
            0.0
        };
        let reserve = if transcript_open { 48.0 * z } else { 0.0 };
        let room = (bottom_limit - title_floor - reserve - key_shift).max(18.0 * z);
        // Two extra pixels keep the caret from sitting on the clip.
        let field_h = (prompt_h + 2.0 * z).max(18.0 * z).min(room);
        let field_top = if transcript_open {
            (bottom_limit - field_h).max(title_floor)
        } else {
            title_floor + key_shift
        };
        let input = Rect::from_min_max(
            Pos2::new(text_x, field_top),
            Pos2::new(
                body.right() - pad,
                (field_top + field_h).min(body.bottom() - 8.0 * z),
            ),
        );
        if key_open && !transcript_open {
            let strip = Rect::from_min_max(
                Pos2::new(body.left() + pad, title_floor),
                Pos2::new(body.right() - pad, title_floor + 64.0 * z),
            );
            self.paint_agent_key_entry(ui, node.id, strip, z);
        }
        let key_strip = (key_open && transcript_open).then(|| {
            Rect::from_min_max(
                Pos2::new(body.left() + pad, input.top() - 80.0 * z),
                Pos2::new(body.right() - pad, input.top() - 8.0 * z),
            )
        });
        if self.agents.key_rect.is_some_and(|(id, _)| id == node.id) && !key_open {
            self.agents.key_rect = None;
        }
        // Sent cards have no composer, so their text runs to the bottom pad.
        let composer = !self.agent_has_child(node.id);
        let transcript_bottom = if composer {
            key_strip
                .map(|r| r.top() - COMPOSER_GAP * z)
                .unwrap_or(input.top() - COMPOSER_GAP * z)
                .max(title_floor)
        } else {
            bottom_limit.max(title_floor)
        };
        let transcript = Rect::from_min_max(
            Pos2::new(body.left() + pad, title_floor),
            Pos2::new(body.right() - pad, transcript_bottom),
        );
        if transcript.height() > 16.0 * z {
            let mut chat_ui = egui::Ui::new(
                ui.ctx().clone(),
                Id::new(("agent-transcript", node.id.0)),
                egui::UiBuilder::new()
                    .layer_id(ui.layer_id())
                    .max_rect(transcript),
            );
            chat_ui.set_clip_rect(transcript.intersect(ui.clip_rect()));
            chat_ui.style_mut().visuals = palette.visuals();
            chat_ui.style_mut().override_font_id = Some(font.clone());
            chat_ui.spacing_mut().item_spacing = egui::vec2(8.0 * z, 12.0 * z);
            let text_key = self.agent_text_key(painter, z);
            let scroll = egui::ScrollArea::vertical()
                .id_salt(("agent-history-scroll", node.id.0))
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .vertical_scroll_offset(
                    self.agents
                        .transcript_scroll
                        .get(&node.id)
                        .copied()
                        .unwrap_or(0.0)
                        * z,
                )
                .max_height(transcript.height())
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(&mut chat_ui, |ui| {
                    for (index, turn) in turns.iter().enumerate() {
                        let width = transcript.width();
                        if agent.chat.detail == slate_doc::agent_chat::Detail::Pair
                            && index > 0
                            && turns[index - 1].role == "user"
                            && turn.role == "assistant"
                        {
                            let (rule, _) =
                                ui.allocate_exact_size(egui::vec2(width, 10.0 * z), Sense::hover());
                            ui.painter().hline(
                                (rule.left() + 12.0 * z)..=(rule.right() - 12.0 * z),
                                rule.center().y,
                                egui::Stroke::new(1.0 * z, palette.sub.gamma_multiply(0.4)),
                            );
                        }
                        let streaming = index + 1 == turns.len()
                            && agent.chat.end.is_none()
                            && turn.role == "assistant"
                            && self
                                .agents
                                .session(node.id)
                                .is_some_and(|s| matches!(s.status, AgentStatus::Thinking));
                        let text = if streaming {
                            format!("{} |", turn.text)
                        } else {
                            turn.text.clone()
                        };
                        let user = turn.role == "user";
                        let wrap = (width * if user { 0.78 } else { 0.94 } - 20.0 * z).max(1.0);
                        let ink = if turn.role == "system" {
                            palette.sub
                        } else {
                            palette.ink
                        };
                        let cache_id = (node.id, index);
                        if self
                            .agents
                            .transcript_cache
                            .get(&cache_id)
                            .is_none_or(|e| (e.0, e.1, e.2) != text_key)
                        {
                            let laid =
                                canvas_text::layout(ui.painter(), text, font.clone(), ink, wrap);
                            self.agents.transcript_cache.insert(
                                cache_id,
                                (
                                    text_key.0,
                                    text_key.1,
                                    text_key.2,
                                    laid.galley(),
                                    laid.scale() / z,
                                ),
                            );
                        }
                        let entry = &self.agents.transcript_cache[&cache_id];
                        let laid = canvas_text::Scaled::from_galley(entry.3.clone(), entry.4 * z);
                        let size = laid.size() + egui::vec2(20.0, 20.0) * z;
                        let (row, _) =
                            ui.allocate_exact_size(egui::vec2(width, size.y), Sense::hover());
                        let bubble = Rect::from_min_size(
                            Pos2::new(
                                if user {
                                    row.right() - size.x
                                } else {
                                    row.left()
                                },
                                row.top(),
                            ),
                            size,
                        );
                        ui.painter().rect_filled(
                            bubble,
                            10.0 * z,
                            if user {
                                palette.card_hover
                            } else {
                                palette.card
                            },
                        );
                        laid.selectable(
                            ui,
                            Id::new(("agent-turn-selection", node.id.0, index)),
                            bubble.min + egui::vec2(10.0, 10.0) * z,
                            ink,
                        );
                    }
                    if let Some(approval) = &approval {
                        ui.label(&approval.description);
                        ui.horizontal(|ui| {
                            for (label, allow) in [("Allow once", true), ("Deny", false)] {
                                if ui.button(label).clicked() {
                                    self.dispatch(
                                        ui.ctx(),
                                        atlas_commands::CommandId("portal.agent.approval"),
                                        Some(serde_json::to_string(&(node.id.0, allow)).unwrap()),
                                    );
                                }
                            }
                        });
                    }
                    match &awaiting {
                        Some(AgentAwait::Sent { .. } | AgentAwait::Thinking { .. }) => {
                            ui.label("Thinking…");
                        }
                        Some(AgentAwait::Responding { .. }) => {
                            ui.label("Responding…");
                        }
                        Some(AgentAwait::Failed { reason, actions }) => {
                            ui.label(egui::RichText::new(reason).color(palette.sub));
                            ui.horizontal_wrapped(|ui| {
                                for action in actions {
                                    if ui.small_button(recover_label(action)).clicked() {
                                        self.apply_agent_recover(node.id, action);
                                    }
                                }
                            });
                        }
                        None if turns.is_empty() => {
                            ui.label("Start a conversation");
                        }
                        None => {}
                    }
                    if composer {
                        ui.add_space(COMPOSER_GAP * z);
                    }
                });
            self.agents
                .transcript_scroll
                .insert(node.id, scroll.state.offset.y / z);
            let content = scroll.content_size.y / z;
            let signature = self.transcript_signature(node.id, &turns);
            let width = node.rect.w;
            let remeasure = match self.agents.content_heights.get(&node.id) {
                Some(&(w, h, s)) if (w - width).abs() <= 0.5 && s == signature => content > h + 1.0,
                _ => true,
            };
            if remeasure {
                self.agents
                    .content_heights
                    .insert(node.id, (width, content, signature));
                self.agents.measure_epoch = self.agents.measure_epoch.wrapping_add(1);
                ui.ctx().request_repaint();
            }
            if agent.chat.size.is_some() {
                self.agents.card_overflow.insert(
                    node.id,
                    (scroll.content_size.y - scroll.inner_rect.height()).max(0.0) / z,
                );
            }
        }
        if let Some(strip) = key_strip {
            self.paint_agent_key_entry(ui, node.id, strip, z);
        }
        if composer && input.height() > 16.0 * z {
            self.paint_agent_composer(ui, node.id, input, z, false);
        }
    }

    fn paint_agent_key_entry(&mut self, ui: &egui::Ui, id: NodeId, strip: Rect, z: f32) {
        self.agents.key_rect = Some((id, strip));
        let mut key_ui = egui::Ui::new(
            ui.ctx().clone(),
            Id::new(("agent-key", id.0)),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::new(
                    egui::Order::Foreground,
                    Id::new(("agent-key-layer", id.0)),
                ))
                .max_rect(strip),
        );
        key_ui.set_clip_rect(strip.intersect(ui.clip_rect()));
        key_ui.style_mut().visuals = self.palette().visuals();
        key_ui.style_mut().override_font_id = Some(FontId::proportional(14.0 * z));
        let focus_id = Id::new(("agent-key-field", id.0));
        let mut save = false;
        let mut focused = false;
        key_ui.horizontal(|key_ui| {
            let response = key_ui.add(
                egui::TextEdit::singleline(&mut self.agents.key_draft)
                    .id(focus_id)
                    .password(true)
                    .hint_text("Paste API key")
                    .desired_width((strip.width() - 96.0 * z).max(40.0)),
            );
            focused = response.has_focus();
            if self.agents.key_focus == Some(id) {
                response.request_focus();
            }
            if key_ui.button("Save").clicked() {
                save = true;
            }
        });
        if self.agents.key_focus == Some(id) && focused {
            self.agents.key_focus = None;
        }
        if save
            || (focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)))
        {
            self.save_cursor_api_key(Some(id));
        }
    }

    pub(crate) fn paint_agent_composer(
        &mut self,
        ui: &egui::Ui,
        id: NodeId,
        field: Rect,
        z: f32,
        minimal: bool,
    ) {
        if !minimal && self.agent_has_child(id) {
            return;
        }
        self.agents
            .composer_rects
            .insert(id, field.intersect(ui.clip_rect()));
        let take_focus = self.agents.composer_focus == Some(id);
        let editing = self.agents.composer_editing == Some(id) || take_focus;
        if take_focus {
            self.web_release_keyboard();
        }
        let mut text = self.agents.prompt_mut(id).clone();
        let field_out = atlas_shell::selection_tools::prompt_field(
            ui,
            Id::new(("agent-composer", id.0)),
            field,
            z,
            &mut text,
            self.composer_hint(id),
            editing,
            take_focus,
            self.palette(),
        );
        if !editing {
            self.paint_agent_stop(ui, id, field, z);
            return;
        }
        if take_focus && field_out.focused {
            self.agents.composer_focus = None;
        }
        if field_out.focused {
            self.agents.composer_editing = Some(id);
            self.agent_focus(id);
        } else if self.agents.composer_editing == Some(id) {
            self.agents.composer_editing = None;
        }
        if field_out.changed {
            *self.agents.prompt_mut(id) = text;
            self.agents.prompt_epoch = self.agents.prompt_epoch.wrapping_add(1);
        }
        if field_out.submit {
            self.send_agent_prompt(id);
        }
        self.paint_agent_stop(ui, id, field, z);
    }

    fn paint_agent_stop(&mut self, ui: &egui::Ui, id: NodeId, field: Rect, z: f32) {
        if self.agent_connecting(id) {
            self.paint_agent_connecting(ui, field, z);
            return;
        }
        if !self.agent_is_running(id) {
            return;
        }
        ui.ctx().request_repaint();
        let side = canvas_scale::px(11.0, z);
        let stop = Rect::from_min_size(
            Pos2::new(field.right() - side, field.bottom() - side),
            egui::vec2(side, side),
        );
        let resp = ui.interact(stop, Id::new(("agent-stop", id.0)), Sense::click());
        ui.painter().rect_filled(
            stop,
            canvas_scale::px(2.0, z),
            if resp.hovered() {
                Color32::from_rgb(176, 176, 176)
            } else {
                Color32::from_rgb(128, 128, 128)
            },
        );
        paint_agent_spinner(
            ui.painter(),
            Pos2::new(stop.left() - side * 0.9, stop.center().y),
            side * 0.42,
            ui.input(|i| i.time) as f32,
            self.palette().sub,
        );
        if resp.clicked() {
            self.board_sel.clear();
            self.board_sel.insert(id);
            self.stop_selected_agent();
        }
    }

    fn visible_agent_turns(&self, id: NodeId) -> Vec<AgentTurn> {
        let turns = self
            .agents
            .local_turns
            .get(&id)
            .or_else(|| self.agents.sessions.get(&id).map(|s| &s.turns));
        let view = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| &a.chat);
        let start = self
            .doc()
            .scene
            .node(id)
            .and_then(|n| slate_doc::agent_chat::bundle_entry(&self.doc().scene, n))
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.chat.start)
            .unwrap_or(0);
        let end = view.and_then(|v| v.end);
        let Some(v) = turns else {
            return Vec::new();
        };
        let window: Vec<_> = v
            .iter()
            .take(end.unwrap_or(v.len()))
            .skip(start.min(v.len()))
            .cloned()
            .collect();
        window
            .into_iter()
            .map(|mut turn| {
                if turn.role == "user" {
                    turn.text = atlas_ai::agent::display_prompt(&turn.text).into();
                }
                turn
            })
            .collect()
    }

    fn portal_has_new_reply(&self, id: NodeId, req_at: u64) -> bool {
        let local = self
            .agents
            .local_turns
            .get(&id)
            .map(|t| t.as_slice())
            .unwrap_or(&[]);
        let session = self
            .agents
            .sessions
            .get(&id)
            .map(|s| s.turns.as_slice())
            .unwrap_or(&[]);
        await_new_reply(local, req_at) || await_new_reply(session, req_at)
    }

    pub(crate) fn agent_failure_reason(&self, id: NodeId) -> Option<&str> {
        match self.agents.awaiting.get(&id) {
            Some(AgentAwait::Failed { reason, .. }) => Some(reason.as_str()),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn agent_is_awaiting(&self, id: NodeId) -> bool {
        matches!(
            self.agents.awaiting.get(&id),
            Some(
                AgentAwait::Sent { .. }
                    | AgentAwait::Thinking { .. }
                    | AgentAwait::Responding { .. }
            )
        )
    }

    pub(crate) fn open_agent_chat_picker(&mut self, portal: NodeId) {
        let Some(folder) = self.agent_folder_for(portal) else {
            self.toast("Bind a project folder before switching chats.");
            return;
        };
        self.agent_focus(portal);
        let provider = self
            .agent_session_for(portal)
            .map(|v| v.1)
            .unwrap_or_default();
        let cached = chat_key(&folder);
        self.agents
            .chats
            .remove(&(provider.clone(), cached.clone()));
        self.agents.chats_inflight.remove(&cached);
        self.request_agent_chats(folder, &provider);
        self.agents.pending_chat_pick = Some(portal);
    }

    pub(crate) fn dismiss_agent_picker(&mut self) -> bool {
        {
            let artifact = self.agents.artifact_popup.take().is_some();
            self.agents.artifact_popup_rect = None;
            self.agents.chat_picker.take().is_some() || artifact
        }
    }

    #[cfg(test)]
    pub(crate) fn present_agent_picker(&mut self, portal: NodeId, chats: Vec<CursorChat>) {
        self.finish_chat_pick(portal, PathBuf::from("test-agents"), chats);
    }

    #[cfg(test)]
    pub(crate) fn agent_picker_titles(&self) -> Option<Vec<String>> {
        self.agents
            .chat_picker
            .as_ref()
            .map(|p| p.chats.iter().map(|c| c.title.clone()).collect())
    }

    #[cfg(test)]
    pub(crate) fn pick_agent_from_list(&mut self, portal: NodeId, id: &str) -> bool {
        let found = self
            .agents
            .chat_picker
            .as_ref()
            .is_some_and(|p| p.portal == portal && p.chats.iter().any(|c| c.id == id));
        if !found {
            return false;
        }
        self.set_agent_channel(portal, Some(id.to_string()));
        self.agents.chat_picker = None;
        true
    }

    pub(crate) fn answer_agent_approval(&mut self, detail: Option<&str>) {
        let Some((raw, allow)) = detail.and_then(|s| serde_json::from_str::<(u64, bool)>(s).ok())
        else {
            return;
        };
        let node = NodeId(raw);
        let Some(approval) = self.agents.session(node).and_then(|s| s.approval.clone()) else {
            return;
        };
        let Some(ws) = self.ai.config.workspace_dir.as_ref() else {
            return;
        };
        let Some(dir) = self.agent_link_dir(node, ws) else {
            return;
        };
        std::thread::spawn(move || {
            let _ = atlas_ai::agent::atomic_write_json(
                &dir.join("approval.json"),
                &serde_json::json!({"id":approval.id,"allow":allow}),
            );
        });
    }

    pub(crate) fn set_agent_channel(&mut self, portal: NodeId, channel: Option<String>) {
        if self.agent_linear(portal)
            && self.agents.session(portal).is_some_and(|s| {
                !s.conversation.is_empty() && Some(s.conversation.as_str()) != channel.as_deref()
            })
        {
            self.toast(
                "Create a new agent portal for another conversation; this train keeps its history.",
            );
            return;
        }
        if self.agents.connection_rx.is_some() {
            self.toast("Wait for the selected conversation to finish loading.");
            return;
        }
        if self.agent_is_running(portal) {
            self.toast("Stop this response before switching conversations.");
            return;
        }
        let selected = channel.clone();
        let provider = self
            .agent_session_for(portal)
            .map(|(_, provider)| provider)
            .unwrap_or_default();
        if provider == "cursor" {
            if let Some((session, _)) = self.agent_session_for(portal) {
                self.detach_cursor_sidecar(&session);
            }
        }
        self.patch_nodes(&[portal], move |node| {
            if let Some(agent) = slate_doc::agent_chat::agent_mut(node) {
                agent.channel = channel.clone();
            }
        });
        if let Some(channel) = selected {
            #[cfg(not(test))]
            self.load_agent_connection(portal, channel);
            #[cfg(test)]
            let _ = channel;
        } else {
            if provider == "cursor" {
                self.forget_owned_thread(portal, "cursor-agent.txt");
            } else if provider == "codex" {
                if let Some((session, _)) = self.agent_session_for(portal) {
                    self.agents.codex.remove(&session);
                }
                self.forget_owned_thread(portal, "codex-thread.txt");
            }
            self.agents.composer_focus = Some(portal);
        }
    }

    fn load_agent_connection(&mut self, portal: NodeId, channel: String) {
        let Some((session, provider)) = self.agent_session_for(portal) else {
            return;
        };
        let Some(cwd) = self.agent_folder_for(portal) else {
            return;
        };
        let Some(ws) = self.ai.config.workspace_dir.clone() else {
            self.toast("Set an AI workspace to cache this conversation.");
            return;
        };
        let Some(dir) = self.agent_link_dir(portal, &ws) else {
            return;
        };
        if self.agents.connection_rx.is_some() {
            self.toast("A conversation is already loading.");
            return;
        }
        let (tx, rx) = unbounded();
        self.agents.connection_rx = Some(rx);
        self.agents.connection_pending = Some(portal);
        self.agents.connection_background = false;
        self.agents.connection_tick = Some(Instant::now());
        #[cfg(test)]
        {
            let _ = (&provider, &cwd);
            if self.agents.life.hold_loads {
                self.agents.life.loads.push(life::TestLoad {
                    portal,
                    session,
                    channel,
                    dir,
                    tx,
                });
            } else {
                let _ = tx.send((portal, session, Err("No provider runs in tests.".into())));
            }
        }
        #[cfg(not(test))]
        std::thread::spawn(move || {
            let result =
                atlas_ai::runtime::read_conversation(&provider, &cwd, &channel).and_then(|state| {
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    if provider == "codex" {
                        std::fs::write(dir.join("codex-thread.txt"), &channel)
                            .map_err(|e| e.to_string())?;
                    } else if provider == "cursor" {
                        std::fs::write(dir.join("cursor-agent.txt"), &channel)
                            .map_err(|e| e.to_string())?;
                    }
                    atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &state)
                        .map_err(|e| e.to_string())?;
                    Ok(state)
                });
            let _ = tx.send((portal, session, result));
        });
    }

    fn pump_agent_models(&mut self) {
        if let Some((provider, result)) = self
            .agents
            .models_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.agents.models_rx = None;
            match result {
                Ok(models) => {
                    self.agents.models.insert(provider.clone(), models);
                    self.agents.models_error.remove(&provider);
                }
                Err(error) => {
                    self.agents.models_error.insert(provider, error);
                }
            }
            self.agents.fit_revision = None;
        }
        if self.agents.models_rx.is_some() {
            return;
        }
        let provider = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(slate_doc::agent_chat::agent)
            .map(|a| atlas_ai::agent::model_catalog(&a.provider))
            .chain(self.agents.models_wanted.iter().map(String::as_str))
            .find(|p| {
                matches!(
                    *p,
                    "codex" | "ollama" | "cursor" | "comfy" | "openai-image" | "openai-text"
                ) && !self.agents.models_started.contains(*p)
            })
            .map(str::to_owned);
        if let Some(provider) = provider {
            self.agents.models_started.insert(provider.clone());
            let (tx, rx) = unbounded();
            self.agents.models_rx = Some(rx);
            std::thread::spawn(move || {
                let result = match provider.as_str() {
                    "codex" => atlas_ai::runtime::codex_models(),
                    "cursor" => atlas_ai::runtime::cursor_models(),
                    "comfy" => atlas_ai::runtime::comfy_models(),
                    "openai-image" => atlas_ai::runtime::openai_image_models(),
                    "openai-text" => atlas_ai::runtime::openai_text_models(),
                    _ => atlas_ai::runtime::local_models(),
                };
                let _ = tx.send((provider, result));
            });
        }
    }

    pub(crate) fn agent_toggle_live(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        if self
            .agent_session_for(id)
            .is_none_or(|(_, provider)| !atlas_ai::agent::local_image_engine(&provider))
        {
            return false;
        }
        let enable = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| !a.live);
        self.set_generator_live(id, enable);
        true
    }

    /// Live is journaled; its run identity and last input signature are not.
    pub(crate) fn set_generator_live(&mut self, id: NodeId, live: bool) {
        self.patch_nodes(&[id], |n| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                a.live = live;
            }
            // Live shows every new frame, so a picked result lets go.
            if let NodeKind::Image(i) = &mut n.kind {
                if live {
                    i.item = slate_doc::ItemId::NONE;
                }
            }
        });
        self.agents.live_sent.remove(&id);
        if live {
            self.agents
                .live_run
                .insert(id, atlas_ai::agent::request_id());
        } else {
            self.agents.live_run.remove(&id);
            if let Some(model) = self.agents.steering.remove(&id) {
                if self.model3d.live.contains_key(&model) {
                    self.lock_model(model);
                }
            }
        }
    }

    /// The shown live frame stays in the album; the next frame starts a new slot.
    pub(crate) fn agent_keep_live(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        if !self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| atlas_ai::agent::local_image_engine(&a.provider) && a.live)
        {
            self.toast("Keep applies while Live is on.");
            return false;
        }
        self.agents
            .live_run
            .insert(id, atlas_ai::agent::request_id());
        true
    }

    pub(crate) fn agent_set_model(&mut self, value: &str) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        // A sent card records the model that answered it; only the tail chooses.
        if self.agent_is_running(id) || self.agent_has_child(id) {
            return false;
        }
        let Some((_, provider)) = self.agent_session_for(id) else {
            return false;
        };
        let local = provider.starts_with("ollama");
        let key = atlas_ai::agent::model_catalog(&provider);
        // An empty choice is the provider's default (Auto for ComfyUI); a local
        // chat model is always explicit.
        if value.is_empty() && local && !self.is_text_block(id) {
            return false;
        }
        if !value.is_empty()
            && !self
                .agents
                .models
                .get(key)
                .is_some_and(|models| models.iter().any(|m| m.id == value))
        {
            return false;
        }
        let model = (!value.is_empty()).then(|| value.to_owned());
        self.patch_nodes(&[id], |n| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                a.model = model.clone();
                if local {
                    a.provider = "ollama".into();
                }
            }
        });
        true
    }

    fn refresh_agent_connection(&mut self) {
        if self.agents.connection_rx.is_some()
            || self
                .agents
                .connection_tick
                .is_some_and(|t| t.elapsed() < Duration::from_secs(10))
        {
            return;
        }
        let Some(id) = self.selected_agent_portal() else {
            return;
        };
        if !self.agent_linear(id)
            || self.agent_is_running(id)
            || self.agents.project_picker.is_some()
            || self.agents.chat_picker.is_some()
        {
            return;
        }
        let Some(state) = self.agents.session(id) else {
            return;
        };
        if state.conversation.is_empty()
            || (state.provider != "codex" && state.provider != "cursor")
        {
            return;
        }
        let channel = state.conversation.clone();
        if let Some((session, provider)) = self.agent_session_for(id) {
            if provider == "codex" {
                self.agents.codex.remove(&session);
            } else if self.agents.sidecar_child.contains_key(&session)
                || self.agents.sidecar_booting.contains(&session)
            {
                return;
            }
        }
        self.load_agent_connection(id, channel);
        self.agents.connection_background = true;
    }

    fn pump_agent_connection(&mut self, ctx: &egui::Context) {
        let Some(result) = self
            .agents
            .connection_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        else {
            return;
        };
        self.agents.connection_rx = None;
        self.agents.connection_pending = None;
        let background = std::mem::take(&mut self.agents.connection_background);
        let (id, session, result) = result;
        if !self.agent_session_for(id).is_some_and(|v| v.0 == session) {
            return;
        }
        let failure = result.as_ref().err().cloned();
        match result {
            Ok(state) => {
                self.agents.bindings.insert(id, session.clone());
                self.agents.local_turns.remove(&id);
                let changed = self.agents.session(id).is_none_or(|old| {
                    old.turns != state.turns
                        || old.artifacts != state.artifacts
                        || old.status != state.status
                });
                self.agents.sessions.insert(id, std::sync::Arc::new(state));
                if changed {
                    self.agents.output_epoch = self.agents.output_epoch.wrapping_add(1);
                }
                if !background {
                    self.bundle_agent_history(ctx, id);
                }
            }
            Err(_) if background => {}
            Err(error) => self.fail_agent_await(
                id,
                format!("Could not connect to this conversation: {error}"),
            ),
        }
        self.settle_agent_connecting(id, &session, failure);
        ctx.request_repaint();
    }

    fn finish_chat_pick(&mut self, portal: NodeId, path: PathBuf, chats: Vec<CursorChat>) {
        self.agents.pending_chat_pick = None;
        let provider = self
            .agent_session_for(portal)
            .map(|v| v.1)
            .unwrap_or_default();
        self.agents
            .chats
            .insert((provider, path.clone()), chats.clone());
        self.agents.chat_picker = Some(ChatPicker {
            portal,
            folder: path,
            chats,
        });
    }

    /// The shared source resolver owns path semantics; the session manifest owns
    /// the link directory, including after a global AI workspace change.
    fn agent_link_dir(&self, portal: NodeId, workspace: &std::path::Path) -> Option<PathBuf> {
        let agent = slate_doc::agent_chat::agent(self.doc().scene.node(portal)?)?;
        if let Some(uri) = &agent.bundle {
            return resolve_source(self.tab().path.as_deref(), &uri.locator)
                .parent()
                .map(PathBuf::from);
        }
        (!workspace.as_os_str().is_empty())
            .then(|| atlas_ai::agent::agent_dir(workspace, &agent.session))
    }

    fn agent_folder_for(&self, portal: NodeId) -> Option<PathBuf> {
        let node = self.doc().scene.node(portal)?;
        let NodeKind::Portal(p) = &node.kind else {
            return None;
        };
        let loc = p.source.as_ref()?.locator.as_str();
        Some(resolve_source(self.tab().path.as_deref(), loc))
    }

    fn request_agent_chats(&mut self, folder: PathBuf, provider: &str) {
        let folder = chat_key(&folder);
        if self
            .agents
            .chats
            .contains_key(&(provider.into(), folder.clone()))
        {
            return;
        }
        if self.agents.chats_rx.is_some() {
            return;
        }
        if self
            .agents
            .chats_inflight
            .get(&folder)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(8))
        {
            return;
        }
        self.agents
            .chats_inflight
            .insert(folder.clone(), Instant::now());
        let (tx, rx) = unbounded();
        self.agents.chats_rx = Some(rx);
        let provider = provider.to_string();
        std::thread::spawn(move || {
            let chats = atlas_ai::runtime::conversations(&provider, &folder);
            let _ = tx.send((provider, folder, chats));
        });
    }

    fn pump_agent_chats(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.agents.chats_rx else {
            return;
        };
        match rx.try_recv() {
            Ok((provider, path, result)) => {
                self.agents.catalog_error = result.as_ref().err().cloned();
                let chats = result.unwrap_or_default();
                let path = chat_key(&path);
                self.agents.chats_inflight.remove(&path);
                self.agents.chats_rx = None;
                self.agents
                    .chats
                    .insert((provider.clone(), path.clone()), chats.clone());
                if let Some(portal) = self.agents.pending_chat_pick {
                    if self.agent_folder_for(portal).as_ref().map(|p| chat_key(p))
                        == Some(path.clone())
                        && self
                            .agent_session_for(portal)
                            .is_some_and(|v| v.1 == provider)
                    {
                        self.finish_chat_pick(portal, path, chats);
                    } else if let Some(folder) = self.agent_folder_for(portal) {
                        let provider = self
                            .agent_session_for(portal)
                            .map(|v| v.1)
                            .unwrap_or_default();
                        self.request_agent_chats(folder, &provider);
                    }
                }
                ctx.request_repaint();
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.agents.chats_rx = None;
            }
        }
    }

    fn pump_cursor_ide(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.agents.ide_rx {
            match rx.try_recv() {
                Ok(status) => {
                    self.agents.ide = status;
                    self.agents.ide_rx = None;
                    self.agents.ide_inflight = false;
                    self.agents.ide_next = Some(Instant::now() + Duration::from_secs(2));
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.agents.ide_rx = None;
                    self.agents.ide_inflight = false;
                }
            }
        }
        if self.agents.ide_inflight {
            return;
        }
        if self.agents.ide_next.is_some_and(|t| Instant::now() < t) {
            return;
        }
        let (tx, rx) = unbounded();
        self.agents.ide_rx = Some(rx);
        self.agents.ide_inflight = true;
        std::thread::spawn(move || {
            let _ = tx.send(atlas_ai::launch::probe_cursor_ide());
        });
    }
}

fn recover_label(action: &AgentRecover) -> &'static str {
    match action {
        AgentRecover::OpenUrl { label, .. } | AgentRecover::OpenPath { label, .. } => label,
        AgentRecover::PasteKey => "Paste key",
        AgentRecover::PickWorkspace => "Choose workspace",
    }
}

fn classify_agent_failure(reason: String) -> (String, Vec<AgentRecover>) {
    let raw = reason.trim().to_string();
    let lower = raw.to_ascii_lowercase();
    let setup = atlas_ai::sidecar::setup_doc().map(|path| AgentRecover::OpenPath {
        label: "Setup steps",
        path,
    });
    if lower.contains("codex") {
        (
            raw,
            vec![AgentRecover::OpenUrl {
                label: "Codex setup",
                url: "https://learn.chatgpt.com/docs/auth",
            }],
        )
    } else if lower.contains("api key") || lower.contains("cursor_api_key") {
        let mut actions = vec![
            AgentRecover::OpenUrl {
                label: "Get a key",
                url: atlas_ai::cursor_key::DASHBOARD_URL,
            },
            AgentRecover::PasteKey,
        ];
        if let Some(setup) = setup {
            actions.push(setup);
        }
        (
            "Cursor needs an API key to reach an agent. Get one, then paste it here — you do not have to set a system environment variable.".into(),
            actions,
        )
    } else if lower.contains("workspace") {
        (
            "Choose an AI workspace folder so this portal has a place to write the agent link."
                .into(),
            vec![AgentRecover::PickWorkspace],
        )
    } else if looks_like_missing_node(&lower) {
        let mut actions = vec![AgentRecover::OpenUrl {
            label: "Download Node.js",
            url: "https://nodejs.org/en/download",
        }];
        if let Some(setup) = setup {
            actions.push(setup);
        }
        let text = if raw.contains("Looked in:") {
            raw
        } else {
            "Slate could not see node.exe. It looks in Program Files and on PATH. \
Set ATLAS_NODE to your node.exe if it is installed somewhere else."
                .into()
        };
        (text, actions)
    } else {
        let mut actions = Vec::new();
        if let Some(setup) = setup {
            actions.push(setup);
        }
        (raw, actions)
    }
}

fn looks_like_missing_node(lower: &str) -> bool {
    if lower.contains("npm") {
        return false;
    }
    lower.contains("could not see node")
        || lower.contains("looked in:")
        || lower.contains("node.js was not found")
        || lower.contains("node was not found")
        || (lower.contains("atlas_node") && lower.contains("not"))
}

fn await_new_reply(turns: &[AgentTurn], req_at: u64) -> bool {
    turns
        .iter()
        .any(|t| t.role == "assistant" && t.at >= req_at)
}

/// What the model menu calls a portal's unset model.
fn model_default(agent: &slate_doc::scene::AgentPortalRef) -> &'static str {
    if agent.view == atlas_ai::agent::PortalView::Text {
        "Auto"
    } else {
        model_fallback(&agent.provider)
    }
}

fn model_fallback(provider: &str) -> &'static str {
    if provider == "cursor" {
        "Auto"
    } else if provider.starts_with("ollama") {
        "Choose model"
    } else if atlas_ai::agent::local_image_engine(provider) {
        "Auto"
    } else {
        "Default"
    }
}

fn agent_status_chip(
    provider: &str,
    ide: CursorIdeStatus,
    sidecar: Option<&AgentStatus>,
    awaiting: Option<&AgentAwait>,
) -> (Color32, &'static str, bool) {
    match awaiting {
        Some(AgentAwait::Failed { .. }) => {
            return (Color32::from_rgb(230, 90, 90), "Unreachable", false);
        }
        Some(AgentAwait::Responding { .. }) => {
            return (Color32::from_rgb(61, 156, 245), "Responding", true);
        }
        Some(AgentAwait::Sent { .. } | AgentAwait::Thinking { .. }) => {
            return (Color32::from_rgb(61, 156, 245), "Thinking", true);
        }
        None => {}
    }
    let (color, label) = agent_live_chip(provider, ide, sidecar);
    let pulse = matches!(sidecar, Some(AgentStatus::Thinking));
    (color, label, pulse)
}

fn agent_live_chip(
    provider: &str,
    ide: CursorIdeStatus,
    sidecar: Option<&AgentStatus>,
) -> (Color32, &'static str) {
    let _ = (provider, ide);
    match sidecar {
        Some(AgentStatus::Thinking) => (Color32::from_rgb(61, 156, 245), "Working"),
        Some(AgentStatus::Idle) => (Color32::from_rgb(160, 168, 180), "Ready"),
        Some(AgentStatus::Error(_)) => (Color32::from_rgb(230, 90, 90), "Error"),
        _ => (Color32::from_rgb(120, 128, 140), "Not connected"),
    }
}

fn chat_key(path: &std::path::Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

fn collect_agent_project_recents(provider: &str) -> Vec<RecentEntry> {
    let mut out: Vec<RecentEntry> = Vec::new();
    let projects = if provider == "codex" {
        atlas_ai::runtime::codex_projects()
    } else {
        atlas_ai::cursor_recents::discover()
            .into_iter()
            .map(|p| (String::new(), p))
            .collect()
    };
    for (name, path) in projects {
        if out.iter().any(|e| paths_same(&e.path, &path)) {
            continue;
        }
        let title = if !name.is_empty() {
            name
        } else {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Project")
                .to_string()
        };
        out.push(RecentEntry {
            path,
            title,
            opened_at: 0,
            cover: None,
        });
    }
    out
}

fn paths_same(a: &std::path::Path, b: &std::path::Path) -> bool {
    let ac = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let bc = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    #[cfg(windows)]
    {
        ac.to_string_lossy()
            .eq_ignore_ascii_case(&bc.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        ac == bc
    }
}

fn fit_frame_height(width: f32, image_w: u32, image_h: u32) -> f32 {
    let aspect = image_h as f32 / image_w.max(1) as f32;
    (width * aspect).clamp(160.0, 1400.0)
}

/// What [`SlateApp::program_binding`] resolved for one bind.
pub(crate) struct ProgramBinding {
    session: String,
    bundle: Option<slate_doc::SourceUri>,
    locator: Option<String>,
}

/// The size a freshly bound program's card takes.
pub(crate) fn program_card_size(view: atlas_ai::agent::PortalView) -> egui::Vec2 {
    use atlas_ai::agent::PortalView;
    match view {
        PortalView::Chat => egui::vec2(slate_doc::agent_chat::CARD_WIDTH, 540.0),
        PortalView::Images => egui::vec2(960.0, 540.0),
        PortalView::Text => egui::vec2(440.0, 320.0),
    }
}

/// Bind a program onto an agent portal: provider identity, a fresh session,
/// the view and its card size.
pub(crate) fn bind_program(
    node: &mut Node,
    program: &atlas_ai::agent::AgentProvider,
    binding: &ProgramBinding,
) {
    let Some(a) = slate_doc::agent_chat::agent_mut(node) else {
        return;
    };
    a.provider = program.id.clone();
    a.chat.linear = atlas_ai::runtime::linear_provider(&program.id);
    a.session = binding.session.clone();
    a.channel = None;
    a.bundle = binding.bundle.clone();
    a.seed = None;
    a.view = program.view;
    if program.view == atlas_ai::agent::PortalView::Chat {
        a.chat.train = true;
        a.chat.detail = slate_doc::agent_chat::Detail::Full;
    }
    let size = program_card_size(program.view);
    node.rect.w = size.x;
    node.rect.h = size.y;
    if let NodeKind::Portal(p) = &mut node.kind {
        p.title = program.display_name.clone();
        if let Some(locator) = &binding.locator {
            p.source = Some(slate_doc::SourceUri {
                locator: locator.clone(),
            });
        }
    }
    // An image or text engine makes media, not a chat card.
    slate_doc::scene::agent_card_as_media(node);
}

#[cfg(test)]
mod agent_await_tests {
    use super::*;

    #[test]
    fn generator_frame_follows_image_aspect() {
        assert_eq!(fit_frame_height(960.0, 512, 512), 960.0);
        assert_eq!(fit_frame_height(960.0, 512, 768), 1400.0);
        assert_eq!(fit_frame_height(800.0, 1024, 512), 400.0);
    }

    #[test]
    fn focused_agent_keeps_canvas_zoom_and_title_rename() {
        for z in [0.5, 1.0, 2.0] {
            let mut h = super::super::tests::Harness::new("agent_header_zoom");
            h.app.leave_home();
            h.app.ensure_work_tab();
            h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
            h.app.place_agent_portal_at(Pos2::ZERO);
            let id = h.app.doc().scene.nodes[0].id;
            h.app.set_agent_program(id, "ollama");
            h.app.agents.models_started.insert("ollama".into());
            h.app.patch_nodes(&[id], |n| {
                if let NodeKind::Portal(p) = &mut n.kind {
                    p.title = "Review".into();
                    p.agent.as_mut().unwrap().model = Some("llama3.1:8b".into());
                }
            });
            h.frame();
            let r = h.app.doc().scene.node(id).unwrap().rect;
            h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
            h.app.tab_mut().cam.z = z;
            h.frame();
            h.frame();
            let r = h
                .app
                .board_xf()
                .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
            let point = r.center();
            h.frame_with(|i| i.events.push(egui::Event::PointerMoved(point)));
            h.frame_with(|i| {
                i.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 60.0),
                    modifiers: Default::default(),
                })
            });
            assert!(h.app.tab().cam.z > z, "focused agent swallowed zoom at {z}");
            let z = h.app.tab().cam.z;
            let r = h
                .app
                .board_xf()
                .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
            let point = r.min + egui::vec2(30.0, 15.0) * z;
            h.frame_with(|i| i.events.push(egui::Event::PointerMoved(point)));
            for pressed in [true, false] {
                h.frame_with(|i| {
                    i.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    })
                });
            }
            assert!(
                h.app.agents.title_edit.is_some(),
                "single title click at {z}"
            );
            assert!(h.app.board_drag.is_none());
        }
    }

    #[test]
    fn provider_tiles_accept_first_click_near_their_edges() {
        for (z, offset) in [0.5, 1.0, 2.0].into_iter().flat_map(|z| {
            [(-47.0, -39.0), (47.0, -39.0), (-47.0, 39.0), (47.0, 39.0)]
                .into_iter()
                .map(move |offset| (z, offset))
        }) {
            let mut h = super::super::tests::Harness::new("picker_edge");
            h.app.leave_home();
            h.app.ensure_work_tab();
            h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
            h.app.place_agent_portal_at(Pos2::ZERO);
            let id = h.app.doc().scene.nodes[0].id;
            h.app.agents.programs_started = true;
            h.app.agents.programs = vec![atlas_ai::agent::provider_by_id("local")];
            h.app.agents.focused = None;
            h.app.portals.contents = None;
            let r = h.app.doc().scene.node(id).unwrap().rect;
            h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
            h.app.tab_mut().cam.z = z;
            h.frame();
            h.frame();
            let rect = h
                .app
                .board_xf()
                .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
            let point = rect.center() + egui::vec2(offset.0, offset.1) * z;
            h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
            for pressed in [true, false] {
                h.frame_with(|input| {
                    input.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    })
                });
            }
            assert_eq!(
                h.app.agent_session_for(id).unwrap().1,
                "local",
                "tile edge at {z}"
            );
        }
    }

    #[test]
    fn picker_exit_and_streaming_text_remeasure_cards() {
        let mut h = super::super::tests::Harness::new("picker_fit");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "local");
        h.app.agents.project_picker = Some(id);
        h.frame();
        let picker_h = h.app.doc().scene.node(id).unwrap().rect.h;
        h.app.agents.project_picker = None;
        h.frame();
        assert!(h.app.doc().scene.node(id).unwrap().rect.h < picker_h);
        h.app.patch_nodes(&[id], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.chat.train = true;
                a.chat.detail = slate_doc::agent_chat::Detail::Summary;
                a.bundle = None;
            }
        });
        h.app.agents.local_turns.insert(
            id,
            vec![AgentTurn {
                role: "assistant".into(),
                text: "Short reply".into(),
                at: 0,
            }],
        );
        h.app.agents.output_epoch += 1;
        h.frame();
        let short = h.app.doc().scene.node(id).unwrap().rect.h;
        h.app.agents.local_turns.get_mut(&id).unwrap()[0].text =
            "A response that grows while streaming. ".repeat(30);
        h.app.agents.output_epoch += 1;
        h.frame();
        assert!(h.app.doc().scene.node(id).unwrap().rect.h > short * 2.0);
    }

    #[test]
    fn local_dropdown_changes_model_without_replacing_conversation() {
        let mut h = super::super::tests::Harness::new("local_model");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "ollama/old-model");
        h.app.board_sel = [id].into_iter().collect();
        let session = h.app.agent_session_for(id).unwrap().0;
        h.app.agents.models.insert(
            "ollama".into(),
            vec![atlas_ai::agent::AgentModel {
                id: "local-model".into(),
                name: "local-model".into(),
            }],
        );
        assert!(h.app.agent_set_model("local-model"));
        let a = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
        assert_eq!(a.provider, "ollama");
        assert_eq!(a.model.as_deref(), Some("local-model"));
        assert_eq!(a.session, session);
        assert!(!h.app.agent_set_model("not-installed"));
    }

    #[test]
    fn comfy_checkpoint_choice_keeps_the_image_portal() {
        let mut h = super::super::tests::Harness::new("comfy_model");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "comfy");
        let rect = h.app.doc().scene.node(id).unwrap().rect;
        assert_eq!(rect.w, 960.0);
        assert_eq!(rect.h, 540.0);
        h.app.board_sel = [id].into_iter().collect();
        h.app.agents.models.insert(
            "comfy".into(),
            vec![atlas_ai::agent::AgentModel {
                id: "sd_xl_base_1.0.safetensors".into(),
                name: "sd_xl_base_1.0.safetensors".into(),
            }],
        );
        assert!(h.app.agent_set_model("sd_xl_base_1.0.safetensors"));
        assert!(!h.app.agent_set_model("not-installed.safetensors"));
        let a = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
        assert_eq!(a.provider, "comfy");
        assert_eq!(a.view, atlas_ai::agent::PortalView::Images);
        assert_eq!(a.model.as_deref(), Some("sd_xl_base_1.0.safetensors"));
        // An empty choice returns the generator to Auto.
        assert!(h.app.agent_set_model(""));
        let a = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
        assert!(a.model.is_none());
    }

    /// A ComfyUI generator with a sticky note wired to an edge that is not the
    /// left midpoint, and a workspace to write its link folder into.
    fn generator_with_note(
        tag: &str,
        note: &str,
    ) -> (super::super::tests::Harness, NodeId, NodeId) {
        use slate_doc::scene::{ConnectorEnd, Side};
        let mut h = super::super::tests::Harness::new(tag);
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.ai.config.workspace_dir = Some(h.base.clone());
        h.app.place_agent_portal_at(Pos2::new(600.0, 0.0));
        let portal = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(portal, "comfy");
        let sticky = h.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(-400.0, 0.0, 220.0, 220.0),
            NodeKind::Text(slate_doc::scene::TextNode {
                text: note.into(),
                family: slate_doc::scene::Typeface::Sans,
                size: 24.0,
                color: slate_doc::scene::Rgba::opaque(20, 20, 20),
                align: slate_doc::scene::TextAlign::Left,
                fill: Some(super::super::board_color::STICKY_FILL),
                agent: None,
            }),
        );
        let note_id = sticky.id;
        let wire = h.app.build_connector(
            ConnectorEnd::Anchored {
                node: note_id,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: portal,
                side: Side::Top,
                t: 0.3,
            },
        );
        h.app.add_nodes(vec![sticky, wire]);
        h.app.board_sel = [portal].into_iter().collect();
        (h, portal, note_id)
    }

    #[test]
    fn live_frames_use_the_wired_note_and_one_seed() {
        let note = "a nice public park setting with happy people";
        let (mut h, id, note_id) = generator_with_note("gen_prompt", note);
        let view = h.app.generator_view(id);
        assert_eq!(view.inputs.len(), 1);
        assert_eq!(view.inputs[0].role, InputRole::Prompt);
        assert_eq!(view.inputs[0].node, note_id);
        assert_eq!(view.task().action(), "Generate");
        assert!(view.error.is_none());

        h.app.set_generator_live(id, true);
        let first = h.app.generation_request(id, true).unwrap();
        let second = h.app.generation_request(id, true).unwrap();
        assert_eq!(
            first.prompt, note,
            "the note is the prompt, never a stand-in"
        );
        let (a, b) = (first.image.unwrap(), second.image.unwrap());
        assert_eq!(a.seed, b.seed, "live frames repeat one seed");
        assert_eq!(a.live, b.live);
        assert!(a.live.is_some());
        let still = h.app.generation_request(id, false).unwrap();
        let still = still.image.unwrap();
        assert!(still.seed.is_none(), "a pressed Render varies its seed");
        assert!(still.live.is_none());
    }

    #[test]
    fn rapid_presses_wait_their_turn_and_stop_clears_them() {
        let (mut h, id, _) = generator_with_note("gen_queue", "timber facade");
        h.app.queue_generation(id);
        h.app.queue_generation(id);
        h.app.queue_generation(id);
        assert_eq!(h.app.agents.dispatched.len(), 1, "one run at a time");
        assert_eq!(h.app.generations_waiting(id), 2);
        assert_eq!(
            h.app.agents.requests.get(&id),
            Some(&h.app.agents.dispatched[0].1.id),
            "the running request is tracked, not the newest press"
        );
        h.app.agents.awaiting.remove(&id);
        h.app.pump_comfy_queue();
        assert_eq!(h.app.agents.dispatched.len(), 2);
        assert_eq!(h.app.generations_waiting(id), 1);
        assert_ne!(
            h.app.agents.dispatched[0].1.id,
            h.app.agents.dispatched[1].1.id
        );
        assert!(h.app.stop_selected_agent());
        assert_eq!(h.app.generations_waiting(id), 0);
    }

    #[test]
    fn keep_starts_a_new_slot_and_stop_ends_live() {
        let (mut h, id, _) = generator_with_note("gen_live", "watercolor");
        assert!(!h.app.agent_keep_live(), "Keep applies only while live");
        assert!(h.app.agent_toggle_live());
        let live = |h: &super::super::tests::Harness| {
            slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .live
        };
        assert!(live(&h));
        let run = h.app.agents.live_run.get(&id).cloned().unwrap();
        assert!(h.app.agent_keep_live());
        assert_ne!(h.app.agents.live_run.get(&id), Some(&run));
        assert!(h.app.stop_selected_agent());
        assert!(!live(&h), "Stop also ends live rendering");
        assert!(!h.app.agents.live_run.contains_key(&id));
    }

    fn wire_model(h: &mut super::super::tests::Harness, generator: NodeId) -> NodeId {
        use slate_doc::scene::{ConnectorEnd, Side};
        let path = h.base.join("pavilion.3dm");
        std::fs::write(&path, b"not a real model").unwrap();
        let item = h
            .app
            .doc_mut()
            .add_item(path, "pavilion.3dm", 16, 0, "pavilion-key");
        let model = h.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(-400.0, 400.0, 400.0, 300.0),
            NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
        );
        let model_id = model.id;
        let wire = h.app.build_connector(
            ConnectorEnd::Anchored {
                node: model_id,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: generator,
                side: Side::Bottom,
                t: 0.4,
            },
        );
        h.app.add_nodes(vec![model, wire]);
        model_id
    }

    /// Every string one real frame paints, with the pointer at `pointer`.
    fn painted_text(h: &mut super::super::tests::Harness, pointer: Option<Pos2>) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
            ..Default::default()
        };
        input.events.push(egui::Event::PointerMoved(
            pointer.unwrap_or(Pos2::new(2.0, 890.0)),
        ));
        let ctx = h.ctx.clone();
        let app = &mut h.app;
        let output = ctx.run(input, |c| app.update_app(c));
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut texts);
        }
        texts
    }

    #[test]
    fn hovering_a_generator_shows_what_it_reads_and_can_do() {
        let (mut h, id, _) = generator_with_note("gen_hover", "a nice public park");
        wire_model(&mut h, id);
        h.frame();
        let r = h.app.doc().scene.node(id).unwrap().rect;
        h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
        h.app.tab_mut().cam.z = 1.0;
        h.frame();
        let body = h.app.board_xf().rect_w2s(r);
        // Choosing a program focuses the portal, which reveals its controls.
        h.app.agent_blur();
        h.app.board_sel.clear();
        let rest = painted_text(&mut h, None);
        assert!(
            !rest
                .iter()
                .any(|t| t.starts_with("Prompt ·") || t == "Render"),
            "quiet at rest: {rest:?}"
        );
        assert!(
            !rest.iter().any(|t| t.contains("Connect")),
            "a wired generator asks for nothing: {rest:?}"
        );
        let hover = painted_text(&mut h, Some(body.center()));
        for expected in [
            "Prompt · a nice public park",
            "Geometry · pavilion.3dm",
            "Render",
        ] {
            assert!(
                hover.iter().any(|t| t == expected),
                "missing {expected:?} in {hover:?}"
            );
        }
        // Model and Live moved to the Agent squircle.
        assert!(
            !hover.iter().any(|t| t == "ComfyUI · Auto" || t == "Live"),
            "no model chip or Live toggle on the picture: {hover:?}"
        );

        // Live swaps the primary action for Keep.
        h.app.set_generator_live(id, true);
        let live = painted_text(&mut h, Some(body.center()));
        assert!(live.iter().any(|t| t == "Keep"), "{live:?}");
    }

    #[test]
    fn live_follows_a_prompt_while_it_is_typed() {
        let (mut h, id, note) = generator_with_note("gen_typing", "morning light");
        h.app.set_generator_live(id, true);
        h.app.pump_live_generators();
        h.app.agents.awaiting.remove(&id);
        h.app.text_edit = Some((note, "storm clouds".into()));
        h.app.pump_live_generators();
        assert_eq!(h.app.agents.dispatched.len(), 1, "typing waits for a pause");
        if let Some((_, at)) = h.app.agents.live_settle.get_mut(&id) {
            *at -= LIVE_TYPING_SETTLE;
        }
        h.app.pump_live_generators();
        assert_eq!(
            h.app.agents.dispatched.len(),
            2,
            "a pause renders the draft"
        );
        assert_eq!(h.app.agents.dispatched[1].1.prompt, "storm clouds");
    }

    #[test]
    fn live_rerenders_when_a_wired_input_changes() {
        let (mut h, id, note) = generator_with_note("gen_stream", "morning light");
        h.app.set_generator_live(id, true);
        h.app.pump_live_generators();
        assert_eq!(h.app.agents.dispatched.len(), 1, "live renders at once");
        assert_eq!(h.app.agents.dispatched[0].1.prompt, "morning light");
        h.app.agents.awaiting.remove(&id);
        h.app.pump_live_generators();
        assert_eq!(
            h.app.agents.dispatched.len(),
            1,
            "unchanged inputs do not rerun"
        );
        h.app.patch_nodes(&[note], |n| {
            if let NodeKind::Text(t) = &mut n.kind {
                t.text = "evening light".into();
            }
        });
        h.app.pump_live_generators();
        assert_eq!(h.app.agents.dispatched.len(), 2, "an edited prompt reruns");
        assert_eq!(h.app.agents.dispatched[1].1.prompt, "evening light");
        let (a, b) = (&h.app.agents.dispatched[0].1, &h.app.agents.dispatched[1].1);
        assert_eq!(
            a.image.as_ref().and_then(|i| i.seed),
            b.image.as_ref().and_then(|i| i.seed)
        );

        // A camera move on a wired model changes the signature live watches.
        let model = wire_model(&mut h, id);
        let before = h.app.model_pose_hash(model).unwrap();
        h.app.patch_nodes(&[model], |n| {
            if let NodeKind::Image(img) = &mut n.kind {
                img.model.yaw += 0.4;
            }
        });
        assert_ne!(h.app.model_pose_hash(model), Some(before));
        h.app.agents.awaiting.remove(&id);
        assert!(!h.app.pointer_over_image_album(&h.app.board_xf(), None));
    }

    #[test]
    fn a_wired_model_renders_and_names_a_missing_gpu() {
        let (mut h, id, _) = generator_with_note("gen_model", "night, warm lights");
        let model_id = wire_model(&mut h, id);
        let view = h.app.generator_view(id);
        assert_eq!(view.task().action(), "Render");
        assert_eq!(view.geometry(), Some(model_id));
        assert!(view.inputs.iter().any(|i| i.role == InputRole::Prompt));
        h.app.queue_generation(id);
        assert!(
            h.app.agents.dispatched.is_empty(),
            "no run starts without a captured view"
        );
        assert!(matches!(
            h.app.agents.awaiting.get(&id),
            Some(AgentAwait::Failed { .. })
        ));
    }

    #[test]
    fn cursor_dropdown_journals_a_model_without_replacing_the_session() {
        let mut h = super::super::tests::Harness::new("cursor_model");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "cursor");
        h.app.board_sel = [id].into_iter().collect();
        let session = h.app.agent_session_for(id).unwrap().0;
        h.app.agents.models.insert(
            "cursor".into(),
            vec![atlas_ai::agent::AgentModel {
                id: "composer-2.5".into(),
                name: "Composer 2.5".into(),
            }],
        );
        assert!(h.app.agent_set_model("composer-2.5"));
        let a = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
        assert_eq!(a.provider, "cursor");
        assert_eq!(a.model.as_deref(), Some("composer-2.5"));
        assert_eq!(a.session, session);
        assert!(h.app.agent_set_model(""));
        assert!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .model
                .is_none()
        );
    }

    #[test]
    fn full_access_is_a_local_grant_per_conversation_and_never_journaled() {
        let mut h = super::super::tests::Harness::new("agent_full_access");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "cursor");
        h.app.board_sel = [id].into_iter().collect();
        let session = h.app.agent_session_for(id).unwrap().0;
        let scene = h.app.doc().scene.clone();
        let store = std::env::temp_dir().join(format!(
            "slate_full_access_{}_{}.json",
            std::process::id(),
            session
        ));
        h.app.agents.access_path = Some(store.clone());
        assert!(!h.app.agent_full_access(&session));
        assert!(h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId("portal.agent.full_access"),
            None
        ));
        assert!(h.app.agent_full_access(&session));
        assert!(atlas_ai::access::granted_in(&store, &session));
        assert_eq!(
            h.app.doc().scene,
            scene,
            "the grant never enters the workbook"
        );
        assert!(h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId("portal.agent.full_access"),
            None
        ));
        assert!(!h.app.agent_full_access(&session));
        assert!(!atlas_ai::access::granted_in(&store, &session));
        let _ = std::fs::remove_file(store);
    }

    #[test]
    fn background_refresh_keeps_history_and_model_choice_is_journaled() {
        let mut h = super::super::tests::Harness::new("agent_refresh");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "codex");
        h.app.agents.project_picker = None;
        h.app.agents.models_started.insert("codex".into());
        h.app.agents.models.insert(
            "codex".into(),
            vec![atlas_ai::agent::AgentModel {
                id: "gpt-6-astra".into(),
                name: "Astra".into(),
            }],
        );
        h.app.board_sel = [id].into_iter().collect();
        assert!(h.app.agent_set_model("gpt-6-astra"));
        assert_eq!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .model
                .as_deref(),
            Some("gpt-6-astra")
        );
        let state:AgentSession=serde_json::from_value(serde_json::json!({"conversation":"test","provider":"codex","turns":[{"role":"assistant","text":"Keep this answer","at":0}]})).unwrap();
        h.app
            .agents
            .sessions
            .insert(id, std::sync::Arc::new(state.clone()));
        let session = h.app.agent_session_for(id).unwrap().0;
        for result in [Ok(state), Err("offline".into())] {
            let (tx, rx) = unbounded();
            tx.send((id, session.clone(), result)).unwrap();
            h.app.agents.connection_rx = Some(rx);
            h.app.agents.connection_pending = Some(id);
            h.app.agents.connection_background = true;
            let epoch = h.app.agents.output_epoch;
            h.app.pump_agent_connection(&h.ctx);
            assert_eq!(h.app.visible_agent_turns(id)[0].text, "Keep this answer");
            assert_eq!(h.app.agents.output_epoch, epoch);
            assert!(h.app.agents.awaiting.is_empty());
        }
        let r = h.app.doc().scene.node(id).unwrap().rect;
        let xf = h.app.board_xf();
        let mid = xf.w2s(Pos2::new(r.x, r.y + r.h * 0.5));
        let right = xf.w2s(Pos2::new(r.x + r.w, r.y + r.h * 0.5));
        assert_eq!(
            h.app.agent_artifact_at(mid, &xf),
            None,
            "a train card with no referenced files has no left input dot"
        );
        assert_eq!(
            h.app.agent_artifact_at(right, &xf),
            None,
            "an untouched train card has no edited-document handle"
        );
        h.app.agents.project_picker = Some(id);
        assert_eq!(h.app.agent_artifact_at(mid, &xf), None);
        h.app.agents.project_picker = None;
        assert_eq!(h.app.agent_output_at(mid, &xf), None);
        let snapshot = h.app.agent_input_snapshot(id).unwrap();
        assert!(snapshot.context.is_empty());
        assert!(snapshot.wired.is_empty());
    }

    #[test]
    fn coding_train_keeps_identity_and_artifacts_open_only_on_request() {
        let mut h = super::super::tests::Harness::new("coding_artifacts");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let root = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(root, "codex");
        h.app.agents.project_picker = None;
        h.app
            .set_agent_channel(root, Some("provider-conversation".into()));
        h.app.patch_nodes(&[root], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.chat.train = true;
                a.chat.linear = false; // Older workbooks did not persist the coding-stream flag.
                a.chat.detail = slate_doc::agent_chat::Detail::Full;
                a.bundle = None;
            }
        });
        h.app.board_sel = [root].into_iter().collect();
        let session = h.app.agent_session_for(root).unwrap();
        let before = h.app.doc().scene.nodes.len();
        assert!(h.app.agent_spawn_command(Some("[400,0]")));
        assert_eq!(h.app.doc().scene.nodes.len(), before + 1);
        let draft = *h.app.board_sel.iter().next().unwrap();
        assert_eq!(h.app.agent_session_for(draft).unwrap(), session);
        h.app.board_sel = [root].into_iter().collect();
        assert!(
            !h.app.agent_spawn_command(Some("[400,200]")),
            "coding handles cannot fork"
        );
        let (reply, replay) = h
            .app
            .prepare_agent_train_send(draft, std::path::Path::new("test-workspace"))
            .unwrap();
        assert!(replay.is_empty());
        assert_eq!(h.app.agent_session_for(reply).unwrap(), session);
        assert_eq!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(reply).unwrap())
                .unwrap()
                .channel
                .as_deref(),
            Some("provider-conversation")
        );
        assert!(h
            .app
            .prepare_agent_train_send(root, std::path::Path::new("test-workspace"))
            .is_none());
        let state:AgentSession=serde_json::from_value(serde_json::json!({"provider":"codex","conversation":"provider-conversation","artifacts":[
            {"id":"read","turn":1,"kind":"web","source":"https://example.com/reference","title":"Reference"},
            {"id":"edit","turn":1,"kind":"modified","source":"C:/fixture/main.rs","title":"main.rs"}
        ]})).unwrap();
        h.app
            .agents
            .sessions
            .insert(reply, std::sync::Arc::new(state));
        assert_eq!(h.app.agent_artifacts(reply, false).len(), 1);
        assert_eq!(h.app.agent_artifacts(reply, true).len(), 1);
        let before = h.app.doc().scene.nodes.len();
        h.app
            .toggle_agent_artifacts(Some(&format!("[{},true]", reply.0)));
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            before,
            "hover/list does not spawn portals"
        );
        assert!(h
            .app
            .open_agent_artifact(Some(&serde_json::to_string(&(reply.0, "edit")).unwrap())));
        assert_eq!(h.app.doc().scene.nodes.len(), before + 2);
        let atlas = &h.app.doc().scene.nodes[before];
        let NodeKind::Portal(p) = &atlas.kind else {
            panic!("expected atlas portal")
        };
        assert_eq!(p.kind, PortalKind::FileAtlas);
        assert_eq!(p.atlas.files, vec!["main.rs"]);
        let NodeKind::Connector(wire) = &h.app.doc().scene.nodes[before + 1].kind else {
            panic!("expected wire")
        };
        assert!(
            wire.binding.is_none(),
            "provenance is not implicit model input"
        );
    }

    #[test]
    fn agent_nested_bundles_restore_one_level_with_horizontal_spacing() {
        let mut h = super::super::tests::Harness::new("nested_chat_bundle");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        let mut ids = Vec::new();
        for i in 0..4 {
            let mut portal = PortalNode::unbound_agent("Nested", "codex");
            let a = portal.agent.as_mut().unwrap();
            a.chat.train = true;
            a.chat.start = i;
            a.chat.end = Some(i + 1);
            a.chat.parent = ids.last().copied();
            let n = h.app.doc_mut().scene.build_node(
                slate_doc::WorldRect::new(0.0, 0.0, 200.0, 100.0),
                NodeKind::Portal(portal),
            );
            ids.push(n.id);
            h.app.add_nodes(vec![n]);
        }
        for pair in [&ids[..2], &ids[2..]] {
            h.app.board_sel = pair.iter().copied().collect();
            assert!(h.app.agent_bundle_selection());
        }
        h.app.board_sel = [ids[1], ids[3]].into_iter().collect();
        assert!(h.app.agent_bundle_selection());
        assert_eq!(
            h.app.doc().scene.nodes.iter().filter(|n| !n.hidden).count(),
            1
        );
        let encoded = serde_json::to_string(&h.app.doc().scene).unwrap();
        let decoded: slate_doc::scene::Scene = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, h.app.doc().scene);
        assert!(h.app.agent_expand_bundle());
        for i in [1, 3] {
            let n = h.app.doc().scene.node(ids[i]).unwrap();
            assert!(!n.hidden);
            assert_eq!(
                slate_doc::agent_chat::agent(n).unwrap().chat.bundled,
                vec![ids[i - 1]]
            );
            assert!(h.app.doc().scene.node(ids[i - 1]).unwrap().hidden);
        }
        let left = h.app.doc().scene.node(ids[1]).unwrap().rect;
        let right = h.app.doc().scene.node(ids[3]).unwrap().rect;
        assert!(right.x >= left.x + left.w + slate_doc::agent_chat::CARD_GAP);
        assert!(h.app.agent_expand_bundle());
        let left = h.app.doc().scene.node(ids[2]).unwrap().rect;
        let right = h.app.doc().scene.node(ids[3]).unwrap().rect;
        assert!(right.x >= left.x + left.w + slate_doc::agent_chat::CARD_GAP);
        assert!(h.app.doc().scene.node(ids[0]).unwrap().hidden);
    }

    #[test]
    fn agent_full_window_output_creates_two_branches_and_sends_in_place() {
        for train in [false, true] {
            let mut h = super::super::tests::Harness::new("full_chat_fork");
            h.app.leave_home();
            h.app.ensure_work_tab();
            h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
            h.app.place_agent_portal_at(Pos2::ZERO);
            let root = h.app.doc().scene.nodes[0].id;
            h.app.set_agent_program(root, "local");
            h.app.patch_nodes(&[root], |n| {
                if let NodeKind::Portal(p) = &mut n.kind {
                    let a = p.agent.as_mut().unwrap();
                    a.bundle = None;
                    a.chat.train = train;
                    a.chat.detail = slate_doc::agent_chat::Detail::Full;
                }
            });
            h.app.board_sel = [root].into_iter().collect();
            assert!(h.app.agent_spawn_command(Some("[400,0]")));
            let children: Vec<_> = h
                .app
                .doc()
                .scene
                .nodes
                .iter()
                .filter(|n| {
                    slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(root))
                })
                .map(|n| n.id)
                .collect();
            assert_eq!(children.len(), 2);
            let a = h.app.doc().scene.node(children[0]).unwrap().rect;
            let b = h.app.doc().scene.node(children[1]).unwrap().rect;
            assert!(b.y >= a.y + a.h);
            let count = h.app.doc().scene.nodes.len();
            let (recipient, history) = h
                .app
                .prepare_agent_train_send(children[0], std::path::Path::new("C:/sample"))
                .unwrap();
            assert_eq!(recipient, children[0]);
            assert!(history.is_empty());
            assert_eq!(h.app.doc().scene.nodes.len(), count);
            assert!(
                !slate_doc::agent_chat::agent(h.app.doc().scene.node(recipient).unwrap())
                    .unwrap()
                    .chat
                    .draft
            );
        }
    }

    #[test]
    fn agent_history_cache_tracks_live_drag_and_grip_suppresses_resize() {
        let mut h = super::super::tests::Harness::new("live_chat_rails");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let root = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(root, "local");
        h.app.patch_nodes(&[root], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                p.agent.as_mut().unwrap().chat.train = true;
            }
        });
        h.app.board_sel = [root].into_iter().collect();
        h.app.agent_spawn_command(Some("[400,0]"));
        h.frame();
        let old = h.app.agents.rail_cache.borrow().1[0].curve.p0;
        h.app.doc_mut().scene.node_mut(root).unwrap().rect.x += 47.0;
        h.frame();
        let new = h.app.agents.rail_cache.borrow().1[0].curve.p0;
        assert!((new[0] - old[0] - 47.0).abs() < 0.01);
        let xf = h.app.board_xf();
        let p = xf
            .rect_w2s(h.app.doc().scene.node(root).unwrap().rect)
            .right_top()
            + egui::vec2(
                -slate_doc::agent_chat::PORT_INSET,
                slate_doc::agent_chat::RAIL_INSET,
            ) * xf.z;
        assert_eq!(
            h.app.agent_output_at(p, &xf),
            None,
            "the first train card has no output grip"
        );
    }

    #[test]
    fn agent_full_transcript_scales_without_rewrapping_between_raster_steps() {
        let mut h = super::super::tests::Harness::new("full_chat_zoom");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "local");
        h.frame();
        h.app.agents.local_turns.insert(
            id,
            vec![AgentTurn {
                role: "assistant".into(),
                text: "A stable conversation line".into(),
                at: 0,
            }],
        );
        h.app.agents.output_epoch += 1;
        h.frame();
        let first = h.app.agents.transcript_cache[&(id, 0)].clone();
        let logical_width = first.3.size().x * first.4;
        let r = h.app.doc().scene.node(id).unwrap().rect;
        h.app.tab_mut().cam.offset.x = r.x + r.w * 0.5;
        h.app.tab_mut().cam.offset.y = r.y + r.h * 0.5;
        for z in [1.01, 1.02, 1.03] {
            h.app.tab_mut().cam.z = z;
            h.frame();
            let current = &h.app.agents.transcript_cache[&(id, 0)];
            assert_eq!(current.3.rows.len(), first.3.rows.len());
            assert!((current.3.size().x * current.4 - logical_width).abs() < 0.5);
        }
    }

    #[test]
    fn agent_single_window_shrinks_but_keeps_its_original_viewport_ceiling() {
        let mut h = super::super::tests::Harness::new("agent_window_fit");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "local");
        h.app.patch_nodes(&[id], |n| {
            n.rect.h = 200.0;
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.bundle = None;
                a.chat.train = false;
                a.chat.window_height = Some(200);
            }
        });
        h.frame();
        h.app.agents.output_epoch += 1;
        let turn = |text: String| AgentTurn {
            role: "assistant".into(),
            text,
            at: 0,
        };
        h.app
            .agents
            .local_turns
            .insert(id, vec![turn("Short answer.".into())]);
        h.frame();
        let short = h.app.doc().scene.node(id).unwrap().rect.h;
        assert!(short < 200.0);
        h.app
            .agents
            .local_turns
            .insert(id, vec![turn("Long answer with many lines. ".repeat(100))]);
        h.app.agents.output_epoch += 1;
        h.frame();
        assert_eq!(h.app.doc().scene.node(id).unwrap().rect.h, 200.0);
        h.app
            .agents
            .local_turns
            .insert(id, vec![turn("Short answer.".into())]);
        h.app.agents.output_epoch += 1;
        h.frame();
        assert_eq!(h.app.doc().scene.node(id).unwrap().rect.h, short);
    }

    #[test]
    fn agent_draft_focus_enter_send_and_text_sizing() {
        let mut h = super::super::tests::Harness::new("agent_minimal_draft");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let root = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(root, "local");
        h.app.patch_nodes(&[root], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.bundle = None;
                a.chat.train = true;
            }
        });
        h.app.board_sel.clear();
        h.app.board_sel.insert(root);
        let original = h.app.doc().scene.node(root).unwrap().clone();
        let mut child = h.app.doc_mut().scene.build_duplicate(&original, 420.0, 0.0);
        if let NodeKind::Portal(p) = &mut child.kind {
            p.agent.as_mut().unwrap().chat.parent = Some(root);
        }
        let child_id = child.id;
        h.app.add_nodes(vec![child]);
        h.app.board_sel.clear();
        h.app.board_sel.insert(child_id);
        h.frame();
        let r = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(child_id).unwrap().rect);
        let grip = r.right_top()
            + egui::vec2(
                -slate_doc::agent_chat::PORT_INSET,
                slate_doc::agent_chat::RAIL_INSET,
            );
        for pressed in [true, false] {
            h.frame_with(|input| {
                input.events.push(egui::Event::PointerMoved(grip));
                input.events.push(egui::Event::PointerButton {
                    pos: grip,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            });
        }
        assert_eq!(
            h.app.portal_chrome.maximized, None,
            "output grip must not maximize"
        );
        let id = *h.app.board_sel.iter().next().unwrap();
        assert_ne!(id, child_id, "output grip creates a draft");
        h.frame();
        h.frame();
        assert!(h
            .ctx
            .memory(|m| m.has_focus(Id::new(("agent-composer", id.0)))));
        let height = h.app.doc().scene.node(id).unwrap().rect.h;
        assert!(height < 80.0);
        h.frame_with(|input| {
            input.events.push(egui::Event::Text(
                "A long message with several lines. ".repeat(20),
            ))
        });
        h.frame();
        assert!(h.app.doc().scene.node(id).unwrap().rect.h > height);
        h.app.ai.config.workspace_dir = None;
        h.frame_with(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        assert!(
            h.app
                .agent_failure_reason(id)
                .is_some_and(|s| s.contains("workspace")),
            "Enter dispatches send"
        );
        assert!(!h.app.agents.prompts[&id].ends_with('\n'));
    }

    #[test]
    fn agent_summary_cache_refreshes_for_theme_and_zoom() {
        let mut h = super::super::tests::Harness::new("agent_summary_raster");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "local");
        h.frame();
        h.app.agents.local_turns.insert(
            id,
            vec![AgentTurn {
                role: "assistant".into(),
                text: "Readable in both themes".into(),
                at: 0,
            }],
        );
        h.app.agents.output_epoch += 1;
        let node = h.app.doc().scene.node(id).unwrap();
        let center = egui::vec2(
            node.rect.x + node.rect.w * 0.5,
            node.rect.y + node.rect.h * 0.5,
        );
        h.app.tab_mut().cam.offset = center;
        h.app.tab_mut().cam.z = 1.0;
        h.app.dark_mode = true;
        h.frame();
        let dark = h.app.agents.transcript_cache[&(id, 0)].2;
        h.app.dark_mode = false;
        h.frame();
        let light = h.app.agents.transcript_cache[&(id, 0)].2;
        assert_ne!(dark, light, "theme changes must invalidate colored glyphs");
        h.app.tab_mut().cam.z = 3.0;
        h.frame();
        let zoom = h.app.agents.transcript_cache[&(id, 0)].2;
        assert_ne!(zoom, light, "high zoom must re-rasterize text");
    }

    #[test]
    fn an_older_assistant_turn_is_not_this_reply() {
        let turns = vec![
            AgentTurn {
                role: "assistant".into(),
                text: "old".into(),
                at: 10,
            },
            AgentTurn {
                role: "user".into(),
                text: "hi".into(),
                at: 20,
            },
        ];
        assert!(
            !await_new_reply(&turns, 20),
            "a prior answer must not clear the current send"
        );
    }

    #[test]
    fn a_new_assistant_turn_counts() {
        let turns = vec![
            AgentTurn {
                role: "user".into(),
                text: "hi".into(),
                at: 20,
            },
            AgentTurn {
                role: "assistant".into(),
                text: "ok".into(),
                at: 21,
            },
        ];
        assert!(await_new_reply(&turns, 20));
    }

    #[test]
    fn the_chip_names_unreachable_on_failure() {
        let waiting = AgentAwait::Failed {
            reason: "no key".into(),
            actions: Vec::new(),
        };
        let (_, label, pulse) = agent_status_chip(
            "cursor",
            CursorIdeStatus::Running,
            Some(&AgentStatus::Idle),
            Some(&waiting),
        );
        assert_eq!(label, "Unreachable");
        assert!(!pulse);
    }

    #[test]
    fn an_invalid_key_from_the_sidecar_offers_paste() {
        let (_, actions) =
            classify_agent_failure("Cursor startup failed: Invalid User API Key".into());
        assert!(
            actions.iter().any(|a| matches!(a, AgentRecover::PasteKey)),
            "a 401 from Agent.create must finish in the portal"
        );
    }

    #[test]
    fn an_api_key_failure_offers_the_dashboard() {
        let (reason, actions) = classify_agent_failure(
            "CURSOR_API_KEY is not set — the sidecar cannot reach Cursor agents without it.".into(),
        );
        assert!(
            !reason.starts_with("Link"),
            "the old 'Link:' prefix is not a hyperlink"
        );
        assert!(
            actions.iter().any(|a| matches!(
                a,
                AgentRecover::OpenUrl { url, .. }
                    if *url == atlas_ai::cursor_key::DASHBOARD_URL
            )),
            "must open the page where the key is minted"
        );
        assert!(
            actions.iter().any(|a| matches!(a, AgentRecover::PasteKey)),
            "must let the user finish in the portal"
        );
    }

    #[test]
    fn a_sidecar_spawn_error_is_not_rewritten_as_missing_node() {
        let (reason, _) = classify_agent_failure(
            "Could not start Cursor sidecar with C:\\Program Files\\nodejs\\node.exe: access denied"
                .into(),
        );
        assert!(
            reason.contains("access denied"),
            "keep the real spawn error: {reason}"
        );
        assert!(
            !reason.contains("Send again"),
            "must not hide a spawn error behind the PATH sermon: {reason}"
        );
    }

    #[test]
    fn a_node_search_failure_keeps_the_paths_we_tried() {
        let raw = "Slate could not see node.exe. Looked in:\nC:\\nowhere\\node.exe (not found)";
        let (reason, actions) = classify_agent_failure(raw.into());
        assert!(
            reason.contains("Looked in:"),
            "the tried paths are the next step: {reason}"
        );
        assert!(
            actions.iter().any(|a| matches!(
                a,
                AgentRecover::OpenUrl { url, .. } if *url == "https://nodejs.org/en/download"
            )),
            "still offer the installer"
        );
    }

    #[test]
    fn the_chip_pulses_while_thinking() {
        let waiting = AgentAwait::Sent {
            at: Instant::now(),
            req_at: 1,
        };
        let (_, label, pulse) =
            agent_status_chip("cursor", CursorIdeStatus::Running, None, Some(&waiting));
        assert_eq!(label, "Thinking");
        assert!(pulse);
    }

    #[test]
    fn saving_an_api_key_clears_the_failure() {
        let path = std::env::temp_dir().join(format!(
            "slate_key_{}_{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::env::set_var("ATLAS_CURSOR_KEY_PATH", &path);
        let mut h = super::super::tests::Harness::new("save_key");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app
            .fail_agent_await(id, "CURSOR_API_KEY is not set".into());
        h.app.agents.key_entry = Some(id);
        h.app.agents.key_draft = "cursor_test_key".into();
        let saved = h.app.save_cursor_api_key(Some(id));
        let cleared = h.app.agents.awaiting.get(&id).is_none();
        let ready = h.app.agents.composer_focus == Some(id);
        let quiet = h
            .app
            .agents
            .local_turns
            .get(&id)
            .is_none_or(|t| t.iter().all(|turn| turn.role != "system"));
        let _ = std::fs::remove_file(&path);
        std::env::remove_var("ATLAS_CURSOR_KEY_PATH");
        assert!(saved);
        assert!(cleared, "the failure leaves with the saved key");
        assert!(ready, "the composer is ready");
        assert!(quiet, "the key error is not left in the transcript");
    }

    fn board(tag: &str) -> super::super::tests::Harness {
        let mut h = super::super::tests::Harness::new(tag);
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h
    }

    /// A local train's first card at `at`.
    fn train(h: &mut super::super::tests::Harness, at: Pos2, provider: &str) -> NodeId {
        h.app.place_agent_portal_at(at);
        let id = h.app.doc().scene.nodes.last().unwrap().id;
        h.app.set_agent_program(id, provider);
        h.app.agents.project_picker = None;
        h.app.patch_nodes(&[id], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.bundle = None;
                a.chat.train = true;
            }
        });
        id
    }

    /// The next card of `parent`'s train, which becomes the tail.
    fn next_card(h: &mut super::super::tests::Harness, parent: NodeId) -> NodeId {
        let original = h.app.doc().scene.node(parent).unwrap().clone();
        let mut child = h.app.doc_mut().scene.build_duplicate(&original, 420.0, 0.0);
        if let NodeKind::Portal(p) = &mut child.kind {
            p.agent.as_mut().unwrap().chat.parent = Some(parent);
        }
        let id = child.id;
        h.app.add_nodes(vec![child]);
        id
    }

    #[test]
    fn after_send_the_caret_is_already_on_the_new_tail() {
        let mut h = board("caret_follows");
        let first = train(&mut h, Pos2::ZERO, "cursor");
        let ws = h.base.join("ai-ws");
        std::fs::create_dir_all(&ws).unwrap();
        h.app.ai.config.workspace_dir = Some(ws);
        h.frame();
        *h.app.agents.prompt_mut(first) = "first question".into();
        h.app.agents.composer_editing = Some(first);
        h.app.send_agent_prompt(first);
        let tail = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .find(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(first)))
            .map(|n| n.id)
            .expect("sending spawns the next card");
        let r = h.app.doc().scene.node(tail).unwrap().rect;
        h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
        h.app.tab_mut().cam.z = 1.0;
        for _ in 0..4 {
            h.frame();
        }
        assert!(
            h.ctx
                .memory(|m| m.has_focus(Id::new(("agent-composer", tail.0)))),
            "typing continues on the new card without a click"
        );
    }

    #[test]
    fn pair_cards_show_their_first_line_while_streaming_and_after() {
        let mut h = board("pair_fit");
        let upstream = train(&mut h, Pos2::ZERO, "local");
        h.app.patch_nodes(&[upstream], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                p.agent.as_mut().unwrap().chat.detail = slate_doc::agent_chat::Detail::Pair;
            }
        });
        let tail = next_card(&mut h, upstream);
        h.frame();
        let ask = "can you take a look at your folder and output the newest html file for a dashboard right here?";
        let reply = "I'll find the newest dashboard in the project and register it. ".repeat(5);
        for (id, reply) in [(upstream, reply.clone()), (tail, "I'll".to_string())] {
            h.app.agents.local_turns.insert(
                id,
                vec![
                    AgentTurn {
                        role: "user".into(),
                        text: ask.into(),
                        at: 0,
                    },
                    AgentTurn {
                        role: "assistant".into(),
                        text: reply,
                        at: 1,
                    },
                ],
            );
        }
        h.app
            .agents
            .awaiting
            .insert(tail, AgentAwait::Responding { req_at: 1 });
        h.app.agents.output_epoch += 1;
        let r = h.app.doc().scene.node(upstream).unwrap().rect;
        h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w, r.y + r.h * 0.5);
        h.app.tab_mut().cam.z = 1.0;
        for _ in 0..8 {
            h.frame();
        }
        for id in [upstream, tail] {
            assert_eq!(
                h.app
                    .agents
                    .transcript_scroll
                    .get(&id)
                    .copied()
                    .unwrap_or(0.0),
                0.0,
                "the first line of {id:?} is not scrolled out of the top"
            );
        }
        let sent = h.app.doc().scene.node(upstream).unwrap().rect;
        let (_, content, _) = h.app.agents.content_heights[&upstream];
        assert!(
            (sent.h - (COMPOSER_TOP + content + COMPOSER_BOTTOM)).abs() <= 1.0,
            "a sent card ends at its text, with no composer band: {} vs {}",
            sent.h,
            content
        );
    }

    fn note(h: &mut super::super::tests::Harness, at: Pos2, text: &str) -> NodeId {
        let node = h.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(at.x, at.y, 180.0, 120.0),
            NodeKind::Text(slate_doc::scene::TextNode {
                text: text.into(),
                family: slate_doc::scene::Typeface::Sans,
                size: 18.0,
                color: slate_doc::scene::Rgba::opaque(20, 20, 20),
                align: slate_doc::scene::TextAlign::Left,
                fill: None,
                agent: None,
            }),
        );
        h.app.add_nodes(vec![node])[0]
    }

    #[test]
    fn wires_into_a_train_start_calm_gray_and_keep_a_chosen_color() {
        use slate_doc::scene::{ConnectorEnd, Rgba, Side};
        let mut h = board("chat_wire_gray");
        let card = train(&mut h, Pos2::ZERO, "cursor");
        let text = note(&mut h, Pos2::new(-400.0, 0.0), "context");
        h.app.board_colors.fg = Rgba([230, 60, 150, 255]);
        let wire = h.app.build_connector(
            ConnectorEnd::Anchored {
                node: text,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: card,
                side: Side::Left,
                t: 0.5,
            },
        );
        let id = h.app.add_nodes(vec![wire])[0];
        let color =
            |h: &super::super::tests::Harness| match &h.app.doc().scene.node(id).unwrap().kind {
                NodeKind::Connector(c) => c.stroke.color,
                _ => unreachable!(),
            };
        assert_eq!(
            color(&h),
            h.app.chat_wire_color(),
            "not the pink drawing color"
        );
        let chosen = Rgba([40, 120, 220, 255]);
        h.app.patch_nodes(&[id], |n| {
            if let NodeKind::Connector(c) = &mut n.kind {
                c.stroke.color = chosen;
            }
        });
        for _ in 0..3 {
            h.frame();
        }
        assert_eq!(color(&h), chosen, "a color the person picks is never reset");
        let loose = h.app.build_connector(
            ConnectorEnd::Anchored {
                node: text,
                side: Side::Bottom,
                t: 0.5,
            },
            ConnectorEnd::Free {
                point: [0.0, 400.0],
            },
        );
        let NodeKind::Connector(c) = &loose.kind else {
            unreachable!()
        };
        assert_eq!(
            c.stroke.color, h.app.board_colors.fg,
            "other wires keep the drawing color"
        );
    }

    /// A context wire from `source` into `card` that a send already used.
    fn sent_wire(h: &mut super::super::tests::Harness, source: NodeId, card: NodeId) -> NodeId {
        use slate_doc::scene::{ConnectorEnd, Side};
        let a = ConnectorEnd::Anchored {
            node: source,
            side: Side::Right,
            t: 0.5,
        };
        let b = ConnectorEnd::Anchored {
            node: card,
            side: Side::Left,
            t: 0.5,
        };
        let mut binding =
            slate_doc::agent_inputs::infer_binding(&h.app.doc().scene, &a, &b).unwrap();
        binding.consumed = true;
        let mut wire = h.app.build_connector(a, b);
        if let NodeKind::Connector(c) = &mut wire.kind {
            c.binding = Some(binding);
        }
        h.app.add_nodes(vec![wire])[0]
    }

    fn press(h: &mut super::super::tests::Harness, at: Pos2) {
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(at)));
        for pressed in [true, false] {
            h.frame_with(|i| {
                i.events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                })
            });
        }
    }

    fn key(h: &mut super::super::tests::Harness, key: egui::Key) {
        h.frame_with(|i| {
            i.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }

    fn chat(h: &super::super::tests::Harness, id: NodeId) -> slate_doc::agent_chat::ChatView {
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .clone()
    }

    fn center_on(h: &mut super::super::tests::Harness, id: NodeId) {
        let r = h.app.doc().scene.node(id).unwrap().rect;
        h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
        h.app.tab_mut().cam.z = 1.0;
    }

    #[test]
    fn sent_cards_name_their_model_and_only_the_tail_offers_the_dropdown() {
        let mut h = board("agent_tail_model");
        let root = train(&mut h, Pos2::ZERO, "ollama");
        h.app.agents.models_started.insert("ollama".into());
        h.app.agents.models.insert(
            "ollama".into(),
            vec![atlas_ai::agent::AgentModel {
                id: "qwen3:8b".into(),
                name: "qwen3:8b".into(),
            }],
        );
        h.app.patch_nodes(&[root], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                p.title = "Review".into();
                p.agent.as_mut().unwrap().model = Some("llama3.1:8b".into());
            }
        });
        let tail = next_card(&mut h, root);
        h.frame();
        center_on(&mut h, root);
        h.frame();
        let label = {
            let node = h.app.doc().scene.node(root).unwrap();
            h.app
                .model_menu_label(slate_doc::agent_chat::agent(node).unwrap())
        };
        let texts = painted_text(&mut h, None);
        assert!(
            texts.iter().any(|t| *t == format!("Review · {label}")),
            "a sent card names its model as text: {texts:?}"
        );
        h.app.board_sel = [root].into_iter().collect();
        assert!(!h.app.agent_set_model("qwen3:8b"), "sent card refuses");
        h.app.board_sel = [tail].into_iter().collect();
        assert!(h.app.agent_set_model("qwen3:8b"), "the tail chooses");
    }

    #[test]
    fn collapse_keeps_width_three_lines_and_the_tail_composer_with_one_undo() {
        let mut h = board("agent_collapse");
        let root = train(&mut h, Pos2::ZERO, "local");
        let tail = next_card(&mut h, root);
        h.frame();
        let long = "A reply long enough to wrap across many lines of the card. ".repeat(12);
        for id in [root, tail] {
            h.app.agents.local_turns.insert(
                id,
                vec![AgentTurn {
                    role: "assistant".into(),
                    text: long.clone(),
                    at: 0,
                }],
            );
        }
        h.app.agents.output_epoch += 1;
        h.frame();
        center_on(&mut h, tail);
        h.frame();
        let open = h.app.doc().scene.node(root).unwrap().rect;
        let open_tail = h.app.doc().scene.node(tail).unwrap().rect;
        let text = |h: &super::super::tests::Harness, id| {
            h.app
                .visible_agent_turns(id)
                .iter()
                .map(|t| t.text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        h.app.board_sel = [root].into_iter().collect();
        assert!(h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId("portal.agent.collapse"),
            None
        ));
        h.frame();
        let rect = h.app.doc().scene.node(root).unwrap().rect;
        assert!(chat(&h, root).collapsed);
        assert_eq!(rect.w, open.w, "a collapsed card keeps its width");
        let three = collapsed_card_height(&h.ctx, text(&h, root), rect.w);
        assert!((rect.h - three).abs() < 0.5, "{} vs {three}", rect.h);
        assert!(rect.h < open.h, "{} vs open {}", rect.h, open.h);

        h.app.board_sel = [tail].into_iter().collect();
        h.app.agent_toggle_collapse(&h.ctx);
        h.frame();
        h.frame();
        let rect = h.app.doc().scene.node(tail).unwrap().rect;
        let composer = composer_text_height(&h.ctx, "", composer_wrap(rect.w), 14.0);
        let expected =
            collapsed_card_height(&h.ctx, text(&h, tail), rect.w) + composer + COMPOSER_BOTTOM;
        assert!((rect.h - expected).abs() < 0.5, "{} vs {expected}", rect.h);
        assert!(
            h.app.agents.composer_rects.contains_key(&tail),
            "the collapsed tail keeps its composer"
        );
        h.app.board_undo();
        h.frame();
        assert!(!chat(&h, tail).collapsed, "one undo expands");
        assert_eq!(h.app.doc().scene.node(tail).unwrap().rect.h, open_tail.h);
    }

    #[test]
    fn a_resize_records_its_size_and_fit_to_text_forgets_it() {
        let mut h = board("agent_resize");
        let root = train(&mut h, Pos2::ZERO, "local");
        h.frame();
        h.app.agents.local_turns.insert(
            root,
            vec![AgentTurn {
                role: "assistant".into(),
                text: "Short reply".into(),
                at: 0,
            }],
        );
        h.app.agents.output_epoch += 1;
        h.frame();
        h.app.board_sel = [root].into_iter().collect();
        h.app.agent_toggle_collapse(&h.ctx);
        h.frame();
        let fitted = h.app.doc().scene.node(root).unwrap().rect;
        let before = h.app.doc().scene.node(root).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        {
            let live = h.app.doc_mut().scene.node_mut(root).unwrap();
            live.rect.w = 520.0;
            live.rect.h = 90.0;
        }
        h.app.board_drag = Some(super::super::board::BoardDrag::Resize {
            id: root,
            before,
            handle: 0,
            dup: false,
        });
        h.app
            .end_gesture_for_test(Pos2::ZERO, None, egui::Modifiers::NONE);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
        let view = chat(&h, root);
        assert_eq!(view.size, Some([520.0, 90.0]));
        assert!(!view.collapsed, "resizing opens a collapsed card");
        h.frame();
        h.frame();
        let rect = h.app.doc().scene.node(root).unwrap().rect;
        assert_eq!((rect.w, rect.h), (520.0, 90.0), "fit keeps the size");

        assert!(h
            .app
            .dispatch(&h.ctx, atlas_commands::CommandId("portal.agent.fit"), None));
        h.frame();
        assert!(chat(&h, root).size.is_none());
        assert_ne!(h.app.doc().scene.node(root).unwrap().rect.w, 520.0);
        h.app.board_undo();
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(root).unwrap().rect.w, fitted.w);
        assert!(chat(&h, root).collapsed, "undo restores the collapsed card");
    }

    #[test]
    fn clicking_each_trains_message_area_focuses_its_own_composer() {
        let mut h = board("agent_two_trains");
        let first = train(&mut h, Pos2::new(-520.0, -260.0), "local");
        let second = train(&mut h, Pos2::new(120.0, 140.0), "local");
        h.app.tab_mut().cam.offset = egui::vec2(0.0, 0.0);
        h.app.tab_mut().cam.z = 1.0;
        h.frame();
        h.frame();
        for id in [first, second, first] {
            let before = h.app.doc().scene.node(id).unwrap().rect;
            let field = *h
                .app
                .agents
                .composer_rects
                .get(&id)
                .expect("each tail paints its Message field");
            press(&mut h, field.left_center() + egui::vec2(8.0, 0.0));
            h.frame();
            assert!(
                h.ctx
                    .memory(|m| m.has_focus(Id::new(("agent-composer", id.0)))),
                "the clicked train's composer has the caret"
            );
            assert!(h.app.board_drag.is_none(), "the press did not start a move");
            assert_eq!(h.app.doc().scene.node(id).unwrap().rect, before);
            assert_eq!(h.app.agents.composer_editing, Some(id));
        }
    }

    #[test]
    fn the_tail_deletes_by_key_unless_text_is_being_typed() {
        let mut h = board("agent_delete_tail");
        let root = train(&mut h, Pos2::ZERO, "local");
        let tail = next_card(&mut h, root);
        h.frame();
        center_on(&mut h, tail);
        h.frame();

        // Typed text keeps Delete and Backspace in the field.
        h.app.board_sel = [tail].into_iter().collect();
        h.app.agents.composer_focus = Some(tail);
        h.frame();
        h.frame();
        h.frame_with(|i| i.events.push(egui::Event::Text("draft".into())));
        assert_eq!(h.app.agents.composer_editing, Some(tail));
        key(&mut h, egui::Key::Delete);
        key(&mut h, egui::Key::Backspace);
        assert!(h.app.doc().scene.node(tail).is_some(), "typing is editing");
        assert_eq!(h.app.agents.prompts[&tail], "draf");

        // An idle composer: the selected tail deletes, with its unsent text.
        h.ctx
            .memory_mut(|m| m.surrender_focus(Id::new(("agent-composer", tail.0))));
        h.frame();
        h.frame();
        assert_eq!(h.app.agents.composer_editing, None);
        h.app.agent_focus(tail);
        h.app.board_sel = [tail].into_iter().collect();
        let depth = h.app.tab().journal.undo_depth();
        key(&mut h, egui::Key::Delete);
        assert!(
            h.app.doc().scene.node(tail).is_none(),
            "Delete removes the tail"
        );
        assert!(!h.app.agents.prompts.contains_key(&tail), "draft discarded");
        assert!(!h.app.agent_has_child(root), "the parent is the tail again");
        h.frame();
        h.frame();
        assert!(
            h.app.agents.composer_rects.contains_key(&root),
            "the parent regains the composer"
        );
        h.app.board_undo();
        assert!(h.app.doc().scene.node(tail).is_some(), "one undo restores");
        assert_eq!(h.app.tab().journal.undo_depth(), depth);

        // An empty, focused composer lets Delete through.
        h.app.board_sel = [tail].into_iter().collect();
        h.app.agents.composer_focus = Some(tail);
        h.frame();
        h.frame();
        assert_eq!(h.app.agents.composer_editing, Some(tail));
        key(&mut h, egui::Key::Backspace);
        assert!(
            h.app.doc().scene.node(tail).is_some(),
            "Backspace stays text"
        );
        key(&mut h, egui::Key::Delete);
        assert!(h.app.doc().scene.node(tail).is_none(), "empty composer");
    }

    #[test]
    fn deleting_sent_context_pockets_it_and_the_handle_restores_it() {
        let mut h = board("agent_pocket");
        let card = train(&mut h, Pos2::ZERO, "local");
        let used = note(&mut h, Pos2::new(-400.0, 0.0), "site plan");
        let fresh = note(&mut h, Pos2::new(-400.0, 300.0), "not sent");
        let wire = sent_wire(&mut h, used, card);
        h.frame();

        h.app.board_sel = [fresh].into_iter().collect();
        h.app.delete_board_nodes(&[fresh]);
        assert!(h.app.doc().scene.node(fresh).is_none(), "unsent deletes");

        let depth = h.app.tab().journal.undo_depth();
        h.app.delete_board_nodes(&[used]);
        let n = h.app.doc().scene.node(used).expect("same node id");
        assert!(n.hidden, "pocketed, not removed");
        assert!(h.app.doc().scene.node(wire).is_some(), "the wire stays");
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        assert_eq!(h.app.agents.retract_ghosts.len(), 1);
        let left = slate_doc::connector_anchor_on(
            h.app.doc().scene.node(card).unwrap(),
            slate_doc::scene::Side::Left,
            0.5,
        );
        assert_eq!(
            h.app.agents.retract_ghosts[0].1,
            Pos2::new(left[0], left[1])
        );
        assert!(h.app.agent_has_pocket(card));

        h.app.board_undo();
        assert!(
            !h.app.doc().scene.node(used).unwrap().hidden,
            "undo shows it"
        );
        h.app.board_redo();
        assert!(h.app.doc().scene.node(used).unwrap().hidden);

        let card_rect = h.app.doc().scene.node(card).unwrap().rect;
        assert!(h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId("portal.agent.pocket"),
            Some(card.0.to_string())
        ));
        let shown = h.app.doc().scene.node(used).unwrap();
        assert!(!shown.hidden, "a handle click shows it");
        assert!(
            shown.rect.x + shown.rect.w <= card_rect.x,
            "beside the card"
        );
        let ghosts = h.app.agents.retract_ghosts.len();
        assert!(h.app.agent_toggle_pocket(Some(&card.0.to_string())));
        assert!(h.app.doc().scene.node(used).unwrap().hidden, "second click");
        assert_eq!(h.app.agents.retract_ghosts.len(), ghosts + 1);
    }

    #[test]
    fn shared_context_pockets_into_every_train_that_used_it() {
        let mut h = board("agent_pocket_shared");
        let a = train(&mut h, Pos2::ZERO, "local");
        let b = train(&mut h, Pos2::new(0.0, 500.0), "local");
        let used = note(&mut h, Pos2::new(-400.0, 250.0), "shared brief");
        let to_a = sent_wire(&mut h, used, a);
        let to_b = sent_wire(&mut h, used, b);
        h.frame();
        let before = h.app.doc().scene.nodes.len();
        h.app.delete_board_nodes(&[used]);
        assert_eq!(h.app.doc().scene.nodes.len(), before, "nothing removed");
        assert_eq!(
            h.app.doc().scene.nodes.iter().filter(|n| n.hidden).count(),
            1,
            "one hidden node"
        );
        assert_eq!(h.app.agents.retract_ghosts.len(), 2, "one ghost per train");
        assert!(h.app.agent_has_pocket(a) && h.app.agent_has_pocket(b));

        assert!(h.app.agent_toggle_pocket(Some(&b.0.to_string())));
        assert!(!h.app.doc().scene.node(used).unwrap().hidden);
        for wire in [to_a, to_b] {
            assert!(
                h.app.doc().scene.node(wire).is_some(),
                "still wired to both"
            );
        }
        assert!(!h.app.agent_has_pocket(a), "visible to every train");
        h.app.delete_board_nodes(&[used]);
        assert!(h.app.doc().scene.node(used).unwrap().hidden);
        assert_eq!(h.app.agents.retract_ghosts.len(), 4);
        assert!(
            h.app.agent_toggle_pocket(Some(&a.0.to_string())),
            "from either"
        );
        assert!(!h.app.doc().scene.node(used).unwrap().hidden);
    }

    #[test]
    fn deleting_the_last_consuming_card_removes_its_pocket_in_the_same_step() {
        let mut h = board("agent_pocket_orphan");
        let card = train(&mut h, Pos2::ZERO, "local");
        let other = train(&mut h, Pos2::new(0.0, 500.0), "local");
        let lone = note(&mut h, Pos2::new(-400.0, 0.0), "only here");
        let shared = note(&mut h, Pos2::new(-400.0, 300.0), "also elsewhere");
        let lone_wire = sent_wire(&mut h, lone, card);
        sent_wire(&mut h, shared, card);
        sent_wire(&mut h, shared, other);
        h.app.delete_board_nodes(&[lone, shared]);
        assert!(h.app.doc().scene.node(lone).unwrap().hidden);
        assert!(h.app.doc().scene.node(shared).unwrap().hidden);
        let depth = h.app.tab().journal.undo_depth();
        h.app.board_sel = [card].into_iter().collect();
        h.app.delete_board_nodes(&[card]);
        assert!(h.app.doc().scene.node(card).is_none());
        assert!(
            h.app.doc().scene.node(lone).is_none(),
            "no invisible orphan"
        );
        assert!(h.app.doc().scene.node(lone_wire).is_none());
        assert!(
            h.app.doc().scene.node(shared).is_some_and(|n| n.hidden),
            "still pocketed in the other train"
        );
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one commit");
        h.app.board_undo();
        assert!(h.app.doc().scene.node(card).is_some());
        assert!(h.app.doc().scene.node(lone).is_some_and(|n| n.hidden));
        assert!(h.app.doc().scene.node(lone_wire).is_some());
    }
}
