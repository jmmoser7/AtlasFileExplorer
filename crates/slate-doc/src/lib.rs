//! Document model for [`.slate`](SLATE_EXTENSION) workbook files.
//!
//! A Slate workbook stores links to source files (not copies), a hierarchical
//! faceted tag system, and persisted view state.

mod doc;
mod error;
mod ids;
mod item;
pub mod lease;
mod link;
pub mod media;
pub mod scene;
mod spatial;
mod tags;
mod view;

pub use doc::{SlateDoc, SLATE_EXTENSION};
pub use error::SlateLoadError;
pub use ids::{GroupId, ItemId, TagId};
pub use item::SlateItem;
pub use lease::{Lease, LeaseInfo, LeaseState, LEASE_HEARTBEAT_SECS, LEASE_STALE_SECS};
pub use link::{link_status, LinkStatus};
pub use media::{media_kind, MediaKind};
pub use scene::{
    Node, NodeId, NodeKind, PortalClass, PortalKind, PortalNode, RepoPortalQuery, RepoTimeAxis,
    Scene, SceneCmd, SceneJournal, SourceUri, WorldRect, REPO_PORTAL_DEFAULT_H,
    REPO_PORTAL_DEFAULT_W,
};
pub use tags::{Tag, TagGroup};
pub use view::{ViewKind, ViewState};
