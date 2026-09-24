//! Slate application shell and state.
//!
//! UI hierarchy mirrors File Atlas (see `atlas-shell`):
//! - `ui/tabs` — top chrome (workbook tabs)
//! - `ui/tools` — left tools rail (Tags / Display / Workbook panels)
//! - `ui/readouts` — bottom metrics bar
//! - `ui/advanced` — floating advanced settings
//! - `canvas` — board entry point and camera helpers
//! - `session` — linked File Atlas viewport (in-process)

use atlas_core::thumbs::{cache_key, ThumbPool, ThumbRequest};
use atlas_shell::theme::{dark_visuals, light_visuals, Palette};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui::{self, Rect, TextureHandle, Vec2};
use slate_doc::scene::SceneJournal;
use slate_doc::{
    GroupId, ItemId, Lease, LeaseInfo, LeaseState, NodeId, SlateDoc, TagId, ViewKind,
    SLATE_EXTENSION,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub mod association;
pub mod board;
mod board_agent;
mod board_align;
mod board_atlas;
mod board_bumper;
mod board_color;
pub mod board_crop;
mod board_deck;
mod board_direct;
mod board_dock_embed;
mod board_flags;
mod board_flow;
mod board_forcefield;
mod board_handles;
pub mod board_icons;
mod board_join;
mod board_line;
mod board_osnap;
mod board_path;
mod board_place;
mod board_portal;
mod board_portal_chrome;
mod board_properties;
mod board_slate;
mod board_snap;
mod board_style;
mod board_transform;
mod board_trim;
mod board_video;
pub mod board_web;
#[cfg(windows)]
mod board_web_win;
mod board_wire;
pub mod canvas;
pub mod chrome;
mod clipboard;
pub mod commands;
mod dispatch;
mod enscape_host;
mod external_drop;
pub mod imagefx;
pub mod kits;
pub mod model3d;
mod overlays;
pub mod pdf;
pub mod present;
pub mod preview;
pub mod session;
pub mod settings;
#[cfg(test)]
mod tests;
mod ui;

pub use chrome::ChromeConfig;

/// All Slate thumbnail requests share one generation (no root swaps here).
const THUMB_GENERATION: u64 = 1;

/// Cycle of pleasant tag accent colors for newly created tags.
pub const TAG_COLOR_CYCLE: [[u8; 3]; 10] = [
    [0x2d, 0xd4, 0xbf], // teal
    [0xf4, 0x72, 0x5e], // coral
    [0x6f, 0xb7, 0xff], // sky
    [0xe0, 0xa8, 0x3c], // amber
    [0xa7, 0x8b, 0xfa], // violet
    [0x7d, 0xd8, 0x7d], // green
    [0xf2, 0x8c, 0xd6], // pink
    [0xc9, 0xd4, 0x5e], // lime
    [0x5e, 0xd4, 0xf4], // cyan
    [0xd4, 0x8c, 0x5e], // clay
];

#[derive(Clone, Copy)]
pub struct Camera {
    pub offset: Vec2,
    pub z: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            offset: Vec2::ZERO,
            z: atlas_core::display::SLATE_CANVAS.default_z,
        }
    }
}

/// One workbook tab. Unlike Atlas (which swaps heavyweight state), a Slate
/// document is links-only and lightweight, so each tab owns its whole doc.
pub struct SlateTab {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub doc: SlateDoc,
    /// Derived link health shared by every view; no filesystem access in paint.
    pub(crate) link_health: slate_doc::LinkHealthCache,
    link_health_revision: Option<(u64, usize)>,
    pub dirty: bool,
    /// Write lease for `path`, when this process owns it.
    pub lease: Option<Lease>,
    /// True when another process holds a live lease (view / present / export only).
    pub read_only: bool,
    /// Holder named in the lock file when [`Self::read_only`] (for the readout).
    pub lease_holder: Option<LeaseInfo>,
    pub chrome: ChromeConfig,
    pub cam: Camera,
    pub grid_fade: atlas_shell::grid_fade::GridFade,
    pub grid_fade_armed: bool,
    /// Board undo/redo history (session-local, not saved with the doc).
    pub journal: SceneJournal,
    /// Ctrl+Z order for this session, including spreadsheet file writes.
    pub(crate) edits: Vec<board::BoardMark>,
    pub(crate) edit_redo: Vec<board::BoardMark>,
}

impl SlateTab {
    pub fn empty() -> SlateTab {
        static NEXT_TAB_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        SlateTab {
            id: NEXT_TAB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            path: None,
            doc: SlateDoc::new("Untitled"),
            link_health: slate_doc::LinkHealthCache::default(),
            link_health_revision: None,
            dirty: false,
            lease: None,
            read_only: false,
            lease_holder: None,
            chrome: chrome::default_chrome(),
            cam: Camera::default(),
            grid_fade: atlas_shell::grid_fade::GridFade::default(),
            grid_fade_armed: false,
            journal: SceneJournal::default(),
            edits: Vec::new(),
            edit_redo: Vec::new(),
        }
    }

    pub fn title(&self) -> String {
        let base = match &self.path {
            Some(p) => p
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.doc.name.clone()),
            None => self.doc.name.clone(),
        };
        let mut title = base;
        if self.read_only {
            title.push_str(" (read-only)");
        }
        if self.dirty {
            title.push_str(" •");
        }
        title
    }

    pub fn is_blank(&self) -> bool {
        self.path.is_none() && self.doc.items.is_empty() && self.doc.groups.is_empty()
    }
}

/// Pending Save / Don't save / Cancel after a close that would lose edits.
#[derive(Clone, Copy, Debug)]
enum UnsavedClose {
    Tab(usize),
    Exit,
}

/// Async results from native file dialogs (spawned threads, like Atlas).
pub enum PickerMsg {
    OpenDoc(Option<PathBuf>),
    SaveDocAs {
        tab_id: u64,
        path: Option<PathBuf>,
    },
    AddFiles(Option<Vec<PathBuf>>),
    AddMedia {
        tab_id: u64,
        at: egui::Pos2,
        paths: Option<Vec<PathBuf>>,
    },
    /// Files picked from a frame's "Add images…" — placed inside the frame
    /// and inheriting its tags.
    AddToFrame {
        frame: NodeId,
        paths: Option<Vec<PathBuf>>,
    },
    /// Folder picked for "Export artifact…".
    ExportArtifact(Option<PathBuf>),
    /// File or folder picked as a web portal source (D19).
    WebPortalSource {
        portal: NodeId,
        path: Option<PathBuf>,
    },
    /// Folder picked as an agent portal project (D19).
    AgentPortalSource {
        portal: NodeId,
        path: Option<PathBuf>,
    },
    /// Folder picked as a File Atlas lens source (D19).
    AtlasPortalSource {
        portal: NodeId,
        path: Option<PathBuf>,
    },
    /// Workbook picked as a Slate board portal source.
    SlatePortalSource {
        portal: NodeId,
        path: Option<PathBuf>,
    },
}

pub enum ThumbState {
    Pending,
    Ready(TextureHandle),
    Failed,
}

pub struct SlateApp {
    pub(crate) updater: atlas_update::Updater,
    pub thumbs: ThumbPool,
    pub tabs: Vec<SlateTab>,
    pub active_tab: usize,
    pub dark_mode: bool,
    /// Cover Flow home (recent workbooks) — default launch surface.
    pub at_home: bool,
    /// Chrome prefs while at home with no work tabs (dock, advanced, etc.).
    home_chrome: chrome::ChromeConfig,
    /// Read-only stand-in when `at_home` and the tab list is empty (frame pump).
    fallback_tab: SlateTab,
    pub recents: atlas_shell::recent::RecentList,
    /// Off-thread existence check for the MRU. Entries stay until it answers.
    recent_prune_rx: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
    /// System font bytes, read off the UI thread and installed once.
    font_rx: Option<std::sync::mpsc::Receiver<Vec<(String, Vec<u8>)>>>,
    /// WebView2 host is created on the first frame that has a web portal.
    web_host_ready: bool,
    /// Shared home surface (shelf focus + cover textures) from `atlas-shell`.
    pub home: atlas_shell::home::HomeScreen,
    /// Floating tools dock placement (Preferences → Dock location).
    pub dock_side: atlas_shell::dock::DockSide,
    /// Dock panels pinned as persistent palettes (restored across sessions).
    pub dock_pins: Vec<String>,
    /// Tool flyouts that open as an icon strip instead of a stacked list.
    pub dock_icon_strips: Vec<String>,
    /// Tools hidden from each palette's icon strip (`palette → tool ids`).
    pub dock_strip_hidden: Vec<(String, Vec<String>)>,
    /// Authored tool order on each palette strip (`palette → tool ids`).
    pub dock_strip_order: Vec<(String, Vec<String>)>,
    /// Primary icon bar collapsed into the readout blister.
    pub dock_bar_collapsed: bool,

    pub selection: HashSet<ItemId>,
    pub canvas_rect: Rect,
    external_drop: external_drop::Inbox,
    #[cfg(windows)]
    drop_registration: Option<external_drop::win::Registration>,
    pub turbo_pan: commands::TurboPanState,

    /// Texture cache keyed by thumbnail cache key.
    pub textures: HashMap<String, ThumbState>,
    /// Last paint frame that needed each thumb key (LRU with `textures`).
    pub(crate) thumb_used: HashMap<String, u64>,
    /// Round-trip mapping for the thumb pool's u32 ids.
    thumb_slots: HashMap<u32, String>,
    next_thumb_slot: u32,

    /// Lazy full-resolution tier above the thumbnails (see `preview.rs`).
    pub previews: atlas_core::preview::PreviewPool,
    /// Resident full-res textures, LRU-bounded by the settings budget.
    pub preview_cache: HashMap<String, preview::PreviewEntry>,
    /// Round-trip mapping for in-flight preview requests: slot → (key, tier).
    preview_slots: HashMap<u32, (String, u32)>,
    /// Highest tier currently in flight per cache key (dedupes requests).
    preview_inflight: HashMap<String, u32>,
    /// Keys that can never beat their thumbnail (undecodable or tiny source).
    preview_failed: HashSet<String>,
    next_preview_slot: u32,
    /// Per-frame budget: how many decodes were started this frame.
    preview_reqs_this_frame: u32,

    /// Persisted UI settings (`slate-settings.json`).
    pub settings: settings::SlateSettings,

    pub picker_rx: Option<Receiver<PickerMsg>>,
    export_rx: Option<Receiver<(PathBuf, Result<slate_artifact::ExportReport, String>)>>,
    unsaved_close: Option<UnsavedClose>,
    pub toasts: Vec<(String, Instant)>,
    /// Rate-limit for the read-only edit refusal toast (one per second).
    last_read_only_toast: Option<Instant>,

    /// Inline "new tag" editor state: (group, buffer). `None` group = new group name.
    pub new_tag_edit: Option<(Option<GroupId>, String)>,
    pub tag_color_cursor: usize,

    /// Linked File Atlas session (in-process second viewport).
    pub atlas: Option<session::AtlasSession>,

    /// AI / Cursor integration: workspace link, launcher, context beacon
    /// (shared plumbing and panel body from `atlas-ai`).
    pub ai: atlas_ai::AiPanel,

    /// Shared contents-focus slot for host portals (web, agent, File Atlas).
    pub portals: board_portal::PortalRuntime,
    /// Nested workbook boards. Derived; never journaled.
    pub(crate) slate_boards: board_slate::SlateBoards,
    /// Agent portal runtime (derived sessions/proposals; never journaled).
    pub agents: board_agent::AgentRuntime,
    /// Web portal runtime: live pool, poster cache, per-origin consent. All
    /// derived — none of it is journaled or saved (D31, D32).
    pub web: board_web::WebRuntime,
    /// File Atlas lens portals: shared folder scans + per-portal cameras.
    /// Derived — never journaled (D31).
    pub atlas_lenses: board_atlas::AtlasRuntime,
    /// Shared portal chrome: maximize overlay and folded identity tabs.
    /// Derived view-state (P1.portal.maximize / chrome).
    pub portal_chrome: board_portal_chrome::PortalChrome,

    /// Board tool definitions: the built-in kit plus any in the user's kit
    /// folder. Read once at startup — the board consults it per commit.
    pub kits: kits::KitState,
    /// Kit tool id armed from an Advanced-catalog duplicate. `None` uses the
    /// built-in recipe for [`board_tool`].
    pub armed_kit_id: Option<String>,

    // ----- board (authored canvas) state -----
    /// Selected scene nodes (board view). Disjoint from `selection` (pool items).
    pub board_sel: HashSet<NodeId>,
    pub board_tool: board::BoardTool,
    /// Frames explicitly ordered by the Deck tool, oldest arming included.
    pub deck: board_deck::DeckState,
    /// Last-used navigation tool (Select or Pan) shown on the combined dock button.
    pub board_nav_tool: board::BoardTool,
    pub board_frame_preset: board::FramePreset,
    pub board_frame_custom: Option<board::FrameCustomDraft>,
    pub board_drag: Option<board::BoardDrag>,
    /// InDesign-style crop mode: the image node whose crop is being edited
    /// directly on the canvas (`None` = normal interaction).
    pub board_crop: Option<NodeId>,
    /// Inline text editing: (node, live buffer).
    pub text_edit: Option<(NodeId, String)>,
    /// Fitted sticky font sizes. Derived from text and box; not journaled.
    sticky_fit: HashMap<NodeId, board::StickyFit>,
    /// Cell editor on a CSV / Excel card.
    pub(crate) sheet_edit: Option<board::SheetEdit>,
    /// Spreadsheet entered by a double-click. Scroll and cell edits stay off
    /// until this is set.
    pub(crate) sheet_open: Option<NodeId>,
    sheet_dirty: bool,
    sheet_baseline: Option<Vec<Vec<atlas_core::office::SheetCell>>>,
    sheet_prompt: bool,
    /// Cell and add-column hits from the previous paint.
    sheet_hits: Vec<board::SheetHit>,
    sheet_grips: Vec<board::SheetGrip>,
    sheet_save_hit: Option<egui::Rect>,
    sheet_resize: Option<board::SheetResize>,
    /// Scroll offset, in world units, for a spreadsheet card. Derived from
    /// the pointer; not part of the document.
    sheet_scroll: HashMap<NodeId, Vec2>,
    /// Board right-click menu: (node, screen position).
    pub board_menu: Option<(NodeId, egui::Pos2)>,
    pub presenting: Option<present::Present>,
    /// Retained source pixels for board images (needed to apply filters).
    pub thumb_pixels: HashMap<String, egui::ColorImage>,
    /// Cached text-file excerpts for board snippet cards (`None` = unreadable).
    pub snippets: HashMap<ItemId, Option<String>>,
    /// Cached CSV / Excel grids for the same cards (`None` = not a spreadsheet).
    sheets: HashMap<ItemId, Option<Vec<Vec<atlas_core::office::SheetCell>>>>,
    /// Adjusted-texture cache keyed by (cache_key, adjust hash, source tier).
    /// Tier 0 is the thumbnail; a committed filter upgrades when the full-resolution preview lands.
    pub fx_textures: HashMap<(String, u64, u32), TextureHandle>,
    /// 32px center crops used as photo-filter radio faces.
    filter_swatch_src: HashMap<String, egui::ColorImage>,
    filter_swatch_tex: HashMap<(String, u64), TextureHandle>,
    /// Export artifact with base64-inlined assets (single portable file).
    pub export_inline: bool,
    /// Coalescing anchor for continuous board edits (node, last edit time).
    pub last_board_edit: Option<(NodeId, Instant)>,
    /// Alt modifier state this frame (Alt-drag duplicates).
    pub alt_down: bool,
    /// Shift modifier state this frame (3D viewport drag = pan).
    pub shift_down: bool,
    /// Ctrl modifier state this frame (Ctrl+double-click opens a web portal's
    /// page in the system browser).
    pub ctrl_down: bool,

    /// The glow GL context, for offscreen 3D viewport rendering. `None` in
    /// the headless test harness (3D stays poster/thumbnail-only there).
    pub gl: Option<std::sync::Arc<eframe::glow::Context>>,
    /// Native frame window. `0` in the headless harness. Enscape's walkthrough
    /// is parented here on double-click.
    frame_hwnd: isize,
    /// Interactive 3D model viewport state (see `model3d.rs`).
    pub model3d: model3d::ModelSpace,
    /// Canvas video scrub and playback. Derived; not journaled.
    video: board_video::VideoBoard,
    /// Transient smart-guide lines shown during board move/resize (cleared each frame).
    pub board_snap_guides: Vec<board_snap::SnapGuide>,
    /// Live forcefield pulses (outlive the guide that spawned them).
    pub board_forcefield: board_forcefield::Forcefield,
    /// Show the board dot grid (Board view).
    pub board_show_grid: bool,
    /// Snap moved objects to the board grid.
    pub board_snap_grid: bool,
    /// Persistent object-snap set (Document Settings). Not journaled.
    pub board_osnap: slate_doc::ObjectSnapSet,
    /// Object-to-object smart guides (forcefield ripples). Not journaled.
    pub board_smart_guides: bool,
    /// Neighborhood for smart guides (Tight / Nearby / Wide).
    pub board_snap_reach: settings::SnapReach,
    /// Bezier vs orthogonal wire display (Document Settings). Not journaled.
    pub board_wire_routing: slate_doc::WireRouting,
    /// Last object-snap hit this frame (marker paint).
    pub board_osnap_hit: Option<board_osnap::OsnapHit>,
    /// Last resolved live point (osnap or smart-guide). GhostFollow and
    /// rubber-bands read this instead of the raw cursor.
    pub board_point_snap: Option<egui::Pos2>,
    /// DragScale rect after `resolve_draw_rect` (preview + commit).
    pub board_draw_rect: Option<slate_doc::scene::WorldRect>,
    /// Hover target on a node's bounding-box chrome (handles / rotate zones).
    pub board_hover_hit: Option<board_handles::BoardHitTarget>,
    /// Node whose chrome `board_hover_hit` belongs to. `None` when the hit
    /// is the multi-selection group box.
    pub board_hover_node: Option<NodeId>,
    /// Body-hover highlight progress per node (0..1, derived, never journaled).
    pub board_hover_glow: HashMap<NodeId, f32>,
    /// Hovered Grasshopper-style align-widget action (multi-selection chrome).
    pub board_align_hover: Option<board_align::AlignAction>,
    /// The current primary press started on an align icon — swallow move /
    /// click-clear for the rest of the press.
    pub board_align_eat_press: bool,
    /// Multi-click path tools (polyline, arc, bezier).
    pub board_path_draft: Option<board_path::BoardPathDraft>,
    /// Line tool draft (contracts/line.md): first point placed, rubber band
    /// live. `None` = the tool is merely armed.
    pub line_draft: Option<board_line::LineDraft>,
    /// Trim command session (contracts/trim.md). `None` = not armed.
    pub trim: Option<board_trim::TrimSession>,
    /// Cached tessellated path strokes (Article II).
    pub path_mesh_cache: board_path::PathMeshCache,

    /// `.slate` files encountered in add/drop flows this frame. Workbooks
    /// never become items — they open as tabs at a safe point in the frame
    /// (after drop placement runs against the tab that received the drop).
    pub pending_workbooks: Vec<PathBuf>,

    /// Cached PDF page counts keyed by absolute path string.
    documents: pdf::documents::Documents,

    frame_no: u64,
    /// `ctx.input.time` snapshot for this frame (camera fades, repeat taps).
    pub(crate) frame_time: f64,
    /// Frame/activity recorder shared with File Atlas. Test builds stay in memory.
    pub(crate) session_log: atlas_core::session_log::SessionLog,

    // ----- command registry (keymap wave 2a) -----
    /// The command registry over `commands::SPECS` — keyboard, palette,
    /// menus, and docks all dispatch through it (Constitution Art. VII).
    pub(crate) registry: atlas_commands::Registry,
    /// Execution history: the F2 window's data and the Space/Enter repeat
    /// source. Intent log, distinct from the scene journal (Art. VI).
    pub(crate) cmd_history: atlas_commands::History,
    /// Space-tap repeat tracking (tap = repeat, hold+drag = pan).
    pub(crate) space_tap: dispatch::SpaceTap,
    /// Bare-letter shortcut held briefly so typed command names can win.
    pub(crate) bare_letter_hold: Option<dispatch::BareLetterHold>,
    /// F2 command-history window visibility.
    pub history_open: bool,
    /// Minimap overlay pinned on (M); persisted in chrome prefs.
    pub minimap_on: bool,
    pub(crate) minimap_state: atlas_shell::minimap::MinimapState,
    /// Canvas palette (double-click empty board) + its current query results.
    pub(crate) palette_state: atlas_shell::palette::PaletteState,
    pub(crate) palette_items: Vec<atlas_commands::PaletteItem>,
    /// Ctrl+F board search overlay (paint-time dimming, never journaled).
    pub(crate) search: overlays::SearchState,
    /// Board ortho constraint toggle (F8; persisted in settings). Wave 2b
    /// binds the 45° gesture math to it.
    pub board_ortho: bool,
    /// Ctrl+U image-adjust popover visibility (anchored to the selection).
    pub(crate) adjust_popover_open: bool,
    /// App-internal board clipboard (mirrored to the OS clipboard as JSON).
    pub(crate) board_clipboard: Vec<slate_doc::scene::Node>,
    /// OS clipboard text delivered by this frame's platform Paste event;
    /// consumed by the `board.paste` dispatch arm.
    pub(crate) pending_paste_text: Option<String>,
    /// Successive Ctrl+V pastes of one payload step +24,+24 each.
    pub(crate) board_paste_count: u32,
    /// Ctrl+V was down on the previous frame. egui never emits the V key
    /// for a paste chord, so image paste watches the key itself.
    #[cfg(windows)]
    paste_chord_down: bool,
    /// Cheap content generation: bumped on journal commits / undo / redo /
    /// tab switches. Keys the minimap texture cache and search recompute.
    pub(crate) scene_gen: u64,

    // ----- board tools (keymap wave 2b) -----
    /// Shared fg/bg color pair (Brush strokes, wires, eyedropper targets).
    /// Persisted in `SlateSettings`; `D` resets to theme defaults, `X` swaps.
    pub shape_properties: board_properties::ShapeProperties,
    /// Live bumper-cars drag and glide replay (derived, never journaled).
    pub(crate) bumper: board_bumper::BumperState,
    pub desktop_sample: Option<board_color::DesktopSample>,
    pub desktop_alt_latched: bool,
    pub board_colors: board_color::BoardColors,
    /// Last single-node style edit — seeds the next compatible create
    /// (P1.curve.create-style / P1.shape.create-style).
    pub(crate) board_last_style: board_style::BoardLastStyle,
    /// Brush stroke width, world units (persisted; `[`/`]` step it).
    pub brush_width: f32,
    /// Brush edge falloff 0..=1 (persisted; `Shift+[` / `Shift+]` and the size HUD).
    pub brush_softness: f32,
    /// Brush paint opacity 0.1..=1 (persisted; Shift+click steps it).
    pub brush_opacity: f32,
    /// Live Alt+right size HUD, Shift+right opacity HUD, or Ctrl+right color wheel.
    pub(crate) brush_hud: Option<board_color::BrushHud>,
    /// Tool settings when the right-button HUD opened, for Ctrl+Z.
    pub(crate) brush_hud_before: Option<board_color::BrushSettingUndo>,
    /// Alt or Shift primary press waiting for a short click (sample / opacity).
    pub(crate) brush_mod_click: Option<board_color::BrushModClick>,
    /// Swatch pick: (press point, where that color sits on the wheel). The
    /// pointer moves; the wheel does not.
    pub(crate) brush_cursor_warp: Option<(egui::Pos2, egui::Pos2)>,
    /// Shift+drag straight line, from the press tip.
    pub(crate) brush_straight: Option<board_color::BrushStraight>,
    /// End of the last brush mark, where the next Shift segment starts.
    pub(crate) brush_line_anchor: Option<board_color::BrushAnchor>,
    /// Size, color, and opacity edits undone by Ctrl+Z until another action.
    pub(crate) brush_setting_undo: Vec<board_color::BrushSettingUndo>,
    /// The brush drag's screen-aligned canvas (freehand or Shift preview).
    pub(crate) brush_live: Option<board_path::BrushLiveCanvas>,
    /// Committed radial stamps, keyed by node. The bitmap is derived.
    pub(crate) brush_stamps: HashMap<NodeId, (u64, board_path::BrushStampGpu)>,
    /// Stamp resolution upgrades spent this frame.
    pub(crate) brush_stamp_rebuilds: u32,
    /// Eraser pick-circle width, world units (persisted; `[`/`]` while E).
    pub eraser_width: f32,
    /// Eraser falloff and strength on painted strokes (persisted).
    pub eraser_softness: f32,
    pub eraser_opacity: f32,
    /// End of the last eraser pass, where a Shift pass starts.
    pub(crate) eraser_anchor: Option<egui::Pos2>,
    /// Painted strokes under the eraser this drag, shown with the pass applied.
    pub(crate) erase_live: HashMap<NodeId, board_path::EraseLive>,
    /// Last brush stroke end — Shift+click chains a straight segment from
    /// it; cleared whenever the Brush tool re-arms or changes.
    pub(crate) brush_chain: Option<egui::Pos2>,
    /// Direct-selection (A) state: target path node + selected anchors.
    pub(crate) direct: board_direct::DirectState,
    /// Per-frame connector grip hover (Select tool near a node edge).
    pub(crate) wire_grips: Option<board_wire::GripHover>,
    /// Inline connector label editor: (connector node, live buffer).
    pub(crate) wire_label_edit: Option<(NodeId, String)>,
    /// Right-click on empty board: "show/unlock all" menu position.
    pub(crate) board_empty_menu: Option<egui::Pos2>,
    /// Just-hidden nodes fading out (Ctrl+H's 150 ms ghost feedback).
    pub(crate) hide_ghosts: Vec<(slate_doc::scene::Node, Instant)>,
    /// Scene generation the connector AABBs were last synced for.
    pub(crate) connector_sync_gen: u64,
    /// Live ortho-constrained drag: (world origin, snapped axis) for the
    /// hash-tick feedback; cleared every frame.
    pub(crate) ortho_feedback: Option<(egui::Pos2, egui::Vec2)>,
    /// Z zoom tool: transient app-level mode over Board/Grid/Venn (camera
    /// only, never journaled). The underlying tool re-arms on disarm.
    pub(crate) zoom_armed: bool,
    /// Screen-space origin of a live zoom-window marquee.
    pub(crate) zoom_marquee: Option<egui::Pos2>,
}

impl SlateApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_doc: Option<PathBuf>) -> Self {
        association::ensure_file_association();
        let mut app = Self::with_ctx(&cc.egui_ctx, initial_doc);
        app.gl = cc.gl.clone();
        app.store_frame_hwnd(cc);
        app.install_local_grants();
        #[cfg(windows)]
        match external_drop::win::Registration::install(cc, app.external_drop.clone()) {
            Ok(registration) => app.drop_registration = Some(registration),
            Err(error) => app.toast(format!("Drag and drop unavailable: {error}")),
        }
        app
    }

    #[cfg(windows)]
    fn store_frame_hwnd(&mut self, cc: &eframe::CreationContext<'_>) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = cc.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(win32) = handle.as_raw() else {
            return;
        };
        self.frame_hwnd = win32.hwnd.get() as isize;
    }

    #[cfg(not(windows))]
    fn store_frame_hwnd(&mut self, _cc: &eframe::CreationContext<'_>) {}

    /// Create the WebView2 host the first time a portal needs one.
    ///
    /// Construction used to pay for the compositor and D3D device even when
    /// the workbook had no web portal. The null host stays until this runs, so
    /// a machine with no runtime still reports `NoRuntime` (D29). Must stay on
    /// the UI thread: the compositor wants this thread's dispatcher queue.
    fn ensure_web_host(&mut self, ctx: &egui::Context) {
        if self.web_host_ready || self.frame_hwnd == 0 {
            return;
        }
        self.web_host_ready = true;
        #[cfg(windows)]
        {
            let hwnd = windows::Win32::Foundation::HWND(self.frame_hwnd as *mut std::ffi::c_void);
            let user_data = atlas_core::index::data_dir().join("webview2");
            let _ = std::fs::create_dir_all(&user_data);
            if let Some(host) = board_web_win::Webview2Host::new(hwnd, &user_data, ctx.clone()) {
                self.web.set_host(Box::new(host));
            }
        }
        #[cfg(not(windows))]
        let _ = ctx;
    }

    /// Full construction from a bare egui context. Used by `new` and by the
    /// headless test harness (no eframe window, no registry writes).
    fn with_ctx(egui_ctx: &egui::Context, initial_doc: Option<PathBuf>) -> Self {
        // Overlaps the rest of construction. A board opened at launch waits
        // for these bytes so the first frame is not in the wrong face; Home
        // installs them on a later frame (egui's built-in proportional face
        // until then — Home titles do not use the system list).
        let font_rx = Self::spawn_system_fonts(egui_ctx.clone());
        egui_ctx.set_theme(egui::ThemePreference::Dark);
        egui_ctx.set_visuals(dark_visuals());
        atlas_shell::canvas_text::install(egui_ctx);
        #[cfg(test)]
        let chrome_prefs =
            atlas_shell::prefs::ChromePrefs::default_for(atlas_shell::dock::DockSide::BottomCenter);
        #[cfg(not(test))]
        let chrome_prefs = atlas_shell::prefs::ChromePrefs::load(
            "slate",
            atlas_shell::dock::DockSide::BottomCenter,
        );
        let mut app = SlateApp {
            updater: atlas_update::Updater::default(),
            thumbs: ThumbPool::new(),
            tabs: vec![],
            active_tab: 0,
            dark_mode: true,
            at_home: initial_doc.is_none(),
            home_chrome: chrome::default_chrome(),
            fallback_tab: SlateTab::empty(),
            recents: {
                #[cfg(test)]
                {
                    atlas_shell::recent::RecentList::default()
                }
                #[cfg(not(test))]
                {
                    atlas_shell::recent::RecentList::load("slate")
                }
            },
            recent_prune_rx: None,
            font_rx: None,
            web_host_ready: false,
            home: atlas_shell::home::HomeScreen::new(
                "slate",
                atlas_shell::home::HomeShelfKind::Workbooks,
            ),
            dock_side: chrome_prefs.dock_side,
            dock_pins: chrome_prefs.pinned_panels,
            dock_icon_strips: chrome_prefs.panel_icon_strip,
            dock_strip_hidden: chrome_prefs.panel_strip_hidden,
            dock_strip_order: chrome_prefs.panel_strip_order,
            dock_bar_collapsed: chrome_prefs.dock_bar_collapsed,
            selection: HashSet::new(),
            canvas_rect: Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1440.0, 900.0)),
            external_drop: external_drop::Inbox::default(),
            #[cfg(windows)]
            drop_registration: None,
            turbo_pan: commands::TurboPanState::default(),
            textures: HashMap::new(),
            thumb_used: HashMap::new(),
            thumb_slots: HashMap::new(),
            next_thumb_slot: 0,
            previews: atlas_core::preview::PreviewPool::new(),
            preview_cache: HashMap::new(),
            preview_slots: HashMap::new(),
            preview_inflight: HashMap::new(),
            preview_failed: HashSet::new(),
            next_preview_slot: 0,
            preview_reqs_this_frame: 0,
            settings: settings::SlateSettings::load(),
            picker_rx: None,
            export_rx: None,
            unsaved_close: None,
            toasts: Vec::new(),
            last_read_only_toast: None,
            new_tag_edit: None,
            tag_color_cursor: 0,
            atlas: None,
            ai: atlas_ai::AiPanel::new(),
            portals: board_portal::PortalRuntime::default(),
            slate_boards: board_slate::SlateBoards::new(),
            agents: board_agent::AgentRuntime::default(),
            web: board_web::WebRuntime::default(),
            atlas_lenses: board_atlas::AtlasRuntime::default(),
            portal_chrome: board_portal_chrome::PortalChrome::default(),
            kits: kits::KitState::load(),
            armed_kit_id: None,
            board_sel: HashSet::new(),
            board_tool: board::BoardTool::default(),
            deck: board_deck::DeckState::default(),
            board_nav_tool: board::BoardTool::Select,
            board_frame_preset: board::FramePreset::default(),
            board_frame_custom: None,
            board_drag: None,
            board_crop: None,
            text_edit: None,
            sticky_fit: HashMap::new(),
            sheet_edit: None,
            sheet_open: None,
            sheet_dirty: false,
            sheet_baseline: None,
            sheet_prompt: false,
            sheet_hits: Vec::new(),
            sheet_grips: Vec::new(),
            sheet_save_hit: None,
            sheet_resize: None,
            sheet_scroll: HashMap::new(),
            board_menu: None,
            presenting: None,
            thumb_pixels: HashMap::new(),
            snippets: HashMap::new(),
            sheets: HashMap::new(),
            fx_textures: HashMap::new(),
            filter_swatch_src: HashMap::new(),
            filter_swatch_tex: HashMap::new(),
            export_inline: false,
            last_board_edit: None,
            alt_down: false,
            shift_down: false,
            ctrl_down: false,
            gl: None,
            frame_hwnd: 0,
            model3d: model3d::ModelSpace::default(),
            video: board_video::VideoBoard::default(),
            board_snap_guides: Vec::new(),
            board_forcefield: board_forcefield::Forcefield::default(),
            board_show_grid: true,
            board_snap_grid: false,
            board_osnap: slate_doc::ObjectSnapSet::default(),
            board_smart_guides: true,
            board_snap_reach: settings::SnapReach::Nearby,
            board_wire_routing: slate_doc::WireRouting::default(),
            board_osnap_hit: None,
            board_point_snap: None,
            board_draw_rect: None,
            board_hover_hit: None,
            board_hover_node: None,
            board_hover_glow: HashMap::new(),
            board_align_hover: None,
            board_align_eat_press: false,
            board_path_draft: None,
            line_draft: None,
            trim: None,
            path_mesh_cache: board_path::PathMeshCache::default(),
            pending_workbooks: Vec::new(),
            documents: pdf::documents::Documents::default(),
            frame_no: 0,
            frame_time: 0.0,
            session_log: if cfg!(test) {
                atlas_core::session_log::SessionLog::memory("slate")
            } else {
                atlas_core::session_log::SessionLog::new("slate")
            },
            registry: commands::registry(),
            cmd_history: atlas_commands::History::new(),
            space_tap: dispatch::SpaceTap::default(),
            bare_letter_hold: None,
            history_open: false,
            minimap_on: chrome_prefs.minimap,
            minimap_state: atlas_shell::minimap::MinimapState::default(),
            palette_state: atlas_shell::palette::PaletteState::default(),
            palette_items: Vec::new(),
            search: overlays::SearchState::default(),
            board_ortho: false,
            adjust_popover_open: false,
            board_clipboard: Vec::new(),
            pending_paste_text: None,
            board_paste_count: 0,
            #[cfg(windows)]
            paste_chord_down: false,
            scene_gen: 0,
            shape_properties: board_properties::ShapeProperties::default(),
            bumper: Default::default(),
            desktop_sample: None,
            desktop_alt_latched: false,
            board_colors: board_color::BoardColors::theme_default(true),
            board_last_style: board_style::BoardLastStyle::default(),
            brush_width: settings::BRUSH_WIDTH_DEFAULT,
            brush_softness: 0.0,
            brush_opacity: 1.0,
            brush_hud: None,
            brush_hud_before: None,
            brush_mod_click: None,
            brush_cursor_warp: None,
            brush_straight: None,
            brush_line_anchor: None,
            brush_setting_undo: Vec::new(),
            brush_live: None,
            brush_stamps: HashMap::new(),
            brush_stamp_rebuilds: 0,
            eraser_width: settings::ERASER_WIDTH_DEFAULT,
            eraser_softness: 0.0,
            eraser_opacity: 1.0,
            eraser_anchor: None,
            erase_live: HashMap::new(),
            brush_chain: None,
            direct: board_direct::DirectState::default(),
            wire_grips: None,
            wire_label_edit: None,
            board_empty_menu: None,
            hide_ghosts: Vec::new(),
            connector_sync_gen: 0,
            ortho_feedback: None,
            zoom_armed: false,
            zoom_marquee: None,
        };
        app.board_ortho = app.settings.board_ortho;
        app.board_osnap = app.settings.board_osnap;
        app.board_smart_guides = app.settings.board_smart_guides;
        app.board_snap_reach = app.settings.board_snap_reach;
        app.board_wire_routing = app.settings.board_wire_routing;
        app.board_colors = board_color::BoardColors::from_settings(&app.settings, app.dark_mode);
        app.brush_width = app.settings.brush_width;
        app.brush_softness = app.settings.brush_softness;
        app.brush_opacity = app.settings.brush_opacity;
        app.eraser_width = app.settings.eraser_width;
        app.eraser_softness = app.settings.eraser_softness;
        app.eraser_opacity = app.settings.eraser_opacity;
        debug_assert!(
            app.registry.validate().is_ok(),
            "SPECS table inconsistent: {:?}",
            app.registry.validate()
        );
        app.thumbs.retain_generation(THUMB_GENERATION);
        app.thumbs
            .ensure_workers(atlas_core::display::THUMB_WORKERS_SLATE);
        #[cfg(not(test))]
        {
            let paths = app.recents.entries.iter().map(|e| e.path.clone()).collect();
            app.recent_prune_rx = Some(atlas_shell::recent::spawn_prune_missing(paths));
        }
        // Tests and a workbook opened at launch wait out the font read so the
        // first frame is not in the wrong face. Home (no document) returns
        // immediately and installs on a later frame.
        if initial_doc.is_some() || cfg!(test) {
            if let Ok(files) = font_rx.recv() {
                Self::install_loaded_fonts(egui_ctx, &files);
            }
        } else {
            app.font_rx = Some(font_rx);
        }
        if let Some(path) = initial_doc {
            app.at_home = false;
            app.ensure_work_tab();
            app.open_doc_at(path);
        }
        app.ensure_home_cover_bakes();
        app
    }

    pub(crate) fn go_home(&mut self) {
        self.at_home = true;
    }

    pub(crate) fn leave_home(&mut self) {
        self.at_home = false;
    }

    fn record_recent_workbook(&mut self, path: &Path, doc: &slate_doc::SlateDoc) {
        let title = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        self.recents.record(path.to_path_buf(), title);
        #[cfg(test)]
        let _ = doc;
        // Covers and MRU persistence are personal state, not fixture output.
        #[cfg(not(test))]
        {
            let media = sample_workbook_cover_media(doc, 9);
            let key = path.to_path_buf();
            if atlas_shell::covers::schedule_cover_bake(&key) {
                std::thread::spawn(move || {
                    let ok = atlas_shell::covers::bake_workbook_cover(&key, &media).is_some();
                    atlas_shell::covers::note_bake_finished(&key, ok);
                });
            }
            for e in &mut self.recents.entries {
                let cover = atlas_shell::recent::cover_cache_path(&e.path);
                if cover.is_file() {
                    e.cover = Some(cover);
                }
            }
            self.recents.save("slate");
        }
    }

    /// Read each system face once on a worker. `cour.ttf` is both Courier and
    /// the agent title family, so it is in the list a single time.
    fn spawn_system_fonts(ctx: egui::Context) -> std::sync::mpsc::Receiver<Vec<(String, Vec<u8>)>> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut names: Vec<&str> = slate_doc::scene::Typeface::ALL
                .iter()
                .filter_map(|face| face.font_file())
                .collect();
            names.sort_unstable();
            names.dedup();
            let mut files = Vec::new();
            for name in names {
                if let Some(bytes) = Self::windows_font_bytes(name) {
                    files.push((name.to_string(), bytes));
                }
            }
            if !files.iter().any(|(name, _)| name == "cour.ttf") {
                if let Some(bytes) = Self::courier_fallback_bytes() {
                    files.push(("cour.ttf".into(), bytes));
                }
            }
            let _ = tx.send(files);
            ctx.request_repaint();
        });
        rx
    }

    fn poll_deferred_fonts(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.font_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(files) => Self::install_loaded_fonts(ctx, &files),
            Err(std::sync::mpsc::TryRecvError::Empty) => self.font_rx = Some(rx),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
        }
    }

    fn poll_recent_prune(&mut self) {
        let Some(rx) = self.recent_prune_rx.take() else {
            return;
        };
        let missing = match rx.try_recv() {
            Ok(missing) => missing,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.recent_prune_rx = Some(rx);
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
        };
        let before = self.recents.entries.len();
        self.recents.retain_existing(&missing);
        if self.recents.entries.len() != before {
            #[cfg(not(test))]
            self.recents.save("slate");
        }
    }

    /// Register the bundled serif face plus any system faces the worker read.
    /// One `set_fonts` invalidates galleys a single time.
    fn install_loaded_fonts(ctx: &egui::Context, files: &[(String, Vec<u8>)]) {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "slate-serif".into(),
            std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
                "../../assets/fonts/DejaVuSerif.ttf"
            ))),
        );
        fonts.families.insert(
            egui::FontFamily::Name("slate-serif".into()),
            vec!["slate-serif".into()],
        );
        let cour = files.iter().find(|(name, _)| name == "cour.ttf");
        let name = atlas_shell::home::AGENT_TITLE_FAMILY;
        let mut stack = vec![name.to_string()];
        if let Some((_, bytes)) = cour {
            fonts.font_data.insert(
                name.into(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes.clone())),
            );
        }
        if let Some(mono) = fonts.families.get(&egui::FontFamily::Monospace) {
            stack.extend(mono.iter().cloned());
        }
        fonts
            .families
            .insert(egui::FontFamily::Name(name.into()), stack);
        for face in slate_doc::scene::Typeface::ALL {
            let (Some(file), Some(key)) = (face.font_file(), face.egui_family()) else {
                continue;
            };
            let Some((_, bytes)) = files.iter().find(|(name, _)| name == file) else {
                continue;
            };
            fonts.font_data.insert(
                key.into(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes.clone())),
            );
            let mut family = vec![key.to_string()];
            if let Some(proportional) = fonts.families.get(&egui::FontFamily::Proportional) {
                family.extend(proportional.iter().cloned());
            }
            fonts
                .families
                .insert(egui::FontFamily::Name(key.into()), family);
        }
        ctx.set_fonts(fonts);
    }

    fn windows_font_bytes(file: &str) -> Option<Vec<u8>> {
        let mut paths = Vec::new();
        if let Some(windir) = std::env::var_os("WINDIR") {
            paths.push(PathBuf::from(windir).join("Fonts").join(file));
        }
        paths.push(PathBuf::from(r"C:\Windows\Fonts").join(file));
        paths
            .into_iter()
            .find(|path| path.is_file())
            .and_then(|path| std::fs::read(path).ok())
    }

    /// Linux stand-in when `cour.ttf` is not installed. Windows already loaded
    /// that file through [`Self::windows_font_bytes`].
    fn courier_fallback_bytes() -> Option<Vec<u8>> {
        [
            "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        ]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .and_then(|p| std::fs::read(p).ok())
    }

    pub fn palette(&self) -> Palette {
        Palette::for_mode(self.dark_mode)
    }

    /// Hide the bottom readout strip (View menu, lower-left chevron, or F11).
    pub fn toggle_canvas_fullscreen(&mut self) {
        let on = !self.chrome().canvas_fullscreen;
        self.chrome_mut().canvas_fullscreen = on;
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
        ctx.set_theme(if self.dark_mode {
            egui::ThemePreference::Dark
        } else {
            egui::ThemePreference::Light
        });
        ctx.set_visuals(if self.dark_mode {
            dark_visuals()
        } else {
            light_visuals()
        });
        atlas_shell::menu::apply_style(ctx, self.dark_mode);
    }

    pub fn tab(&self) -> &SlateTab {
        if self.tabs.is_empty() {
            return &self.fallback_tab;
        }
        &self.tabs[self.active_tab.min(self.tabs.len() - 1)]
    }

    pub fn tab_mut(&mut self) -> &mut SlateTab {
        if self.tabs.is_empty() {
            self.ensure_work_tab();
        }
        let i = self.active_tab.min(self.tabs.len() - 1);
        &mut self.tabs[i]
    }

    pub(crate) fn chrome(&self) -> &chrome::ChromeConfig {
        if self.tabs.is_empty() {
            &self.home_chrome
        } else {
            &self.tab().chrome
        }
    }

    pub(crate) fn chrome_mut(&mut self) -> &mut chrome::ChromeConfig {
        if self.tabs.is_empty() {
            &mut self.home_chrome
        } else {
            &mut self.tab_mut().chrome
        }
    }

    /// Ensure a blank work tab exists (after leaving home or opening a doc).
    pub(crate) fn ensure_work_tab(&mut self) {
        if self.tabs.is_empty() {
            let mut tab = SlateTab::empty();
            tab.chrome = self.home_chrome.clone();
            self.tabs.push(tab);
            self.active_tab = 0;
            self.fallback_tab.chrome = self.home_chrome.clone();
            self.sync_agent_doc();
        }
    }

    pub(crate) fn home_new_workspace(&mut self) {
        self.leave_home();
        if self.tabs.is_empty() {
            self.ensure_work_tab();
        } else if !self.tab().is_blank() {
            self.new_tab();
        }
    }

    pub fn doc(&self) -> &SlateDoc {
        &self.tab().doc
    }

    /// Mutable doc access; marks the workbook dirty (all edits go through
    /// this or set `dirty` themselves).
    pub fn doc_mut(&mut self) -> &mut SlateDoc {
        if self.refuse_read_only_edit() {
            // Still hand back the real doc so existing call sites compile;
            // board mutation helpers early-return before writing, and we
            // skip the dirty mark so a read-only tab can still close.
            return &mut self.tab_mut().doc;
        }
        let tab = self.tab_mut();
        tab.dirty = true;
        &mut tab.doc
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toasts.push((msg.into(), Instant::now()));
    }

    /// When the active tab is read-only, rate-limited toast and `true`
    /// (caller must not mutate). Writable tabs return `false`.
    pub(crate) fn refuse_read_only_edit(&mut self) -> bool {
        if !self.tab().read_only {
            return false;
        }
        let due = self
            .last_read_only_toast
            .map(|t| t.elapsed() >= Duration::from_secs(1))
            .unwrap_or(true);
        if due {
            self.toast("Workbook is read-only — use Save a copy… to edit");
            self.last_read_only_toast = Some(Instant::now());
        }
        true
    }

    pub fn next_tag_color(&mut self) -> [u8; 3] {
        let c = TAG_COLOR_CYCLE[self.tag_color_cursor % TAG_COLOR_CYCLE.len()];
        self.tag_color_cursor += 1;
        c
    }

    // ----- tabs -------------------------------------------------------------

    pub fn new_tab(&mut self) {
        self.lock_all_models();
        self.sync_agent_doc();
        let mut tab = SlateTab::empty();
        tab.chrome = self.chrome().clone();
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.selection.clear();
        self.sync_agent_doc();
    }

    pub fn switch_tab(&mut self, i: usize) {
        self.finish_bumper_glide();
        if i < self.tabs.len() {
            if i != self.active_tab {
                // Live 3D viewports are keyed by node id, which is per-document:
                // freeze them before another doc's ids can collide.
                self.lock_all_models();
                self.sync_agent_doc();
                self.active_tab = i;
                self.sync_agent_doc();
                self.selection.clear();
                self.note_scene_change();
                self.publish_session_tags();
            }
            // Leaving the Cover Flow home for a workbook tab.
            self.leave_home();
        }
    }

    pub fn close_tab(&mut self, i: usize) {
        self.finish_bumper_glide();
        if i >= self.tabs.len() {
            return;
        }
        // Freezing can dirty an otherwise saved workbook. Check after it
        // journals the camera so closing cannot discard the latest view.
        if i == self.active_tab {
            self.lock_all_models();
        }
        if self.tabs[i].dirty {
            self.unsaved_close = Some(UnsavedClose::Tab(i));
            return;
        }
        self.force_close_tab(i);
    }

    fn force_close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.release_agent_doc(i);
        if let Some(lease) = self.tabs[i].lease.take() {
            lease.release();
        }
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.at_home = true;
            self.active_tab = 0;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        } else if self.active_tab > i {
            self.active_tab -= 1;
        }
        self.selection.clear();
        self.publish_session_tags();
    }

    fn discard_dirty_tabs(&mut self) {
        for tab in &mut self.tabs {
            tab.dirty = false;
        }
    }

    fn save_tab_for_close(&mut self, i: usize) -> bool {
        if i >= self.tabs.len() {
            return true;
        }
        if !self.tabs[i].dirty {
            return true;
        }
        self.switch_tab(i);
        self.save_doc();
        i < self.tabs.len() && !self.tabs[i].dirty
    }

    fn resume_unsaved_close_if_ready(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.unsaved_close else {
            return;
        };
        match kind {
            UnsavedClose::Tab(i) if i < self.tabs.len() && !self.tabs[i].dirty => {
                self.continue_unsaved_close(ctx);
            }
            UnsavedClose::Exit if !self.tabs.iter().any(|t| t.dirty) => {
                self.continue_unsaved_close(ctx);
            }
            _ => {}
        }
    }

    fn continue_unsaved_close(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.unsaved_close else {
            return;
        };
        match kind {
            UnsavedClose::Tab(i) => {
                if i < self.tabs.len() && !self.tabs[i].dirty {
                    self.unsaved_close = None;
                    self.force_close_tab(i);
                }
            }
            UnsavedClose::Exit => {
                if let Some(i) = self.tabs.iter().position(|t| t.dirty) {
                    if !self.save_tab_for_close(i) {
                        return;
                    }
                    self.continue_unsaved_close(ctx);
                } else {
                    self.unsaved_close = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    pub(crate) fn apply_unsaved_choice(
        &mut self,
        ctx: &egui::Context,
        choice: atlas_shell::widgets::ConfirmChoice,
    ) {
        use atlas_shell::widgets::ConfirmChoice;
        let Some(kind) = self.unsaved_close else {
            return;
        };
        match choice {
            ConfirmChoice::Cancel => self.unsaved_close = None,
            ConfirmChoice::Secondary => match kind {
                UnsavedClose::Tab(i) => {
                    self.unsaved_close = None;
                    if i < self.tabs.len() {
                        self.tabs[i].dirty = false;
                        self.force_close_tab(i);
                    }
                }
                UnsavedClose::Exit => {
                    self.discard_dirty_tabs();
                    self.unsaved_close = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
            ConfirmChoice::Primary => match kind {
                UnsavedClose::Tab(i) => {
                    if self.save_tab_for_close(i) {
                        self.continue_unsaved_close(ctx);
                    }
                }
                UnsavedClose::Exit => self.continue_unsaved_close(ctx),
            },
        }
    }

    fn unsaved_prompt_frame(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.unsaved_close else {
            return;
        };
        let i = match kind {
            UnsavedClose::Tab(i) => i,
            UnsavedClose::Exit => match self.tabs.iter().position(|t| t.dirty) {
                Some(i) => i,
                None => {
                    self.unsaved_close = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
            },
        };
        if i >= self.tabs.len() {
            self.unsaved_close = None;
            return;
        }
        let name = self.tabs[i].doc.name.clone();
        let many =
            matches!(kind, UnsavedClose::Exit) && self.tabs.iter().filter(|t| t.dirty).count() > 1;
        let body = if many {
            format!(
                "“{name}” and other workbooks have unsaved changes. Save this one, discard all, or cancel."
            )
        } else {
            format!("“{name}” has unsaved changes. Save it, or close without saving.")
        };
        if let Some(choice) = atlas_shell::widgets::confirm_window(
            ctx,
            "Unsaved changes",
            &body,
            "Save",
            "Don't save",
        ) {
            self.apply_unsaved_choice(ctx, choice);
        }
    }

    /// Drop every tab's write lease (app exit). Best-effort; `Lease::Drop` is
    /// the backstop if this is skipped.
    fn release_all_leases(&mut self) {
        for tab in &mut self.tabs {
            if let Some(lease) = tab.lease.take() {
                lease.release();
            }
        }
    }

    // ----- document I/O ------------------------------------------------------

    pub fn open_doc_dialog(&mut self) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("Slate workbook", &[SLATE_EXTENSION])
                .pick_file();
            let _ = tx.send(PickerMsg::OpenDoc(picked));
        });
    }

    pub fn save_doc(&mut self) {
        if self.tab().read_only {
            self.toast("Workbook is read-only — use Save a copy…");
            return;
        }
        let tab_id = self.tab().id;
        match self.tab().path.clone() {
            Some(path) => self.save_doc_to(tab_id, path),
            None => self.save_doc_as_dialog(),
        }
    }

    pub fn save_doc_as_dialog(&mut self) {
        if self.picker_rx.is_some() {
            return;
        }
        let tab_id = self.tab().id;
        let suggested = format!("{}.{}", self.doc().name, SLATE_EXTENSION);
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("Slate workbook", &[SLATE_EXTENSION])
                .set_file_name(&suggested)
                .save_file();
            let _ = tx.send(PickerMsg::SaveDocAs {
                tab_id,
                path: picked,
            });
        });
    }

    pub fn add_files_dialog(&mut self) {
        self.pick_linked_files(None);
    }

    pub(crate) fn add_media_dialog(&mut self, group: slate_doc::media::MediaGroup) {
        if self.tab().read_only {
            return;
        }
        self.pick_linked_files(Some(group));
    }

    fn pick_linked_files(&mut self, group: Option<slate_doc::media::MediaGroup>) {
        let tab_id = self.tab().id;
        let at = self.board_xf().s2w(self.canvas_rect.center());
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new();
            let picked = match group {
                Some(group) => dialog
                    .set_title(format!("Media: {}", group.label()))
                    .add_filter(group.label(), &group.extensions())
                    .pick_files()
                    .map(|paths| paths.into_iter().filter(|p| group.accepts(p)).collect()),
                None => dialog.pick_files(),
            };
            let message = if group.is_some() {
                PickerMsg::AddMedia {
                    tab_id,
                    at,
                    paths: picked,
                }
            } else {
                PickerMsg::AddFiles(picked)
            };
            let _ = tx.send(message);
        });
    }

    fn open_doc_at(&mut self, path: PathBuf) {
        // Same workbook already open (compare canonical paths): focus that
        // tab instead of opening a second copy that would race it on save.
        // This is also the "load a workbook into itself" guard — re-opening
        // the active workbook is a no-op with a toast.
        let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
        let target = canon(&path);
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.path.as_deref().map(&canon) == Some(target.clone()))
        {
            self.switch_tab(i);
            self.leave_home();
            self.toast("Workbook is already open — switched to its tab");
            return;
        }
        match SlateDoc::load_from(&path) {
            Ok(doc) => {
                if self.tabs.is_empty() {
                    self.ensure_work_tab();
                } else if !self.tab().is_blank() {
                    self.new_tab();
                }
                // A reused blank tab keeps its id; its old board's runtime must not.
                self.release_agent_doc(self.active_tab);
                self.record_recent_workbook(&path, &doc);
                let (lease, read_only, holder, held_toast) = match Lease::acquire(&path) {
                    Ok(LeaseState::Acquired(lease)) => (Some(lease), false, None, None),
                    Ok(LeaseState::Held(info)) => {
                        let msg = format!(
                            "Opened read-only — {} has this workbook open",
                            info.describe()
                        );
                        (None, true, Some(info), Some(msg))
                    }
                    Err(e) => {
                        let msg = format!("Opened read-only — could not take write lease ({e})");
                        (None, true, Some(LeaseInfo::unknown()), Some(msg))
                    }
                };
                {
                    let tab = self.tab_mut();
                    // Replacing a blank tab: drop any prior lease (untitled has none).
                    if let Some(old) = tab.lease.take() {
                        old.release();
                    }
                    tab.doc = doc;
                    tab.link_health_revision = None;
                    tab.path = Some(path);
                    tab.dirty = false;
                    tab.lease = lease;
                    tab.read_only = read_only;
                    tab.lease_holder = holder;
                    let view = tab.doc.view.clone();
                    tab.cam.offset = Vec2::new(view.cam_x, view.cam_y);
                    tab.cam.z = atlas_core::display::SLATE_CANVAS.clamp(view.zoom);
                }
                self.selection.clear();
                self.note_scene_change();
                self.leave_home();
                self.publish_session_tags();
                if let Some(msg) = held_toast {
                    self.toast(msg);
                }
            }
            Err(e) => self.toast(format!("Could not open workbook: {e}")),
        }
    }

    fn save_doc_to(&mut self, tab_id: u64, mut path: PathBuf) {
        if path.extension().is_none() {
            path.set_extension(SLATE_EXTENSION);
        }
        let Some(tab_idx) = self.tabs.iter().position(|t| t.id == tab_id) else {
            return;
        };
        if tab_idx == self.active_tab {
            self.lock_all_models();
        }
        // Derive the workbook name from the file name on first save.
        if let Some(stem) = path.file_stem() {
            self.tabs[tab_idx].doc.name = stem.to_string_lossy().into_owned();
        }
        {
            let cam = self.tabs[tab_idx].cam;
            let view = &mut self.tabs[tab_idx].doc.view;
            view.cam_x = cam.offset.x;
            view.cam_y = cam.offset.y;
            view.zoom = cam.z;
        }
        if let Err(e) = self.tabs[tab_idx].doc.save_to(&path) {
            self.toast(format!("Save failed: {e}"));
            return;
        }
        let path_changed = self.tabs[tab_idx].path.as_ref() != Some(&path);
        let need_lease = path_changed || self.tabs[tab_idx].lease.is_none();
        self.tabs[tab_idx].path = Some(path.clone());
        self.tabs[tab_idx].dirty = false;
        // Save-as (or first save of an untitled): take the lease on the new
        // path and clear read-only. Same-path save keeps the lease we hold.
        if !need_lease {
            self.toast("Workbook saved");
            return;
        }
        if let Some(old) = self.tabs[tab_idx].lease.take() {
            old.release();
        }
        let toast = match Lease::acquire(&path) {
            Ok(LeaseState::Acquired(lease)) => {
                let tab = &mut self.tabs[tab_idx];
                tab.lease = Some(lease);
                tab.read_only = false;
                tab.lease_holder = None;
                "Workbook saved".to_string()
            }
            Ok(LeaseState::Held(info)) => {
                let tab = &mut self.tabs[tab_idx];
                tab.read_only = true;
                tab.lease_holder = Some(info.clone());
                format!(
                    "Saved, but open read-only — {} has it open",
                    info.describe()
                )
            }
            Err(e) => {
                let tab = &mut self.tabs[tab_idx];
                tab.read_only = true;
                tab.lease_holder = Some(LeaseInfo::unknown());
                format!("Saved, but could not take write lease ({e})")
            }
        };
        self.toast(toast);
    }

    fn heartbeat_active_lease(&mut self) {
        if self.at_home {
            return;
        }
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            if let Some(lease) = tab.lease.as_mut() {
                let _ = lease.heartbeat();
            }
        }
    }

    /// Add files to the active workbook (uncategorized). Returns new ids.
    ///
    /// `.slate` files are diverted: a workbook can't be an item of a workbook
    /// (that road leads to a board embedding itself), so they're queued to
    /// open as tabs instead — see [`Self::drain_pending_workbooks`].
    /// One pool item for a file already on disk. Workbooks are queued as tabs.
    pub(crate) fn item_for_path(&mut self, path: &std::path::Path) -> Option<ItemId> {
        if !path.is_file() {
            return None;
        }
        if slate_doc::media_kind(path) == slate_doc::MediaKind::Workbook {
            self.pending_workbooks.push(path.to_path_buf());
            return None;
        }
        let (size, mtime) = std::fs::metadata(path)
            .map(|m| {
                let mtime = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                (m.len(), mtime)
            })
            .unwrap_or((0, 0));
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let key = cache_key(&path.to_string_lossy(), size, mtime);
        Some(
            self.doc_mut()
                .add_item(path.to_path_buf(), name, size, mtime, key),
        )
    }

    pub fn add_paths(&mut self, paths: &[PathBuf]) -> Vec<ItemId> {
        let mut added = Vec::new();
        let mut workbooks = 0usize;
        for p in paths {
            if !p.is_file() {
                continue;
            }
            if let Some(item) = self.item_for_path(p) {
                added.push(item);
            } else if slate_doc::media_kind(p) == slate_doc::MediaKind::Workbook {
                workbooks += 1;
            }
        }
        if !added.is_empty() {
            self.toast(format!("Added {} file(s)", added.len()));
        }
        if workbooks > 0 {
            self.toast("Workbooks open as tabs (they can't be placed as items)");
        }
        added
    }

    /// Open queued `.slate` files as tabs (deduped inside `open_doc_at`).
    /// Runs after drop placement so item placement targets the tab the drop
    /// landed on, not a tab a workbook drop just switched to.
    fn drain_pending_workbooks(&mut self) {
        for path in std::mem::take(&mut self.pending_workbooks) {
            self.open_doc_at(path);
        }
    }

    /// Open an item's file: workbooks open in Slate as a tab, everything
    /// else goes to the OS handler.
    pub(crate) fn open_item_path(&mut self, path: &std::path::Path) {
        if slate_doc::media_kind(path) == slate_doc::MediaKind::Workbook {
            self.open_doc_at(path.to_path_buf());
        } else {
            Self::open_path(path);
        }
    }

    // ----- tagging -----------------------------------------------------------

    /// Assign a tag to every item in `ids` (mutual exclusion per group is
    /// enforced by the document).
    pub fn assign_tag(&mut self, ids: &[ItemId], tag: TagId) {
        for id in ids {
            self.doc_mut().assign(*id, tag);
        }
    }

    pub fn unassign_group(&mut self, ids: &[ItemId], group: GroupId) {
        for id in ids {
            self.doc_mut().unassign_group(*id, group);
        }
    }

    /// The set the context-menu action applies to: the whole selection when
    /// the clicked item is part of it, otherwise just the clicked item.
    pub fn action_targets(&self, clicked: ItemId) -> Vec<ItemId> {
        if self.selection.contains(&clicked) {
            self.selection.iter().copied().collect()
        } else {
            vec![clicked]
        }
    }

    // ----- thumbnails ---------------------------------------------------------

    /// Ensure a texture request is in flight for an arbitrary file path
    /// (File Atlas lens cards share this pool with board items).
    pub(crate) fn request_path_thumb(&mut self, path: PathBuf, size: u64, mtime: i64) {
        let key = cache_key(&path.to_string_lossy(), size, mtime);
        if key.is_empty() || self.textures.contains_key(&key) {
            return;
        }
        let slot = self.next_thumb_slot;
        self.next_thumb_slot = self.next_thumb_slot.wrapping_add(1);
        self.thumb_slots.insert(slot, key.clone());
        self.thumbs.request(ThumbRequest {
            id: slot,
            generation: THUMB_GENERATION,
            path,
            key: key.clone(),
            color_only: false,
            shared_dir: None,
            src_bytes: size,
            pdf_page: None,
        });
        self.textures.insert(key, ThumbState::Pending);
    }

    /// Ensure a texture request is in flight for the item's thumbnail.
    pub fn request_thumb(&mut self, item_id: ItemId) {
        let Some((key, path, size, pdf_page)) = self.resolved_item_preview(item_id) else {
            return;
        };
        if key.is_empty() || self.textures.contains_key(&key) {
            return;
        }
        let slot = self.next_thumb_slot;
        self.next_thumb_slot = self.next_thumb_slot.wrapping_add(1);
        self.thumb_slots.insert(slot, key.clone());
        self.thumbs.request(ThumbRequest {
            id: slot,
            generation: THUMB_GENERATION,
            path,
            key: key.clone(),
            color_only: false,
            shared_dir: None,
            src_bytes: size,
            pdf_page,
        });
        self.textures.insert(key, ThumbState::Pending);
    }

    fn drain_thumbs(&mut self, ctx: &egui::Context) {
        let cap = atlas_core::display::SLATE_TEXTURES.uploads_per_frame;
        let mut uploads = 0;
        while uploads < cap {
            let Ok(res) = self.thumbs.rx.try_recv() else {
                break;
            };
            let Some(key) = self.thumb_slots.remove(&res.id) else {
                continue;
            };
            if res.dropped {
                // Shed from an over-full hot queue: forget the pending marker
                // so the paint pass re-requests it while the item is visible.
                self.textures.remove(&key);
                self.thumb_used.remove(&key);
                continue;
            }
            let state = match res.image {
                Some((w, h, rgba)) => {
                    let img =
                        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    // Retain source pixels so board image adjustments (CSS
                    // filter math) can re-render without re-decoding.
                    self.thumb_pixels.insert(key.clone(), img.clone());
                    let tex = ctx.load_texture(
                        format!("slate-thumb-{key}"),
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    uploads += 1;
                    ThumbState::Ready(tex)
                }
                None => ThumbState::Failed,
            };
            self.thumb_used.insert(key.clone(), self.frame_no);
            self.textures.insert(key, state);
            ctx.request_repaint();
        }
        if uploads >= cap {
            ctx.request_repaint();
        }
        self.evict_thumbs();
    }

    fn evict_thumbs(&mut self) {
        let cap = atlas_core::display::SLATE_TEXTURES.resident_cap;
        if self.textures.len() <= cap {
            return;
        }
        let mut by_age: Vec<(u64, String)> = self
            .textures
            .keys()
            .map(|k| (self.thumb_used.get(k).copied().unwrap_or(0), k.clone()))
            .collect();
        by_age.sort_by_key(|(used, _)| *used);
        let drop_n = self.textures.len() - cap + 64;
        for (_, key) in by_age.into_iter().take(drop_n) {
            self.textures.remove(&key);
            self.thumb_pixels.remove(&key);
            self.thumb_used.remove(&key);
        }
    }

    // ----- artifact export ------------------------------------------------------

    /// Poster thumbnails for placed non-image items (PDF pages, doc previews,
    /// video posters) from the shared thumbnail cache. Best effort — only
    /// thumbnails that were already generated (item was viewed) exist; items
    /// without one export as labeled cards.
    fn export_thumb_map(&self) -> std::collections::BTreeMap<ItemId, PathBuf> {
        let cache_dir = atlas_core::index::data_dir().join("thumbs");
        let mut map = std::collections::BTreeMap::new();
        for node in &self.doc().scene.nodes {
            let slate_doc::scene::NodeKind::Image(img) = &node.kind else {
                continue;
            };
            let Some(item) = self.doc().item(img.item) else {
                continue;
            };
            if slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Image
                || item.cache_key.is_empty()
            {
                continue;
            }
            let thumb_key = self.resolved_item_key(item);
            let thumb = cache_dir.join(format!("{}.jpg", thumb_key));
            if thumb.exists() {
                map.insert(img.item, thumb);
            }
        }
        map
    }

    /// Frozen-camera posters for placed 3D model nodes (one per node — the
    /// same model can appear from several saved perspectives). Best effort:
    /// nodes whose poster was never rendered (model never seen on the
    /// board) fall back to the item thumbnail card.
    fn export_model_poster_map(&self) -> std::collections::BTreeMap<slate_doc::NodeId, PathBuf> {
        let mut map = std::collections::BTreeMap::new();
        for info in self.model_nodes() {
            let cam = if info.cam.distance > 0.0 {
                info.cam
            } else {
                // Auto-fit pose: resolvable only if the model was loaded.
                match self.model3d.bounds.get(&info.cache_key) {
                    Some((min, max)) => model3d::resolve_camera(&info.cam, *min, *max),
                    None => continue,
                }
            };
            let aq = model3d::aspect_q(info.rect.w, info.rect.h);
            let poster = model3d::poster_path(&info.cache_key, &cam, aq);
            if poster.exists() {
                map.insert(info.node, poster);
            }
        }
        map
    }

    /// Write the HTML artifact into `<dir>/<workbook>-slides/`.
    fn do_export(&mut self, dir: PathBuf) {
        if self.export_rx.is_some() {
            self.toast("An export is already running.");
            return;
        }
        // Freeze live 3D viewports so the export shows their latest poses.
        self.lock_all_models();
        // Web portals: resolve local sources and capture whatever posters the
        // pool can give us, before anything is written.
        let (web_sources, web_posters) = self.export_web_maps();
        let safe: String = self
            .doc()
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let out = dir.join(format!("{}-slides", safe.trim_matches('-')));
        let mut opts = slate_artifact::ExportOptions {
            agent_images: self.export_agent_images(),
            agent_replies: self.export_agent_replies(),
            inline_assets: self.export_inline,
            thumbs: self.export_thumb_map(),
            model_posters: self.export_model_poster_map(),
            web_sources,
            web_posters,
            wire_routing: self.board_wire_routing,
            ..Default::default()
        };
        let doc = self.doc().clone();
        let workbook = self.tab().path.clone();
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.export_rx = Some(rx);
        self.toast("Exporting artifact…");
        std::thread::spawn(move || {
            let loaded = board_slate::collect_slate_boards(&doc, workbook.as_deref());
            opts.workbook = workbook;
            opts.slate_boards = loaded
                .into_iter()
                .map(|(key, (path, child))| {
                    (key, slate_artifact::ExportedBoard { path, doc: child })
                })
                .collect();
            let mut cloud_only = 0;
            for images in opts.agent_images.values_mut() {
                images.retain(|path| {
                    if atlas_core::cloud::is_dehydrated(path) {
                        cloud_only += 1;
                        false
                    } else {
                        true
                    }
                });
            }
            let result = slate_artifact::export_html(&doc, &out, &opts)
                .map(|mut report| {
                    report.missing_assets += cloud_only;
                    report
                })
                .map_err(|e| e.to_string());
            let _ = tx.send((out, result));
        });
    }

    fn poll_artifact_export(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.export_rx else {
            return;
        };
        match rx.try_recv() {
            Ok((out, result)) => {
                self.export_rx = None;
                match result {
                    Ok(rep) => {
                        let missing = if rep.missing_assets > 0 {
                            format!(" · {} unavailable file(s)", rep.missing_assets)
                        } else {
                            String::new()
                        };
                        self.toast(format!(
                            "Artifact exported — {} slide(s), {} asset(s){missing}",
                            rep.slides, rep.assets_copied
                        ));
                        Self::open_path(&out);
                    }
                    Err(error) => self.toast(format!("Export failed: {error}")),
                }
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(150))
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.export_rx = None;
                self.toast("Export worker stopped.");
            }
        }
    }

    // ----- frame loop ---------------------------------------------------------

    fn drain_pickers(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.picker_rx else { return };
        match rx.try_recv() {
            Ok(msg) => {
                self.picker_rx = None;
                match msg {
                    PickerMsg::OpenDoc(Some(path)) => self.open_doc_at(path),
                    PickerMsg::SaveDocAs {
                        tab_id,
                        path: Some(path),
                    } => self.save_doc_to(tab_id, path),
                    PickerMsg::AddMedia {
                        tab_id,
                        at,
                        paths: Some(paths),
                    } => {
                        if !self.at_home && self.tab().id == tab_id && !self.tab().read_only {
                            let items = self.add_paths(&paths);
                            self.place_items_on_board(&items, at);
                        }
                    }
                    PickerMsg::AddFiles(Some(paths)) => {
                        self.add_paths(&paths);
                    }
                    PickerMsg::AddToFrame {
                        frame,
                        paths: Some(paths),
                    } => {
                        let items = self.add_paths(&paths);
                        self.place_items_in_frame(frame, &items);
                    }
                    PickerMsg::ExportArtifact(Some(dir)) => self.do_export(dir),
                    PickerMsg::WebPortalSource {
                        portal,
                        path: Some(path),
                    } => {
                        self.bind_web_path(portal, path);
                    }
                    PickerMsg::AgentPortalSource {
                        portal,
                        path: Some(path),
                    } => self.bind_agent_project(portal, path),
                    PickerMsg::AtlasPortalSource {
                        portal,
                        path: Some(path),
                    } => self.bind_atlas_folder(portal, path),
                    PickerMsg::SlatePortalSource {
                        portal,
                        path: Some(path),
                    } => self.bind_slate_workbook(ctx, portal, path),
                    _ => {}
                }
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => self.picker_rx = None,
        }
    }

    // `hotkeys` lives in `dispatch.rs`: key input is routed through the
    // command registry (`commands::SPECS`) and `dispatch`, preserving the
    // pre-registry suppression gates. The old ad-hoc Escape cascade became
    // the `atlas_commands::cancel_target` stack.

    fn snapshot_activity(&self) {
        let view = if self.at_home {
            "home"
        } else if self.presenting.is_some() {
            "present"
        } else {
            match self.doc().view.active_view {
                ViewKind::Board => "board",
                ViewKind::Grid => "grid",
                ViewKind::Venn => "venn",
                ViewKind::Lens => "lens",
                ViewKind::Branch => "branch",
                ViewKind::Unknown => "unknown",
            }
        };
        self.session_log
            .set_snapshot(atlas_core::session_log::Snapshot {
                entries: self.doc().items.len() as u32,
                nodes: self.doc().scene.nodes.len() as u32,
                selected: if matches!(self.doc().view.active_view, ViewKind::Board) {
                    self.board_sel.len() as u32
                } else {
                    self.selection.len() as u32
                },
                thumbs_pending: 0,
                extra: self.path_mesh_cache.tess_misses,
                scan_active: false,
                view,
                tool: self.board_tool.label(),
            });
    }

    /// Reconcile links only after link edits; frame work is bounded channel IO.
    fn link_health_frame(&mut self, ctx: &egui::Context) {
        if self.at_home || self.tabs.is_empty() {
            return;
        }
        let _span = atlas_core::session_log::span("slate.link_health");
        let tab = self.tab_mut();
        let revision = (tab.doc.item_paths_revision(), tab.doc.items.len());
        if tab.link_health_revision != Some(revision) {
            tab.link_health
                .sync_paths(tab.doc.items.iter().map(|item| item.path.as_path()));
            tab.link_health_revision = Some(revision);
        }
        let pending = tab.link_health.tick();
        // Poll results while busy, and keep periodic refresh alive when idle.
        ctx.request_repaint_after(Duration::from_millis(if pending { 100 } else { 1000 }));
    }

    /// A restart must account for every workbook, including inactive tabs.
    pub(crate) fn update_close_blocked(&self) -> Option<&'static str> {
        if self.tabs.iter().any(|tab| tab.dirty) {
            Some("Save all open workbooks before restarting.")
        } else if self.export_rx.is_some() || self.picker_rx.is_some() {
            Some("Finish the open file dialog or export before restarting.")
        } else {
            self.atlas
                .as_ref()
                .and_then(|session| session.atlas.update_close_blocked())
        }
    }

    /// One full UI frame (split out for testability, mirroring Atlas).
    pub fn update_app(&mut self, ctx: &egui::Context) {
        self.frame_no += 1;
        self.poll_deferred_fonts(ctx);
        self.poll_recent_prune();
        self.apply_theme(ctx);
        self.preview_reqs_this_frame = 0;
        self.alt_down = ctx.input(|i| i.modifiers.alt);
        self.shift_down = ctx.input(|i| i.modifiers.shift);
        self.ctrl_down = ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);
        self.frame_time = ctx.input(|i| i.time);
        self.drain_pickers(ctx);
        self.resume_unsaved_close_if_ready(ctx);
        self.documents.poll(ctx);
        self.poll_artifact_export(ctx);
        self.heartbeat_active_lease();
        {
            let _span = atlas_core::session_log::span("slate.thumbs");
            self.drain_thumbs(ctx);
        }
        {
            let _span = atlas_core::session_log::span("slate.previews");
            self.drain_previews(ctx);
        }
        self.model3d_frame(ctx);
        self.video_pump(ctx);
        self.note_engine_failure();
        self.session_pump(ctx);
        if self.ai.poll() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        self.agent_pump(ctx);
        self.web_pump(ctx);
        self.atlas_pump(ctx);
        self.slate_pump(ctx);
        self.ai_context_frame();

        // Dropped files land in the active workbook, uncategorized. On the
        // board they're also placed at the drop point; landing on a tagged
        // frame assigns its tags.
        let native_drop = self.external_drop.pop();
        let mut drop_at = None;
        let mut drop_alt = None;
        let mut dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(event) = native_drop {
            ctx.request_repaint(); // drain any remaining bounded OS-drop backlog
            drop_at = Some(event.at);
            drop_alt = Some(event.alt);
            match event.payload {
                external_drop::Payload::Url(url) => {
                    self.drop_web_url(&url, event.at);
                }
                external_drop::Payload::Files(paths) => dropped.extend(paths),
            }
        }
        if !dropped.is_empty() {
            if self.at_home {
                self.leave_home();
                self.ensure_work_tab();
            }
            let at = drop_at
                .or_else(|| ctx.input(|i| i.pointer.hover_pos()))
                .map(|p| self.board_xf().s2w(p))
                .unwrap_or_else(|| self.tab().cam.offset.to_pos2());
            // An HTML page dropped on the board is a portal, not a snippet card
            // (D01). Alt keeps the old text card, which is the only way back.
            let alt = drop_alt.unwrap_or_else(|| ctx.input(|i| i.modifiers.alt));
            self.ingest_dropped_paths(dropped, at, alt);
        }
        // Dropped/added .slate files open as tabs, after placement above.
        self.drain_pending_workbooks();

        self.desktop_sample_frame(ctx);
        if !self.shape_property_keys(ctx) && self.desktop_sample.is_none() {
            self.hotkeys(ctx);
        }
        self.drop_stale_portal_chrome();
        self.link_health_frame(ctx);

        let portal_max = self.portal_chrome.maximized.is_some() && self.presenting.is_none();

        // A maximized portal is the sole interface: skip Slate chrome so the
        // page fills the window at the screen's aspect (P1.portal.maximize).
        if !portal_max {
            // Register the unified top bar first so it is the outermost panel and
            // always spans the full viewport width. Side/bottom chrome is then
            // constrained to the workspace below it.
            self.draw_top_bar(ctx);
            let fullscreen = self.chrome().canvas_fullscreen;
            if !fullscreen {
                let _span = atlas_core::session_log::span("slate.readouts");
                self.draw_readout_bar(ctx);
            }
        }
        self.draw_advanced_window(ctx);
        atlas_shell::tuning::show(ctx);

        // Full bleed against the top bar / readout — same as File Atlas.
        // egui's default CentralPanel frame insets and strokes the canvas,
        // which clips the dock blister's shoulders at the seam.
        let mut central =
            egui::CentralPanel::default().frame(egui::Frame::new().fill(self.palette().bg));
        if portal_max {
            central = central.frame(egui::Frame::NONE);
        }
        central.show(ctx, |ui| {
            if portal_max {
                self.paint_maximized_portal(ui);
            } else if self.at_home {
                self.home_screen(ui);
            } else {
                self.canvas(ui);
            }
        });

        if self.presenting.is_none() && !self.at_home && !portal_max {
            self.draw_tools_rail(ctx);
        }
        // Registry-fed overlays above the canvas (zero cost while closed;
        // presentation mode owns the whole surface).
        if !self.at_home && self.presenting.is_none() && !portal_max {
            self.palette_frame(ctx);
            self.search_frame(ctx);
            self.adjust_popover_frame(ctx);
        }
        if portal_max {
            self.board_action_menu(ctx);
        }
        self.paint_folder_drop_chooser(ctx);
        self.paint_workbook_drop_chooser(ctx);
        if self.presenting.is_none() {
            self.history_frame(ctx);
        }
        self.unsaved_prompt_frame(ctx);
        self.draw_toasts(ctx);
        // Presentation overlay paints above everything, last.
        self.present_frame(ctx);
        self.session_render_atlas(ctx);
        let blocked = self.update_close_blocked();
        if let Some(action) = atlas_shell::updates::window(ctx, &mut self.updater, blocked) {
            self.dispatch(ctx, atlas_commands::CommandId(action), None);
        }
        if matches!(self.updater.state, atlas_update::State::Scheduled)
            && self.update_close_blocked().is_none()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.tabs.iter().any(|tab| tab.dirty) || self.unsaved_close.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if self.unsaved_close.is_none() {
                    self.unsaved_close = Some(UnsavedClose::Exit);
                }
            } else if let Some(reason) = self.update_close_blocked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.toast(reason);
            }
        }

        // Preview upkeep after painting so this frame's `last_used` marks
        // are fresh; keep pumping frames while decodes are in flight.
        self.evict_previews();
        if !self.preview_slots.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        if self.ai.picker_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.external_drop
            .set_url_area(self.web_drop_enabled().then_some(self.canvas_rect));
        self.atlas_run_shell_drag(ctx);
        self.debug_screenshot(ctx);
    }

    /// Dev harness: `SLATE_SHOT=<path>[;delay_frames]` saves a screenshot and exits.
    fn debug_screenshot(&mut self, ctx: &egui::Context) {
        let Ok(spec) = std::env::var("SLATE_SHOT") else {
            return;
        };
        let (path, delay) = match spec.split_once(';') {
            Some((p, d)) => (p.to_string(), d.parse().unwrap_or(240u64)),
            None => (spec, 240),
        };
        if self.frame_no == delay {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot: Option<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            let mut rgba = Vec::with_capacity(w * h * 4);
            for px in &img.pixels {
                rgba.extend_from_slice(&px.to_array());
            }
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
                let _ = buf.save(&path);
            }
            std::process::exit(0);
        }
        ctx.request_repaint();
    }

    /// Maintain the AI live-link beacon: which workbook is open, what's
    /// selected, which files it links to. Self-throttled inside `AiPanel`.
    fn ai_context_frame(&mut self) {
        let tab = self.tab();
        let selection = self.selection.clone();
        let doc = &tab.doc;
        let selection_paths: Vec<PathBuf> = doc
            .items
            .iter()
            .filter(|it| selection.contains(&it.id))
            .map(|it| it.path.clone())
            .collect();
        let truncated = doc.items.len() > atlas_ai::context::MAX_FILES;
        let files: Vec<PathBuf> = doc
            .items
            .iter()
            .take(atlas_ai::context::MAX_FILES)
            .map(|it| it.path.clone())
            .collect();
        let title = doc.name.clone();
        let root = tab.path.clone();
        let session_log = self.session_log.log_path();
        let session_latest = self.session_log.latest_path();
        let last_stall_app_ms = self.session_log.last_stall_app_ms();
        self.ai.update_context(move || atlas_ai::AiAppContext {
            app: "slate",
            title,
            root,
            selection: selection_paths,
            files,
            files_truncated: truncated,
            generated_at: 0,
            session_log,
            session_latest,
            last_stall_app_ms,
        });
    }

    fn draw_toasts(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|(_, t)| t.elapsed().as_secs_f32() < 3.0);
        if self.toasts.is_empty() {
            return;
        }
        let palette = self.palette();
        egui::Area::new(egui::Id::new("slate_toasts"))
            .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -48.0))
            .show(ctx, |ui| {
                for (msg, _) in &self.toasts {
                    egui::Frame::popup(ui.style())
                        .fill(palette.card)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(msg).color(palette.ink));
                        });
                }
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }

    pub(crate) fn ensure_home_cover_bakes(&mut self) {
        #[cfg(not(test))]
        for e in self.recents.entries.clone() {
            let path = e.path.clone();
            if !atlas_shell::covers::schedule_cover_bake(&path) {
                continue;
            }
            std::thread::spawn(move || {
                if !path.is_file() {
                    atlas_shell::covers::note_bake_finished(&path, false);
                    return;
                }
                if atlas_shell::recent::cover_cache_path(&path).is_file() {
                    atlas_shell::covers::note_bake_finished(&path, true);
                    return;
                }
                let ok = if let Ok(doc) = SlateDoc::load_from(&path) {
                    let media = sample_workbook_cover_media(&doc, 9);
                    atlas_shell::covers::bake_workbook_cover(&path, &media).is_some()
                } else {
                    false
                };
                atlas_shell::covers::note_bake_finished(&path, ok);
            });
        }
    }
}

#[cfg(not(test))]
fn sample_workbook_cover_media(doc: &slate_doc::SlateDoc, limit: usize) -> Vec<PathBuf> {
    doc.items
        .iter()
        .filter_map(|it| {
            let ext = it
                .path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let fam = atlas_core::types::Family::from_ext(&ext);
            if atlas_core::types::wants_thumb(fam)
                && matches!(
                    fam,
                    atlas_core::types::Family::Image
                        | atlas_core::types::Family::Video
                        | atlas_core::types::Family::Design
                )
            {
                Some(it.path.clone())
            } else {
                None
            }
        })
        .take(limit)
        .collect()
}

impl eframe::App for SlateApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let _attach = self.session_log.attach();
        let t0 = Instant::now();
        let delivered = ctx.input(|i| i.unstable_dt);
        self.update_app(ctx);
        self.snapshot_activity();
        self.session_log.end_frame(t0.elapsed(), delivered);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.close_enscape();
        self.stop_agent_sidecars();
        self.release_all_leases();
    }
}
