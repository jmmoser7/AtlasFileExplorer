use super::*;
use crate::app::board_path::cache_tests::triangle_ink;

#[test]
fn instrumentation_is_opt_in_and_scope_restores_conversion_collection() {
    let mut cache = PathMeshCache::default();
    cache.get_or_tessellate(NodeId(1), 1, triangle_ink);
    assert!(cache.probe_snapshot().is_none());
    assert!(CONVERSIONS.with(|stats| stats.borrow().is_none()));
    {
        let _scope = cache.enable_probe();
        assert!(CONVERSIONS.with(|stats| stats.borrow().is_some()));
        cache.disable_probe();
        assert!(cache.probe_snapshot().is_none());
    }
    assert!(CONVERSIONS.with(|stats| stats.borrow().is_none()));
}

#[test]
fn probe_reports_a_stable_over_budget_subset_and_explicit_key_turnover() {
    let mut cache = PathMeshCache {
        budget_bytes: 96,
        ..Default::default()
    };
    let _scope = cache.enable_probe();
    cache.begin_frame(1);
    for id in 0..3 {
        cache.get_or_tessellate(NodeId(id), 7, triangle_ink);
    }
    for frame in 2..=4 {
        cache.begin_frame(frame);
        cache.begin_probe_frame();
        for id in 0..3 {
            cache.get_or_tessellate(NodeId(id), 7, triangle_ink);
        }
        let row = cache.probe_snapshot().unwrap();
        assert_eq!(
            (
                row.cache.hits,
                row.cache.builds,
                row.cache.evictions,
                row.cache.key_changes
            ),
            (2, 1, 0, 0)
        );
        assert_eq!(row.cache.requested_bytes, 144);
        assert_eq!(row.cache.unique_requested_bytes, 144);
        assert_eq!((row.resident_bytes, row.resident_entries), (96, 2));
        assert_eq!(
            (row.cache.eviction_scans, row.cache.admission_bypasses),
            (2, 1)
        );
    }
    cache.begin_frame(5);
    cache.begin_probe_frame();
    cache.get_or_tessellate(NodeId(0), 8, triangle_ink);
    assert_eq!(cache.probe_snapshot().unwrap().cache.key_changes, 1);
    // No old key is touched in frame5; its replacement becomes retainable
    // after the grace and then hits without another build.
    cache.begin_frame(6);
    cache.get_or_tessellate(NodeId(0), 8, triangle_ink);
    cache.begin_frame(7);
    cache.begin_probe_frame();
    cache.get_or_tessellate(NodeId(0), 8, || panic!("replacement did not become warm"));
    let row = cache.probe_snapshot().unwrap();
    assert_eq!(
        (row.cache.hits, row.cache.builds, row.cache.key_changes),
        (1, 0, 0)
    );
}

#[test]
fn probe_counts_oversized_rejection_without_changing_admission() {
    let mut cache = PathMeshCache {
        budget_bytes: 48,
        ..Default::default()
    };
    let _scope = cache.enable_probe();
    cache.get_or_tessellate(NodeId(1), 7, triangle_ink);
    cache.get_or_tessellate(NodeId(2), 7, || InkMesh {
        vertices: vec![
            vector_ink::InkVertex {
                pos: [0.0; 2],
                alpha: 1.0
            };
            20
        ],
        ..Default::default()
    });
    let row = cache.probe_snapshot().unwrap();
    assert_eq!(
        (row.cache.oversized_rejections, row.cache.oversized_bytes),
        (1, 240)
    );
    assert_eq!(
        (
            row.cache.evictions,
            row.resident_entries,
            row.resident_bytes
        ),
        (0, 1, 48)
    );
}

#[test]
fn warm_mesh_conversion_is_measured_separately_from_geometry_builds() {
    let mut cache = PathMeshCache::default();
    let _scope = cache.enable_probe();
    cache.get_or_tessellate(NodeId(1), 7, triangle_ink);
    cache.begin_probe_frame();
    let cached = cache.get_or_tessellate(NodeId(1), 7, || panic!("warm geometry rebuilt"));
    let xf = BoardXf {
        center: Pos2::ZERO,
        offset: Vec2::ZERO,
        z: 1.0,
    };
    let mesh = ink_mesh_to_epaint(&cached, &xf, Color32::WHITE, |c| c);
    let row = cache.probe_snapshot().unwrap();
    assert_eq!(
        (row.cache.hits, row.cache.builds, row.cache.key_changes),
        (1, 0, 0)
    );
    assert_eq!(
        (
            row.conversion.calls,
            row.conversion.vertices,
            row.conversion.indices
        ),
        (1, 3, 3)
    );
    assert_eq!(
        row.conversion.output_bytes as usize,
        mesh.vertices.len() * std::mem::size_of::<egui::epaint::Vertex>() + mesh.indices.len() * 4
    );
}

#[test]
fn probe_metadata_stays_bounded_after_more_requests_than_cache_entries() {
    let mut cache = PathMeshCache::default();
    let _scope = cache.enable_probe();
    for id in 0..(CACHE_ENTRIES + 3) {
        cache.get_or_tessellate(NodeId(id as u64), 7, InkMesh::default);
    }
    let row = cache.probe_snapshot().unwrap();
    assert_eq!(
        (row.tracked_identities, row.tracked_requests),
        (CACHE_ENTRIES, CACHE_ENTRIES)
    );
    assert!(row.cache.identity_tracking_saturated && row.cache.request_tracking_saturated);
    assert!(row.resident_entries <= CACHE_ENTRIES && row.resident_bytes <= CACHE_BYTES);
}
