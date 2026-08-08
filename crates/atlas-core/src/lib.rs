//! Shared backend for the Atlas ecosystem (File Atlas + Slate).
//!
//! Everything here is UI-free application logic: the file taxonomy, the
//! parallel directory scanner, the SQLite index, the thumbnail worker pool
//! and cache tiers, the tidy-tree layout engine, the undo journal, and the
//! copy-only exporter. Both `apps/file-atlas` and `apps/slate` build on this
//! crate; app-specific state and chrome never live here.

pub mod cloud;
pub mod dirmeta;
pub mod export;
pub mod folder_heat;
pub mod fsops;
pub mod index;
pub mod journal;
pub mod metadata;
pub mod office;
pub mod owners;
pub mod pdf;
pub mod preview;
pub mod rasterthumb;
pub mod scanner;
pub mod shell_drag;
pub mod skiplist;
pub mod svg;
pub mod threedm;
pub mod thumbs;
pub mod timeline;
pub mod tree;
pub mod types;
pub mod watcher;
