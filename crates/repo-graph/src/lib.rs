//! Deterministic git history extraction and layout for Slate repository portals.
//!
//! This crate is deliberately UI-free: it reads a local git worktree or bare
//! repository, builds an honest commit/ref model, and produces layout geometry
//! that the Slate painter and artifact writer can interpret later.

pub mod extract;
pub mod layout;
pub mod model;

pub use extract::{extract_repository, RepoError};
pub use layout::{layout_graph, Elision, Join, PlacedCommit, RefLabel, RepoLayout, Ribbon, Size};
pub use model::{
    Author, Commit, CommitIx, Oid, RefKind, RefSelection, Remote, RepoGraph, RepoQuery, RepoRef,
    TimeAxis, TimeWindow,
};
