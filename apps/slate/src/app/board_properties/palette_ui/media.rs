//! The actual prepared media controls own the same-frame body cutout.
use super::*;

#[derive(Clone, Copy)]
pub(in crate::app::board_properties) struct PreparedBlister {
    pub(in crate::app::board_properties) tab: u64,
    pub(in crate::app::board_properties) host: NodeId,
    pub(in crate::app::board_properties) count: usize,
    pub(in crate::app::board_properties) presentation: chrome::BlisterPresentation,
}

impl SlateApp {
    pub(in crate::app::board_properties) fn prepare_media_blister(
        &mut self,
        count: usize,
        presentation: chrome::BlisterPresentation,
    ) {
        let tab = self.shape_properties.tab;
        let Some(state) = self.shape_properties.last_chrome.as_mut() else {
            return;
        };
        let host = match self.shape_properties.ids.as_slice() {
            [host] => Some(*host),
            [] => state
                .media
                .filter(|prepared| prepared.tab == tab)
                .map(|p| p.host),
            _ => None,
        };
        state.media = host.map(|host| PreparedBlister {
            tab,
            host,
            count,
            presentation,
        });
    }

    pub(in crate::app::board_properties) fn clear_prepared_media_blister(&mut self) {
        if let Some(state) = self.shape_properties.last_chrome.as_mut() {
            state.media = None;
        }
    }
}
