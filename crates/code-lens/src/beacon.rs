use std::path::Path;
use std::time::{Instant, SystemTime};

use crate::model::CodeGraph;
use crate::overlay::LensOverlay;

#[derive(Debug, Default)]
pub struct LensBeacon {
    #[allow(dead_code)]
    last_write: Option<Instant>,
    #[allow(dead_code)]
    last_fingerprint: u64,
    #[allow(dead_code)]
    last_overlay_mtime: Option<SystemTime>,
}

impl LensBeacon {
    pub fn new() -> Self {
        Self::default()
    }

    /// Throttled (>=1s, fingerprint-gated) atomic write of
    /// <ai_workspace>/.atlas-ai/lens/graph.json. Safe to call every frame.
    /// Returns true when a write happened.
    pub fn tick_write(
        &mut self,
        _ai_workspace: &Path,
        _source_root: &Path,
        _graph: &CodeGraph,
    ) -> bool {
        false
    }

    /// Polls overlay.json mtime (>=1s). Returns Some only when the file
    /// (re)appeared or changed since last successful load.
    pub fn tick_read(&mut self, _ai_workspace: &Path) -> Option<LensOverlay> {
        None
    }
}
