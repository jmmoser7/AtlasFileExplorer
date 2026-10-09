//! A nested board portal over an eraser bar.

use super::*;

/// A nested board portal over screen corners `span` whose child workbook
/// holds a clone of stroke `id` (same node id), loaded and painted.
pub(crate) fn nested_bar_portal(
    h: &mut Harness,
    raster: &mut FrameRaster,
    id: NodeId,
    span: (Pos2, Pos2),
) -> NodeId {
    let wb = h.base.join("child.slate");
    let mut child = SlateDoc::new("Child");
    child
        .scene
        .nodes
        .push(h.app.doc().scene.node(id).unwrap().clone());
    child.save_to(&wb).unwrap();
    h.app.tab_mut().path = Some(h.base.join("parent.slate"));
    let c = h.app.canvas_rect.center();
    let xf = h.app.board_xf();
    let ctx = h.ctx.clone();
    h.app
        .apply_workbook_drop(&ctx, board_slate::WorkbookDropChoice::Insert, wb, xf.s2w(c));
    let portal = h.app.doc().scene.nodes.last().unwrap().id;
    assert_ne!(portal, id);
    let (a, b) = (xf.s2w(span.0), xf.s2w(span.1));
    h.app.patch_nodes(&[portal], |n| {
        n.rect = slate_doc::scene::WorldRect::new(a.x, a.y, b.x - a.x, b.y - a.y);
    });
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Eraser);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.slate_boards_ready() == 0 || !h.app.brush_tiles.last.settled {
        assert!(
            std::time::Instant::now() < deadline,
            "the nested board never loaded"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        shot(h, raster, |_| {});
    }
    portal
}

/// [`nested_bar_portal`] over most of the view, then with stroke bitmaps
/// held the zoom doubles until the child bar's raster for it lands.
pub(crate) fn nested_bar_raster_landed(
    h: &mut Harness,
    raster: &mut FrameRaster,
    id: NodeId,
) -> NodeId {
    let c = h.app.canvas_rect.center();
    let portal = nested_bar_portal(
        h,
        raster,
        id,
        (c - EVec2::new(400.0, 230.0), c + EVec2::new(400.0, 230.0)),
    );
    h.app.brush_tiles.hold_rasters = true;
    h.app.tab_mut().cam.z *= 2.0;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.brush_tiles.stroke_landed_len(true) == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the child bar's raster never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        shot(h, raster, |_| {});
    }
    portal
}
