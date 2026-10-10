//! Lazy full-resolution preview tests.

use super::*;

// ----- lazy full-resolution previews ----------------------------------------------

#[test]
fn full_res_preview_upgrades_and_evicts() {
    let mut h = Harness::new("preview");
    // Tests must not depend on the developer's persisted settings file.
    h.app.settings.preview = settings::PreviewSettings::default();
    let p = h.base.join("real.png");
    image::RgbaImage::from_pixel(600, 400, image::Rgba([10, 200, 30, 255]))
        .save(&p)
        .unwrap();
    let ids = h.app.add_paths(&[p]);
    let key = h.app.doc().item(ids[0]).unwrap().cache_key.clone();

    // Below the upgrade threshold nothing is queued.
    let _ = h.app.item_texture(ids[0], 100.0);
    assert!(h.app.preview_slots.is_empty());

    // A zoomed-in paint queues one decode; frames drain it into the cache.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let _ = h.app.item_texture(ids[0], 800.0);
        h.frame();
        if h.app.preview_cache.contains_key(&key) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "preview never arrived"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let entry = h.app.preview_cache.get(&key).unwrap();
    // A 600×400 source decoded toward tier 1024 is exhausted: it satisfies
    // every future zoom level without re-decoding.
    assert_eq!(entry.px, preview::PX_EXACT);
    assert_eq!(entry.bytes, 600 * 400 * 4);
    let tex = h.app.item_texture(ids[0], 800.0).expect("preview texture");
    assert_eq!(tex.size(), [600, 400], "preview replaced the 192px thumb");
    assert_eq!(h.app.preview_cache_stats(), (1, 600 * 400 * 4));

    // Shrinking the budget evicts entries once they age past the two-frame
    // protection window (the default zoomed-out grid never touches them).
    h.app.settings.preview.budget_mb = 0;
    for _ in 0..3 {
        h.frame();
    }
    assert!(
        h.app.preview_cache.is_empty(),
        "over-budget preview evicted"
    );
}

/// Seeded "png-ish" bytes decode as neither thumbnail nor preview: the key
/// must land in the failed set and never be re-requested.
#[test]
fn undecodable_sources_fail_once_and_stop_asking() {
    let mut h = Harness::new("preview_fail");
    h.app.settings.preview = settings::PreviewSettings::default();
    h.seed();
    let item = h.app.doc().items[0].id;
    let key = h.app.doc().item(item).unwrap().cache_key.clone();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let _ = h.app.item_texture(item, 800.0);
        h.frame();
        if h.app.preview_failed.contains(&key) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "failure never recorded"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // Failed keys are never re-requested…
    let _ = h.app.item_texture(item, 800.0);
    assert!(h.app.preview_slots.is_empty());
    // …until the cache is cleared (environment may have changed).
    h.app.clear_preview_cache();
    assert!(h.app.preview_failed.is_empty());
}

#[test]
fn remove_group_strips_assignments_via_menu_path() {
    let mut h = Harness::new("rmgroup");
    let (_big, _small, red) = h.seed();
    let group = h.app.doc().groups[0].id; // Size
    h.app.doc_mut().remove_group(group);
    for item in &h.app.doc().items {
        assert!(!item.assignments.contains_key(&group));
    }
    // Red assignment (other group) survives.
    assert_eq!(h.app.doc().items_with_tag(red).len(), 1);
    h.frame();
}

#[test]
fn path_node_add_undo_via_journal() {
    use slate_doc::scene::{PathData, PathSeg, ShapeKind, ShapeNode};
    let mut h = Harness::new("path_journal");
    h.seed();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0);
    let node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(std::sync::Arc::new(PathData {
                start: [0.0, 0.5],
                segs: vec![PathSeg::Line { to: [1.0, 0.5] }],
                closed: false,
                ..Default::default()
            })),

            text: None,
        }),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.app.board_undo();
    assert!(h.app.doc().scene.node(id).is_none());
    h.frame();
}
