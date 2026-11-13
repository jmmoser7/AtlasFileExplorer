use super::*;

pub(in crate::app) fn parse_grasshopper_preview(
    path: &Path,
) -> Result<(PreviewScene, String), String> {
    parse_with_progress(path, &ParseProgress::new())
}

impl SlateApp {
    pub(in crate::app) fn install_grasshopper_preview(
        &mut self,
        doc: u64,
        id: NodeId,
        key: String,
        scene: PreviewScene,
        hash: String,
    ) {
        let bounds = (scene.bounds_min, scene.bounds_max);
        self.model3d.bounds.insert(key.clone(), bounds);
        self.model3d.model_hashes.insert(key.clone(), hash);
        self.model3d.models.insert(
            key.clone(),
            CpuEntry {
                state: ModelState::Ready(Arc::new(scene)),
                last_used: Instant::now(),
                progress: Arc::new(ParseProgress::new()),
            },
        );
        if self.tab().id == doc {
            if let Some(view) = self.model3d.live.get_mut(&id) {
                view.cache_key = key;
                view.rendered = None;
                view.tex = None;
                view.radius = bounds_sphere(bounds.0, bounds.1).1;
            }
            self.model3d.display_swatch_stamp.remove(&(doc, id));
        }
    }

    pub(in crate::app) fn clear_grasshopper_preview(&mut self, doc: u64, id: NodeId) {
        if self.tab().id == doc {
            self.model3d.live.remove(&id);
        }
        self.model3d.want_poster.remove(&(doc, id));
    }
}
