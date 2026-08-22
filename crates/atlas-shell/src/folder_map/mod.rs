//! File Atlas folder map — one painter, both apps.
//!
//! The standalone File Atlas window and the Slate File Atlas *portal* paint
//! and navigate the same surface: tidy tree, orthogonal leaders, directory
//! collapse grips, file cards, group-preview mosaics, and the File Atlas
//! camera (`screen = world × z + offset`). Window chrome stays out of this
//! module (Art. X). Apps supply scan/thumbs and decide which Atlas-only
//! gestures (Edit drag, shell drag-out) are armed.

mod cam;
mod collapse;
mod input;
mod paint;

pub use cam::FolderCam;
pub use collapse::{apply_collapse, record_collapse, toggle_dir, DirGrip, ToggleOutcome};
pub use input::{hover_at, MapHover};
pub use paint::{
    folder_heat_color, lod_for, paint_tree, LeaderStyle, MapMedia, MapStyle, PaintArgs, LOD_DETAIL,
    LOD_FULL, LOD_MID,
};
