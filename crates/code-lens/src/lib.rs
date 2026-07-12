//! Deterministic codebase graph analysis with an optional agent-written overlay.
//!
//! Layer 1 builds a [`CodeGraph`] from Cargo workspaces and Rust sources without
//! inference. Layer 2 is a file contract: agents read `graph.json` and write
//! [`LensOverlay`] JSON that Slate renders on top of the deterministic graph.

pub mod beacon;
pub mod extract;
pub mod layout;
pub mod model;
pub mod overlay;

pub use beacon::LensBeacon;
pub use extract::analyze_workspace;
pub use layout::{layout_graph, LensLayout, LensWire, PlacedNode, Rectf};
pub use model::{
    CodeGraph, EdgeKind, ItemKind, LensEdge, LensError, LensNode, NodeId, NodeKind,
    PackagePressure, WorkspaceSummary,
};
pub use overlay::{lens_dir, match_cluster, read_overlay, LensOverlay, OverlayCluster};
