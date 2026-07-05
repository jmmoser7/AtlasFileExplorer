//! Shared backend for the Atlas ecosystem (File Atlas + Slate).
//!
//! Everything here is UI-free application logic: the file taxonomy, the
//! parallel directory scanner, the SQLite index, the thumbnail worker pool
//! and cache tiers, the tidy-tree layout engine, the undo journal, and the
//! copy-only exporter. Both `apps/file-atlas` and `apps/slate` build on this
//! crate; app-specific state and chrome never live here.

pub mod export;
pub mod index;
pub mod journal;
pub mod metadata;
pub mod office;
pub mod pdf;
pub mod scanner;
pub mod threedm;
pub mod thumbs;
pub mod tree;
pub mod types;
pub mod watcher;
