use serde::{Deserialize, Serialize};

/// Identifier for a tag within a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TagId(pub u64);

/// Identifier for a tag group (facet) within a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupId(pub u64);

/// Identifier for a linked file entry within a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(pub u64);

impl ItemId {
    /// No linked file yet: an image an agent is generating before anyone has
    /// picked a result. Allocated ids start at 1, so no item has it.
    pub const NONE: ItemId = ItemId(0);

    pub fn is_none(self) -> bool {
        self == Self::NONE
    }
}
