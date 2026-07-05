//! The live link: a continuously updated context file inside the AI
//! workspace describing what each app is previewing right now.
//!
//! Written to `<workspace>/.atlas-ai/<app>-context.json`. Future MCP servers
//! read these files to give Cursor full view of Atlas/Slate state; keeping
//! them plain JSON means the link works before any server exists.

use crate::config::LINK_DIR;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Cap on listed file paths so beacons stay small on 100k-file roots.
pub const MAX_FILES: usize = 500;

/// Snapshot of one app's current view, serialized into the workspace.
#[derive(Clone, Debug, Serialize)]
pub struct AiAppContext {
    /// `"file-atlas"` or `"slate"`.
    pub app: &'static str,
    /// Human title: folder name in Atlas, workbook name in Slate.
    pub title: String,
    /// Open root folder (Atlas) or `.slate` file (Slate).
    pub root: Option<PathBuf>,
    /// Absolute paths currently selected by the user.
    pub selection: Vec<PathBuf>,
    /// Absolute paths of the files in view (capped at [`MAX_FILES`]).
    pub files: Vec<PathBuf>,
    /// True when `files` was truncated by the cap.
    pub files_truncated: bool,
    /// Seconds since the Unix epoch at write time.
    pub generated_at: u64,
}

impl AiAppContext {
    /// Fingerprint of the *content* (not the timestamp) — used to skip
    /// rewrites when nothing changed.
    pub fn fingerprint(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.app.hash(&mut h);
        self.title.hash(&mut h);
        self.root.hash(&mut h);
        self.selection.hash(&mut h);
        self.files.hash(&mut h);
        h.finish()
    }
}

/// Write the context beacon (atomic: temp file + rename). Returns the path
/// written to, for status display.
pub fn write_context(workspace: &Path, ctx: &AiAppContext) -> std::io::Result<PathBuf> {
    let dir = workspace.join(LINK_DIR);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}-context.json", ctx.app));
    let json = serde_json::to_string_pretty(ctx)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacon_round_trip_and_fingerprint_stability() {
        let dir = std::env::temp_dir().join(format!(
            "atlas_ai_ctx_{}_{}",
            std::process::id(),
            now_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut ctx = AiAppContext {
            app: "slate",
            title: "Moodboard".into(),
            root: Some(PathBuf::from("/tmp/moodboard.slate")),
            selection: vec![PathBuf::from("/tmp/a.png")],
            files: vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")],
            files_truncated: false,
            generated_at: 123,
        };
        let fp = ctx.fingerprint();
        // Timestamp changes must not change the fingerprint…
        ctx.generated_at = 456;
        assert_eq!(fp, ctx.fingerprint());
        // …but content changes must.
        ctx.selection.clear();
        assert_ne!(fp, ctx.fingerprint());

        let path = write_context(&dir, &ctx).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"app\": \"slate\""));
        assert!(text.contains("moodboard.slate"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
