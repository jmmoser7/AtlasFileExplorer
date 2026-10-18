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
    /// Occupancy is published only by geometry painted during this palette pass.
    pub(in crate::app::board_properties) fn occupy_result_chrome(
        &mut self,
        owner: Option<(u64, NodeId)>,
        rect: Rect,
    ) {
        let Some((tab, host)) = owner.filter(|(tab, _)| *tab == self.tab().id) else {
            return;
        };
        let rect = rect.intersect(self.canvas_rect);
        if rect.is_positive() {
            let previous = self
                .shape_properties
                .result_chrome_bottom
                .filter(|(old_tab, old_host, _)| (*old_tab, *old_host) == (tab, host))
                .map_or(rect.bottom(), |(_, _, bottom)| bottom.max(rect.bottom()));
            self.shape_properties.result_chrome_bottom = Some((tab, host, previous));
        }
    }

    pub(in crate::app::board_properties) fn occupy_property_popups(&mut self, popups: &[Rect]) {
        let owner = self
            .shape_properties
            .last_chrome
            .as_ref()
            .and_then(|last| last.host);
        for &popup in popups {
            self.occupy_result_chrome(owner, popup);
        }
    }

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
