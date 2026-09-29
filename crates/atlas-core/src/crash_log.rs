//! Process-death record for File Atlas and Slate.
//!
//! Installed once from `main`. A Rust panic, or on Windows an unhandled
//! native exception, appends one block to
//! `data_dir()/session-log/<app>-crash.log` and flushes it before the
//! process dies. The session activity log still holds the last stalls;
//! this file holds why the process stopped.
//!
//! The Windows filter returns "continue search" so Windows Error Reporting
//! still sees the crash. A stack overflow writes the exception line only —
//! walking the stack from the guard page can fault again.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Rotate the live crash file once it passes this size. Done at install,
/// not in the handler.
const CRASH_ROTATE: u64 = 1_000_000;
/// How much of `<app>-latest.json` to copy into the crash block.
const LATEST_EXCERPT: usize = 2_000;

static APP: OnceLock<&'static str> = OnceLock::new();
static REPORTING: AtomicBool = AtomicBool::new(false);

pub(crate) fn install(app: &'static str) {
    if crate::session_log::env_off() {
        return;
    }
    let first = APP.set(app).is_ok();
    if first {
        let path = path_for(app);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        rotate_if_huge(&path);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            write_panic(info);
            previous(info);
        }));
    }
    #[cfg(windows)]
    install_seh();
}

/// Put the Windows unhandled-exception filter back. Libraries loaded after
/// startup (WebView2 in particular) replace it; the frame loop calls this
/// so a later native crash still reaches the crash file. At most once a
/// second, and a no-op until [`install`] has run.
pub(crate) fn reassert_seh() {
    #[cfg(windows)]
    {
        use std::time::{Duration, Instant};
        if APP.get().is_none() {
            return;
        }
        thread_local! {
            static NEXT: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
        }
        let due = NEXT.with(|next| {
            let now = Instant::now();
            match next.get() {
                Some(due) if now < due => false,
                _ => {
                    next.set(Some(now + Duration::from_secs(1)));
                    true
                }
            }
        });
        if due {
            install_seh();
        }
    }
}

fn path_for(app: &str) -> PathBuf {
    crate::index::data_dir()
        .join("session-log")
        .join(format!("{app}-crash.log"))
}

fn write_panic(info: &std::panic::PanicHookInfo<'_>) {
    if REPORTING.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed");
        let location = info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let message = info.payload_as_str().unwrap_or("non-string panic payload");
        let backtrace = std::backtrace::Backtrace::force_capture();
        let body = format!(
            "kind: panic\nthread: {thread_name}\nlocation: {location}\nmessage: {message}\n{}{}\nbacktrace:\n{backtrace}\n",
            exe_line(),
            context_lines(true),
        );
        let _ = append_block(&body);
    }));
}

fn exe_line() -> String {
    match std::env::current_exe() {
        Ok(path) => format!("exe: {}\n", path.display()),
        Err(_) => String::new(),
    }
}

fn context_lines(include_latest: bool) -> String {
    let Some(app) = APP.get().copied() else {
        return String::new();
    };
    let dir = crate::index::data_dir().join("session-log");
    let mut text = format!(
        "session_log: {}\nsession_latest: {}\n",
        dir.join(format!("{app}.jsonl")).display(),
        dir.join(format!("{app}-latest.json")).display(),
    );
    if include_latest {
        let latest = dir.join(format!("{app}-latest.json"));
        if let Ok(snapshot) = std::fs::read_to_string(&latest) {
            let excerpt = snapshot.chars().take(LATEST_EXCERPT).collect::<String>();
            text.push_str("latest:\n");
            text.push_str(&excerpt);
            if snapshot.chars().count() > LATEST_EXCERPT {
                text.push_str("\n…");
            }
            text.push('\n');
        }
    }
    text
}

fn append_block(body: &str) -> std::io::Result<()> {
    let Some(app) = APP.get().copied() else {
        return Ok(());
    };
    append_crash_block(&path_for(app), app, body)
}

pub(crate) fn append_crash_block(path: &Path, app: &str, body: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let header = format!(
        "===== {} pid={} app={} =====\n",
        format_utc(crate::session_log::now_unix_ms()),
        std::process::id(),
        app
    );
    file.write_all(header.as_bytes())?;
    file.write_all(body.as_bytes())?;
    if !body.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

fn rotate_if_huge(path: &Path) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() < CRASH_ROTATE {
        return;
    }
    let prev = path.with_extension("prev.log");
    let _ = std::fs::remove_file(&prev);
    let _ = std::fs::rename(path, prev);
}

/// `unix_ms` since the Unix epoch, formatted as UTC.
pub(crate) fn format_utc(unix_ms: u64) -> String {
    let secs = (unix_ms / 1000) as i64;
    let ms = (unix_ms % 1000) as u32;
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400) as u32;
    let (year, month, day) = civil_from_days(days);
    let hour = sod / 3600;
    let minute = (sod % 3600) / 60;
    let second = sod % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{ms:03}Z")
}

/// Howard Hinnant's `civil_from_days`. `days` is days since 1970-01-01.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year as i32, month as u32, day as u32)
}

#[cfg(windows)]
fn install_seh() {
    unsafe {
        let previous = SetUnhandledExceptionFilter(Some(seh_filter));
        if let Some(previous) = previous {
            if previous as usize != seh_filter as *const () as usize {
                PREVIOUS_FILTER.store(previous as *mut (), Ordering::Relaxed);
            }
        }
    }
}

#[cfg(windows)]
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
#[cfg(windows)]
const EXCEPTION_STACK_OVERFLOW: u32 = 0xC000_00FD;

#[cfg(windows)]
#[repr(C)]
#[allow(dead_code)]
struct ExceptionRecord {
    code: u32,
    flags: u32,
    record: *mut ExceptionRecord,
    address: *mut std::ffi::c_void,
    number_parameters: u32,
    information: [usize; 15],
}

#[cfg(windows)]
#[repr(C)]
#[allow(dead_code)]
struct ExceptionPointers {
    exception_record: *mut ExceptionRecord,
    context_record: *mut std::ffi::c_void,
}

#[cfg(windows)]
type ExceptionFilter = unsafe extern "system" fn(*mut ExceptionPointers) -> i32;

/// Filter installed by someone else (WebView, the CRT). Invoked after we
/// write, so their handler and Windows Error Reporting still run.
#[cfg(windows)]
static PREVIOUS_FILTER: std::sync::atomic::AtomicPtr<()> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetUnhandledExceptionFilter(filter: Option<ExceptionFilter>) -> Option<ExceptionFilter>;
}

#[cfg(windows)]
unsafe extern "system" fn seh_filter(info: *mut ExceptionPointers) -> i32 {
    if !REPORTING.swap(true, Ordering::SeqCst) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            write_exception(info);
        }));
    }
    let previous = PREVIOUS_FILTER.load(Ordering::Relaxed);
    if !previous.is_null() {
        let previous: ExceptionFilter = unsafe { std::mem::transmute(previous) };
        return unsafe { previous(info) };
    }
    EXCEPTION_CONTINUE_SEARCH
}

#[cfg(windows)]
fn write_exception(info: *mut ExceptionPointers) {
    let (code, address) = unsafe {
        if info.is_null() || (*info).exception_record.is_null() {
            (0, 0usize)
        } else {
            let record = &*(*info).exception_record;
            (record.code, record.address as usize)
        }
    };
    let name = exception_name(code);
    let stack = code != EXCEPTION_STACK_OVERFLOW;
    let backtrace = if stack {
        format!(
            "\nbacktrace:\n{}\n",
            std::backtrace::Backtrace::force_capture()
        )
    } else {
        String::new()
    };
    let body = format!(
        "kind: exception\nname: {name}\ncode: 0x{code:08X}\naddress: 0x{address:X}\n{}{}{backtrace}",
        exe_line(),
        context_lines(stack),
    );
    let _ = append_block(&body);
}

#[cfg(windows)]
fn exception_name(code: u32) -> &'static str {
    match code {
        0xC000_0005 => "ACCESS_VIOLATION",
        0xC000_0374 => "HEAP_CORRUPTION",
        0xC000_00FD => "STACK_OVERFLOW",
        0xC000_0409 => "STACK_BUFFER_OVERRUN",
        0xC000_001D => "ILLEGAL_INSTRUCTION",
        0xC000_0094 => "INTEGER_DIVIDE_BY_ZERO",
        0xC000_0096 => "PRIVILEGED_INSTRUCTION",
        0x8000_0003 => "BREAKPOINT",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_epoch_and_the_next_day() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_utc(86_400_000), "1970-01-02T00:00:00.000Z");
        assert_eq!(format_utc(1_000), "1970-01-01T00:00:01.000Z");
        assert_eq!(format_utc(1_700_000_000_000), "2023-11-14T22:13:20.000Z");
    }

    #[test]
    fn crash_blocks_append() {
        let dir = std::env::temp_dir().join(format!(
            "atlas_crash_log_{}_{}",
            std::process::id(),
            crate::session_log::now_unix_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("slate-crash.log");
        append_crash_block(&path, "slate", "kind: panic\nmessage: first\n").unwrap();
        append_crash_block(&path, "slate", "kind: panic\nmessage: second\n").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("app=slate"));
        assert!(text.contains("message: first"));
        assert!(text.contains("message: second"));
        let first = text.find("message: first").unwrap();
        let second = text.find("message: second").unwrap();
        assert!(first < second);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_huge_crash_file_rotates_aside() {
        let dir = std::env::temp_dir().join(format!(
            "atlas_crash_rotate_{}_{}",
            std::process::id(),
            crate::session_log::now_unix_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("slate-crash.log");
        std::fs::write(&path, vec![b'x'; (CRASH_ROTATE as usize) + 1]).unwrap();
        rotate_if_huge(&path);
        assert!(!path.exists());
        assert!(dir.join("slate-crash.prev.log").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
