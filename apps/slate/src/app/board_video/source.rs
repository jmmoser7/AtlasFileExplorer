//! Workbook and media identity for all derived video state.

use super::*;

#[derive(PartialEq)]
pub(super) struct VideoSource {
    path: PathBuf,
    start: f32,
    end: Option<f32>,
    looped: bool,
}

impl VideoSource {
    fn from_video(video: &Vid) -> Self {
        Self {
            path: video.path.clone(),
            start: video.opts.start,
            end: video.opts.end,
            looped: video.opts.looped,
        }
    }
}

impl VideoBoard {
    pub(super) fn forget_source(&mut self, id: NodeId) {
        if self.play.as_ref().is_some_and(|play| play.id == id) {
            self.play = None;
            if let Some(pool) = &self.pool {
                pool.stop();
            }
        }
        if self.live.as_ref().is_some_and(|live| live.id == id) {
            self.live = None;
        }
        if self.hover.is_some_and(|(hover, _)| hover == id) {
            self.hover = None;
        }
        self.strips.remove(&id);
        self.sources.remove(&id);
        self.paused.remove(&id);
        self.textures.remove(&id);
        self.lru.retain(|node| *node != id);
    }

    fn enter_tab(&mut self, tab: Option<u64>) {
        if self.tab == tab {
            return;
        }
        if let Some(pool) = &self.pool {
            pool.stop();
        }
        self.tab = tab;
        self.sources.clear();
        self.strips.clear();
        self.paused.clear();
        self.textures.clear();
        self.lru.clear();
        self.hover = None;
        self.play = None;
        self.live = None;
        // Keep next_token: delayed work from an old workbook cannot acquire
        // the token of a new node with the same document-local NodeId.
    }
}

impl SlateApp {
    pub(super) fn video_at(&self, id: NodeId) -> Option<Vid> {
        let node = self.doc().scene.node(id)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        let item = self.doc().item(img.item)?;
        if slate_doc::media_kind(&item.path) != slate_doc::MediaKind::Video {
            return None;
        }
        Some(Vid {
            path: self.item_path(img.item)?,
            opts: img.video,
        })
    }

    fn active_video_tab(&self) -> Option<u64> {
        (!self.at_home && !self.tabs.is_empty()).then(|| self.tab().id)
    }

    fn source_matches(&self, id: NodeId, source: &VideoSource) -> bool {
        let Some(node) = self.doc().scene.node(id) else {
            return false;
        };
        let NodeKind::Image(image) = &node.kind else {
            return false;
        };
        self.doc().item(image.item).is_some_and(|item| {
            self.item_path(image.item).as_deref() == Some(source.path.as_path())
                && slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Video
                && image.video.start == source.start
                && image.video.end == source.end
                && image.video.looped == source.looped
        })
    }

    /// Visits only the bounded resident sources, never the whole workbook.
    pub(super) fn sync_video_context(&mut self) {
        self.video.enter_tab(self.active_video_tab());
        while let Some(id) = self
            .video
            .sources
            .iter()
            .find_map(|(id, source)| (!self.source_matches(*id, source)).then_some(*id))
        {
            self.video.forget_source(id);
        }
    }

    pub(super) fn current_video_source(&self, id: NodeId) -> Option<&VideoSource> {
        if self.video.tab != self.active_video_tab() || self.video.tab.is_none() {
            return None;
        }
        self.video
            .sources
            .get(&id)
            .filter(|source| self.source_matches(id, source))
    }

    pub(super) fn ensure_video_source(&mut self, id: NodeId) -> Option<Vid> {
        self.sync_video_context();
        self.active_video_tab()?;
        let video = self.video_at(id)?;
        if !self.video.sources.contains_key(&id) {
            self.evict_strips(id);
            self.video
                .sources
                .insert(id, VideoSource::from_video(&video));
            self.touch_strip(id);
        }
        Some(video)
    }
}
