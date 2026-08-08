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

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind};
use slate_doc::scene::{
    classify_web_locator, web_display_locator, web_origin, Node, NodeId, NodeKind, PortalKind,
    PortalNode, SceneCmd, SourceUri, WebPortalRef, WebRefusal, WebSourceKind, WebZoom,
};

use super::board::{rgba32, BoardXf};
use super::SlateApp;

// ---------------------------------------------------------------------------
// Feel constants (contract "Feel constants" table)
// ---------------------------------------------------------------------------

/// Below this on-screen height, a portal is a chrome strip and nothing else.
pub const LOD_STRIP_PX: f32 = 96.0;
/// On-screen height at which a portal becomes eligible for the live pool.
pub const LIVE_MIN_PX: f32 = 320.0;
/// Webviews alive at once, across the whole board.
pub const LIVE_POOL: usize = 6;
/// Render rate for pooled portals that do not hold input focus.
pub const IDLE_FPS: f32 = 5.0;
/// Contents textures uploaded per frame; the rest wait in the backlog.
pub const UPLOADS_PER_FRAME: usize = 2;
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

impl WebHealth {
    fn color(self) -> Color32 {
        match self {
            WebHealth::Ok => Color32::from_rgb(120, 200, 140),
            WebHealth::Unknown => Color32::from_rgb(150, 160, 175),
            WebHealth::Missing => Color32::from_rgb(230, 130, 120),
        }
    }
}

/// What a portal is worth drawing at its current on-screen size (D23).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebLod {
    /// Chrome strip only; no texture is bound at all.
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
}

/// Input forwarded to the one portal holding input focus (D22). Composition
/// hosting gives the app no HWND for the page, so every event is explicit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WebInput {
    /// Pointer position in page CSS pixels.
    Move {
        x: f32,
        y: f32,
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
    Wheel {
        x: f32,
        y: f32,
        dy: f32,
    },
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
    /// Whether a runtime exists at all. `false` puts every portal in
    /// [`WebState::NoRuntime`] rather than stalling (D29).
    fn available(&self) -> bool;
    /// Create or update the view for `id`. Called only for pool members.
    fn admit(&mut self, id: NodeId, req: &WebRequest);
    /// Tear down the view for `id`, releasing its slot.
    fn evict(&mut self, id: NodeId);
    /// The newest frame, if one arrived since the last call.
    fn take_frame(&mut self, id: NodeId) -> Option<egui::ColorImage>;
    /// A one-off capture for the poster cache (D21).
    fn capture_poster(&mut self, id: NodeId) -> Option<egui::ColorImage>;
    fn send_input(&mut self, id: NodeId, input: WebInput);
    /// The cursor the page is asking for, while it holds input focus (D10).
    fn cursor(&self, id: NodeId) -> Option<egui::CursorIcon>;
    /// Whether the page reported a load failure.
    fn load_error(&self, id: NodeId) -> Option<String>;
}

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
    fn take_frame(&mut self, _id: NodeId) -> Option<egui::ColorImage> {
        None
    }
    fn capture_poster(&mut self, _id: NodeId) -> Option<egui::ColorImage> {
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
    /// Physical on-screen height last frame; 0 when off screen.
    height_px: f32,
    area_px: f32,
    on_screen: bool,
    live: bool,
    last_focus: Option<Instant>,
    last_poll: Option<Instant>,
    source_mtime: Option<SystemTime>,
}

impl WebView {
    fn new(key: String) -> Self {
        Self {
            generation: 0,
            key,
            state: WebState::Unknown,
            poster: None,
            poster_at: None,
            height_px: 0.0,
            area_px: 0.0,
            on_screen: false,
            live: false,
            last_focus: None,
            last_poll: None,
            source_mtime: None,
        }
    }
}

/// Board-wide web portal state. One per app, not per portal.
pub struct WebRuntime {
    views: HashMap<NodeId, WebView>,
    /// Origins the human has permitted this session, keyed `scheme://host`.
    /// Deliberately not journaled and not saved: a workbook you receive must
    /// never arrive already trusting a host (D26, D32).
    consent: HashSet<String>,
    /// The one portal receiving pointer and keyboard, if any (D22).
    pub focused: Option<NodeId>,
    host: Box<dyn WebHost>,
    next_generation: u64,
    /// Texture uploads already spent this frame (D29).
    uploads_this_frame: usize,
    /// Portals with a frame waiting for an upload slot.
    backlog: Vec<NodeId>,
}

impl Default for WebRuntime {
    fn default() -> Self {
        Self {
            views: HashMap::new(),
            consent: HashSet::new(),
            focused: None,
            host: default_host(),
            next_generation: 1,
            uploads_this_frame: 0,
            backlog: Vec::new(),
        }
    }
}

impl WebRuntime {
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
        self.consent.contains(origin)
    }

    /// Permit one origin for this session. Local state only — never a journal
    /// command, never saved into the workbook (D32).
    pub fn grant_consent(&mut self, origin: impl Into<String>) {
        self.consent.insert(origin.into());
    }

    pub fn revoke_consent(&mut self, origin: &str) {
        self.consent.remove(origin);
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
}

/// Chooses which portals hold the pool's webviews this frame (D29).
///
/// Priority is input focus, then greatest on-screen area, then most recently
/// focused. Pure so the hundred-tile research-hub case can be asserted without
/// a browser, a GPU, or a window.
pub fn admit(candidates: &[Candidate], pool: usize) -> Vec<NodeId> {
    let mut eligible: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| c.renderable && c.on_screen && lod_for(c.height_px) == WebLod::Eligible)
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

// ---------------------------------------------------------------------------
// Locator resolution
// ---------------------------------------------------------------------------

/// Workbook-relative first, absolute as fallback (Art. IX.2). Remote locators
/// pass through untouched.
pub fn resolve_web_source(workbook: Option<&Path>, locator: &str) -> PathBuf {
    let p = PathBuf::from(locator);
    if p.is_absolute() {
        return p;
    }
    if let Some(wb) = workbook.and_then(|p| p.parent()) {
        return wb.join(p);
    }
    p
}

/// The inverse: store a path relative to the workbook when it lives under it.
pub fn web_source_locator(workbook: Option<&Path>, path: &Path) -> String {
    if let Some(wb) = workbook.and_then(|p| p.parent()) {
        if let Ok(rel) = path.strip_prefix(wb) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    path.to_string_lossy().into_owned()
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

    /// Per-frame web portal work: resolve state, run the pool, spend the upload
    /// budget. Nothing here blocks, and nothing here reaches the network.
    pub(crate) fn web_pump(&mut self, ctx: &egui::Context) {
        self.web.uploads_this_frame = 0;
        let portals = self.web_portals();
        if portals.is_empty() {
            if !self.web.views.is_empty() {
                let stale: Vec<NodeId> = self.web.views.keys().copied().collect();
                for id in stale {
                    self.web.host.evict(id);
                    self.web.views.remove(&id);
                }
                self.web.focused = None;
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
            self.web.focused = None;
        }

        let workbook = self.tab().path.clone();
        let host_ok = self.web.host.available();
        let mut candidates = Vec::with_capacity(portals.len());

        for (id, portal, _rect) in &portals {
            let web = portal.web_ref();
            let key = view_key(portal, &web);
            let regenerate = self.web.views.get(id).is_none_or(|v| v.key != key);
            if regenerate {
                let generation = self.web.generation();
                self.web.host.evict(*id);
                let mut view = WebView::new(key);
                view.generation = generation;
                self.web.views.insert(*id, view);
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
            });
        }

        let admitted = admit(&candidates, LIVE_POOL);
        let admitted_set: HashSet<NodeId> = admitted.iter().copied().collect();
        for (id, portal, rect) in &portals {
            let want_live = admitted_set.contains(id);
            let was_live = self.web.views.get(id).is_some_and(|v| v.live);
            if want_live && !was_live {
                if let Some(req) = self.web_request(portal, workbook.as_deref(), *rect) {
                    self.web.host.admit(*id, &req);
                    if let Some(v) = self.web.views.get_mut(id) {
                        v.live = true;
                    }
                }
            } else if !want_live && was_live {
                // Capture on the way out, so a demoted portal degrades to its
                // last frame instead of blanking (D29).
                if let Some(img) = self.web.host.capture_poster(*id) {
                    self.upload_poster(ctx, *id, img);
                }
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
        let mut carried = Vec::new();
        for id in pending {
            if self.web.uploads_this_frame >= UPLOADS_PER_FRAME {
                carried.push(id);
                continue;
            }
            if let Some(img) = self.web.host.take_frame(id) {
                self.upload_poster(ctx, id, img);
                self.web.uploads_this_frame += 1;
            }
        }
        if !carried.is_empty() {
            self.web.backlog = carried;
            ctx.request_repaint();
        }
        if self.web.views.values().any(|v| v.live) {
            // Pooled portals animate; ask for the next frame at the idle rate
            // unless one of them holds focus, in which case the board's own
            // repaint cadence already covers it.
            let after = if self.web.focused.is_some() {
                Duration::ZERO
            } else {
                Duration::from_secs_f32(1.0 / IDLE_FPS)
            };
            ctx.request_repaint_after(after);
        }
    }

    fn upload_poster(&mut self, ctx: &egui::Context, id: NodeId, img: egui::ColorImage) {
        let tex = ctx.load_texture(
            format!("slate-web-{}", id.0),
            img,
            egui::TextureOptions::LINEAR,
        );
        if let Some(v) = self.web.views.get_mut(&id) {
            v.poster = Some(tex);
            v.poster_at = Some(Instant::now());
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
                if !self.web.consent.contains(&origin) {
                    return (WebState::Blocked { origin }, false);
                }
            }
        } else {
            // Local staleness is a worker-interval poll, never a per-frame stat
            // (Art. II.2). `Missing` keeps the last poster and names the path.
            let due = self
                .web
                .views
                .get(&id)
                .and_then(|v| v.last_poll)
                .is_none_or(|t| t.elapsed().as_secs_f32() >= POLL_SECS);
            if due {
                let path = resolve_web_source(workbook, &locator);
                let probe = std::fs::metadata(&path).ok();
                let exists = probe.is_some();
                let mtime = probe.and_then(|m| m.modified().ok());
                if let Some(v) = self.web.views.get_mut(&id) {
                    v.last_poll = Some(Instant::now());
                    let changed = v.source_mtime.is_some() && v.source_mtime != mtime;
                    v.source_mtime = mtime;
                    if changed {
                        // Content moved under us: the poster is stale, and a
                        // pooled portal reloads on its next admission.
                        v.poster_at = None;
                    }
                }
                if !exists {
                    return (WebState::Missing { locator }, false);
                }
            }
        }

        if !host_ok {
            return (WebState::NoRuntime, false);
        }
        let view = self.web.views.get(&id);
        let height = view.map(|v| v.height_px).unwrap_or(0.0);
        let on_screen = view.is_some_and(|v| v.on_screen);
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
        if !on_screen || lod_for(height) != WebLod::Eligible {
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
        &self,
        portal: &PortalNode,
        workbook: Option<&Path>,
        rect: slate_doc::scene::WorldRect,
    ) -> Option<WebRequest> {
        let locator = portal.source.as_ref()?.locator.clone();
        let web = portal.web_ref();
        let looks_like_dir = !locator.to_ascii_lowercase().ends_with(".html")
            && !locator.to_ascii_lowercase().ends_with(".htm");
        let kind = classify_web_locator(&locator, looks_like_dir).ok()?;
        let target = match kind {
            WebSourceKind::Remote => locator,
            WebSourceKind::LocalFile => resolve_web_source(workbook, &locator)
                .to_string_lossy()
                .into_owned(),
            WebSourceKind::LocalDir => resolve_web_source(workbook, &locator)
                .join(&web.entry)
                .to_string_lossy()
                .into_owned(),
        };
        let (width_css, height_css) = css_size(&web, rect);
        Some(WebRequest {
            target,
            kind,
            width_css,
            height_css,
        })
    }

    /// Record what the board just painted, so the next pump's admission has
    /// real on-screen sizes to sort by.
    pub(crate) fn note_web_geometry(&mut self, id: NodeId, srect: Rect, clip: Rect) {
        if let Some(v) = self.web.views.get_mut(&id) {
            v.height_px = srect.height();
            v.area_px = srect.width() * srect.height();
            v.on_screen = clip.intersects(srect);
        }
    }

    /// Take input focus (D22). Rendering is unaffected — this is only about
    /// where pointer and keyboard go.
    pub(crate) fn web_focus(&mut self, id: NodeId) {
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
            self.board_sel = std::iter::once(id).collect();
        }
    }

    /// Release input focus without tearing the page down: scroll position,
    /// form contents, and running charts survive (D12).
    pub(crate) fn web_blur(&mut self) -> bool {
        self.web.focused.take().is_some()
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
        if p.web.is_none() {
            p.web = Some(WebPortalRef::default());
        }
        let committed = self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(node),
            after: Box::new(after),
        }]);
        if committed {
            self.web.host.evict(id);
            self.web.views.remove(&id);
        }
        committed
    }

    /// Bind a local path, storing it workbook-relative where possible.
    pub(crate) fn bind_web_path(&mut self, id: NodeId, path: PathBuf) -> bool {
        let workbook = self.tab().path.clone();
        let locator = web_source_locator(workbook.as_deref(), &path);
        self.bind_web_source(id, locator)
    }

    /// Drop the cached poster so the next admission recaptures (D21).
    pub(crate) fn web_recapture(&mut self, id: NodeId) {
        if let Some(v) = self.web.views.get_mut(&id) {
            v.poster = None;
            v.poster_at = None;
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
            if !is_web_drop(path) {
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
            self.add_web_portal(rect, Some(locator), "dropped");
            placed += 1;
        }
        rest
    }

    /// Pasted clipboard text that names a page becomes a portal at the pointer
    /// (D01). Returns false for anything else, so the ordinary paste continues.
    pub(crate) fn paste_web_url(&mut self, text: &str, at: egui::Pos2) -> bool {
        let trimmed = text.trim();
        if web_origin(trimmed).is_none() {
            return false;
        }
        if classify_web_locator(trimmed, false) != Ok(WebSourceKind::Remote) {
            return false;
        }
        let rect = slate_doc::scene::WorldRect::new(
            at.x - PORTAL_W * 0.5,
            at.y - PORTAL_H * 0.5,
            PORTAL_W,
            PORTAL_H,
        );
        self.add_web_portal(rect, Some(trimmed.to_string()), "pasted");
        true
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
                if path.exists() {
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
        let Some(id) = self.selected_web_portal() else {
            return false;
        };
        self.web.host.evict(id);
        self.web.views.remove(&id);
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
                family: slate_doc::scene::FontChoice::Sans,
                size: 12.0,
                color: slate_doc::scene::Rgba::opaque(198, 208, 224),
                align: slate_doc::scene::TextAlign::Left,
                fill: None,
            }),
        );
        let ids = self.add_nodes(vec![image, note]);
        self.board_sel = ids.into_iter().collect();
        self.toast("Baked the poster and its provenance; the portal is still live.");
        true
    }

    fn write_poster_png(&mut self, id: NodeId, img: egui::ColorImage) -> Option<PathBuf> {
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
    /// Returns whether the board should stand down for this frame. The chrome
    /// strip and a `BORDER_HIT_PX` band stay Slate's, so a focused portal can
    /// always be grabbed by its edge and moved (D17).
    pub(crate) fn web_input_frame(
        &mut self,
        ui: &egui::Ui,
        xf: &BoardXf,
        pointer: Option<Pos2>,
    ) -> bool {
        let Some(id) = self.web.focused else {
            return false;
        };
        let Some(node) = self.doc().scene.node(id) else {
            self.web.focused = None;
            return false;
        };
        let srect = xf.rect_w2s(node.rect);
        let strip_h = 26.0_f32.min(srect.height());
        let page = Rect::from_min_max(
            Pos2::new(srect.left() + BORDER_HIT_PX, srect.top() + strip_h),
            Pos2::new(
                srect.right() - BORDER_HIT_PX,
                srect.bottom() - BORDER_HIT_PX,
            ),
        );
        let Some(p) = pointer else {
            return false;
        };
        if !page.contains(p) {
            return false;
        }

        // Page CSS coordinates, so the host never has to know about the board
        // camera.
        let web = match &node.kind {
            NodeKind::Portal(portal) => portal.web_ref(),
            _ => return false,
        };
        let (css_w, css_h) = css_size(&web, node.rect);
        let u = ((p.x - page.left()) / page.width().max(1.0)).clamp(0.0, 1.0);
        let v = ((p.y - page.top()) / page.height().max(1.0)).clamp(0.0, 1.0);
        let x = u * css_w as f32;
        let y = v * css_h as f32;

        let mut events = vec![WebInput::Move { x, y }];
        ui.input(|i| {
            for (button, code) in [
                (egui::PointerButton::Primary, 0u8),
                (egui::PointerButton::Secondary, 1),
                (egui::PointerButton::Middle, 2),
            ] {
                if i.pointer.button_pressed(button) {
                    events.push(WebInput::Down { x, y, button: code });
                }
                if i.pointer.button_released(button) {
                    events.push(WebInput::Up { x, y, button: code });
                }
            }
            // Ctrl+wheel stays the camera zoom, so P0.5 survives; the plain
            // wheel is the one thing this frame surrenders (D22).
            if !i.modifiers.command && i.raw_scroll_delta.y != 0.0 {
                events.push(WebInput::Wheel {
                    x,
                    y,
                    dy: i.raw_scroll_delta.y,
                });
            }
            for event in &i.events {
                match event {
                    // Esc is Slate's, always: it is how focus is peeled back
                    // off the page (D22), so the page never sees it.
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
        for event in events {
            self.web.host.send_input(id, event);
        }
        if let Some(cursor) = self.web.host.cursor(id) {
            ui.ctx().set_cursor_icon(cursor);
        }
        true
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
        self.note_web_geometry(node.id, srect, ui.clip_rect());

        let alpha = node.opacity.clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(alpha);
        let state = self.web.state(node.id);
        let focused = self.web.focused == Some(node.id);
        let live = self.web.is_live(node.id);

        painter.rect(
            srect,
            8.0,
            fade(rgba32(portal.fill)),
            Stroke::new(
                if focused { 2.0_f32 } else { 1.0 },
                fade(if focused {
                    Color32::from_rgb(120, 170, 255)
                } else {
                    Color32::from_rgba_unmultiplied(140, 150, 175, 150)
                }),
            ),
            StrokeKind::Outside,
        );

        let strip_h = 26.0_f32.min(srect.height());
        let strip = Rect::from_min_max(
            srect.left_top(),
            Pos2::new(srect.right(), srect.top() + strip_h),
        );
        self.paint_web_strip(painter, strip, portal, &state, live, alpha);

        if lod_for(srect.height()) == WebLod::Strip {
            return;
        }

        let body = Rect::from_min_max(
            Pos2::new(srect.left(), srect.top() + strip_h),
            srect.right_bottom(),
        );
        let poster = self.web.views.get(&node.id).and_then(|v| v.poster.clone());
        if let Some(tex) = poster {
            let stale = !live;
            let tint =
                Color32::WHITE.gamma_multiply(if stale { STALE_ALPHA * alpha } else { alpha });
            painter.image(
                tex.id(),
                body,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                tint,
            );
        } else {
            self.paint_web_empty_state(painter, body, &state, alpha);
        }
    }

    fn paint_web_strip(
        &self,
        painter: &egui::Painter,
        strip: Rect,
        portal: &PortalNode,
        state: &WebState,
        live: bool,
        alpha: f32,
    ) {
        let fade = |c: Color32| c.gamma_multiply(alpha);
        painter.rect_filled(
            strip,
            0.0,
            fade(Color32::from_rgba_unmultiplied(10, 12, 16, 190)),
        );
        let locator = portal
            .source
            .as_ref()
            .map(|s| web_display_locator(&s.locator))
            .unwrap_or_else(|| "unbound".into());
        let remote = portal
            .source
            .as_ref()
            .is_some_and(|s| web_origin(&s.locator).is_some());
        let glyph = if remote { "🌐" } else { "▤" };
        painter.circle_filled(
            Pos2::new(strip.left() + 12.0, strip.center().y),
            3.5,
            fade(state.health().color()),
        );
        painter.text(
            Pos2::new(strip.left() + 24.0, strip.center().y),
            Align2::LEFT_CENTER,
            format!("{glyph}  {locator}"),
            FontId::proportional(12.0),
            fade(Color32::from_rgb(214, 222, 236)),
        );
        if live {
            // The dot says "this frame is rendering"; the border says "this
            // frame has my keyboard" (D09).
            painter.circle_filled(
                Pos2::new(strip.right() - 12.0, strip.center().y),
                3.0,
                fade(Color32::from_rgb(120, 220, 150)),
            );
        }
        painter.text(
            Pos2::new(
                strip.right() - (if live { 24.0 } else { 12.0 }),
                strip.center().y,
            ),
            Align2::RIGHT_CENTER,
            state.label(),
            FontId::proportional(11.0),
            fade(Color32::from_rgb(158, 170, 190)),
        );
    }

    /// Every failure says itself in the frame rather than blanking it (D30).
    fn paint_web_empty_state(
        &self,
        painter: &egui::Painter,
        body: Rect,
        state: &WebState,
        alpha: f32,
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
        painter.text(
            body.center() - egui::vec2(0.0, 10.0),
            Align2::CENTER_CENTER,
            headline,
            FontId::proportional(15.0),
            fade(Color32::from_rgb(198, 208, 224)),
        );
        if !detail.is_empty() {
            painter.text(
                body.center() + egui::vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                detail,
                FontId::proportional(12.0),
                fade(Color32::from_rgb(150, 162, 182)),
            );
        }
    }
}

use slate_doc::scene::{PORTAL_DEFAULT_H as PORTAL_H, PORTAL_DEFAULT_W as PORTAL_W};

/// Whether a dropped path is a page rather than an ordinary board item: an
/// HTML file, or a folder holding an entry file. Only the directory entry is
/// read — nothing here opens a file.
pub fn is_web_drop(path: &Path) -> bool {
    if path.is_dir() {
        return [slate_doc::scene::WEB_DEFAULT_ENTRY, "index.htm"]
            .iter()
            .any(|entry| path.join(entry).is_file());
    }
    matches!(
        classify_web_locator(&path.to_string_lossy(), false),
        Ok(WebSourceKind::LocalFile)
    )
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
    #[cfg(windows)]
    {
        // `explorer` treats both URLs and paths as shell targets, and unlike
        // `cmd /C start` it needs no quoting dance.
        std::process::Command::new("explorer")
            .arg(target)
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(target)
            .spawn()
            .map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: u64, height: f32, area: f32) -> Candidate {
        Candidate {
            id: NodeId(id),
            height_px: height,
            area_px: area,
            on_screen: true,
            focused: false,
            last_focus: None,
            renderable: true,
        }
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
    }
}
