use super::*;

pub(crate) fn paint_stamped_stroke(
    app: &mut SlateApp,
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
    fade: &impl Fn(Color32) -> Color32,
) {
    // A nested board numbers its strokes as the host does: they go by
    // their cache id, never meet the host's eraser or live canvas, and no
    // tile paints them.
    let nested = app.slate_nesting();
    let cache = app.stroke_cache_id(node.id);
    let want = stamp_pixel_for_zoom(xf.z, painter.ctx().pixels_per_point());
    if let Some(crate::app::board::BoardDrag::Erase {
        points,
        spot,
        straight,
        ..
    }) = &app.board_drag
    {
        if !nested && spot.contains(&node.id) {
            let (points, straight) = (points.clone(), *straight);
            let tip = app.eraser_tip();
            if !app.erase_live.contains_key(&node.id) {
                start_erase_live(app, painter, node, want);
            }
            if let Some(live) = app.erase_live.get_mut(&node.id) {
                live.feed(&points, tip, straight);
                live.pump(
                    &mut app.brush_tiles,
                    tiles::erase_lane(node.id),
                    painter.ctx(),
                );
                live.paint(painter, xf, fade(Color32::WHITE), [0.0; 2]);
                return;
            }
        }
    }
    if !nested && paint_settling_erase(app, painter, xf, node, fade) {
        return;
    }
    let key = stamp_key(node, shape, path);
    if !nested {
        app.sync_agent_brush_previews();
        app.update_agent_brush_opacity(node.id, node.opacity);
    }
    let mut agent_stand_in = !nested
        && app.agent_brush_coverage(node.id, Some(key))
            == Some(crate::app::agent_presence::BrushCoverage::Held);
    if !nested
        && app.agent_brush_coverage(node.id, Some(key))
            == Some(crate::app::agent_presence::BrushCoverage::Active)
        && app.paint_agent_brush_preview(painter, xf, node.id)
    {
        // The cached live prefix paints at this stroke's z-order slot.
        return;
    }
    if agent_stand_in && stroke_raster_current(app, node, key, want) {
        app.release_agent_brush_preview(node.id, key);
        agent_stand_in = false;
    }
    promote_stamp_fallback(app, cache, key);
    let exact = |app: &SlateApp| {
        app.brush_stamps
            .get(&cache)
            .is_some_and(|(k, g)| g.exact && *k == key && g.wanted_pixel == want)
    };
    let tab = app.tab().id;
    if !exact(app) {
        if let Some(r) = app.brush_tiles.take_stroke(cache) {
            let current = r.key == key && r.pixel == want;
            // A stale bitmap replaces neither an eraser stand-in whose pass
            // is applied nor a bitmap of this very content.
            let shown = app
                .brush_stamps
                .get(&cache)
                .is_some_and(|(k, g)| *k == key || g.seal.is_some_and(|s| !s.undone(app)));
            match r.stamp {
                Some(stamp) if current || !shown => {
                    let name = format!("brush-stamp-{}", cache.0);
                    let mut gpu = upload_stamp(painter, &name, stamp, r.image, r.pixel, r.rect);
                    gpu.exact = current;
                    gpu.fallback = stand_in_fallback(app, cache, r.key);
                    gpu.tab = tab;
                    app.brush_stamps.insert(cache, (r.key, gpu));
                    evict_brush_stamps(&mut app.brush_stamps, app.frame_no);
                }
                Some(_) => {}
                None if current => {
                    app.brush_stamps.remove(&cache);
                    return;
                }
                None => {}
            }
        }
    }
    if !exact(app) {
        if take_sync_budget(app, stamp_area_px(node, shape, want)) {
            let Some(stamp) = stroke_stamp(node, shape, path, want) else {
                app.brush_stamps.remove(&cache);
                return;
            };
            let name = format!("brush-stamp-{}", cache.0);
            let mut gpu = upload_stamp(painter, &name, stamp, None, want, node.rect);
            gpu.fallback = stand_in_fallback(app, cache, key);
            gpu.tab = tab;
            app.brush_stamps.insert(cache, (key, gpu));
            evict_brush_stamps(&mut app.brush_stamps, app.frame_no);
        } else {
            // Tiles already rasterize a plain stroke of the host off the
            // frame loop.
            let tiled =
                !nested && app.brush_tiles_enabled && tiles::plain_stamp(app, node).is_some();
            if !tiled {
                app.brush_tiles.request_stroke(cache, tab, node, key, want);
            }
            painter.ctx().request_repaint();
        }
    }
    let stand_in = app
        .brush_live
        .as_ref()
        .filter(|c| !nested && !exact(app) && c.stands_in(node.id, Some(key)))
        .map(|c| c.awaits_anchor());
    if let Some(adds) = stand_in {
        // A canvas that adds only this drag's segments paints over the
        // stroke as it was: the tiles show that, or its last bitmap where
        // it was.
        if adds && !(app.brush_tiles_enabled && tiles::plain_stamp(app, node).is_some()) {
            if let Some((_, gpu)) = app.brush_stamps.get_mut(&cache) {
                gpu.used = app.frame_no;
                paint_stamp_quad(painter, xf, gpu, gpu.rect, fade(Color32::WHITE));
            }
        }
        if let Some(canvas) = app.brush_live.as_mut() {
            canvas.paint(painter, xf);
        }
        return;
    }
    // A synchronous or newly landed exact stamp may become ready later in
    // this same paint call. Hand the held preview back before the final
    // fallback return so the canonical quad is painted in this frame.
    if agent_stand_in && stroke_raster_current(app, node, key, want) {
        app.release_agent_brush_preview(node.id, key);
        agent_stand_in = false;
    }
    if agent_stand_in && app.paint_agent_brush_preview(painter, xf, node.id) {
        return;
    }
    if let Some((built, gpu)) = app.brush_stamps.get_mut(&cache) {
        gpu.used = app.frame_no;
        let moved = gpu.rect == node.rect || stamp_key_at(node, shape, path, gpu.rect) == *built;
        let rect = if moved { node.rect } else { gpu.rect };
        paint_stamp_quad(painter, xf, gpu, rect, fade(Color32::WHITE));
    }
}
