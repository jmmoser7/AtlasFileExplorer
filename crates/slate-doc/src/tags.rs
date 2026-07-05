use serde::{Deserialize, Serialize};

use crate::ids::{GroupId, TagId};

/// A single tag with a display name and RGB accent color.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub name: String,
    pub color: [u8; 3],
}

/// A facet grouping mutually exclusive tags (e.g. Size: Big / Medium / Small).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagGroup {
    pub id: GroupId,
    pub name: String,
    pub tags: Vec<Tag>,
}
