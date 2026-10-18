use super::*;

impl SlateApp {
    pub(crate) fn drawing_brush_debug(&self, node: NodeId) -> serde_json::Value {
        let cache = &self.presence.brush_preview;
        let key = BrushKey {
            tab: self.tab().id,
            node,
        };
        let entry = cache.entries.get(&key);
        serde_json::json!({
            "frame":self.frame_no,"synced":cache.synced_frame,"painted":cache.painted.get(&key),
            "entry":entry.map(|e| serde_json::json!({
                "seen":e.seen,"rejected":e.rejected,"points":e.points.len(),
                "traced":e.traced,"vertex":e.vertex,"tip":[e.tip_position.x,e.tip_position.y],
                "canvas":e.canvas.as_ref().map(BrushLiveCanvas::preview_debug)
            }))
        })
    }
}
