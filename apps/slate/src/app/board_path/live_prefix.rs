use super::*;

impl BrushLiveCanvas {
    /// Stamp the newly reached points of a cached, monotonically growing
    /// route. The active endpoint is supplied separately with [`set_line`],
    /// so a moving tip never gets stranded behind an unchanged point count.
    pub fn add_freehand(&mut self, points: &[Pos2], tips: &[StrokeSpan]) {
        if tips.is_empty() || points.is_empty() {
            return;
        }
        let at = |index: usize| TipPoint {
            pos: [points[index].x, points[index].y],
            tip: stamp_style(tips[index.min(tips.len() - 1)]),
        };
        let mut dirty = None;
        if self.freehand_done == 0 {
            if let Some(bounds) = self.stamp(at(0), at(0)) {
                dirty = Some(bounds);
            }
        }
        for index in self.freehand_done.max(1)..points.len() {
            if let Some(bounds) = self.stamp(at(index - 1), at(index)) {
                dirty = Some(dirty.map_or(bounds, |old| union_box(Some(old), bounds)));
            }
        }
        self.freehand_done = points.len();
        if let Some(bounds) = dirty {
            self.upload(bounds);
        }
    }

    /// Remove the changing endpoint after the complete route has been stamped.
    pub fn clear_live_line(&mut self) {
        self.line = None;
    }
}
