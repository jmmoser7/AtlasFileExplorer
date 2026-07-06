//! In-process session bridge between Slate and File Atlas.
//!
//! When the user opens File Atlas *from Slate*, both apps run in one process
//! (two native viewports) and communicate through a [`SharedSession`]:
//!
//! - **Slate → Atlas**: the open workbook's tag groups ([`SessionState::tag_groups`])
//!   so Atlas can offer them in its right-click menu, plus the Slate window
//!   rectangle in screen coordinates for cross-window drag targeting.
//! - **Atlas → Slate**: tag assignments made in Atlas ([`SessionState::inbox`])
//!   and the live cross-window drag payload ([`SessionState::drag`]).
//!
//! The bridge is deliberately dumb: plain data behind a mutex, no callbacks,
//! drained once per frame by each side. Neither app crate depends on the
//! other's internals through it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// One tag inside a group, as published by Slate.
#[derive(Clone, Debug)]
pub struct SessionTag {
    pub tag_id: u64,
    pub name: String,
    pub color: [u8; 3],
}

/// One tag group (facet). Tags within a group are mutually exclusive on a
/// file; tags across groups combine freely.
#[derive(Clone, Debug)]
pub struct SessionTagGroup {
    pub group_id: u64,
    pub name: String,
    pub tags: Vec<SessionTag>,
}

/// Identity + thumbnail linkage for a file crossing the bridge.
#[derive(Clone, Debug)]
pub struct SessionFile {
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub mtime: i64,
    /// Thumbnail cache key (see `atlas_core::thumbs::cache_key`) so Slate can
    /// reuse Atlas's cached thumbnails without re-extraction.
    pub cache_key: String,
}

/// A tagging event flowing from Atlas to Slate. An empty `tag_ids` means the
/// file arrives uncategorized (e.g. a plain drag-drop without tags).
#[derive(Clone, Debug)]
pub struct TagAssignment {
    pub file: SessionFile,
    pub tag_ids: Vec<u64>,
}

/// Live cross-window drag of thumbnails from Atlas toward Slate.
#[derive(Clone, Debug)]
pub struct DragPayload {
    pub files: Vec<SessionFile>,
    /// Latest pointer position in *screen* coordinates while dragging.
    pub screen_pos: Option<(f32, f32)>,
    /// Set by Atlas on release; Slate consumes the payload if the release
    /// point falls inside its window.
    pub released: bool,
}

/// Screen-space rectangle `(min_x, min_y, max_x, max_y)`.
pub type ScreenRect = (f32, f32, f32, f32);

/// Everything the two apps share during a linked session.
pub struct SessionState {
    /// Published by Slate whenever the workbook's tag structure changes.
    pub tag_groups: Vec<SessionTagGroup>,
    /// Name of the open workbook (for Atlas menu headers).
    pub workbook_name: String,
    /// Tag assignments queued by Atlas, drained by Slate each frame.
    pub inbox: Vec<TagAssignment>,
    /// Active cross-window drag from Atlas, if any.
    pub drag: Option<DragPayload>,
    /// Slate's outer window rect in screen coordinates (drag drop target).
    pub slate_window: Option<ScreenRect>,
    /// Atlas's outer window rect in screen coordinates.
    pub atlas_window: Option<ScreenRect>,
    /// Slate sets this to ask the Atlas viewport to close with the session.
    pub close_requested: bool,
    /// Shared dark/light preference — kept in sync by both apps during a link.
    pub dark_mode: bool,
}

/// Handle shared between the Slate host and the embedded Atlas viewport.
pub type SharedSession = Arc<Mutex<SessionState>>;

impl Default for SessionState {
    fn default() -> Self {
        Self {
            tag_groups: Vec::new(),
            workbook_name: String::new(),
            inbox: Vec::new(),
            drag: None,
            slate_window: None,
            atlas_window: None,
            close_requested: false,
            dark_mode: true,
        }
    }
}

/// Convenience constructor.
pub fn new_session() -> SharedSession {
    Arc::new(Mutex::new(SessionState::default()))
}

impl SessionState {
    /// True when the point (screen coordinates) is inside Slate's window.
    pub fn point_in_slate(&self, x: f32, y: f32) -> bool {
        self.slate_window
            .map(|(x0, y0, x1, y1)| x >= x0 && x <= x1 && y >= y0 && y <= y1)
            .unwrap_or(false)
    }
}
