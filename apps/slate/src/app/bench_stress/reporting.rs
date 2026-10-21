//! Shared stress sample reporting and retained JSON output.
use super::*;

pub(super) fn report(
    rows: &mut Vec<ResultRow>,
    nodes: usize,
    name: &str,
    samples: Vec<Sample>,
    mutation_ms: f64,
) {
    let mut times: Vec<_> = samples.iter().map(|s| s.cpu_ms).collect();
    times.sort_by(f64::total_cmp);
    let q = |p: f64| times[((times.len() - 1) as f64 * p).ceil() as usize];
    println!("mixed nodes={nodes} {name}: n={} p50={:.3} p95={:.3} p99={:.3} max={:.3} ms, >16.67ms={} >33.33ms={} mutation={mutation_ms:.3}ms",
        times.len(), q(0.50), q(0.95), q(0.99), q(1.0),
        times.iter().filter(|t| **t > 16.667).count(), times.iter().filter(|t| **t > 33.333).count());
    rows.push(ResultRow {
        nodes,
        scenario: name.into(),
        mutation_ms,
        samples,
    });
}

pub(super) fn label() -> String {
    let label = std::env::var("SLATE_STRESS_LABEL").unwrap_or_else(|_| "latest".into());
    assert!(label
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')));
    label
}

pub(super) fn persist(base: &Path, rows: &[ResultRow], images: &[PathBuf], models: &[PathBuf]) {
    let label = label();
    let report = serde_json::json!({
        "profile": if cfg!(debug_assertions) { "dev" } else { "release" },
        "screen": [1440, 900], "pixels_per_point": 1,
        "mix_percent": { "image": 40, "model": 10, "brush": 25, "pen": 15, "frame": 5, "text": 5 },
        "images": images, "models": models,
        "caveat": "CPU update + egui tessellation; no GPU upload/submission, present/vsync, or live 3D. Process-cold is not OS-cache-cold. Camera paths are deterministic direct state changes; pointer and selection use real egui events. Warmup is bounded, asynchronous work may remain.",
        "rows": rows,
    });
    std::fs::write(
        base.join(format!("mixed-{label}.json")),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
