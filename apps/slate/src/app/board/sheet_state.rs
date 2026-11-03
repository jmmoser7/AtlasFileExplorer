//! Shared board spreadsheet interaction state.
use super::*;

pub(crate) struct SheetMark {
    pub item: ItemId,
    pub path: PathBuf,
    pub row: usize,
    pub col: usize,
    pub prior: atlas_core::office::PriorCell,
}

#[derive(Clone)]
pub(crate) struct SheetHit {
    pub node: NodeId,
    pub item: ItemId,
    pub row: usize,
    pub col: usize,
    pub rect: Rect,
    pub add: bool,
}

pub(crate) struct SheetEdit {
    pub node: NodeId,
    pub item: ItemId,
    pub row: usize,
    pub col: usize,
    pub buf: String,
    pub origin: String,
    /// The edit started as a new column, so naming it stays one undo step.
    pub fresh: bool,
    pub screen: Rect,
    pub font_px: f32,
}

/// A column or row boundary the open spreadsheet can drag.
#[derive(Clone, Copy)]
pub(crate) struct SheetGrip {
    pub node: NodeId,
    pub rect: Rect,
    /// Column to the left of this boundary. `None` is a row boundary.
    pub col: Option<usize>,
    pub row: Option<usize>,
}
