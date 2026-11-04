//! Prepared property chrome state shared by the palette and its media owner.
use super::*;

#[derive(Clone)]
pub(in crate::app::board_properties) struct LastChrome {
    pub(in crate::app::board_properties) host: Option<(u64, NodeId)>,
    pub(in crate::app::board_properties) bounds: WorldRect,
    pub(in crate::app::board_properties) items: Vec<StripItem>,
    pub(in crate::app::board_properties) count: usize,
    pub(super) generation: (u64, u64),
    pub(super) picked_vertices: bool,
    pub(in crate::app::board_properties) bottom: Pos2,
    pub(in crate::app::board_properties) blister: bool,
    pub(in crate::app::board_properties) image: bool,
    pub(in crate::app::board_properties) media: Option<media::PreparedBlister>,
}
