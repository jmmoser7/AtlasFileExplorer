//! Exact native-stall pen subset through the shared headless frame owner.
//! This probe changes no cache policy and is excluded from ordinary tests.

use super::*;

#[derive(serde::Deserialize)]
struct Fixture {
    source: String,
    source_sha256: String,
    source_nodes: usize,
    camera: [f32; 3],
    nodes: Vec<slate_doc::Node>,
}

fn install(h: &mut Harness, fixture: &Fixture) {
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.add_nodes(fixture.nodes.clone());
    let cam = &mut h.app.tab_mut().cam;
    cam.offset = Vec2::new(fixture.camera[0], fixture.camera[1]);
    cam.z = fixture.camera[2];
}

fn phase(h: &mut Harness, rows: &mut Vec<ResultRow>, name: &str, frames: usize) {
    let samples = (0..frames)
        .map(|_| frame(h, Vec::new()))
        .collect::<Vec<_>>();
    let last = samples.last().unwrap().path_mesh.as_ref().unwrap();
    let key_changes: u64 = samples
        .iter()
        .map(|s| s.path_mesh.as_ref().unwrap().cache.key_changes)
        .sum();
    println!("cache_probe {name}: last_requests={} last_hits={} last_builds={} last_unique_bytes={} resident_bytes={} entries={} last_evictions={} last_oversized={} phase_key_changes={} last_conversion_calls={} last_conversion_ns={}",
        last.cache.requests, last.cache.hits, last.cache.builds, last.cache.unique_requested_bytes,
        last.resident_bytes, last.resident_entries, last.cache.evictions, last.cache.oversized_rejections,
        key_changes, last.conversion.calls, last.conversion.cpu_ns);
    if matches!(name, "stationary" | "pan" | "zoom_same_bucket") {
        assert!(
            last.cache.hits > 0,
            "{name}: the resident subset must stay warm"
        );
        assert!(
            last.cache.builds < last.cache.requests,
            "{name}: stable geometry must not all rebuild"
        );
        assert!(
            last.cache.admission_bypasses > 0,
            "{name}: the over-budget fixture must exercise protected retention"
        );
        assert_eq!(
            key_changes, 0,
            "{name}: camera movement must preserve geometry keys"
        );
    }
    report(rows, h.app.doc().scene.nodes.len(), name, samples, 0.0);
}

/// `cargo test --locked -p slate --lib bench_native_pen_cache_probe -- --ignored --nocapture --test-threads=1`
/// Uses 750 exact authored pen nodes from the retained 5,000-node native fixture.
/// SLATE_CACHE_PROBE_FRAMES=2..12 controls each short phase; default is three.
#[test]
#[ignore = "bounded native-pen mesh-cache diagnosis; CPU only, no GPU"]
fn bench_native_pen_cache_probe() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("fixtures/native-pen-750.json")).unwrap();
    assert_eq!(fixture.nodes.len(), 750);
    assert!(fixture.nodes.iter().all(|node| matches!(&node.kind,
        NodeKind::Shape(shape) if shape.shape == ShapeKind::Path && !shape.stroke.paints_as_stamp())));
    let frames = std::env::var("SLATE_CACHE_PROBE_FRAMES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3usize);
    assert!((2..=12).contains(&frames));
    let mut h = Harness::new("native_pen_cache_probe");
    install(&mut h, &fixture);
    let _scope = h.app.path_mesh_cache.enable_probe();
    let mut rows = Vec::new();
    phase(&mut h, &mut rows, "cold", 1);
    assert_eq!(
        rows[0].samples[0]
            .path_mesh
            .as_ref()
            .unwrap()
            .cache
            .requests,
        750,
        "the cold overview must actually request all750nativepengeometries"
    );
    phase(&mut h, &mut rows, "stationary", frames);
    let original = h.app.tab().cam;
    h.app.tab_mut().cam.offset += Vec2::new(120.0, -80.0);
    phase(&mut h, &mut rows, "pan", frames);
    h.app.tab_mut().cam = original;
    h.app.tab_mut().cam.z *= 1.1;
    assert_eq!(
        super::super::board_path::zoom_bucket(original.z),
        super::super::board_path::zoom_bucket(h.app.tab().cam.z)
    );
    phase(&mut h, &mut rows, "zoom_same_bucket", frames);
    h.app.tab_mut().cam.z = (super::super::board_path::zoom_bucket(original.z) + 1) as f32 / 8.0;
    phase(&mut h, &mut rows, "zoom_changed_bucket", frames);
    h.app.tab_mut().cam = original;
    phase(&mut h, &mut rows, "restored_stationary", frames);
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/slate-perf-audit");
    std::fs::create_dir_all(&base).unwrap();
    let output = base.join(format!("path-cache-probe-{}.json", reporting::label()));
    let report = serde_json::json!({
        "profile": if cfg!(debug_assertions) { "dev" } else { "release" },
        "fixture_source": fixture.source, "fixture_sha256": fixture.source_sha256,
        "original_mixed_nodes": fixture.source_nodes, "exact_pen_subset_nodes": fixture.nodes.len(),
        "camera": fixture.camera, "screen": [1440,900], "frames_per_phase": frames,
        "caveat": "Exact authored pen subset, including original NodeIds, placement, paths, styles and camera. Other mixed-board content is omitted to isolate the existing mesh cache. Shared bench_stress frame includes CPU update and egui tessellation; no GPU submission, native presentation or 3D. Diagnostic Instant/counter/map overhead is included. requested_bytes counts all lookups; unique_requested_bytes counts each content key once per frame, bounded to32768keys. build_cpu_ns includes tessellation and packing into cached arrays; conversion is only ink_mesh_to_epaint, not direct fill conversion. Key identity tracking is bounded to32768(node,fill) pairs; saturation flags are explicit. The probe uses the compiled retention policy and leaves its cache budget unchanged.",
        "rows": rows,
    });
    std::fs::write(&output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("cache_probe report={}", output.display());
    h.app.path_mesh_cache.disable_probe();
}
