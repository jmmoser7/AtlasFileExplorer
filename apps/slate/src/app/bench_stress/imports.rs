//! Bounded real model import measurements, separate from headless frame cost.

use super::*;

#[test]
#[ignore]
fn bench_resident_model_imports() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/slate-perf-audit");
    let (_, mut paths) = synthetic_assets(&base.join("assets"));
    paths.extend(resident_downloads().1);
    let mut rows = Vec::new();
    for path in paths {
        if atlas_core::cloud::is_dehydrated(&path) {
            continue;
        }
        let start = Instant::now();
        let bytes = std::fs::read(&path).unwrap();
        let read_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let loaded = model_preview::load_preview_with_reader(&path, &bytes, &mut |p| {
            if atlas_core::cloud::is_dehydrated(p) {
                return Err(std::io::Error::other("dehydrated dependency refused"));
            }
            std::fs::read(p)
        });
        let parse_ms = start.elapsed().as_secs_f64() * 1000.0;
        let (meshes, triangles, error) = match loaded {
            Ok(scene) => (
                scene.meshes.len(),
                scene
                    .meshes
                    .iter()
                    .map(|m| m.indices.len() / 3)
                    .sum::<usize>(),
                None,
            ),
            Err(e) => (0, 0, Some(e.to_string())),
        };
        println!("model_import {} bytes={} read={read_ms:.2} parse={parse_ms:.2}ms meshes={meshes} triangles={triangles} error={error:?}", path.display(), bytes.len());
        rows.push(serde_json::json!({ "path": path, "bytes": bytes.len(), "read_ms": read_ms, "parse_ms": parse_ms, "meshes": meshes, "triangles": triangles, "error": error }));
        std::fs::write(
            base.join("model-imports.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}
