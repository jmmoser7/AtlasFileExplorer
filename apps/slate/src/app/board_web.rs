//! Web portal runtime: the live pool, poster cache, per-origin consent, and the
//! frame painting for every state the contract names.
//!
//! Contract: `docs/keymap/contracts/portal-web-embed.md`. Two rules shape this
//! file. First, the portal is **host** class (Art. V.3): the frame and its
//! parameters are journaled, and everything here — poster pixels, pool
//! membership, input focus, consent grants — is derived and never written into
//! the `.slate` (D31, D32). Second, nothing on this path touches the network or
//! the filesystem on the frame loop (Art. II.2): local staleness is a
//! worker-interval mtime poll, and pixel work is budgeted per frame.
//!
//! Pixels arrive through [`WebHost`], the one seam that knows what a browser
//! is. Everything above it — admission, eviction, LOD, states — is ordinary
//! logic that runs and tests identically on any platform.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect};
use slate_doc::scene::{
    classify_web_locator, web_display_locator, web_origin, Node, NodeId, NodeKind, PortalKind,
    PortalNode, SceneCmd, SourceUri, WebPortalRef, WebRefusal, WebSourceKind, WebZoom,
};

use super::board::{self, rgba32, BoardXf};
use super::board_portal_chrome::{layout_portal_chrome, PortalChromeLayout};
use super::SlateApp;

// ---------------------------------------------------------------------------
// Feel constants (contract "Feel constants" table)
// ---------------------------------------------------------------------------

/// Default remote page for a newly placed web portal. This is a locator, not
/// browser chrome: the page owns its own search UI and Slate remains one page,
/// one locator.
pub(crate) const WEB_START_LOCATOR: &str = "https://www.google.com/";

/// Clipboard and external drops must contain one whole remote URL, not prose
/// or a list of URLs. Classification remains owned by slate-doc.
pub(crate) fn web_url_text(text: &str) -> Option<&str> {
    let url = text.trim();
    (url.len() <= 65_536
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
        && web_origin(url).is_some()
        && classify_web_locator(url, false) == Ok(WebSourceKind::Remote))
    .then_some(url)
}

/// Below this on-screen height (physical pixels), a portal is not worth a
/// live webview. The last poster still paints — a blank fill is D30.
pub const LOD_STRIP_PX: f32 = 96.0;
/// Safety ceiling beyond the monitor-sized quality tiers (supports 8K displays).
pub const RASTER_MAX_PX: f32 = 8192.0;
pub const RASTER_MAX_PIXELS: f32 = 33_177_600.0;
/// On-screen height at which a portal becomes eligible for the live pool.
///
/// This is not a taste threshold, it is a cost one, so it sits low enough that a
/// portal at the board's usual zoom is simply live. It started at 320 px, which
/// meant a freshly placed 540 px-tall portal never loaded at any zoom under
/// 0.6 — the page looked broken rather than budgeted. An explicitly focused
/// portal ignores this entirely (see [`admit`]).
pub const LIVE_MIN_PX: f32 = 160.0;
/// Webviews alive at once, across the whole board.
pub const LIVE_POOL: usize = 6;
/// Render rate for pooled portals that do not hold input focus.
pub const IDLE_FPS: f32 = 30.0;
/// Readback cadence for a focused portal that is sitting still.
pub const FOCUSED_FPS: f32 = 30.0;
/// Readback cadence while the focused page is being scrolled, dragged, or
/// typed into. Idle stays at [`FOCUSED_FPS`] so a parked cursor does not
/// restart the busy readback loop that used to jutter the board.
pub const INTERACTIVE_FPS: f32 = 60.0;
/// How long after the last page input the interactive cadence holds.
pub const INTERACTIVE_HOLD_SECS: f32 = 0.18;
/// Contents textures uploaded per frame; the rest wait in the backlog.
pub const UPLOADS_PER_FRAME: usize = atlas_core::display::WEB_UPLOADS_PER_FRAME;
/// Border band that stays a Slate target while a portal holds input focus.
pub const BORDER_HIT_PX: f32 = 6.0;
/// Floor on how often a local source's mtime is checked, on a worker.
pub const POLL_SECS: f32 = 1.0;
/// Poster alpha while a recapture is in flight.
pub const STALE_ALPHA: f32 = 0.6;

// ---------------------------------------------------------------------------
// States (D30) and level of detail (D23)
// ---------------------------------------------------------------------------

/// Every state a web portal can be in, each of which says itself on the frame
/// rather than leaving it blank (D30).
#[derive(Debug, Clone, PartialEq)]
pub enum WebState {
    /// No locator yet — the draw grammar always commits here (D03).
    Unbound,
    /// Not yet resolved. Neutral, never blocking (P1.portal.health).
    Unknown,
    /// A remote origin the human has not permitted yet (D32).
    Blocked {
        origin: String,
    },
    Loading,
    /// Holding a pool slot and rendering.
    Live,
    /// Eligible, but the pool was full — showing its last frame (D29).
    Budgeted,
    /// Stale by construction, and the age is stated.
    Poster {
        captured: Instant,
    },
    Missing {
        locator: String,
    },
    Refused {
        reason: String,
    },
    /// No WebView2 runtime on this machine.
    NoRuntime,
    /// Painted smaller than `LIVE_MIN_PX`.
    TooSmall,
}

impl WebState {
    /// The short word the chrome strip shows.
    pub fn label(&self) -> String {
        match self {
            WebState::Unbound => "unbound".into(),
            WebState::Unknown => "resolving…".into(),
            WebState::Blocked { origin } => format!("blocked · {origin}"),
            WebState::Loading => "loading…".into(),
            WebState::Live => "live".into(),
            WebState::Budgeted => "budgeted".into(),
            WebState::Poster { captured } => format!("poster · {}", ago(captured.elapsed())),
            WebState::Missing { .. } => "missing".into(),
            WebState::Refused { reason } => format!("refused · {reason}"),
            WebState::NoRuntime => "no WebView2 runtime".into(),
            WebState::TooSmall => "zoom in to run".into(),
        }
    }

    /// The health dot of P1.portal.health — three states, never more.
    pub fn health(&self) -> WebHealth {
        match self {
            WebState::Live | WebState::Budgeted | WebState::Poster { .. } | WebState::Loading => {
                WebHealth::Ok
            }
            WebState::Missing { .. } | WebState::Refused { .. } => WebHealth::Missing,
            _ => WebHealth::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebHealth {
    Ok,
    Unknown,
    Missing,
}

/// What a portal is worth drawing at its current on-screen size (D23).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebLod {
    /// No live webview. The last poster still paints (D23 + D30).
    Strip,
    /// The cached poster.
    Poster,
    /// Big enough on screen to be worth a webview, if the pool has room.
    Eligible,
}

/// The bucket for an on-screen height in physical pixels. Off-screen portals
/// never reach here — the caller drops them to [`WebLod::Poster`].
pub fn lod_for(height_px: f32) -> WebLod {
    if height_px < LOD_STRIP_PX {
        WebLod::Strip
    } else if height_px < LIVE_MIN_PX {
        WebLod::Poster
    } else {
        WebLod::Eligible
    }
}

/// Whether a live URL is still the bound page, ignoring the `file:///` form and
/// a trailing slash — so a portal does not claim to have navigated away the
/// instant it finishes loading.
fn same_page(locator: &str, url: &str) -> bool {
    let norm = |s: &str| {
        s.trim_end_matches('/')
            .trim_start_matches("file:///")
            .replace('\\', "/")
            .to_ascii_lowercase()
    };
    norm(locator) == norm(url)
}

fn ago(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

// ---------------------------------------------------------------------------
// The host seam
// ---------------------------------------------------------------------------

/// What a webview needs to exist: where to point it and how big to lay it out.
#[derive(Debug, Clone, PartialEq)]
pub struct WebRequest {
    /// Absolute path or `http(s)` URL, already resolved and permitted.
    pub target: String,
    pub kind: WebSourceKind,
    /// CSS pixels the page lays out at.
    pub width_css: u32,
    pub height_css: u32,
    /// Capture size in physical pixels. Fit keeps `width_css` for layout and
    /// rasters at this size so a 2× display is not a 1× bitmap stretched.
    pub raster_w: u32,
    pub raster_h: u32,
    /// Runs in the page after load so a wired spreadsheet can feed a chart.
    /// Empty for remote pages.
    pub link_script: String,
}

impl WebRequest {
    pub fn rasterization_scale(&self) -> f64 {
        let css = self.width_css.max(1) as f64;
        (self.raster_w.max(1) as f64 / css).max(0.01)
    }
}

/// Input forwarded to the one portal holding input focus (D22). Composition
/// hosting gives the app no HWND for the page, so every event is explicit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WebInput {
    /// Pointer position in page CSS pixels. `buttons` is the bitmask of what is
    /// currently held (1 = left, 2 = right, 4 = middle) — without it a page
    /// sees a bare hover and drag-selecting text does nothing.
    Move {
        x: f32,
        y: f32,
        buttons: u8,
    },
    Down {
        x: f32,
        y: f32,
        button: u8,
    },
    Up {
        x: f32,
        y: f32,
        button: u8,
    },
    /// Wheel in page units; `horizontal` distinguishes the two axes.
    Wheel {
        x: f32,
        y: f32,
        delta: f32,
        horizontal: bool,
    },
    /// The pointer left the frame, so hover states can clear.
    Leave,
    Key {
        key: egui::Key,
        pressed: bool,
    },
    Text(char),
}

/// The one thing in this feature that knows what a browser is.
///
/// The shipping implementation hosts WebView2 through
/// `CreateCoreWebView2CompositionController`, attaches it to a DirectComposition
/// visual, and pulls frames off a `Direct3D11CaptureFramePool` — the supported
/// route to offscreen pixels, since WebView2 has no render-to-texture API.
/// Keeping it behind this trait is what lets the pool, the LOD buckets, and
/// every state above run and be tested on a machine with no WebView2 at all
/// (Art. I.3).
pub trait WebHost {
    /// Escape intercepted by the native browser, forwarded to Slate's cancel stack.
    fn take_escape(&mut self) -> bool {
        false
    }
    /// Give native keyboard focus back to the board window. A native page keeps
    /// the keyboard after a click lands elsewhere, so a chat field would look
    /// unresponsive until something else took it back.
    fn release_keyboard(&self) {}
    /// PNG exports a page asked to drop on the board, each `(portal, file)`.
    fn take_canvas_drops(&mut self) -> Vec<(NodeId, PathBuf)> {
        Vec::new()
    }
    /// Whether a runtime exists at all. `false` puts every portal in
    /// [`WebState::NoRuntime`] rather than stalling (D29).
    fn available(&self) -> bool;
    /// Create or update the view for `id`. Called only for pool members.
    fn admit(&mut self, id: NodeId, req: &WebRequest);
    /// Tear down the view for `id`, releasing its slot.
    fn evict(&mut self, id: NodeId);
    /// The newest frame, if one arrived since the last call.
    fn take_frame(&mut self, id: NodeId) -> Option<WebFrame>;
    /// A one-off capture for the poster cache (D21). Must not be called from
    /// `web_pump` on demotion — that path retains the accepted poster texture.
    fn capture_poster(&mut self, id: NodeId) -> Option<WebFrame>;
    /// Last uploaded/captured frame, no GPU readback.
    fn last_frame(&self, _id: NodeId) -> Option<WebFrame> {
        None
    }
    fn send_input(&mut self, id: NodeId, input: WebInput);
    /// Page scrollbars follow the same derived visibility as Slate's overlay.
    fn set_scrollbars(&mut self, _id: NodeId, _visible: bool, _width_css: f32, _color: Color32) {}
    /// The cursor the page is asking for, while it holds input focus (D10).
    fn cursor(&self, id: NodeId) -> Option<egui::CursorIcon>;
    /// Whether the page reported a load failure.
    fn load_error(&self, id: NodeId) -> Option<String>;

    /// Where the page has actually got to, which is not the locator once the
    /// human follows a link. Reporting it is what keeps the frame honest about
    /// what it is showing (Art. IV).
    fn current_url(&self, _id: NodeId) -> Option<String> {
        None
    }
    /// In-page history. `false` when there is nowhere to go, so the caller can
    /// leave the command inert rather than pretend.
    fn go_back(&mut self, _id: NodeId) -> bool {
        false
    }
    fn go_forward(&mut self, _id: NodeId) -> bool {
        false
    }
    /// Reload in place, keeping the view and its slot.
    fn reload(&mut self, _id: NodeId) -> bool {
        false
    }
    /// Navigate this derived view without changing the authored locator.
    fn navigate(&mut self, _id: NodeId, _target: &str) -> bool {
        false
    }
    /// Start reading the page's visible text once, read-only, for an agent
    /// run the human started (D15 / D27 amendment, 24 September 2026). Only
    /// `innerText`: never cookies, storage, or a channel the page can call.
    /// `false` when the page has no live view to read.
    fn request_text(&mut self, _id: NodeId) -> bool {
        false
    }
    /// The text `request_text` asked for, once it arrives.
    fn take_text(&mut self, _id: NodeId) -> Option<Result<String, String>> {
        None
    }
}

/// A captured page. Shared, so keeping the latest frame for posters costs a
/// pointer rather than a copy of every pixel.
pub type WebFrame = std::sync::Arc<egui::ColorImage>;

/// The host used where no WebView2 runtime is present — every other platform,
/// and Windows machines without the Evergreen runtime installed. Portals still
/// place, bind, resolve health, export, and bake; they just say `NoRuntime`
/// instead of showing pixels.
#[derive(Default)]
pub struct NullHost;

impl WebHost for NullHost {
    fn available(&self) -> bool {
        false
    }
    fn admit(&mut self, _id: NodeId, _req: &WebRequest) {}
    fn evict(&mut self, _id: NodeId) {}
    fn take_frame(&mut self, _id: NodeId) -> Option<WebFrame> {
        None
    }
    fn capture_poster(&mut self, _id: NodeId) -> Option<WebFrame> {
        None
    }
    fn last_frame(&self, _id: NodeId) -> Option<WebFrame> {
        None
    }
    fn send_input(&mut self, _id: NodeId, _input: WebInput) {}
    fn cursor(&self, _id: NodeId) -> Option<egui::CursorIcon> {
        None
    }
    fn load_error(&self, _id: NodeId) -> Option<String> {
        None
    }
}

fn default_host() -> Box<dyn WebHost> {
    Box::new(NullHost)
}

// ---------------------------------------------------------------------------
// Per-portal derived state
// ---------------------------------------------------------------------------

struct WebView {
    /// Bumped whenever the locator or viewport changes, so a capture that was
    /// already in flight is discarded rather than painted (Art. II.3).
    generation: u64,
    /// Locator + viewport the current derived state belongs to.
    key: String,
    state: WebState,
    poster: Option<egui::TextureHandle>,
    poster_at: Option<Instant>,
    /// Physical on-screen size last frame; 0 when not yet painted.
    width_px: f32,
    height_px: f32,
    area_px: f32,
    on_screen: bool,
    live: bool,
    last_focus: Option<Instant>,
    /// Last time we asked the host for a captured frame. Even "no frame yet"
    /// should not become a zero-delay polling loop.
    last_frame_probe: Option<Instant>,
    last_poll: Option<Instant>,
    source_mtime: Option<SystemTime>,
    /// Local source changed while this portal still holds a pool slot — reload
    /// on the next admit rather than waiting for eviction (D21).
    reload_pending: bool,
    /// A metadata probe is in flight; do not spawn another.
    poll_inflight: bool,
    /// Last off-thread existence probe. `None` means not yet answered.
    source_exists: Option<bool>,
    /// In-page URL last seen while this portal was live. Survives pool
    /// eviction so zooming the camera does not send the user back to the
    /// authored home. Never journaled (D15 / D31).
    resume_url: Option<String>,
}

impl WebView {
    fn new(key: String) -> Self {
        Self {
            generation: 0,
            key,
            state: WebState::Unknown,
            poster: None,
            poster_at: None,
            width_px: 0.0,
            height_px: 0.0,
            area_px: 0.0,
            on_screen: false,
            live: false,
            last_focus: None,
            last_frame_probe: None,
            last_poll: None,
            source_mtime: None,
            reload_pending: false,
            poll_inflight: false,
            source_exists: None,
            resume_url: None,
        }
    }
}

/// One local-source probe result. Generation-tagged so a rebind discards it.
struct SourcePoll {
    id: NodeId,
    generation: u64,
    exists: bool,
    mtime: Option<SystemTime>,
}

/// `web-consent.json` in the per-user Atlas data folder.
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedConsent {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    workbooks: HashMap<String, HashSet<String>>,
}

/// Board-wide web portal state. One per app, not per portal.
pub struct WebRuntime {
    views: HashMap<NodeId, WebView>,
    /// Origins the human has permitted this session, keyed `scheme://host`.
    /// Never journaled and never written into the `.slate`: a workbook you
    /// receive must not arrive already trusting a host (D26, D32).
    consent: HashSet<String>,
    /// Grants kept for later sessions, keyed (workbook, origin), in the
    /// per-user Atlas data folder. None in tests and tools.
    consent_file: Option<std::path::PathBuf>,
    saved_consent: HashMap<String, HashSet<String>>,
    /// The active workbook and its saved origins, swapped only on change so
    /// the per-frame check never allocates.
    consent_workbook: Option<std::path::PathBuf>,
    workbook_consent: HashSet<String>,
    /// The one portal receiving pointer and keyboard, if any (D22).
    pub focused: Option<NodeId>,
    /// Whether the pointer was inside the focused page last frame, so the page
    /// gets exactly one `Leave` when it wanders off.
    pointer_inside: bool,
    /// Buttons held inside the focused page (1 = left, 2 = right, 4 = middle).
    /// A drag that starts in the page keeps the pointer until release.
    pointer_down: u8,
    host: Box<dyn WebHost>,
    next_generation: u64,
    /// Texture uploads already spent this frame (D29).
    uploads_this_frame: usize,
    /// Portals with a frame waiting for an upload slot.
    backlog: Vec<NodeId>,
    /// Last wheel / drag / key that reached the focused page. Drives the
    /// interactive readback burst (D29).
    last_page_input: Option<Instant>,
    /// Off-thread local-source probes (Art. II.2 — never `metadata` on the
    /// frame loop; a share can take seconds).
    poll_tx: crossbeam_channel::Sender<SourcePoll>,
    poll_rx: crossbeam_channel::Receiver<SourcePoll>,
    /// Parsed tables for dashboard wires, refreshed at most once a second.
    link_cache: HashMap<std::path::PathBuf, (std::time::Instant, i64, u64, serde_json::Value)>,
    /// Pages whose text an agent run is waiting on.
    text_pending: HashSet<NodeId>,
}

impl Default for WebRuntime {
    fn default() -> Self {
        let (poll_tx, poll_rx) = crossbeam_channel::unbounded();
        Self {
            views: HashMap::new(),
            consent: HashSet::new(),
            consent_file: None,
            saved_consent: HashMap::new(),
            consent_workbook: None,
            workbook_consent: HashSet::new(),
            focused: None,
            pointer_inside: false,
            pointer_down: 0,
            host: default_host(),
            next_generation: 1,
            uploads_this_frame: 0,
            backlog: Vec::new(),
            last_page_input: None,
            poll_tx,
            poll_rx,
            link_cache: HashMap::new(),
            text_pending: HashSet::new(),
        }
    }
}

impl WebRuntime {
    fn note_page_input(&mut self) {
        self.last_page_input = Some(Instant::now());
    }

    fn recently_interactive(&self) -> bool {
        self.last_page_input
            .is_some_and(|t| t.elapsed().as_secs_f32() < INTERACTIVE_HOLD_SECS)
    }

    fn focused_frame_interval(&self, id: NodeId) -> Duration {
        if self.focused == Some(id) && self.recently_interactive() {
            Duration::from_secs_f32(1.0 / INTERACTIVE_FPS)
        } else if self.focused == Some(id) {
            Duration::from_secs_f32(1.0 / FOCUSED_FPS)
        } else {
            Duration::from_secs_f32(1.0 / IDLE_FPS)
        }
    }

    /// Replace the pixel backend. The app installs the platform host at
    /// startup; tests install a fake to drive the pool without a browser.
    pub fn set_host(&mut self, host: Box<dyn WebHost>) {
        for id in self.views.keys().copied().collect::<Vec<_>>() {
            self.host.evict(id);
            if let Some(v) = self.views.get_mut(&id) {
                v.live = false;
            }
        }
        self.host = host;
    }

    pub fn state(&self, id: NodeId) -> WebState {
        self.views
            .get(&id)
            .map(|v| v.state.clone())
            .unwrap_or(WebState::Unknown)
    }

    pub fn is_live(&self, id: NodeId) -> bool {
        self.views.get(&id).is_some_and(|v| v.live)
    }

    /// How many webviews currently hold a pool slot.
    pub fn live_count(&self) -> usize {
        self.views.values().filter(|v| v.live).count()
    }

    pub fn has_consent(&self, origin: &str) -> bool {
        self.consent.contains(origin) || self.workbook_consent.contains(origin)
    }

    /// Permit one origin: for this session, and for the active saved workbook
    /// in later sessions. Local state only — never a journal command, never
    /// saved into the workbook (D32).
    pub fn grant_consent(&mut self, origin: impl Into<String>) {
        let origin = origin.into();
        self.consent.insert(origin.clone());
        if let Some(key) = self.consent_key() {
            self.workbook_consent.insert(origin.clone());
            self.saved_consent.entry(key).or_default().insert(origin);
            self.save_consent();
        }
    }

    pub fn revoke_consent(&mut self, origin: &str) {
        self.consent.remove(origin);
        if let Some(key) = self.consent_key() {
            self.workbook_consent.remove(origin);
            if let Some(origins) = self.saved_consent.get_mut(&key) {
                origins.remove(origin);
            }
            self.save_consent();
        }
    }

    /// Keep grants across sessions in `path`, loading what is already there.
    pub fn use_consent_file(&mut self, path: std::path::PathBuf) {
        self.saved_consent = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<SavedConsent>(&bytes).ok())
            .map(|saved| saved.workbooks)
            .unwrap_or_default();
        self.consent_file = Some(path);
        self.workbook_consent = self.saved_for_active();
    }

    /// Follow the active tab. Cheap when unchanged.
    pub fn set_consent_workbook(&mut self, workbook: Option<&std::path::Path>) {
        if self.consent_workbook.as_deref() == workbook {
            return;
        }
        self.consent_workbook = workbook.map(std::path::Path::to_path_buf);
        self.workbook_consent = self.saved_for_active();
    }

    fn consent_key(&self) -> Option<String> {
        self.consent_file.as_ref()?;
        Some(
            self.consent_workbook
                .as_ref()?
                .to_string_lossy()
                .into_owned(),
        )
    }

    fn saved_for_active(&self) -> HashSet<String> {
        self.consent_key()
            .and_then(|key| self.saved_consent.get(&key).cloned())
            .unwrap_or_default()
    }

    fn save_consent(&self) {
        let Some(path) = &self.consent_file else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let saved = SavedConsent {
            version: 1,
            workbooks: self.saved_consent.clone(),
        };
        let _ = atlas_ai::agent::atomic_write_json(path, &saved);
    }

    fn generation(&mut self) -> u64 {
        let g = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        g
    }
}

/// One portal's inputs to the admission decision, gathered on the frame loop
/// and resolved without touching the scene — which is what makes the policy
/// testable on its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub id: NodeId,
    pub height_px: f32,
    pub area_px: f32,
    pub on_screen: bool,
    pub focused: bool,
    /// Ties break toward the most recently focused portal.
    pub last_focus: Option<Instant>,
    /// A portal that cannot render (unbound, blocked, refused) never competes.
    pub renderable: bool,
    /// Already holding a pool slot last frame — hysteresis uses this.
    pub was_live: bool,
}

/// Chooses which portals hold the pool's webviews this frame (D29).
///
/// Priority is input focus, then greatest on-screen area, then most recently
/// focused. Pure so the hundred-tile research-hub case can be asserted without
/// a browser, a GPU, or a window.
///
/// Focus overrides the size gate rather than merely sorting ahead of it: a
/// human who double-clicks into a page has said what they want, and answering
/// "too small" would be the app arguing with them.
pub fn admit(candidates: &[Candidate], pool: usize) -> Vec<NodeId> {
    let mut eligible: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| c.renderable && c.on_screen && size_keeps_slot(c))
        .collect();
    eligible.sort_by(|a, b| {
        b.focused
            .cmp(&a.focused)
            .then_with(|| {
                b.area_px
                    .partial_cmp(&a.area_px)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| b.last_focus.cmp(&a.last_focus))
    });
    eligible.into_iter().take(pool).map(|c| c.id).collect()
}

fn size_keeps_slot(c: &Candidate) -> bool {
    if c.focused {
        return true;
    }
    if lod_for(c.height_px) == WebLod::Eligible {
        return true;
    }
    // Keep a visible browser alive while zooming; size controls painting, not
    // document lifetime. The pool cap and off-screen eviction still apply.
    c.was_live && c.height_px > 0.0
}

// ---------------------------------------------------------------------------
// Locator resolution
// ---------------------------------------------------------------------------

/// Workbook-relative first, absolute as fallback (Art. IX.2). Remote locators
/// pass through untouched.
pub fn resolve_web_source(workbook: Option<&Path>, locator: &str) -> PathBuf {
    super::board_portal::resolve_source(workbook, locator)
}

/// The inverse: store a path relative to the workbook when it lives under it.
pub fn web_source_locator(workbook: Option<&Path>, path: &Path) -> String {
    super::board_portal::source_locator(workbook, path)
}

// ---------------------------------------------------------------------------
// App integration
// ---------------------------------------------------------------------------

impl SlateApp {
    /// Every web portal on the board, with the node rect each one occupies.
    fn web_portals(&self) -> Vec<(NodeId, PortalNode, slate_doc::scene::WorldRect)> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Portal(p) if p.kind == PortalKind::Web => Some((n.id, p.clone(), n.rect)),
                _ => None,
            })
            .collect()
    }

    /// Drop each exported PNG on the board directly under its dashboard, then
    /// under the previous export, and pan just enough to show the new one.
    fn place_canvas_exports(&mut self, drops: Vec<(NodeId, PathBuf)>) {
        if self.refuse_read_only_edit() {
            return;
        }
        for (portal_id, path) in drops {
            if !path.is_file() {
                continue;
            }
            let Some(portal_node) = self.doc().scene.node(portal_id) else {
                continue;
            };
            let portal = portal_node.rect;
            let Some(item) = self.item_for_path(&path) else {
                continue;
            };
            let (pw, ph) = image::image_dimensions(&path).unwrap_or((1600, 1000));
            let occupied: Vec<slate_doc::scene::WorldRect> = self
                .doc()
                .scene
                .nodes
                .iter()
                .filter(|n| {
                    n.id != portal_id
                        && !matches!(n.kind, NodeKind::Connector(_))
                        && !rect_contains(n.rect, portal)
                })
                .map(|n| n.rect)
                .collect();
            let rect = canvas_export_rect(portal, pw, ph, &occupied);
            let node = self.doc_mut().scene.build_node(
                rect,
                NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
            );
            let ids = self.add_nodes(vec![node]);
            if ids.is_empty() {
                continue;
            }
            self.inherit_frame_tags_after_move(&ids);
            self.reveal_world_rect(rect);
        }
    }

    /// Shift the camera so `rect` sits inside the canvas, without changing zoom.
    fn reveal_world_rect(&mut self, rect: slate_doc::scene::WorldRect) {
        let xf = self.board_xf();
        if xf.z <= 0.0 {
            return;
        }
        let screen = xf.rect_w2s(rect);
        let canvas = self.canvas_rect;
        let margin = 24.0;
        let dy = if screen.max.y > canvas.max.y - margin {
            screen.max.y - (canvas.max.y - margin)
        } else if screen.min.y < canvas.min.y + margin {
            screen.min.y - (canvas.min.y + margin)
        } else {
            0.0
        };
        if dy != 0.0 {
            self.tab_mut().cam.offset.y += dy / xf.z;
        }
    }

    /// Per-frame web portal work: resolve state, run the pool, spend the upload
    /// budget. Nothing here blocks, and nothing here reaches the network.
    pub(crate) fn web_pump(&mut self, ctx: &egui::Context) {
        let _span = atlas_core::session_log::span("slate.web.pump");
        let workbook = if self.tabs.is_empty() {
            self.fallback_tab.path.as_deref()
        } else {
            self.tabs[self.active_tab.min(self.tabs.len() - 1)]
                .path
                .as_deref()
        };
        self.web.set_consent_workbook(workbook);
        let drops = self.web.host.take_canvas_drops();
        if !drops.is_empty() {
            self.place_canvas_exports(drops);
        }
        if self.web.host.take_escape() {
            ctx.input_mut(|i| {
                if !i.key_pressed(egui::Key::Escape) {
                    i.events.push(egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            });
        }
        self.web.uploads_this_frame = 0;
        self.drain_source_polls();
        let portals = self.web_portals();
        if portals.is_empty() {
            if !self.web.views.is_empty() {
                let stale: Vec<NodeId> = self.web.views.keys().copied().collect();
                for id in stale {
                    self.web.host.evict(id);
                    self.web.views.remove(&id);
                }
                self.web_blur();
            }
            return;
        }

        let live_ids: HashSet<NodeId> = portals.iter().map(|(id, _, _)| *id).collect();
        let dropped: Vec<NodeId> = self
            .web
            .views
            .keys()
            .copied()
            .filter(|id| !live_ids.contains(id))
            .collect();
        for id in dropped {
            self.web.host.evict(id);
            self.web.views.remove(&id);
        }
        if self.web.focused.is_some_and(|id| !live_ids.contains(&id)) {
            self.web_blur();
        }

        self.ensure_web_host(ctx);
        let workbook = self.tab().path.clone();
        let host_ok = self.web.host.available();
        let mut candidates = Vec::with_capacity(portals.len());

        for (id, portal, _rect) in &portals {
            let web = portal.web_ref();
            let key = view_key(portal, &web);
            let regenerate = self.web.views.get(id).is_none_or(|v| v.key != key);
            if regenerate {
                self.remember_web_visit(*id);
                let locator = portal
                    .source
                    .as_ref()
                    .map(|s| s.locator.as_str())
                    .unwrap_or_default();
                let resume = self.web.views.get(id).and_then(|v| {
                    v.key
                        .starts_with(&format!("{locator}|"))
                        .then(|| v.resume_url.clone())
                        .flatten()
                });
                let generation = self.web.generation();
                self.web.host.evict(*id);
                let mut view = WebView::new(key);
                view.generation = generation;
                view.resume_url = resume;
                // Viewport changes restart the host, not the last good image.
                // A different source must never inherit another page's poster.
                if let Some(old) = self.web.views.get_mut(id) {
                    if old.key.starts_with(&format!("{locator}|")) {
                        view.poster = old.poster.take();
                        view.poster_at = old.poster_at;
                    }
                }
                self.web.views.insert(*id, view);
            }
            if self.portal_chrome.maximized == Some(*id) {
                let screen = ctx.screen_rect();
                let collapsed = self.portal_chrome_collapsed(*id);
                let layout = layout_portal_chrome(screen, collapsed, true, 1.0);
                if let Some(v) = self.web.views.get_mut(id) {
                    let ppp = ctx.pixels_per_point().max(0.01);
                    v.width_px = layout.body.width() * ppp;
                    v.height_px = layout.body.height() * ppp;
                    v.area_px = v.width_px * v.height_px;
                    v.on_screen = true;
                }
            }
            let (state, renderable) =
                self.resolve_web_state(*id, portal, workbook.as_deref(), host_ok);
            let view = self.web.views.get_mut(id).expect("view inserted above");
            // A portal that already has pixels keeps saying `poster` rather
            // than regressing to `unknown` while it waits for its slot.
            view.state = state;
            candidates.push(Candidate {
                id: *id,
                height_px: view.height_px,
                area_px: view.area_px,
                on_screen: view.on_screen,
                focused: self.web.focused == Some(*id),
                last_focus: view.last_focus,
                renderable,
                was_live: view.live,
            });
        }

        let admitted = admit(&candidates, LIVE_POOL);
        let admitted_set: HashSet<NodeId> = admitted.iter().copied().collect();
        for (id, portal, rect) in &portals {
            let want_live = admitted_set.contains(id);
            let was_live = self.web.views.get(id).is_some_and(|v| v.live);
            if want_live {
                // Admit every frame while eligible: WebView2's environment is
                // asynchronous, and a one-shot call that lands before it is
                // ready would otherwise leave the portal stuck on Loading with
                // `live` already true and no further admit.
                let layout_rect = self.web_layout_world(*id, *rect, ctx);
                if let Some(req) =
                    self.web_request(ctx, *id, portal, workbook.as_deref(), layout_rect)
                {
                    self.web.host.admit(*id, &req);
                    if let Some(v) = self.web.views.get_mut(id) {
                        v.live = true;
                        if std::mem::take(&mut v.reload_pending) {
                            let _ = self.web.host.reload(*id);
                        }
                    }
                    self.remember_web_visit(*id);
                }
            } else if was_live {
                // Last uploaded frame only — no D3D11 readback on the frame
                // loop (D21). Live frames already land through the upload budget.
                self.remember_web_visit(*id);
                // The existing texture already owns the last accepted frame.
                // Do not replace it with a teardown capture or an unbudgeted upload.
                self.web.host.evict(*id);
                if let Some(v) = self.web.views.get_mut(id) {
                    v.live = false;
                }
            }
        }

        // Spend the upload budget, newest-first, carrying the rest forward.
        let mut pending: Vec<NodeId> = std::mem::take(&mut self.web.backlog);
        for id in admitted {
            if !pending.contains(&id) {
                pending.push(id);
            }
        }
        if let Some(fid) = self.web.focused {
            if pending.contains(&fid) {
                pending.retain(|id| *id != fid);
                pending.insert(0, fid);
            }
        }
        let mut carried = Vec::new();
        let mut next_upload_due: Option<Duration> = None;
        let now = Instant::now();
        for id in pending {
            if self.web.uploads_this_frame >= UPLOADS_PER_FRAME {
                carried.push(id);
                next_upload_due = Some(Duration::ZERO);
                continue;
            }
            let interactive = self.web.focused == Some(id) && self.web.recently_interactive();
            let frame_interval = self.web.focused_frame_interval(id);
            let Some(view) = self.web.views.get_mut(&id) else {
                continue;
            };
            if !view.live || (view.height_px < LOD_STRIP_PX && self.web.focused != Some(id)) {
                continue;
            }
            if !interactive {
                if let Some(last) = view.last_frame_probe {
                    let elapsed = last.elapsed();
                    if elapsed < frame_interval {
                        carried.push(id);
                        let remaining = frame_interval - elapsed;
                        next_upload_due =
                            Some(next_upload_due.map_or(remaining, |d| d.min(remaining)));
                        continue;
                    }
                }
            }
            view.last_frame_probe = Some(now);
            let _ = view;
            if let Some(img) = self.web.host.take_frame(id) {
                self.upload_poster(ctx, id, img);
                self.web.uploads_this_frame += 1;
            } else if self.web.views.get(&id).is_some_and(|v| v.live) {
                carried.push(id);
                next_upload_due =
                    Some(next_upload_due.map_or(frame_interval, |d| d.min(frame_interval)));
            }
        }
        if !carried.is_empty() {
            let cap = atlas_core::display::WEB_BACKLOG_CAP;
            if carried.len() > cap {
                let drop = carried.len() - cap;
                carried.drain(0..drop);
            }
            self.web.backlog = carried;
            ctx.request_repaint_after(next_upload_due.unwrap_or(Duration::ZERO));
        }
        if self.web.views.values().any(|v| v.live) {
            let after = if let Some(fid) = self.web.focused {
                self.web.focused_frame_interval(fid)
            } else {
                Duration::from_secs_f32(1.0 / IDLE_FPS)
            };
            ctx.request_repaint_after(after);
        }
        // Painting renews visibility. A culled/hidden portal must not keep a
        // stale on-screen flag (and browser slot) forever.
        for view in self.web.views.values_mut() {
            view.on_screen = false;
        }
    }

    fn upload_poster(&mut self, ctx: &egui::Context, id: NodeId, img: impl Into<WebFrame>) {
        let img: WebFrame = img.into();
        if !web_frame_has_content(&img) {
            return;
        }
        if let Some(v) = self.web.views.get_mut(&id) {
            if let Some(tex) = &mut v.poster {
                tex.set(img, egui::TextureOptions::LINEAR);
            } else {
                v.poster = Some(ctx.load_texture(
                    format!("slate-web-{}", id.0),
                    img,
                    egui::TextureOptions::LINEAR,
                ));
            }
            v.poster_at = Some(Instant::now());
        }
    }

    /// Apply finished local-source probes. Generation-tagged so a rebind that
    /// happened while the worker was out is discarded rather than painted.
    fn drain_source_polls(&mut self) {
        while let Ok(poll) = self.web.poll_rx.try_recv() {
            let Some(v) = self.web.views.get_mut(&poll.id) else {
                continue;
            };
            if v.generation != poll.generation {
                continue;
            }
            v.poll_inflight = false;
            v.source_exists = Some(poll.exists);
            let changed = v.source_mtime.is_some() && v.source_mtime != poll.mtime;
            v.source_mtime = poll.mtime;
            if changed {
                v.poster_at = None;
                v.reload_pending = true;
            }
        }
    }

    /// What state a portal is in right now, and whether it is worth a webview.
    fn resolve_web_state(
        &mut self,
        id: NodeId,
        portal: &PortalNode,
        workbook: Option<&Path>,
        host_ok: bool,
    ) -> (WebState, bool) {
        let Some(source) = portal.source.as_ref() else {
            return (WebState::Unbound, false);
        };
        let locator = source.locator.clone();
        let looks_like_dir = !locator.to_ascii_lowercase().ends_with(".html")
            && !locator.to_ascii_lowercase().ends_with(".htm");
        let kind = match classify_web_locator(&locator, looks_like_dir) {
            Ok(kind) => kind,
            Err(WebRefusal::NotHtml(_)) | Err(WebRefusal::Empty) => {
                // A bare path that is neither HTML nor a directory we know
                // about: say what we tried rather than guessing (D30).
                return (
                    WebState::Missing {
                        locator: locator.clone(),
                    },
                    false,
                );
            }
            Err(reason) => {
                return (
                    WebState::Refused {
                        reason: reason.to_string(),
                    },
                    false,
                )
            }
        };

        if kind == WebSourceKind::Remote {
            if let Some(origin) = web_origin(&locator) {
                if !self.web.has_consent(&origin) {
                    return (WebState::Blocked { origin }, false);
                }
            }
        } else {
            // Local staleness is a worker-interval poll, never a per-frame
            // `metadata` (Art. II.2) — a share can take seconds and would freeze
            // the window. `Missing` keeps the last poster and names the path.
            let due = self
                .web
                .views
                .get(&id)
                .and_then(|v| v.last_poll)
                .is_none_or(|t| t.elapsed().as_secs_f32() >= POLL_SECS);
            let inflight = self.web.views.get(&id).is_some_and(|v| v.poll_inflight);
            if due && !inflight {
                let path = resolve_web_source(workbook, &locator);
                let generation = self.web.views.get(&id).map(|v| v.generation).unwrap_or(0);
                let tx = self.web.poll_tx.clone();
                if let Some(v) = self.web.views.get_mut(&id) {
                    v.last_poll = Some(Instant::now());
                    v.poll_inflight = true;
                }
                std::thread::spawn(move || {
                    let probe = std::fs::metadata(&path).ok();
                    let _ = tx.send(SourcePoll {
                        id,
                        generation,
                        exists: probe.is_some(),
                        mtime: probe.and_then(|m| m.modified().ok()),
                    });
                });
            }
            if self.web.views.get(&id).and_then(|v| v.source_exists) == Some(false) {
                return (WebState::Missing { locator }, false);
            }
        }

        if !host_ok {
            return (WebState::NoRuntime, false);
        }
        let view = self.web.views.get(&id);
        let height = view.map(|v| v.height_px).unwrap_or(0.0);
        let on_screen = view.is_some_and(|v| v.on_screen);
        // Focus is the human overriding the size budget, so the state has to
        // agree with what the pool will do.
        let focused = self.web.focused == Some(id);
        if let Some(err) = self.web.host.load_error(id) {
            return (WebState::Missing { locator: err }, true);
        }
        if !portal.web_ref().interactive_allowed {
            let captured = view.and_then(|v| v.poster_at);
            return (
                captured
                    .map(|captured| WebState::Poster { captured })
                    .unwrap_or(WebState::Loading),
                false,
            );
        }
        if !on_screen
            || (!focused && !view.is_some_and(|v| v.live) && lod_for(height) != WebLod::Eligible)
        {
            let state = if height > 0.0 && height < LIVE_MIN_PX && on_screen {
                WebState::TooSmall
            } else {
                view.and_then(|v| v.poster_at)
                    .map(|captured| WebState::Poster { captured })
                    .unwrap_or(WebState::Unknown)
            };
            return (state, true);
        }
        let live = view.is_some_and(|v| v.live);
        let has_pixels = view.and_then(|v| v.poster_at).is_some();
        // Eligible but unpooled reads as `Budgeted` whether or not it has a
        // last frame to show: the state is about the slot, and the poster the
        // card paints is a separate question.
        let state = match (live, has_pixels) {
            (true, true) => WebState::Live,
            (true, false) => WebState::Loading,
            (false, _) => WebState::Budgeted,
        };
        (state, true)
    }

    fn web_request(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        portal: &PortalNode,
        workbook: Option<&Path>,
        rect: slate_doc::scene::WorldRect,
    ) -> Option<WebRequest> {
        let locator = portal.source.as_ref()?.locator.clone();
        let web = portal.web_ref();
        let looks_like_dir = !locator.to_ascii_lowercase().ends_with(".html")
            && !locator.to_ascii_lowercase().ends_with(".htm");
        let kind = classify_web_locator(&locator, looks_like_dir).ok()?;
        let authored = match kind {
            WebSourceKind::Remote => locator,
            WebSourceKind::LocalFile => resolve_web_source(workbook, &locator)
                .to_string_lossy()
                .into_owned(),
            WebSourceKind::LocalDir => resolve_web_source(workbook, &locator)
                .join(&web.entry)
                .to_string_lossy()
                .into_owned(),
        };
        let target = self.resume_target(id, &authored);
        let (width_css, height_css) = css_size(&web, rect);
        let display = ctx.screen_rect().size() * ctx.pixels_per_point();
        let (raster_w, raster_h) = self
            .web
            .views
            .get(&id)
            .filter(|v| v.width_px > 2.0 && v.height_px > 2.0)
            .map(|v| raster_for(width_css, height_css, v.width_px, v.height_px, display))
            .unwrap_or_else(|| raster_for(width_css, height_css, 0.0, 0.0, display));
        let link_script = if matches!(kind, WebSourceKind::Remote) {
            String::new()
        } else {
            self.dashboard_link_script(id)
        };
        Some(WebRequest {
            target,
            kind,
            width_css,
            height_css,
            raster_w,
            raster_h,
            link_script,
        })
    }

    /// Tables wired into this page, as the script the host runs after load.
    fn dashboard_link_script(&mut self, id: NodeId) -> String {
        use slate_doc::agent_inputs::{endpoint_node, InputKind};
        use slate_doc::scene::NodeKind;
        let mut wires = Vec::new();
        for node in &self.doc().scene.nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            let Some(binding) = &conn.binding else {
                continue;
            };
            if binding.kind != InputKind::Table {
                continue;
            }
            let (source, target) = if binding.input_b {
                (&conn.a, &conn.b)
            } else {
                (&conn.b, &conn.a)
            };
            if endpoint_node(target) != Some(id) {
                continue;
            }
            let Some(source_id) = endpoint_node(source) else {
                continue;
            };
            let slot = binding
                .slot
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "data".into());
            let order = if binding.order.is_empty() {
                vec![node.id.0]
            } else {
                binding.order.clone()
            };
            wires.push((order, slot, source_id));
        }
        wires.sort_by(|a, b| a.0.cmp(&b.0));
        let mut inputs = serde_json::Map::new();
        let mut used = std::collections::HashSet::new();
        for (_, mut slot, source_id) in wires {
            if used.contains(&slot) {
                slot = format!("{slot}-{}", used.len() + 1);
            }
            used.insert(slot.clone());
            let value = self.linked_table_value(source_id).unwrap_or_else(
                || serde_json::json!({ "health": "missing", "columns": [], "rows": [] }),
            );
            inputs.insert(slot, value);
        }
        atlas_agent::slate_link_script(&serde_json::Value::Object(inputs))
    }

    fn linked_table_value(
        &mut self,
        source: slate_doc::scene::NodeId,
    ) -> Option<serde_json::Value> {
        let path = {
            let doc = self.doc();
            let node = doc.scene.node(source)?;
            let slate_doc::scene::NodeKind::Image(image) = &node.kind else {
                return None;
            };
            doc.item(image.item)?.path.clone()
        };
        if atlas_core::cloud::is_dehydrated(&path) {
            return Some(serde_json::json!({ "health": "missing", "columns": [], "rows": [] }));
        }
        let meta = std::fs::metadata(&path).ok()?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let len = meta.len();
        if let Some((checked, cached_mtime, cached_len, value)) = self.web.link_cache.get(&path) {
            if checked.elapsed().as_secs() < 1 && *cached_mtime == mtime && *cached_len == len {
                return Some(value.clone());
            }
        }
        let table = atlas_core::table::read_linked_table(&path)?;
        let value = serde_json::json!({
            "health": "ok",
            "columns": table.columns,
            "rows": table.rows,
        });
        self.web
            .link_cache
            .insert(path, (std::time::Instant::now(), mtime, len, value.clone()));
        Some(value)
    }

    /// While maximized the page is laid out at the screen's aspect, not the
    /// authored frame's (P1.portal.maximize).
    fn web_layout_world(
        &self,
        id: NodeId,
        node_rect: slate_doc::scene::WorldRect,
        ctx: &egui::Context,
    ) -> slate_doc::scene::WorldRect {
        if self.portal_chrome.maximized != Some(id) {
            return node_rect;
        }
        let screen = ctx.screen_rect();
        let layout = layout_portal_chrome(screen, self.portal_chrome_collapsed(id), true, 1.0);
        slate_doc::scene::WorldRect::new(
            0.0,
            0.0,
            layout.body.width().max(1.0),
            layout.body.height().max(1.0),
        )
    }

    fn remember_web_visit(&mut self, id: NodeId) {
        let Some(url) = self.web.host.current_url(id) else {
            return;
        };
        if let Some(v) = self.web.views.get_mut(&id) {
            v.resume_url = Some(url);
        }
    }

    fn resume_target(&self, id: NodeId, authored: &str) -> String {
        let Some(url) = self
            .web
            .views
            .get(&id)
            .and_then(|v| v.resume_url.as_deref())
        else {
            return authored.to_string();
        };
        if same_page(authored, url) {
            return authored.to_string();
        }
        if classify_web_locator(url, false).is_ok() {
            return url.to_string();
        }
        authored.to_string()
    }

    pub(crate) fn web_visiting_label(&self, id: NodeId, portal: &PortalNode) -> Option<String> {
        self.web_display_url(id)
            .filter(|url| {
                portal
                    .source
                    .as_ref()
                    .is_none_or(|s| !same_page(&s.locator, url))
            })
            .map(|url| web_display_locator(&url))
            .or_else(|| {
                portal
                    .source
                    .as_ref()
                    .map(|s| web_display_locator(&s.locator))
            })
    }

    /// The locator shown on the tab / copied by `portal.web.copy_url`.
    pub(crate) fn web_display_url(&self, id: NodeId) -> Option<String> {
        if let Some(url) = self.web.host.current_url(id) {
            return Some(url);
        }
        if let Some(url) = self.web.views.get(&id).and_then(|v| v.resume_url.clone()) {
            return Some(url);
        }
        self.doc().scene.node(id).and_then(|n| match &n.kind {
            NodeKind::Portal(p) => p.source.as_ref().map(|s| s.locator.clone()),
            _ => None,
        })
    }

    fn web_url_target(&self, id: Option<NodeId>) -> Option<NodeId> {
        id.or(self.portal_chrome.maximized)
            .or(self.web.focused)
            .or_else(|| self.selected_web_portal())
    }

    pub(crate) fn web_copy_url(&mut self, ctx: &egui::Context) -> bool {
        self.web_copy_url_of(ctx, None)
    }

    pub(crate) fn web_copy_url_of(&mut self, ctx: &egui::Context, id: Option<NodeId>) -> bool {
        let Some(id) = self.web_url_target(id) else {
            return false;
        };
        let Some(url) = self.web_display_url(id) else {
            self.toast("This portal has no URL yet.");
            return true;
        };
        ctx.copy_text(url);
        self.toast("Copied URL");
        true
    }

    #[cfg(test)]
    pub(crate) fn web_paste_url_text(&mut self, text: &str) -> bool {
        self.web_paste_url_text_of(None, text)
    }

    pub(crate) fn web_paste_url_text_of(&mut self, id: Option<NodeId>, text: &str) -> bool {
        let Some(id) = self.web_url_target(id) else {
            return false;
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return false;
        }
        if classify_web_locator(trimmed, false).is_err() && web_origin(trimmed).is_none() {
            self.toast("Clipboard is not a URL or HTML path.");
            return true;
        }
        self.bind_web_source(id, trimmed.to_string())
    }

    pub(crate) fn web_paste_url(&mut self) -> bool {
        self.web_paste_url_of(None)
    }

    pub(crate) fn web_paste_url_of(&mut self, id: Option<NodeId>) -> bool {
        let text = self
            .pending_paste_text
            .clone()
            .or_else(os_clipboard_text)
            .unwrap_or_default();
        if text.trim().is_empty() {
            self.toast("Clipboard is empty.");
            return true;
        }
        self.web_paste_url_text_of(id, &text)
    }

    /// Record what the board just painted, so the next pump's admission has
    /// real on-screen sizes to sort by.
    pub(crate) fn note_web_geometry(&mut self, id: NodeId, srect: Rect, clip: Rect, ppp: f32) {
        if let Some(v) = self.web.views.get_mut(&id) {
            let ppp = ppp.max(0.01);
            v.width_px = srect.width() * ppp;
            v.height_px = srect.height() * ppp;
            v.area_px = v.width_px * v.height_px;
            v.on_screen = clip.intersects(srect);
        }
    }

    /// Take input focus (D22). Rendering is unaffected — this is only about
    /// where pointer and keyboard go.
    pub(crate) fn web_focus(&mut self, id: NodeId) {
        self.portal_enter_interactive(id);
    }

    pub(crate) fn web_enter_contents(&mut self, id: NodeId) {
        if self
            .doc()
            .scene
            .node(id)
            .is_some_and(|n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Web))
        {
            self.web.focused = Some(id);
            if let Some(v) = self.web.views.get_mut(&id) {
                v.last_focus = Some(Instant::now());
            }
        }
    }

    /// Release input focus without tearing the page down: scroll position,
    /// form contents, and running charts survive (D12).
    pub(crate) fn web_blur(&mut self) -> bool {
        if self.web.focused.is_some() {
            self.contents_blur()
        } else {
            false
        }
    }

    pub(crate) fn web_leave_contents(&mut self) -> bool {
        let had = self.web.focused.take();
        if let Some(id) = had {
            // Let the page settle its hover and drag state instead of freezing
            // mid-gesture.
            if std::mem::take(&mut self.web.pointer_inside) {
                self.web.host.send_input(id, WebInput::Leave);
            }
            self.web.pointer_down = 0;
            self.web.host.release_keyboard();
        }
        had.is_some()
    }

    /// A board text field is taking the caret; the keyboard must come with it.
    pub(crate) fn web_release_keyboard(&self) {
        self.web.host.release_keyboard();
    }

    /// `portal.web.back` / `portal.web.forward` — in-page history for the
    /// focused or selected portal. Derived state: following a link changes what
    /// the frame shows, never what the workbook says (Art. V.3).
    pub(crate) fn web_history_step(&mut self, forward: bool) -> bool {
        let Some(id) = self.web.focused.or_else(|| self.selected_web_portal()) else {
            return false;
        };
        if forward {
            self.web.host.go_forward(id)
        } else {
            self.web.host.go_back(id)
        }
    }

    /// `portal.web.home` — navigate the live derived view back to the authored
    /// locator. The node is untouched: page history and current URL stay
    /// derived state (D15 / D31).
    pub(crate) fn web_home_selected(&mut self) -> bool {
        let Some(id) = self.web.focused.or_else(|| self.selected_web_portal()) else {
            return false;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let NodeKind::Portal(portal) = &node.kind else {
            return false;
        };
        let Some(locator) = portal.source.as_ref().map(|s| s.locator.clone()) else {
            return false;
        };
        if let Some(origin) = web_origin(&locator) {
            if !self.web.has_consent(&origin) {
                self.toast(format!("{origin} is blocked — allow it before going home."));
                return false;
            }
        }
        if let Some(v) = self.web.views.get_mut(&id) {
            v.resume_url = None;
        }
        if !self.web.host.navigate(id, &locator) {
            self.web.host.evict(id);
            self.web.views.remove(&id);
        }
        true
    }

    /// Permit the selected portal's origin and let it load (D32). Local state,
    /// never journaled — which is why this is not a `SceneCmd`.
    pub(crate) fn web_allow_origin(&mut self, id: NodeId) -> Option<String> {
        let node = self.doc().scene.node(id)?;
        let NodeKind::Portal(p) = &node.kind else {
            return None;
        };
        let origin = web_origin(&p.source.as_ref()?.locator)?;
        self.web.grant_consent(origin.clone());
        Some(origin)
    }

    /// Bind or rebind a locator. One journaled `Patch`; the cached poster goes
    /// with the old locator (D19, D21).
    pub(crate) fn bind_web_source(&mut self, id: NodeId, locator: String) -> bool {
        self.bind_web_source_with_entry(id, locator, None)
    }

    /// Like [`Self::bind_web_source`], optionally setting the directory entry
    /// file in the same undo step.
    pub(crate) fn bind_web_source_with_entry(
        &mut self,
        id: NodeId,
        locator: String,
        entry: Option<String>,
    ) -> bool {
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let NodeKind::Portal(portal) = &node.kind else {
            return false;
        };
        let mut after = node.clone();
        let NodeKind::Portal(p) = &mut after.kind else {
            return false;
        };
        if p.title.is_empty() || portal.source.is_none() {
            p.title = web_display_locator(&locator);
        }
        p.source = Some(SourceUri {
            locator: locator.clone(),
        });
        let mut web = p.web_ref();
        if let Some(entry) = entry {
            web.entry = entry;
        }
        p.web = Some(web);
        let committed = self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(node),
            after: Box::new(after),
        }]);
        if committed {
            // Typing, pasting, or dropping a locator *is* the permission for
            // that origin: asking again on the frame would be theatre (D32).
            // What still asks is the case that matters — a workbook reopened
            // from disk, where nobody has said yet that these pages may run.
            self.grant_web_consent(&locator);
            self.web.host.evict(id);
            self.web.views.remove(&id);
        }
        committed
    }

    /// Permit the origin of a human-supplied locator for this session. Local
    /// files have no origin and need no consent.
    pub(crate) fn grant_web_consent(&mut self, locator: &str) {
        if let Some(origin) = web_origin(locator) {
            self.web.grant_consent(origin);
        }
    }

    /// Bind a local path, storing it workbook-relative where possible. A folder
    /// records whichever entry file it actually holds (`index.html` or
    /// `index.htm`), so a dashboard that only ships the latter still loads.
    pub(crate) fn bind_web_path(&mut self, id: NodeId, path: PathBuf) -> bool {
        let workbook = self.tab().path.clone();
        let locator = web_source_locator(workbook.as_deref(), &path);
        let entry = if path.is_dir() {
            web_entry_for_dir(&path).map(|s| s.to_string())
        } else {
            None
        };
        self.bind_web_source_with_entry(id, locator, entry)
    }

    /// Request another capture while retaining the last good poster (D21).
    /// The page a web portal shows, as a picture for an agent run: the newest
    /// frame, else a one-off capture. Pixels only; the page's DOM, cookies and
    /// storage stay out of reach (D15, D27). `Ok(None)` while nothing is
    /// captured yet.
    pub(crate) fn capture_web_page(
        &mut self,
        id: NodeId,
        dir: &std::path::Path,
    ) -> Result<Option<PathBuf>, String> {
        let frame = self
            .web
            .host
            .last_frame(id)
            .or_else(|| self.web.host.capture_poster(id));
        let Some(frame) = frame else {
            if !self.web.host.available() {
                return Err(
                    "This page cannot be captured: the WebView2 runtime is not available.".into(),
                );
            }
            return Ok(None);
        };
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("web-{}.png", id.0));
        super::model3d::write_fast_png(&path, &frame, false)?;
        Ok(Some(path))
    }

    /// The page's visible text for an agent run the human started, read once
    /// and read-only (D15 / D27 amendment). `Ok(None)` while it is being read.
    pub(crate) fn read_web_text(&mut self, id: NodeId) -> Result<Option<String>, String> {
        /// Enough for a long article; a local model's context is the limit.
        const MAX_CHARS: usize = 30_000;
        if let Some(read) = self.web.host.take_text(id) {
            self.web.text_pending.remove(&id);
            let text = read?;
            let text = text.trim();
            if text.is_empty() {
                return Err("The page shows no text to read.".into());
            }
            let mut out: String = text.chars().take(MAX_CHARS).collect();
            if text.chars().count() > MAX_CHARS {
                out.push_str("\n\n[The page continues; the rest was left out.]");
            }
            return Ok(Some(out));
        }
        if self.web.text_pending.contains(&id) {
            return Ok(None);
        }
        if !self.web.host.available() {
            return Err("This page cannot be read: the WebView2 runtime is not available.".into());
        }
        if !self.web.host.request_text(id) {
            return Err("Bring the page on screen so it loads, then run again.".into());
        }
        self.web.text_pending.insert(id);
        Ok(None)
    }

    pub(crate) fn web_recapture(&mut self, id: NodeId) {
        if let Some(v) = self.web.views.get_mut(&id) {
            v.last_frame_probe = None;
        }
    }

    /// Split a drop into "these become web portals" and "these stay ordinary
    /// items", placing a bound portal for each of the former (D01).
    ///
    /// Returns the paths the normal drop path should still handle, so a mixed
    /// drop of a dashboard and three photos does the right thing with both.
    pub(crate) fn divert_web_drops(&mut self, paths: &[PathBuf], at: egui::Pos2) -> Vec<PathBuf> {
        let mut rest = Vec::new();
        let mut placed = 0usize;
        for path in paths {
            // Folders go through the lens chooser (P1.portal.folder-drop).
            // An HTML *file* is still a page, not a folder of files.
            if path.is_dir() || !is_web_drop(path) {
                rest.push(path.clone());
                continue;
            }
            // Fan multiple pages out rather than stacking them exactly.
            let offset = placed as f32 * 24.0;
            let rect = slate_doc::scene::WorldRect::new(
                at.x - PORTAL_W * 0.5 + offset,
                at.y - PORTAL_H * 0.5 + offset,
                PORTAL_W,
                PORTAL_H,
            );
            let workbook = self.tab().path.clone();
            let locator = web_source_locator(workbook.as_deref(), path);
            let entry = if path.is_dir() {
                web_entry_for_dir(path).map(|s| s.to_string())
            } else {
                None
            };
            self.add_web_portal_with_entry(rect, Some(locator), entry, "dropped");
            placed += 1;
        }
        rest
    }

    /// Pasted clipboard text that names a page becomes a portal at the pointer
    /// (D01). Returns false for anything else, so the ordinary paste continues.
    pub(crate) fn paste_web_url(&mut self, text: &str, at: egui::Pos2) -> bool {
        self.place_web_url(text, at, "pasted")
    }

    fn place_web_url(&mut self, text: &str, at: egui::Pos2, detail: &'static str) -> bool {
        let Some(trimmed) = web_url_text(text) else {
            return false;
        };
        let rect = slate_doc::scene::WorldRect::new(
            at.x - PORTAL_W * 0.5,
            at.y - PORTAL_H * 0.5,
            PORTAL_W,
            PORTAL_H,
        );
        self.add_web_portal(rect, Some(trimmed.to_string()), detail);
        true
    }

    pub(crate) fn web_drop_enabled(&self) -> bool {
        !self.at_home
            && !self.tab().read_only
            && self.presenting.is_none()
            && self.doc().view.active_view == slate_doc::ViewKind::Board
    }

    /// External links use the existing source/placement commands, with OS drop
    /// coordinates (egui may not receive pointer motion during an OLE drag).
    pub(crate) fn drop_web_url(&mut self, text: &str, screen: Pos2) -> bool {
        if !self.web_drop_enabled() || !self.canvas_rect.contains(screen) {
            return false;
        }
        let Some(url) = web_url_text(text) else {
            return false;
        };
        let at = self.board_xf().s2w(screen);
        let hit = self.portal_chrome.maximized.or_else(|| {
            super::board_path::board_pick_node_routed(
                &self.doc().scene,
                at.x,
                at.y,
                self.tab().cam.z,
                true,
                self.board_wire_routing,
            )
        });
        if let Some(node) = hit.and_then(|id| self.doc().scene.node(id)) {
            if matches!(&node.kind, NodeKind::Portal(p) if p.kind == PortalKind::Web) {
                if node.locked {
                    return false;
                }
                return self.bind_web_source(node.id, url.to_string());
            }
        }
        if self.portal_chrome.maximized.is_some() {
            return false;
        }
        self.place_web_url(url, at, "dropped")
    }

    /// What the artifact writer needs from every web portal: local material to
    /// package, and posters for the pages it cannot package.
    ///
    /// Locator resolution happens here rather than in `slate-artifact`, which
    /// has no business knowing where the workbook lives.
    pub(crate) fn export_web_maps(
        &mut self,
    ) -> (
        std::collections::BTreeMap<NodeId, PathBuf>,
        std::collections::BTreeMap<NodeId, PathBuf>,
    ) {
        let mut sources = std::collections::BTreeMap::new();
        let mut posters = std::collections::BTreeMap::new();
        let workbook = self.tab().path.clone();
        for (id, portal, _) in self.web_portals() {
            let Some(locator) = portal.source.as_ref().map(|s| s.locator.clone()) else {
                continue;
            };
            if web_origin(&locator).is_none() {
                let path = resolve_web_source(workbook.as_deref(), &locator);
                // The webview folder and the secret store are this user's
                // sign-in, not workbook material (Art. IX.1).
                if path.exists() && !atlas_core::secrets::is_machine_private(&path) {
                    sources.insert(id, path);
                }
            }
            // A poster is only worth writing for what will not be packaged.
            if sources.contains_key(&id) {
                continue;
            }
            if let Some(img) = self.web.host.capture_poster(id) {
                if let Some(path) = self.write_poster_png(id, img) {
                    posters.insert(id, path);
                }
            }
        }
        (sources, posters)
    }

    /// The selected web portal, if exactly the kind these commands act on.
    pub(crate) fn selected_web_portal(&self) -> Option<NodeId> {
        self.board_sel.iter().copied().find(|id| {
            self.doc().scene.node(*id).is_some_and(
                |n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Web),
            )
        })
    }

    fn selected_web_locator(&self) -> Option<(NodeId, String)> {
        let id = self.selected_web_portal()?;
        let node = self.doc().scene.node(id)?;
        let NodeKind::Portal(p) = &node.kind else {
            return None;
        };
        Some((id, p.source.as_ref()?.locator.clone()))
    }

    /// `portal.web.source` with a locator detail — the agent / palette path.
    /// Accepts an http(s) URL or a filesystem path; folders pick their entry.
    pub(crate) fn web_bind_source_for_selection(&mut self, source: &str) -> bool {
        let Some(id) = self.selected_web_portal() else {
            return false;
        };
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return false;
        }
        if web_origin(trimmed).is_some() {
            return self.bind_web_source(id, trimmed.to_string());
        }
        let path = PathBuf::from(trimmed);
        if path.exists() {
            return self.bind_web_path(id, path);
        }
        // A relative path that is not yet on disk still binds as a locator so
        // the Missing state can name what was asked for (D30).
        self.bind_web_source(id, trimmed.replace('\\', "/"))
    }

    /// `portal.web.source` — pick a local page. URLs are typed into the
    /// inspector field instead; a dialog cannot express one.
    pub(crate) fn web_pick_source_for_selection(&mut self) -> bool {
        let Some(portal) = self.selected_web_portal() else {
            return false;
        };
        if self.picker_rx.is_some() {
            return false;
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("Web page", &["html", "htm"])
                .pick_file();
            let _ = tx.send(super::PickerMsg::WebPortalSource {
                portal,
                path: picked,
            });
        });
        true
    }

    /// `portal.web.source` for a multi-file dashboard: the folder holding the
    /// entry file.
    pub(crate) fn web_pick_folder_for_selection(&mut self) -> bool {
        let Some(portal) = self.selected_web_portal() else {
            return false;
        };
        if self.picker_rx.is_some() {
            return false;
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new().pick_folder();
            let _ = tx.send(super::PickerMsg::WebPortalSource {
                portal,
                path: picked,
            });
        });
        true
    }

    /// `portal.web.allow_origin`.
    pub(crate) fn web_allow_selected_origin(&mut self) -> bool {
        let Some(id) = self.selected_web_portal() else {
            return false;
        };
        match self.web_allow_origin(id) {
            Some(origin) => {
                self.toast(format!("{origin} allowed for this workbook"));
                true
            }
            None => {
                self.toast("Local pages need no permission");
                false
            }
        }
    }

    /// `portal.web.reload` — drop the derived view so the next pump rebuilds
    /// it from the same authored locator. Nothing journaled.
    pub(crate) fn web_reload_selected(&mut self) -> bool {
        let Some(id) = self.web.focused.or_else(|| self.selected_web_portal()) else {
            return false;
        };
        // Reload in place where the host can; otherwise drop the view and let
        // the next admission rebuild it.
        if !self.web.host.reload(id) {
            self.web.host.evict(id);
            self.web.views.remove(&id);
        }
        true
    }

    /// `portal.web.recapture`.
    pub(crate) fn web_recapture_selected(&mut self) -> bool {
        let Some(id) = self.selected_web_portal() else {
            return false;
        };
        self.web_recapture(id);
        true
    }

    /// `portal.web.focus` — toggles, so the palette entry and Esc agree.
    pub(crate) fn web_toggle_focus(&mut self) -> bool {
        if self.web.focused.is_some() {
            return self.web_blur();
        }
        match self.selected_web_portal() {
            Some(id) => {
                self.web_focus(id);
                true
            }
            None => false,
        }
    }

    /// `portal.web.open_external` — hand the locator to the system browser,
    /// which is the honest place for a page Slate is only framing.
    pub(crate) fn web_open_external(&mut self) -> bool {
        let Some((_, locator)) = self.selected_web_locator() else {
            return false;
        };
        let workbook = self.tab().path.clone();
        let target = if web_origin(&locator).is_some() {
            locator
        } else {
            resolve_web_source(workbook.as_deref(), &locator)
                .to_string_lossy()
                .into_owned()
        };
        match open_externally(&target) {
            Ok(()) => true,
            Err(e) => {
                self.toast(format!("Could not open {target}: {e}"));
                false
            }
        }
    }

    /// `portal.web.bake` — one journaled batch adding the captured poster plus
    /// a provenance note. The portal stays: bake copies, it does not convert
    /// (D25).
    pub(crate) fn web_bake_selected(&mut self) -> bool {
        let Some(id) = self.selected_web_portal() else {
            return false;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let NodeKind::Portal(portal) = &node.kind else {
            return false;
        };
        let Some(locator) = portal.source.as_ref().map(|s| s.locator.clone()) else {
            self.toast("Nothing to bake — bind a page first.");
            return true;
        };
        // Ask for pixels now rather than holding every poster's bitmap in
        // memory for a bake that may never happen.
        let Some(img) = self.web.host.capture_poster(id) else {
            self.toast("Nothing to bake — no captured page yet.");
            return true;
        };
        let Some(path) = self.write_poster_png(id, img) else {
            self.toast("Could not write the poster into the workbook.");
            return true;
        };
        let items = self.add_paths(&[path]);
        let Some(item) = items.first().copied() else {
            self.toast("Could not add the baked poster to the workbook.");
            return true;
        };

        let rect = node.rect;
        let captured = chrono_stamp();
        let kind = if web_origin(&locator).is_some() {
            "live page"
        } else {
            "local page"
        };
        let image = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
        );
        let note = self.doc_mut().scene.build_node(
            slate_doc::scene::WorldRect::new(
                rect.x,
                rect.y + rect.h + 8.0,
                rect.w.max(120.0),
                20.0,
            ),
            NodeKind::Text(slate_doc::scene::TextNode {
                text: format!("{locator} · {kind} · captured {captured}"),
                family: slate_doc::scene::Typeface::Sans,
                size: 12.0,
                color: slate_doc::scene::Rgba::opaque(198, 208, 224),
                align: slate_doc::scene::TextAlign::Left,
                fill: None,
                agent: None,
            }),
        );
        let ids = self.add_nodes(vec![image, note]);
        self.board_sel = ids.into_iter().collect();
        self.toast("Baked the poster and its provenance; the portal is still live.");
        true
    }

    fn write_poster_png(&mut self, id: NodeId, img: WebFrame) -> Option<PathBuf> {
        let dir = self
            .tab()
            .path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.join("assets")))
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join(format!("web-poster-{}.png", id.0));
        let [w, h] = [img.width() as u32, img.height() as u32];
        let rgba: Vec<u8> = img
            .pixels
            .iter()
            .flat_map(|p| [p.r(), p.g(), p.b(), p.a()])
            .collect();
        image::RgbaImage::from_raw(w, h, rgba)?.save(&path).ok()?;
        Some(path)
    }

    /// `portal.web.package` — copy local material beside the workbook and
    /// rebind to the copy. A permanent fork that names its origin (Art. IX.4).
    pub(crate) fn web_package_selected(&mut self) -> bool {
        let Some((id, locator)) = self.selected_web_locator() else {
            return false;
        };
        if web_origin(&locator).is_some() {
            self.toast("Only local pages can be packaged; a URL stays a link.");
            return true;
        }
        let Some(workbook_dir) = self
            .tab()
            .path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        else {
            self.toast("Save the workbook first — packaging copies material beside it.");
            return true;
        };
        let source = resolve_web_source(self.tab().path.as_deref(), &locator);
        if !source.exists() {
            self.toast(format!("Cannot package: {locator} is missing."));
            return true;
        }
        let name = source
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "page".into());
        let dest_dir = workbook_dir.join("assets").join("web").join(&name);
        let result = if source.is_dir() {
            copy_tree(&source, &dest_dir).map(|_| dest_dir.clone())
        } else {
            std::fs::create_dir_all(&dest_dir).and_then(|_| {
                let file = dest_dir.join(source.file_name().unwrap_or_default());
                std::fs::copy(&source, &file).map(|_| file)
            })
        };
        match result {
            Ok(copied) => {
                // The fork names where it came from, on disk, permanently.
                let _ = std::fs::write(
                    dest_dir.join("origin.txt"),
                    format!("{}\n", source.display()),
                );
                let new_locator = web_source_locator(self.tab().path.as_deref(), &copied);
                self.bind_web_source(id, new_locator);
                self.toast("Packaged beside the workbook; the portal now points at the copy.");
                true
            }
            Err(e) => {
                self.toast(format!("Could not package: {e}"));
                true
            }
        }
    }

    /// Route this frame's pointer and keyboard to the focused portal's page.
    ///
    /// Returns whether the board should stand down for this frame. The identity
    /// tab and a border band stay Slate's, so a focused portal can always be
    /// grabbed by its edge and moved (D17). Right-click is Slate's too — it
    /// opens the portal menu rather than the page's.
    pub(crate) fn web_input_frame(
        &mut self,
        ui: &egui::Ui,
        xf: &BoardXf,
        pointer: Option<Pos2>,
    ) -> bool {
        let Some(id) = self.web.focused else {
            return false;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            self.web_blur();
            return false;
        };
        let NodeKind::Portal(ref portal) = node.kind else {
            self.web_blur();
            return false;
        };
        if self.portal_chrome.maximized == Some(id) {
            return false;
        }
        // Escape is owned exclusively by the command cancel stack. Handling
        // it here too would peel focus in the same frame as restoring maximize.
        let srect = xf.rect_w2s(node.rect);
        let mut layout = layout_portal_chrome(srect, self.portal_chrome_collapsed(id), false, xf.z);
        layout.retract_when_idle(ui.ctx(), id, true);
        if pointer.is_some_and(|p| layout.pointer_on_chrome(p)) {
            return false;
        }
        if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary)) {
            if let Some(p) = pointer {
                if srect.contains(p) {
                    self.board_menu = Some((id, p));
                    return true;
                }
            }
        }
        self.web_input_in_layout(ui, id, &node, portal, &layout)
    }

    /// Pointer/keyboard for a focused page whose screen layout is already known.
    pub(crate) fn web_input_in_layout(
        &mut self,
        ui: &egui::Ui,
        id: NodeId,
        node: &Node,
        portal: &PortalNode,
        layout: &PortalChromeLayout,
    ) -> bool {
        let web = portal.web_ref();
        let pointer = ui.ctx().pointer_latest_pos();
        if pointer.is_some_and(|p| layout.pointer_on_chrome(p)) {
            return false;
        }
        if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary)) {
            if let Some(p) = pointer {
                if layout.frame.contains(p) {
                    self.board_menu = Some((id, p));
                    return true;
                }
            }
        }
        let css_rect = if self.portal_chrome.maximized == Some(id) {
            slate_doc::scene::WorldRect::new(
                0.0,
                0.0,
                layout.body.width().max(1.0),
                layout.body.height().max(1.0),
            )
        } else {
            node.rect
        };
        self.web_input_page(ui, id, &web, css_rect, layout, pointer)
    }

    fn web_input_page(
        &mut self,
        ui: &egui::Ui,
        id: NodeId,
        web: &WebPortalRef,
        css_rect: slate_doc::scene::WorldRect,
        layout: &PortalChromeLayout,
        pointer: Option<Pos2>,
    ) -> bool {
        // UVs follow the painted body, not the inset frame hit band. Keep
        // native scrollbar tracks reachable where they overlap that band.
        let page = layout.body;
        let scrollbar_width = atlas_shell::tabs::portal_scrollbar_width(
            layout.bar.or(layout.reveal).map_or(0.0, |r| r.height()),
        );
        let inside = pointer.is_some_and(|p| {
            layout.page.contains(p)
                || (page.contains(p)
                    && (p.x >= page.right() - scrollbar_width
                        || p.y >= page.bottom() - scrollbar_width))
        });
        let dragging = self.web.pointer_down != 0;
        if !inside && !dragging {
            if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
                self.web_blur();
                return false;
            }
            if std::mem::take(&mut self.web.pointer_inside) {
                self.web.host.send_input(id, WebInput::Leave);
                self.web.pointer_down = 0;
            }
            return false;
        }
        self.web.pointer_inside = true;

        let (css_w, css_h) = css_size(web, css_rect);
        let p = pointer.unwrap_or(page.center());
        let u = ((p.x - page.left()) / page.width().max(1.0)).clamp(0.0, 1.0);
        let v = ((p.y - page.top()) / page.height().max(1.0)).clamp(0.0, 1.0);
        let x = u * css_w as f32;
        let y = v * css_h as f32;

        let mut events = Vec::new();
        let mut buttons = self.web.pointer_down;
        ui.input(|i| {
            for (button, bit) in [
                (egui::PointerButton::Primary, 1u8),
                (egui::PointerButton::Middle, 4),
            ] {
                if i.pointer.button_pressed(button) {
                    buttons |= bit;
                }
                if i.pointer.button_released(button) {
                    buttons &= !bit;
                }
            }
            events.push(WebInput::Move { x, y, buttons });
            for (button, code) in [
                (egui::PointerButton::Primary, 0u8),
                (egui::PointerButton::Middle, 2),
            ] {
                if i.pointer.button_pressed(button) {
                    events.push(WebInput::Down { x, y, button: code });
                }
                if i.pointer.button_released(button) {
                    events.push(WebInput::Up { x, y, button: code });
                }
            }
            if !i.modifiers.command {
                let d = i.raw_scroll_delta;
                if d.y != 0.0 {
                    events.push(WebInput::Wheel {
                        x,
                        y,
                        delta: d.y,
                        horizontal: false,
                    });
                }
                if d.x != 0.0 {
                    events.push(WebInput::Wheel {
                        x,
                        y,
                        delta: d.x,
                        horizontal: true,
                    });
                }
            }
        });
        self.web.pointer_down = buttons;
        events.extend(self.web_keyboard_events(ui));
        let interacting = events.iter().any(page_input_is_interactive);
        for event in events {
            self.web.host.send_input(id, event);
        }
        if interacting {
            self.web.note_page_input();
            ui.ctx().request_repaint();
        }
        ui.ctx().set_cursor_icon(
            self.web
                .host
                .cursor(id)
                .unwrap_or(egui::CursorIcon::Default),
        );
        true
    }

    /// Keys the focused page is allowed to hear. Esc is Slate's, always: it is
    /// how focus is peeled back off the page (D22).
    fn web_keyboard_events(&self, ui: &egui::Ui) -> Vec<WebInput> {
        let mut events = Vec::new();
        ui.input(|i| {
            for event in &i.events {
                match event {
                    egui::Event::Key { key, pressed, .. } if *key != egui::Key::Escape => {
                        events.push(WebInput::Key {
                            key: *key,
                            pressed: *pressed,
                        });
                    }
                    egui::Event::Text(t) => {
                        events.extend(t.chars().map(WebInput::Text));
                    }
                    _ => {}
                }
            }
        });
        events
    }

    // -----------------------------------------------------------------------
    // Painting
    // -----------------------------------------------------------------------

    pub(crate) fn paint_web_portal(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let srect = xf.rect_w2s(node.rect);
        self.note_web_geometry(node.id, srect, ui.clip_rect(), ui.ctx().pixels_per_point());
        let collapsed = self.portal_chrome_collapsed(node.id);
        let mut layout = layout_portal_chrome(srect, collapsed, false, xf.z);
        layout.retract_when_idle(ui.ctx(), node.id, self.web.focused == Some(node.id));
        self.paint_web_portal_in_rect(ui, painter, node, portal, &layout, xf.z);
        let visiting = self.web_visiting_label(node.id, portal);
        let focused = self.web.focused == Some(node.id);
        let alpha = node.opacity.clamp(0.0, 1.0);
        let border = if focused {
            Color32::from_rgb(120, 170, 255)
        } else {
            Color32::from_rgba_unmultiplied(140, 150, 175, 150)
        };
        self.paint_portal_shell_finish(
            ui,
            painter,
            &layout,
            node.id,
            portal,
            visiting.as_deref(),
            border.gamma_multiply(alpha),
            focused,
            xf.z,
        );
    }

    pub(crate) fn paint_web_portal_in_rect(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        node: &Node,
        portal: &PortalNode,
        layout: &PortalChromeLayout,
        zoom: f32,
    ) {
        let alpha = node.opacity.clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(alpha);
        let state = self.web.state(node.id);
        let focused = self.web.focused == Some(node.id);
        let css_rect = self.web_layout_world(node.id, node.rect, ui.ctx());
        let (css_width, _) = css_size(&portal.web_ref(), css_rect);
        let chrome_height = if self.portal_is_maximized(node.id) {
            atlas_shell::tokens::current().topbar.height
        } else {
            super::board_portal_chrome::tab_bar_height() * zoom
        };
        let scrollbar_css = atlas_shell::tabs::portal_scrollbar_width(chrome_height)
            * css_width as f32
            / layout.body.width().max(1.0);
        let scrollbar_color = self.palette().sub;
        self.web.host.set_scrollbars(
            node.id,
            layout.bar.is_some() || layout.reveal.is_some(),
            scrollbar_css,
            scrollbar_color,
        );
        self.note_web_geometry(
            node.id,
            layout.frame,
            ui.clip_rect(),
            ui.ctx().pixels_per_point(),
        );

        self.paint_portal_frame_fill(
            painter,
            layout,
            fade(rgba32(portal.fill)),
            Color32::TRANSPARENT,
            focused,
        );

        let body = layout.body;
        if body.width() < 2.0 || body.height() < 2.0 {
            return;
        }
        let clip = body.intersect(layout.frame);
        let poster = self.web.views.get(&node.id).and_then(|v| v.poster.clone());
        if let Some(tex) = poster {
            // A resource-saving still frame is the page, not a disabled slide.
            let stale = matches!(state, WebState::Missing { .. });
            let tint =
                Color32::WHITE.gamma_multiply(if stale { STALE_ALPHA * alpha } else { alpha });
            let outline = board::portal_content_outline(layout.frame, clip, layout.radius);
            if !outline.is_empty() {
                board::textured_polygon(
                    painter,
                    &tex,
                    &outline,
                    clip,
                    slate_doc::scene::Crop::full(),
                    tint,
                );
            }
        } else {
            let clipped = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
            self.paint_web_empty_state(&clipped, clip, &state, alpha, zoom);
        }
    }

    /// Every failure says itself in the frame rather than blanking it (D30).
    fn paint_web_empty_state(
        &self,
        painter: &egui::Painter,
        body: Rect,
        state: &WebState,
        alpha: f32,
        zoom: f32,
    ) {
        let fade = |c: Color32| c.gamma_multiply(alpha);
        let (headline, detail) = match state {
            WebState::Unbound => (
                "Choose page or file…".to_string(),
                "URL, an .html file, or a folder with index.html".to_string(),
            ),
            WebState::Blocked { origin } => (
                format!("{origin} is not permitted yet"),
                "Allow this origin in the Portal inspector".into(),
            ),
            WebState::Missing { locator } => ("Source is missing".to_string(), locator.clone()),
            WebState::Refused { reason } => ("Refused".to_string(), reason.clone()),
            WebState::NoRuntime => (
                "No WebView2 runtime".to_string(),
                "Install the Microsoft Edge WebView2 Evergreen runtime".into(),
            ),
            WebState::TooSmall => ("Zoom in to run this page".to_string(), String::new()),
            WebState::Budgeted => (
                "Waiting for a live slot".to_string(),
                format!("{LIVE_POOL} pages run at once"),
            ),
            WebState::Loading => ("Loading…".to_string(), String::new()),
            _ => ("Resolving…".to_string(), String::new()),
        };
        let head = atlas_shell::canvas_scale::px(15.0, zoom);
        if atlas_shell::canvas_text::legible(head) {
            atlas_shell::canvas_text::text(
                painter,
                body.center() - egui::vec2(0.0, atlas_shell::canvas_scale::px(10.0, zoom)),
                Align2::CENTER_CENTER,
                headline,
                FontId::proportional(head),
                fade(Color32::from_rgb(198, 208, 224)),
            );
        }
        if !detail.is_empty() {
            let sub = atlas_shell::canvas_scale::px(12.0, zoom);
            if atlas_shell::canvas_text::legible(sub) {
                atlas_shell::canvas_text::text(
                    painter,
                    body.center() + egui::vec2(0.0, atlas_shell::canvas_scale::px(12.0, zoom)),
                    Align2::CENTER_CENTER,
                    detail,
                    FontId::proportional(sub),
                    fade(Color32::from_rgb(150, 162, 182)),
                );
            }
        }
    }
}

use slate_doc::scene::{PORTAL_DEFAULT_H as PORTAL_H, PORTAL_DEFAULT_W as PORTAL_W};

/// Whether a dropped path is a page rather than an ordinary board item: an
/// HTML file, or a folder holding an entry file. Only the directory entry is
/// read — nothing here opens a file.
pub fn is_web_drop(path: &Path) -> bool {
    if path.is_dir() {
        return web_entry_for_dir(path).is_some();
    }
    matches!(
        classify_web_locator(&path.to_string_lossy(), false),
        Ok(WebSourceKind::LocalFile)
    )
}

/// Prefer `index.html`, then `index.htm`. Returns `None` when the folder is
/// not a page, so a drop of a photo album stays a photo album.
pub fn web_entry_for_dir(path: &Path) -> Option<&'static str> {
    [slate_doc::scene::WEB_DEFAULT_ENTRY, "index.htm"]
        .into_iter()
        .find(|entry| path.join(entry).is_file())
}

fn os_clipboard_text() -> Option<String> {
    #[cfg(windows)]
    {
        clipboard_win::get_clipboard_string().ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Recursive copy for packaging a dashboard folder.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A capture timestamp for provenance captions. Local date, no dependency.
fn chrono_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Howard Hinnant's days-from-civil, inverted. Kept local so a caption does not
/// pull a date crate into the app.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Hand a URL or path to the platform's own handler.
fn open_externally(target: &str) -> Result<(), std::io::Error> {
    #[cfg(test)]
    {
        let _ = target;
        Ok(())
    }
    #[cfg(all(windows, not(test)))]
    {
        // `explorer` treats both URLs and paths as shell targets, and unlike
        // `cmd /C start` it needs no quoting dance.
        std::process::Command::new("explorer")
            .arg(target)
            .spawn()
            .map(|_| ())
    }
    #[cfg(all(target_os = "macos", not(test)))]
    {
        std::process::Command::new("open")
            .arg(target)
            .spawn()
            .map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos"), not(test)))]
    {
        std::process::Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map(|_| ())
    }
}

/// The identity of a portal's derived state: change the locator or the layout
/// parameters and the cached pixels no longer describe it.
fn view_key(portal: &PortalNode, web: &WebPortalRef) -> String {
    let locator = portal
        .source
        .as_ref()
        .map(|s| s.locator.as_str())
        .unwrap_or_default();
    let zoom = match web.viewport.zoom {
        WebZoom::Fit => "fit".to_string(),
        WebZoom::Fixed(f) => format!("fixed:{f}"),
        WebZoom::Auto => "auto".to_string(),
    };
    format!("{locator}|{}|{}|{zoom}", web.entry, web.viewport.width_css)
}

/// Empty compositor/startup frames must never replace a useful still. Treat
/// fully transparent and uniform black/white clears as unavailable captures.
/// Other solid colours remain valid (including the native capture test page).
pub(super) fn web_frame_has_content(image: &egui::ColorImage) -> bool {
    if image.size[0] == 0 || image.size[1] == 0 || image.pixels.is_empty() {
        return false;
    }
    let first = image.pixels[0];
    let clear = first == Color32::BLACK || first == Color32::WHITE;
    image
        .pixels
        .iter()
        .any(|p| p.a() != 0 && (!clear || *p != first))
}

/// The CSS viewport a page is laid out at. `Auto` hands the frame's own size to
/// the page so resizing reflows; the others lay out at the authored width and
/// scale the result (D20).
pub fn css_size(web: &WebPortalRef, rect: slate_doc::scene::WorldRect) -> (u32, u32) {
    let aspect = if rect.w > 0.0 {
        rect.h / rect.w
    } else {
        0.5625
    };
    match web.viewport.zoom {
        WebZoom::Auto => (
            rect.w.max(1.0).round() as u32,
            rect.h.max(1.0).round() as u32,
        ),
        _ => {
            let w = web.viewport.width_css.max(1);
            (w, ((w as f32) * aspect).round().max(1.0) as u32)
        }
    }
}

fn page_input_is_interactive(event: &WebInput) -> bool {
    match event {
        WebInput::Wheel { .. }
        | WebInput::Down { .. }
        | WebInput::Up { .. }
        | WebInput::Key { .. }
        | WebInput::Text(_) => true,
        WebInput::Move { buttons, .. } => *buttons != 0,
        WebInput::Leave => false,
    }
}

/// Physical-pixel quality tiers, bounded by the display rather than deep zoom.
pub fn raster_for(
    css_w: u32,
    css_h: u32,
    screen_w: f32,
    screen_h: f32,
    display: egui::Vec2,
) -> (u32, u32) {
    // Coarse upward tiers keep text sharp without resizing on every wheel tick.
    // Beyond a screenful, retain monitor quality rather than allocating an
    // unbounded canvas-sized surface. Old pixels remain until replacement.
    let (w, h) = (css_w.max(1) as f32, css_h.max(1) as f32);
    let ceiling = (display.x.max(1.0) / w)
        .max(display.y.max(1.0) / h)
        .max(1.0);
    let demand = (screen_w / w).max(screen_h / h).max(1.0).min(ceiling);
    let tier = 2.0_f32.powf(demand.log2().ceil()).min(ceiling);
    let scale = tier
        .min(RASTER_MAX_PX / w.max(h))
        .min((RASTER_MAX_PIXELS / (w * h)).sqrt());
    (
        (w * scale).floor().max(1.0) as u32,
        (h * scale).floor().max(1.0) as u32,
    )
}

fn rect_contains(outer: slate_doc::scene::WorldRect, inner: slate_doc::scene::WorldRect) -> bool {
    outer.x <= inner.x + 0.5
        && outer.y <= inner.y + 0.5
        && outer.x + outer.w >= inner.x + inner.w - 0.5
        && outer.y + outer.h >= inner.y + inner.h - 0.5
}

/// The next export: the portal's width, the picture's aspect, directly under
/// the portal, then under whatever already occupies that column.
pub(crate) fn canvas_export_rect(
    portal: slate_doc::scene::WorldRect,
    pixel_w: u32,
    pixel_h: u32,
    occupied: &[slate_doc::scene::WorldRect],
) -> slate_doc::scene::WorldRect {
    use slate_doc::scene::WorldRect;
    const GAP: f32 = 28.0;
    let aspect = if pixel_w == 0 {
        0.62
    } else {
        (pixel_h as f32 / pixel_w as f32).clamp(0.15, 4.0)
    };
    let w = portal.w.max(80.0);
    let h = (w * aspect).max(40.0);
    let mut y = portal.y + portal.h + GAP;
    for _ in 0..64 {
        let mut bottom = None;
        for r in occupied {
            let overlaps = r.x < portal.x + w - 1.0
                && r.x + r.w > portal.x + 1.0
                && r.y < y + h - 1.0
                && r.y + r.h > y + 1.0;
            if overlaps {
                let b = r.y + r.h;
                bottom = Some(bottom.map_or(b, |a: f32| a.max(b)));
            }
        }
        let Some(b) = bottom else {
            break;
        };
        let next = b + GAP;
        if next <= y + 0.5 {
            break;
        }
        y = next;
    }
    WorldRect::new(portal.x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_exports_stack_under_the_portal() {
        use slate_doc::scene::WorldRect;
        let portal = WorldRect::new(100.0, 40.0, 640.0, 400.0);
        let first = canvas_export_rect(portal, 1600, 900, &[]);
        assert!((first.x - 100.0).abs() < 0.1);
        assert!((first.y - (40.0 + 400.0 + 28.0)).abs() < 0.1);
        assert!((first.w - 640.0).abs() < 0.1);
        assert!((first.h - 640.0 * 900.0 / 1600.0).abs() < 0.5);
        let second = canvas_export_rect(portal, 1600, 900, &[first]);
        assert!((second.y - (first.y + first.h + 28.0)).abs() < 0.5);
        // A frame wrapped around the portal is not passed in as occupied;
        // a picture already to the side does not push the stack down.
        let aside = WorldRect::new(900.0, first.y, 200.0, 200.0);
        let third = canvas_export_rect(portal, 1600, 900, &[first, aside]);
        assert!((third.y - second.y).abs() < 0.5);
    }

    #[test]
    fn consent_returns_for_the_same_workbook_in_a_later_session_only() {
        let dir = std::env::temp_dir().join(format!(
            "slate_web_consent_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = dir.join("web-consent.json");
        let board = dir.join("board.slate");
        let other = dir.join("other.slate");
        let origin = "https://en.wikipedia.org";

        let mut first = WebRuntime::default();
        first.use_consent_file(file.clone());
        first.set_consent_workbook(Some(&board));
        first.grant_consent(origin);
        assert!(first.has_consent(origin));

        let mut later = WebRuntime::default();
        later.use_consent_file(file.clone());
        later.set_consent_workbook(Some(&board));
        assert!(
            later.has_consent(origin),
            "the same user reopening the board"
        );
        later.set_consent_workbook(Some(&other));
        assert!(
            !later.has_consent(origin),
            "grants never spread to other boards"
        );

        let mut elsewhere = WebRuntime::default();
        elsewhere.set_consent_workbook(Some(&board));
        assert!(
            !elsewhere.has_consent(origin),
            "no store, no inherited trust"
        );

        later.set_consent_workbook(Some(&board));
        later.revoke_consent(origin);
        let mut after = WebRuntime::default();
        after.use_consent_file(file);
        after.set_consent_workbook(Some(&board));
        assert!(!after.has_consent(origin));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn capture_tiers_match_high_dpi_displays_and_bound_deep_zoom() {
        let display = egui::vec2(3840.0, 2160.0);
        assert_eq!(raster_for(1280, 720, 3840.0, 2160.0, display), (3840, 2160));
        assert_eq!(raster_for(1280, 720, 1500.0, 844.0, display), (2560, 1440));
        assert_eq!(raster_for(1280, 720, 1700.0, 956.0, display), (2560, 1440));
        for size in [4096.0, 1.0e20, f32::INFINITY] {
            assert_eq!(raster_for(1280, 720, size, size, display), (3840, 2160));
        }
        let eight_k = egui::vec2(7680.0, 4320.0);
        assert_eq!(raster_for(1280, 720, 7680.0, 4320.0, eight_k), (7680, 4320));
        for (w, h) in [(u32::MAX, u32::MAX), (9000, 3000), (1, u32::MAX)] {
            let (rw, rh) = raster_for(w, h, 1.0e20, 1.0e20, eight_k);
            assert!(rw >= 1 && rh >= 1);
            assert!(rw.max(rh) <= RASTER_MAX_PX as u32);
            assert!((rw as u64) * (rh as u64) <= RASTER_MAX_PIXELS as u64);
        }
    }

    #[test]
    fn successive_web_frames_reuse_the_texture() {
        let ctx = egui::Context::default();
        let mut app = SlateApp::with_ctx(&ctx, None);
        let id = NodeId(99);
        app.web.views.insert(id, WebView::new("test".into()));
        app.upload_poster(&ctx, id, egui::ColorImage::new([4, 4], Color32::RED));
        let texture = app.web.views[&id].poster.as_ref().unwrap().id();
        app.upload_poster(&ctx, id, egui::ColorImage::new([4, 4], Color32::BLUE));
        assert_eq!(app.web.views[&id].poster.as_ref().unwrap().id(), texture);
    }

    #[test]
    fn blank_capture_and_refresh_never_replace_the_last_good_poster() {
        let ctx = egui::Context::default();
        let mut app = SlateApp::with_ctx(&ctx, None);
        let id = NodeId(99);
        app.web.views.insert(id, WebView::new("test".into()));
        app.upload_poster(&ctx, id, egui::ColorImage::new([8, 8], Color32::RED));
        let texture = app.web.views[&id].poster.as_ref().unwrap().id();
        let captured = app.web.views[&id].poster_at;
        for clear in [Color32::TRANSPARENT, Color32::BLACK, Color32::WHITE] {
            app.upload_poster(&ctx, id, egui::ColorImage::new([8, 8], clear));
            assert_eq!(app.web.views[&id].poster_at, captured);
            assert_eq!(app.web.views[&id].poster.as_ref().unwrap().id(), texture);
        }
        app.web_recapture(id);
        assert_eq!(app.web.views[&id].poster_at, captured);
        assert_eq!(app.web.views[&id].poster.as_ref().unwrap().id(), texture);
        let mut page = egui::ColorImage::new([8, 8], Color32::WHITE);
        page.pixels[0] = Color32::BLACK;
        assert!(
            web_frame_has_content(&page),
            "a white page with content is valid"
        );
        app.upload_poster(&ctx, id, page);
        assert_eq!(app.web.views[&id].poster.as_ref().unwrap().id(), texture);
        assert!(app.web.views[&id].poster_at > captured);
    }

    #[test]
    fn poster_survives_eviction_and_same_source_regeneration() {
        let ctx = egui::Context::default();
        let mut app = SlateApp::with_ctx(&ctx, None);
        app.leave_home();
        app.ensure_work_tab();
        app.paste_web_url("https://example.com/page", Pos2::ZERO);
        app.web_pump(&ctx);
        let id = app.doc().scene.nodes.last().unwrap().id;
        app.upload_poster(&ctx, id, egui::ColorImage::new([8, 8], Color32::BLUE));
        let texture = app.web.views[&id].poster.as_ref().unwrap().id();
        // Off-screen/native-host unavailable: demote a previously live page.
        app.web.views.get_mut(&id).unwrap().live = true;
        app.web_pump(&ctx);
        assert!(!app.web.views[&id].live);
        assert_eq!(app.web.views[&id].poster.as_ref().unwrap().id(), texture);
        // A changed viewport key must regenerate the host but keep this source's still.
        app.web
            .views
            .get_mut(&id)
            .unwrap()
            .key
            .push_str("-old-viewport");
        app.web_pump(&ctx);
        assert_eq!(app.web.views[&id].poster.as_ref().unwrap().id(), texture);
        app.bind_web_source(id, "https://example.org/different".into());
        app.web_pump(&ctx);
        assert!(
            app.web.views[&id].poster.is_none(),
            "rebind cannot show the old source"
        );
    }

    fn candidate(id: u64, height: f32, area: f32) -> Candidate {
        Candidate {
            id: NodeId(id),
            height_px: height,
            area_px: area,
            on_screen: true,
            focused: false,
            last_focus: None,
            renderable: true,
            was_live: false,
        }
    }

    #[test]
    fn page_scroll_is_interactive_hover_is_not() {
        assert!(page_input_is_interactive(&WebInput::Wheel {
            x: 0.0,
            y: 0.0,
            delta: 40.0,
            horizontal: false,
        }));
        assert!(!page_input_is_interactive(&WebInput::Move {
            x: 1.0,
            y: 1.0,
            buttons: 0,
        }));
        assert!(page_input_is_interactive(&WebInput::Move {
            x: 1.0,
            y: 1.0,
            buttons: 1,
        }));
        let mut rt = WebRuntime::default();
        assert!(!rt.recently_interactive());
        rt.note_page_input();
        assert!(rt.recently_interactive());
        assert_eq!(
            rt.focused_frame_interval(NodeId(1)),
            Duration::from_secs_f32(1.0 / IDLE_FPS)
        );
        rt.focused = Some(NodeId(1));
        assert_eq!(
            rt.focused_frame_interval(NodeId(1)),
            Duration::from_secs_f32(1.0 / INTERACTIVE_FPS)
        );
    }

    #[test]
    fn lod_buckets_follow_on_screen_height() {
        assert_eq!(lod_for(40.0), WebLod::Strip);
        assert_eq!(lod_for(LOD_STRIP_PX), WebLod::Poster);
        assert_eq!(lod_for(LIVE_MIN_PX - 1.0), WebLod::Poster);
        assert_eq!(lod_for(LIVE_MIN_PX), WebLod::Eligible);
    }

    #[test]
    fn a_hundred_tiled_pages_admit_only_the_pool() {
        // The research-hub case: a board full of pages costs a bounded number
        // of processes, not one per page (D29).
        let all: Vec<Candidate> = (0..100)
            .map(|i| candidate(i, 400.0, 400.0 * (i as f32 + 1.0)))
            .collect();
        let admitted = admit(&all, LIVE_POOL);
        assert_eq!(admitted.len(), LIVE_POOL);
        // Biggest on screen win.
        assert_eq!(admitted[0], NodeId(99));
        assert_eq!(admitted[LIVE_POOL - 1], NodeId(100 - LIVE_POOL as u64));
    }

    #[test]
    fn zoomed_out_or_off_screen_pages_run_nothing() {
        let tiny: Vec<Candidate> = (0..100).map(|i| candidate(i, 60.0, 3600.0)).collect();
        assert!(admit(&tiny, LIVE_POOL).is_empty());
        let offscreen: Vec<Candidate> = (0..10)
            .map(|i| Candidate {
                on_screen: false,
                ..candidate(i, 600.0, 400_000.0)
            })
            .collect();
        assert!(admit(&offscreen, LIVE_POOL).is_empty());
    }

    #[test]
    fn input_focus_always_keeps_its_slot() {
        let mut all: Vec<Candidate> = (0..20)
            .map(|i| candidate(i, 500.0, 500.0 * (i as f32 + 1.0)))
            .collect();
        // The smallest portal on the board, but the one being typed into.
        all[0].focused = true;
        let admitted = admit(&all, LIVE_POOL);
        assert_eq!(admitted[0], NodeId(0));
    }

    #[test]
    fn oscillating_height_around_the_live_gate_does_not_evict() {
        let mut was_live = false;
        let mut evicts = 0;
        for height in [161.0, 159.0, 161.0, 159.0, 150.0] {
            let mut c = candidate(1, height, 10_000.0);
            c.was_live = was_live;
            let now_live = !admit(&[c], LIVE_POOL).is_empty();
            if was_live && !now_live {
                evicts += 1;
            }
            was_live = now_live;
        }
        assert_eq!(
            evicts, 0,
            "Schmitt trigger must hold the slot across 160 px"
        );
        let mut demoted = candidate(1, 120.0, 10_000.0);
        demoted.was_live = true;
        assert!(
            !admit(&[demoted], LIVE_POOL).is_empty(),
            "zoom alone must not destroy the running browser"
        );
    }

    #[test]
    fn a_portal_that_cannot_render_never_takes_a_slot() {
        // Unbound, blocked, and refused portals compete for nothing (D30).
        let blocked: Vec<Candidate> = (0..3)
            .map(|i| Candidate {
                renderable: false,
                ..candidate(i, 600.0, 400_000.0)
            })
            .collect();
        assert!(admit(&blocked, LIVE_POOL).is_empty());
    }

    #[test]
    fn locators_resolve_workbook_relative_first() {
        let wb = PathBuf::from("/work/deck.slate");
        assert_eq!(
            resolve_web_source(Some(&wb), "dash/index.html"),
            PathBuf::from("/work").join("dash/index.html")
        );
        assert_eq!(
            web_source_locator(Some(&wb), &PathBuf::from("/work/dash/index.html")),
            "dash/index.html"
        );
        // Outside the workbook, an absolute path is the honest answer.
        let outside = PathBuf::from("/elsewhere/index.html");
        assert_eq!(
            web_source_locator(Some(&wb), &outside),
            outside.to_string_lossy()
        );
    }

    #[test]
    fn auto_zoom_reflows_and_fit_scales() {
        let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 800.0, 400.0);
        let mut web = WebPortalRef::default();
        assert_eq!(
            css_size(&web, rect),
            (1280, 640),
            "Fit lays out at the authored width"
        );
        web.viewport.zoom = WebZoom::Auto;
        assert_eq!(
            css_size(&web, rect),
            (800, 400),
            "Auto makes the frame the viewport"
        );
        assert_eq!(
            raster_for(1280, 640, 1920.0, 1080.0, egui::vec2(1920.0, 1080.0)),
            (2160, 1080),
            "Fit preserves its CSS layout while capturing enough physical pixels"
        );
        assert_eq!(
            raster_for(1280, 640, 40.0, 22.0, egui::vec2(1920.0, 1080.0)),
            (1280, 640),
            "small portals need no high-DPI upgrade"
        );
    }
}
