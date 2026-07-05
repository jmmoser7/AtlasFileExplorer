use crate::item::SlateItem;

/// Whether the linked source file still exists on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    Ok,
    Missing,
}

/// Returns [`LinkStatus::Ok`] when [`SlateItem::path`] exists on the filesystem.
pub fn link_status(item: &SlateItem) -> LinkStatus {
    if item.path.exists() {
        LinkStatus::Ok
    } else {
        LinkStatus::Missing
    }
}
