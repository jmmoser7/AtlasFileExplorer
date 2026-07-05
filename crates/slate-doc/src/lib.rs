//! Document model for [`.slate`](SLATE_EXTENSION) workbook files.
//!
//! A Slate workbook stores links to source files (not copies), a hierarchical
//! faceted tag system, and persisted view state.

mod doc;
mod error;
mod ids;
mod item;
mod link;
mod tags;
mod view;

pub use doc::{SlateDoc, SLATE_EXTENSION};
pub use error::SlateLoadError;
pub use ids::{GroupId, ItemId, TagId};
pub use item::SlateItem;
pub use link::{link_status, LinkStatus};
pub use tags::{Tag, TagGroup};
pub use view::{ViewKind, ViewState};
