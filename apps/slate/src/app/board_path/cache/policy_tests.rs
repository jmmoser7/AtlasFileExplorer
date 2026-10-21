//! Frame retention protects useful geometry without raising the byte limit.
use super::*;
use crate::app::board_path::cache_tests::{triangle_fill, triangle_ink};

fn cache_for_two_strokes() -> PathMeshCache {
    PathMeshCache {
        budget_bytes: 96,
        ..Default::default()
    }
}

fn paint(cache: &mut PathMeshCache, frame: u64, keys: &[(u64, u64)]) -> u32 {
    cache.begin_frame(frame);
    cache.tess_misses = 0;
    for &(id, key) in keys {
        let geometry = cache.get_or_tessellate(NodeId(id), key, triangle_ink);
        assert_eq!(
            geometry.vertices.len(),
            3,
            "every miss still returns paintable geometry"
        );
    }
    assert_eq!(cache.clock.len(), cache.map.len());
    assert!(cache.resident_bytes <= cache.budget_bytes && cache.map.len() <= cache.entry_limit);
    cache.tess_misses
}

#[test]
fn over_budget_scan_keeps_a_warm_subset_even_when_uncached_geometry_paints_first() {
    let mut cache = cache_for_two_strokes();
    assert_eq!(paint(&mut cache, 1, &[(1, 7), (2, 7), (3, 7)]), 3);
    assert!(cache.map.contains_key(&(NodeId(1), 7, false)));
    assert!(cache.map.contains_key(&(NodeId(2), 7, false)));
    for frame in 2..=101 {
        let keys = if frame.is_multiple_of(2) {
            [(3, 7), (2, 7), (1, 7)]
        } else {
            [(1, 7), (2, 7), (3, 7)]
        };
        assert_eq!(paint(&mut cache, frame, &keys), 1);
    }
}

#[test]
fn leaving_the_view_releases_space_after_one_previous_paint_grace() {
    let mut cache = cache_for_two_strokes();
    paint(&mut cache, 1, &[(1, 7), (2, 7), (3, 7)]);
    assert_eq!(paint(&mut cache, 2, &[(3, 7), (2, 7)]), 1);
    assert!(
        cache.map.contains_key(&(NodeId(1), 7, false)),
        "preceding paint is protected"
    );
    assert_eq!(paint(&mut cache, 3, &[(3, 7), (2, 7)]), 1);
    assert!(!cache.map.contains_key(&(NodeId(1), 7, false)));
    assert_eq!(paint(&mut cache, 4, &[(3, 7), (2, 7)]), 0);
}

#[test]
fn changed_zoom_and_authored_hashes_replace_obsolete_residents_after_the_grace() {
    let path = PathData {
        start: [0.0; 2],
        segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
        ..Default::default()
    };
    let mut stroke = default_curve_stroke(Rgba::BLACK);
    let rect = WorldRect::new(0.0, 0.0, 100.0, 50.0);
    let hash = |stroke: &Stroke, zoom| {
        path_content_hash(
            &path,
            stroke,
            rect,
            0.0,
            slate_doc::scene::Corner::Square,
            zoom_bucket(zoom),
        )
    };
    let original = hash(&stroke, 0.075);
    let zoomed = hash(&stroke, 0.25);
    stroke.width += 1.0;
    let edited = hash(&stroke, 0.25);
    assert_ne!(original, zoomed);
    assert_ne!(zoomed, edited);
    let mut cache = cache_for_two_strokes();
    paint(&mut cache, 1, &[(1, original), (2, original)]);
    for (index, key) in [zoomed, edited].into_iter().enumerate() {
        let first = 2 + index as u64 * 3;
        assert_eq!(paint(&mut cache, first, &[(1, key), (2, key)]), 2);
        assert_eq!(paint(&mut cache, first + 1, &[(1, key), (2, key)]), 2);
        assert_eq!(paint(&mut cache, first + 2, &[(1, key), (2, key)]), 0);
    }
}

#[test]
fn shared_node_ids_with_distinct_content_hashes_coexist_across_painters() {
    let mut cache = cache_for_two_strokes();
    paint(&mut cache, 1, &[(1, 100), (1, 200)]);
    for frame in 2..=10 {
        assert_eq!(paint(&mut cache, frame, &[(1, 200), (1, 100)]), 0);
    }
    assert_eq!(
        cache.map.len(),
        2,
        "never purge other hashes by a node id reused in a child board"
    );
}

#[test]
fn eviction_sweep_is_bounded_per_frame_and_same_frame_begin_cannot_refill_it() {
    let mut cache = cache_for_two_strokes();
    let _scope = cache.enable_probe();
    paint(&mut cache, 1, &[(1, 7), (2, 7)]);
    cache.begin_probe_frame();
    paint(&mut cache, 2, &[(3, 7)]);
    assert_eq!(cache.sweep_remaining, 0);
    for id in 4..100 {
        paint(&mut cache, 2, &[(id, 7)]);
    }
    let stats = cache.probe_snapshot().unwrap().cache;
    assert_eq!(stats.eviction_scans, 2);
    assert_eq!(stats.evictions, 0);
    assert_eq!(stats.admission_bypasses, 97);
}

#[test]
fn a_smaller_candidate_can_fit_after_a_larger_admission_failed() {
    let mut cache = PathMeshCache {
        budget_bytes: 100,
        ..Default::default()
    };
    let _scope = cache.enable_probe();
    cache.begin_frame(1);
    cache.get_or_tessellate(NodeId(1), 7, triangle_ink);
    cache.get_or_fill_tris(NodeId(2), 7, triangle_fill);
    cache.begin_frame(3);
    cache.begin_probe_frame();
    cache.get_or_tessellate(NodeId(1), 7, || panic!("recently used stroke rebuilt"));
    let large = cache.get_or_tessellate(NodeId(3), 7, || InkMesh {
        vertices: vec![
            vector_ink::InkVertex {
                pos: [0.0; 2],
                alpha: 1.0
            };
            5
        ],
        ..Default::default()
    });
    assert_eq!(large.vertices.len(), 5);
    assert!(!cache.map.contains_key(&(NodeId(3), 7, false)));
    assert_eq!(cache.sweep_remaining, 0);
    cache.get_or_fill_tris(NodeId(4), 7, triangle_fill);
    assert!(cache.map.contains_key(&(NodeId(4), 7, true)));
    assert_eq!(cache.resident_bytes, 84);
    let stats = cache.probe_snapshot().unwrap().cache;
    assert_eq!(
        (
            stats.eviction_scans,
            stats.evictions,
            stats.admission_bypasses
        ),
        (2, 1, 1)
    );
}

#[test]
fn oversized_and_disabled_admissions_do_not_scan_or_evict_resident_geometry() {
    let mut cache = cache_for_two_strokes();
    let _scope = cache.enable_probe();
    paint(&mut cache, 1, &[(1, 7), (2, 7)]);
    cache.begin_frame(3);
    cache.begin_probe_frame();
    cache.get_or_tessellate(NodeId(3), 7, || InkMesh {
        vertices: vec![
            vector_ink::InkVertex {
                pos: [0.0; 2],
                alpha: 1.0
            };
            20
        ],
        ..Default::default()
    });
    assert_eq!(cache.probe_snapshot().unwrap().cache.eviction_scans, 0);
    cache.entry_limit = 0;
    cache.get_or_tessellate(NodeId(4), 7, triangle_ink);
    let stats = cache.probe_snapshot().unwrap().cache;
    assert_eq!(
        (
            stats.oversized_rejections,
            stats.disabled_admission_rejections
        ),
        (1, 1)
    );
    assert_eq!(
        (stats.evictions, stats.eviction_scans, cache.map.len()),
        (0, 0, 2)
    );
}

#[test]
fn epoch_zero_has_no_age_underflow_and_application_updates_begin_the_cache_frame() {
    let mut cache = cache_for_two_strokes();
    assert_eq!(paint(&mut cache, 0, &[(1, 7), (2, 7), (3, 7)]), 3);
    assert_eq!(paint(&mut cache, 1, &[(3, 7), (2, 7), (1, 7)]), 1);
    let mut h = crate::app::tests::Harness::new("path_cache_frame_boundary");
    let expected = h.app.frame_no + 1;
    h.frame();
    assert_eq!(
        h.app.path_mesh_cache.frame,
        Some(expected),
        "Home updates also age the cache without board_canvas"
    );
}
