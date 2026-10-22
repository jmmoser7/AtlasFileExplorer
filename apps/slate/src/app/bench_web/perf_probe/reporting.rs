//! Diagnostic evidence persistence; no writes during measured CPU intervals.

use super::*;

pub(super) fn percentile(samples: &[Sample], value: impl Fn(&Sample) -> f64) -> Value {
    let mut values: Vec<_> = samples.iter().map(value).collect();
    values.sort_by(f64::total_cmp);
    let at = |q: f64| values[((q * values.len() as f64).ceil() as usize).saturating_sub(1)];
    json!({"count":values.len(),"p50":at(0.5),"p95":at(0.95),"p99":at(0.99),
        "max":values.last(),"over_16_67_ms":values.iter().filter(|v| **v>16.67).count()})
}

pub(super) fn checkpoint(
    clock: Instant,
    path: &Path,
    name: &str,
    warmup: &[Sample],
    samples: &[Sample],
) {
    if clock.elapsed() < DEADLINE {
        return;
    }
    let mut report: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    report["status"] = json!("incomplete_deadline");
    report["elapsed_ms"] = json!(clock.elapsed().as_secs_f64() * 1000.0);
    report["in_progress"] = json!({"scenario":name,"warmup":warmup,"samples":samples});
    std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    panic!("web CPU diagnostic crossed cooperative 110 second deadline; partial evidence retained");
}

fn retained_report(path: &Path) -> Option<Value> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn save_failure(path: &Path, report: &Value) {
    // Best effort on the failure path: never replace the original panic.
    let _ = serde_json::to_vec_pretty(report).map(|bytes| std::fs::write(path, bytes));
}

pub(super) fn scenario_failure(path: &Path, name: &str, warmup: &[Sample], samples: &[Sample]) {
    if let Some(mut report) = retained_report(path).filter(|r| r["status"] == "running") {
        report["status"] = json!("incomplete_failure");
        report["in_progress"] = json!({"scenario":name,"warmup":warmup,"samples":samples});
        save_failure(path, &report);
    }
}

pub(super) struct FailureReport(pub(super) PathBuf);

impl Drop for FailureReport {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            return;
        }
        if let Some(mut report) = retained_report(&self.0).filter(|r| r["status"] == "running") {
            report["status"] = json!("incomplete_failure");
            save_failure(&self.0, &report);
        }
    }
}

pub(super) fn artifact(label: &str) -> PathBuf {
    assert!(
        !label.is_empty()
            && label.len() <= 48
            && label
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "restricted diagnostic label required"
    );
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/slate-web-perf-audit")
        .join(format!("web-large-canvas-{label}.json"))
}
