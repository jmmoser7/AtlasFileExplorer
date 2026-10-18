use super::{capture, now_unix_ms};
use serde::Serialize;
use std::backtrace::Backtrace;
use std::fmt::Write as FmtWrite;
use std::fs::{self, OpenOptions};
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const PANICS_PER_RUN: u64 = 32;
const BACKTRACE_LIMIT: usize = 48 * 1024;

#[derive(Serialize)]
struct PanicReport {
    schema_version: u32,
    unix_ms: u64,
    pid: u32,
    thread: String,
    message: String,
    location: Option<String>,
    backtrace: String,
}

/// Capture Rust panics from UI and worker threads, including caught panics.
///
/// Installs once during app startup, before any workers. No app/session locks
/// are taken. File writes are independent per panic; the original hook still
/// runs. Allocation failures and native exceptions need the external monitor.
pub fn install_panic_hook(dir: PathBuf) {
    let previous = std::panic::take_hook();
    let sequence = AtomicU64::new(0);
    std::panic::set_hook(Box::new(move |info| {
        let number = sequence.fetch_add(1, Ordering::Relaxed);
        // Bound disk use even if a worker repeatedly catches a failing task.
        if number < PANICS_PER_RUN {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            let thread = std::thread::current();
            let mut backtrace = BoundedString::new(BACKTRACE_LIMIT);
            let _ = write!(&mut backtrace, "{}", Backtrace::force_capture());
            let report = PanicReport {
                schema_version: 1,
                unix_ms: now_unix_ms(),
                pid: std::process::id(),
                thread: capture::clip(thread.name().unwrap_or("unnamed"), 256),
                message: capture::clip(message, 8 * 1024),
                location: info.location().map(|location| {
                    capture::clip(
                        &format!(
                            "{}:{}:{}",
                            location.file(),
                            location.line(),
                            location.column()
                        ),
                        2048,
                    )
                }),
                backtrace: backtrace.value,
            };
            let path = dir.join(format!(
                "panic-{}-{}-{number}.json",
                report.pid, report.unix_ms
            ));
            // create_new protects concurrent threads and even an accidental
            // second install from overwriting evidence already on disk.
            if let (Ok(mut file), Ok(bytes)) = (
                OpenOptions::new().write(true).create_new(true).open(path),
                serde_json::to_vec_pretty(&report),
            ) {
                let _ = file.write_all(&bytes);
                let _ = file.sync_all();
                let _ = fs::write(dir.join("unread"), b"");
            }
        }
        previous(info);
    }));
}

struct BoundedString {
    value: String,
    limit: usize,
}

impl BoundedString {
    fn new(limit: usize) -> Self {
        Self {
            value: String::new(),
            limit,
        }
    }
}

impl std::fmt::Write for BoundedString {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let remaining = self.limit.saturating_sub(self.value.len());
        let mut end = value.len().min(remaining);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.value.push_str(&value[..end]);
        Ok(())
    }
}
