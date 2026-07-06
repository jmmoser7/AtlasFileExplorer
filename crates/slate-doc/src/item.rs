use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ids::{GroupId, ItemId, TagId};

/// A link to a source file on disk, with optional facet assignments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlateItem {
    pub id: ItemId,
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub mtime: i64,
    pub cache_key: String,
    /// 0-based PDF page shown as this item's poster (0 = first page).
    #[serde(default)]
    pub pdf_page: u16,
    /// At most one tag per group; empty means uncategorized.
    pub assignments: BTreeMap<GroupId, TagId>,
}
