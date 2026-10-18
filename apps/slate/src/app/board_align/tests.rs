use super::*;

fn r(x: f32, y: f32, w: f32, h: f32) -> WorldRect {
    WorldRect::new(x, y, w, h)
}

fn id(n: u64) -> NodeId {
    NodeId(n)
}

#[test]
fn align_left_shares_the_left_datum() {
    let bounds = r(10.0, 0.0, 100.0, 50.0);
    let a = aligned_rect(r(20.0, 0.0, 10.0, 10.0), bounds, BoardAlign::Left);
    assert_eq!(a.x, 10.0);
    assert_eq!(a.y, 0.0);
}

#[test]
fn align_center_h_centers_in_the_union() {
    let bounds = r(0.0, 0.0, 100.0, 40.0);
    let a = aligned_rect(r(0.0, 0.0, 20.0, 10.0), bounds, BoardAlign::CenterH);
    assert!((a.x - 40.0).abs() < 1e-4);
}

#[test]
fn distribute_h_keeps_ends_and_even_gaps() {
    let ids = [
        (id(1), r(0.0, 0.0, 10.0, 10.0)),
        (id(2), r(20.0, 5.0, 10.0, 10.0)),
        (id(3), r(90.0, 0.0, 10.0, 10.0)),
    ];
    let out = distributed_rects(&ids, DistributeAxis::Horizontal);
    assert!((out[0].1.x - 0.0).abs() < 1e-4);
    assert!((out[2].1.x - 90.0).abs() < 1e-4);
    let g0 = out[1].1.x - (out[0].1.x + out[0].1.w);
    let g1 = out[2].1.x - (out[1].1.x + out[1].1.w);
    assert!((g0 - g1).abs() < 1e-3);
}

#[test]
fn distribute_is_identity_for_two() {
    let ids = [
        (id(1), r(0.0, 0.0, 10.0, 10.0)),
        (id(2), r(40.0, 0.0, 10.0, 10.0)),
    ];
    let out = distributed_rects(&ids, DistributeAxis::Horizontal);
    assert_eq!(out[0].1.x, 0.0);
    assert_eq!(out[1].1.x, 40.0);
}

#[test]
fn locked_ids_do_not_move_in_preview() {
    let ids = [
        (id(1), r(0.0, 0.0, 10.0, 10.0)),
        (id(2), r(40.0, 20.0, 10.0, 10.0)),
    ];
    let out = preview_rects(&ids, &[id(2)], AlignAction::Align(BoardAlign::Left));
    assert!((out[0].1.x - 0.0).abs() < 1e-4);
    assert!((out[1].1.x - 40.0).abs() < 1e-4);
}

#[test]
fn bottom_and_left_t_stems_point_away_from_the_group_box() {
    let bbox = Rect::from_min_max(Pos2::new(100.0, 80.0), Pos2::new(300.0, 200.0));
    let layout = layout_for(bbox, 3, 1.0);
    let bottom = &layout.hits[..4];
    let left = &layout.hits[4..];
    let pitch = ICON_PX + ICON_GAP_PX;
    assert_eq!(bottom[0].rect.center().y, bottom[2].rect.center().y);
    assert_eq!(bottom[3].rect.center().x, bottom[1].rect.center().x);
    assert_eq!(bottom[1].rect.center().y + pitch, bottom[3].rect.center().y);
    assert_eq!(
        bottom[1].rect.center().y,
        bbox.bottom() + bottom_outset_px(1.0) + pitch
    );
    assert_eq!(left[0].rect.center().x, left[2].rect.center().x);
    assert_eq!(left[3].rect.center().y, left[1].rect.center().y);
    assert_eq!(left[1].rect.center().x - pitch, left[3].rect.center().x);
    assert_eq!(
        left[1].rect.center().x,
        bbox.left() - FRAME_OUTSET_PX - pitch
    );
    assert_eq!(bottom[0].action, AlignAction::Align(BoardAlign::Left));
    assert_eq!(
        bottom[3].action,
        AlignAction::Distribute(DistributeAxis::Horizontal)
    );
    assert_eq!(left[0].action, AlignAction::Align(BoardAlign::Top));
    assert!(bottom[3].enabled);
    for hit in &layout.hits {
        assert_eq!(hit_test(hit.rect.center(), &layout), Some(hit.action));
    }
    let top_y = bbox.top() - FRAME_OUTSET_PX;
    let right_x = bbox.right() + FRAME_OUTSET_PX;
    assert!(layout
        .hits
        .iter()
        .all(|h| (h.rect.center().y - top_y).abs() > 0.5));
    assert!(layout
        .hits
        .iter()
        .all(|h| (h.rect.center().x - right_x).abs() > 0.5));
}

#[test]
fn outward_clusters_keep_separate_hits_on_tiny_selections() {
    for size in [
        egui::vec2(0.0, 0.0),
        egui::vec2(10.0, 5.0),
        egui::vec2(80.0, 80.0),
    ] {
        for zoom in [0.05, 0.1, 1.0, 2.0] {
            let layout = layout_for(Rect::from_min_size(Pos2::ZERO, size), 3, zoom);
            for bottom in &layout.hits[..4] {
                for left in &layout.hits[4..] {
                    assert!(
                        !bottom
                            .rect
                            .expand(alignment::HIT_PAD_PX)
                            .intersects(left.rect.expand(alignment::HIT_PAD_PX)),
                        "size {size:?}, zoom {zoom}: {:?} overlaps {:?}",
                        bottom.action,
                        left.action
                    );
                }
            }
        }
    }
}

#[test]
fn distribute_icon_disabled_for_two() {
    let bbox = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(80.0, 80.0));
    let layout = layout_for(bbox, 2, 1.0);
    let dist = layout
        .hits
        .iter()
        .filter(|h| matches!(h.action, AlignAction::Distribute(_)))
        .collect::<Vec<_>>();
    assert!(dist.iter().all(|h| !h.enabled));
    assert!(dist
        .iter()
        .all(|h| hit_test(h.rect.center(), &layout).is_none()));
}

#[test]
fn icon_hit_does_not_overlap_the_inner_bbox() {
    let bbox = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(200.0, 100.0));
    let layout = layout_for(bbox, 3, 1.0);
    for h in &layout.hits {
        assert!(
            !bbox.contains(h.rect.center()),
            "icon {:?} sits inside the selection box",
            h.action
        );
    }
    // Mid-top of the inner box is the N resize handle — must miss the widget.
    assert_eq!(hit_test(Pos2::new(100.0, 0.0), &layout), None);
}

#[test]
fn bottom_cluster_clears_the_width_stringer() {
    for z in [0.5_f32, 0.83, 1.0, 2.0] {
        let bbox = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(200.0, 100.0));
        let layout = layout_for(bbox, 2, z);
        let stringer_y = bbox.bottom() + atlas_shell::selection_tools::STRINGER_GAP * z;
        let min_icon_top = layout
            .hits
            .iter()
            .take(4)
            .filter(|h| h.rect.center().y > bbox.bottom())
            .map(|h| h.rect.min.y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            min_icon_top > stringer_y + 1.0,
            "zoom {z}: icon top {min_icon_top} overlaps stringer {stringer_y}"
        );
    }
}
