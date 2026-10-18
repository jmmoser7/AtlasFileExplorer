use super::SESSION_LIMIT;
use serde::Serialize;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub(super) fn clip(value: &str, limit: usize) -> String {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

pub(super) fn write_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let mut file = File::create(path)?;
    use std::io::Write;
    file.write_all(&bytes)?;
    file.sync_all()
}

/// Read at most limit bytes even if a diagnostic file grew arbitrarily large.
pub(super) fn read_tail(path: &Path, limit: usize) -> io::Result<String> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(limit as u64)))?;
    let mut bytes = Vec::with_capacity(limit.min(len as usize));
    file.take(limit as u64).read_to_end(&mut bytes)?;
    Ok(clip(&String::from_utf8_lossy(&bytes), limit))
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let file = File::open(path).ok()?;
    // A partial/truncated JSON is deliberately ignored rather than risking an
    // unbounded read of a corrupt diagnostic file.
    serde_json::from_reader(file.take(SESSION_LIMIT as u64)).ok()
}

pub(super) fn session_evidence(
    session_dir: &Path,
    app: &str,
    pid: Option<u32>,
    started_unix_ms: u64,
) -> (Option<serde_json::Value>, String) {
    let Some(pid) = pid else {
        return (None, String::new());
    };
    let latest = read_json(&session_dir.join(format!("{app}-latest.json"))).filter(|value| {
        value["pid"].as_u64() == Some(pid as u64)
            && value["app"].as_str() == Some(app)
            && value["started_unix"]
                .as_u64()
                .is_some_and(|time| time >= started_unix_ms / 1000)
    });
    if latest.is_none() {
        return (None, String::new());
    }
    // The jsonl is shared per app. Keep only records belonging to this launch;
    // legacy records without a PID cannot be attributed safely and are omitted.
    let tail =
        read_tail(&session_dir.join(format!("{app}.jsonl")), SESSION_LIMIT).unwrap_or_default();
    let filtered = tail
        .lines()
        .filter(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .is_some_and(|value| {
                    value["pid"].as_u64() == Some(pid as u64)
                        && value["app"].as_str() == Some(app)
                        && value["t"]
                            .as_u64()
                            .is_some_and(|time| time >= started_unix_ms)
                })
        })
        .collect::<Vec<_>>()
        .join("\n");
    (latest, filtered)
}

pub(super) fn read_run(path: &Path) -> Option<super::RunMetadata> {
    let file = File::open(path).ok()?;
    serde_json::from_reader(file.take(4096)).ok()
}

#[derive(serde::Deserialize)]
pub(super) struct Completion {
    pub(super) exit_code: Option<i32>,
    pub(super) ended_unix_ms: Option<u64>,
}

pub(super) fn read_completion(path: &Path) -> Option<Completion> {
    let file = File::open(path).ok()?;
    serde_json::from_reader(file.take(2 * 1024 * 1024)).ok()
}

pub(super) fn is_real_dir(entry: &fs::DirEntry) -> bool {
    // Never traverse symlinks/junction-like entries during cleanup.
    entry
        .file_type()
        .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
}
