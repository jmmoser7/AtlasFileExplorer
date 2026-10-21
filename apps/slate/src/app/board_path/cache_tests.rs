//! Regression tests for the existing bounded path mesh cache.
use super::*;
use vector_ink::stroke_mesh;

#[test]
fn path_mesh_cache_evicts_oldest_instead_of_clearing() {
    let mut cache = PathMeshCache {
        entry_limit: 256,
        ..Default::default()
    };
    let empty = InkMesh::default;
    for i in 0..300u64 {
        cache.get_or_tessellate(NodeId(i), i, empty);
    }
    let n = cache.stroke_len();
    assert_eq!(n, 256, "empty meshes must still obey the metadata bound");
    assert!(
        n > 0,
        "nuclear clear would leave the cache empty after a burst"
    );
    assert!(!cache.map.contains_key(&(NodeId(0), 0, false)));
}

pub(super) fn triangle_ink() -> InkMesh {
    InkMesh {
        vertices: [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]]
            .into_iter()
            .map(|pos| vector_ink::InkVertex { pos, alpha: 1.0 })
            .collect(),
        indices: vec![0, 1, 2],
        colors: Vec::new(),
    }
}

fn triangle_fill() -> FillTriangles {
    (vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]], vec![0, 1, 2])
}

#[test]
fn ten_thousand_visible_paths_stay_warm_in_paint_order() {
    let mut cache = PathMeshCache::default();
    for id in 0..10_000 {
        cache.get_or_fill_tris(NodeId(id), 0, triangle_fill);
        cache.get_or_tessellate(NodeId(id), 0, triangle_ink);
    }
    assert_eq!(cache.tess_misses, 20_000);
    cache.tess_misses = 0;
    for _ in 0..3 {
        for id in 0..10_000 {
            cache.get_or_fill_tris(NodeId(id), 0, || panic!("warm fill rebuilt"));
            cache.get_or_tessellate(NodeId(id), 0, || panic!("warm stroke rebuilt"));
        }
    }
    assert_eq!(cache.tess_misses, 0);
    assert!(cache.resident_bytes <= CACHE_BYTES);
    assert!(cache.map.len() <= CACHE_ENTRIES);
    assert_eq!(cache.clock.len(), cache.map.len());
}

#[test]
fn cache_hits_share_geometry_without_copying_vectors() {
    let mut cache = PathMeshCache::default();
    let stroke = cache.get_or_tessellate(NodeId(1), 7, triangle_ink);
    let hit = cache.get_or_tessellate(NodeId(1), 7, || panic!("cache miss"));
    assert!(Shared::ptr_eq(&stroke, &hit));
    let fill = cache.get_or_fill_tris(NodeId(1), 7, triangle_fill);
    let hit = cache.get_or_fill_tris(NodeId(1), 7, || panic!("cache miss"));
    assert!(Shared::ptr_eq(&fill, &hit));
}

#[test]
fn stroke_and_fill_share_a_byte_budget_and_hits_get_a_second_chance() {
    let mut cache = PathMeshCache {
        budget_bytes: 84, // One 48-byte stroke and one 36-byte fill.
        ..Default::default()
    };
    cache.get_or_tessellate(NodeId(1), 0, triangle_ink);
    cache.get_or_fill_tris(NodeId(2), 0, triangle_fill);
    assert_eq!(cache.resident_bytes, 84);
    cache.get_or_tessellate(NodeId(1), 0, || panic!("cache miss"));
    cache.get_or_fill_tris(NodeId(3), 0, triangle_fill);
    assert!(cache.map.contains_key(&(NodeId(1), 0, false)));
    assert!(!cache.map.contains_key(&(NodeId(2), 0, true)));
    assert_eq!(cache.resident_bytes, 84);
    for id in 4..100 {
        cache.get_or_fill_tris(NodeId(id), 0, triangle_fill);
        assert!(cache.resident_bytes <= cache.budget_bytes);
        assert_eq!(cache.clock.len(), cache.map.len());
    }
}

#[test]
fn oversized_geometry_renders_without_flushing_the_cache() {
    let mut cache = PathMeshCache {
        budget_bytes: 48,
        ..Default::default()
    };
    let resident = cache.get_or_tessellate(NodeId(1), 0, triangle_ink);
    let large = cache.get_or_tessellate(NodeId(2), 0, || InkMesh {
        vertices: vec![
            vector_ink::InkVertex {
                pos: [1.0, 2.0],
                alpha: 1.0
            };
            20
        ],
        indices: vec![],
        colors: Vec::new(),
    });
    assert_eq!(large.vertices.len(), 20);
    assert_eq!(cache.resident_bytes, 48);
    assert_eq!(cache.map.len(), 1);
    let hit = cache.get_or_tessellate(NodeId(1), 0, || panic!("resident flushed"));
    assert!(Shared::ptr_eq(&resident, &hit));
}

#[test]
fn rotating_a_warm_path_rebuilds_the_stroke_geometry() {
    let path = PathData {
        start: [0.0, 0.0],
        segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
        ..Default::default()
    };
    let stroke = default_curve_stroke(Rgba::BLACK);
    let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
    let mut cache = PathMeshCache::default();
    let mut paint = |rotation| {
        let key = path_content_hash(
            &path,
            &stroke,
            rect,
            rotation,
            slate_doc::scene::Corner::Square,
            zoom_bucket(1.0),
        );
        cache.get_or_tessellate(NodeId(1), key, || {
            let bez = path_data_to_world_bez(&path, rect, rotation);
            stroke_mesh(&bez, &stroke_style_world(&stroke, 1.0), FEATHER_PX, 0.25)
        })
    };
    let initial = paint(0.0);
    let rotated = paint(90.0);
    let restored = paint(0.0);
    assert_ne!(initial.vertices, rotated.vertices);
    assert!(Shared::ptr_eq(&initial, &restored));
    assert_eq!(cache.tess_misses, 2);
}
