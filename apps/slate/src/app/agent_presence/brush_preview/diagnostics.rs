use super::*;

#[test]
fn absent_preview_does_not_evaluate_authored_content() {
    let cache = DrawingBrushPreview::default();
    assert_eq!(
        cache.covered(1, NodeId(1), || panic!("absent preview hashed the path")),
        None
    );
}

#[test]
fn ordinary_strokes_do_not_hash_for_agent_coverage() {
    use crate::app::tests::{add_stroke, Harness};
    let mut h = Harness::new("ordinary_brush_coverage");
    let ids: Vec<_> = (0..256)
        .map(|i| add_stroke(&mut h.app, i as f32 * 120.0, 0.0))
        .collect();
    let hashes = board_path::content_hashed_on_this_thread();
    for id in ids {
        assert_eq!(h.app.agent_brush_coverage(id, None), None);
    }
    assert_eq!(board_path::content_hashed_on_this_thread(), hashes);
}

impl SlateApp {
    pub(crate) fn drawing_brush_debug(&self, node: NodeId) -> serde_json::Value {
        let cache = &self.presence.brush_preview;
        let key = BrushKey {
            tab: self.tab().id,
            node,
        };
        let entry = cache.entries.get(&key);
        let mut paths = Vec::new();
        for agent in self.presence.agents.iter().filter(|a| a.tab == key.tab) {
            let Some((work, sample)) = agent.work.as_ref().zip(
                self.presence
                    .roster
                    .session(&agent.motion)
                    .map(PresenceSession::sample),
            ) else {
                continue;
            };
            work.for_each_drawing_brush_path(sample, |path| {
                let content = board_path::node_stamp_key(path.node);
                paths.push(serde_json::json!({
                    "node":path.node.id.0,"lane":path.lane_id,"motion":agent.motion.as_str(),
                    "key":content,"points":path.points.len(),"tips":path.tips.len(),
                    "vertex":path.vertex_index,"finished":path.finished,
                    "admitted":content.is_some_and(|k| cache.can_admit(key.tab,path.node.id,k,
                        agent.motion.as_str(),path.lane_id,path.points.len()))
                }));
            });
        }
        serde_json::json!({
            "frame":self.frame_no,"synced":cache.synced_frame,"painted":cache.painted.get(&key),
            "paths":paths,"pixelBudget":cache.pixel_budget,
            "entry":entry.map(|e| serde_json::json!({
                "key":e.content_key,"motion":e.motion,"lane":e.lane,
                "seen":e.seen,"rejected":e.rejected,"points":e.points.len(),
                "traced":e.traced,"vertex":e.vertex,"tip":[e.tip_position.x,e.tip_position.y],
                "canvas":e.canvas.as_ref().map(BrushLiveCanvas::preview_debug)
            }))
        })
    }
}
