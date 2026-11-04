//! Immutable model revision links; provider memory remains independently owned.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelOutputRef {
    pub id: String,
    pub source: SourceUri,
    pub size: u64,
    pub mtime: i64,
}
