//! Cooperative write lease over a workbook file.
//!
//! The readable .slate.lock marker names the holder. Updated clients also
//! retain an OS lock on .slate.lock.guard: a live owner is never stolen
//! merely because its tab is inactive, its UI is idle, or clocks disagree.
//! The zero-byte guard is persistent; deleting it would split the lock.
//!
//! One maintenance worker refreshes every owned marker and releases dropped
//! leases off the UI thread. Heartbeats write the originally opened marker
//! handle after checking its ownership token, never a replacement marker.
//! Old clients which ignore the guard can still steal a stale marker; save
//! checks and heartbeats fail closed if they do. The clock-based timeout is
//! retained for crash recovery and compatibility with those old clients.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

mod maintenance;
#[cfg(test)]
mod tests;

/// A marker this old can be reclaimed only after its OS guard is unlocked.
pub const LEASE_STALE_SECS: i64 = 30;
/// Shortest interval between two heartbeat writes.
pub const LEASE_HEARTBEAT_SECS: i64 = 10;

/// Who holds a workbook, as recorded in its readable lock marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseInfo {
    pub holder: String,
    pub host: String,
    pub pid: u32,
    pub acquired_at: i64,
    pub heartbeat_at: i64,
    /// Distinguishes reacquisitions on one machine within the same second.
    /// Legacy markers have no token and remain readable.
    #[serde(default)]
    pub token: String,
}

impl LeaseInfo {
    pub fn unknown() -> LeaseInfo {
        LeaseInfo {
            holder: "unknown".to_string(),
            host: "unknown".to_string(),
            pid: 0,
            acquired_at: 0,
            heartbeat_at: 0,
            token: String::new(),
        }
    }

    fn for_this_process(now: i64) -> LeaseInfo {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        LeaseInfo {
            holder: env_name(&["USERNAME", "USER", "LOGNAME"]),
            host: env_name(&["COMPUTERNAME", "HOSTNAME"]),
            pid: std::process::id(),
            acquired_at: now,
            heartbeat_at: now,
            token: format!("{stamp:x}-{:x}", NEXT.fetch_add(1, Ordering::Relaxed)),
        }
    }

    pub fn describe(&self) -> String {
        format!("{} on {}", self.holder, self.host)
    }

    fn same_owner(&self, other: &Self) -> bool {
        self.token == other.token
            && self.pid == other.pid
            && self.host == other.host
            && self.acquired_at == other.acquired_at
    }
}

#[derive(Debug)]
pub enum LeaseState {
    Acquired(Lease),
    Held(LeaseInfo),
}

#[derive(Debug)]
struct Record {
    marker: File,
    // Keep the OS guard locked while owned; cleanup reacquires its authority.
    _guard: File,
    info: LeaseInfo,
}

#[derive(Debug)]
struct LeaseInner {
    lock_path: PathBuf,
    record: Mutex<Record>,
    failure: Mutex<Option<String>>,
    active: AtomicBool,
    // Declared last: completion follows destruction of the record's file handles.
    released: ReleaseSignal,
}

/// Nonblocking confirmation that a released lease's old handles are gone.
/// This does not grant ownership: reacquisition must still use [`Lease::acquire`].
#[derive(Clone, Debug)]
pub struct LeaseRelease(Arc<AtomicBool>);

impl LeaseRelease {
    pub fn is_complete(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Default)]
struct ReleaseSignal(Arc<AtomicBool>);

impl Drop for ReleaseSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

/// An owned lease. Maintenance is independent of any window or active tab.
/// Release/drop queues cleanup rather than reading a shared drive on the UI.
#[derive(Debug)]
pub struct Lease {
    inner: Option<Arc<LeaseInner>>,
    info: LeaseInfo,
    worker: mpsc::Sender<maintenance::Task>,
}

impl Lease {
    pub fn lock_path(doc_path: &Path) -> PathBuf {
        append_suffix(doc_path, ".lock")
    }

    pub fn acquire(doc_path: &Path) -> io::Result<LeaseState> {
        Self::acquire_at(doc_path, unix_now())
    }

    fn acquire_at(doc_path: &Path, now: i64) -> io::Result<LeaseState> {
        let lock_path = Self::lock_path(doc_path);
        let guard = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(append_suffix(&lock_path, ".guard"))?;
        match guard.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Ok(LeaseState::Held(
                    read_info(&lock_path).unwrap_or_else(LeaseInfo::unknown),
                ));
            }
            Err(TryLockError::Error(error)) => return Err(error),
        }
        let worker = maintenance::worker()?;
        let info = LeaseInfo::for_this_process(now);
        let marker = match claim_marker(&lock_path, &info, now)? {
            MarkerClaim::Owned(marker) => marker,
            MarkerClaim::Held(holder) => return Ok(LeaseState::Held(holder)),
        };
        let inner = Arc::new(LeaseInner {
            lock_path,
            record: Mutex::new(Record {
                marker,
                _guard: guard,
                info: info.clone(),
            }),
            failure: Mutex::new(None),
            active: AtomicBool::new(true),
            released: ReleaseSignal::default(),
        });
        worker
            .send(maintenance::Task::Register(Arc::downgrade(&inner)))
            .map_err(io::Error::other)?;
        Ok(LeaseState::Acquired(Self {
            inner: Some(inner),
            info,
            worker,
        }))
    }

    /// Synchronous verification for an explicit save, before any mutation.
    /// Normal frame pumps use failure() and perform no filesystem I/O.
    pub fn validate_ownership(&self) -> io::Result<()> {
        let inner = self.inner.as_ref().expect("unreleased lease");
        let record = inner
            .record
            .lock()
            .map_err(|_| io::Error::other("write lease state was poisoned"))?;
        let result = inner.check_owned(&record.info);
        inner.remember_failure(&result);
        result
    }

    /// An error reported by background maintenance; this never touches disk.
    pub fn failure(&self) -> Option<String> {
        self.inner.as_ref()?.failure.lock().ok()?.clone()
    }

    /// Explicit synchronous refresh, primarily useful to callers without a UI.
    /// App frame loops must not call this; maintenance already refreshes it.
    pub fn heartbeat(&mut self) -> io::Result<()> {
        self.heartbeat_at(unix_now())
    }

    fn heartbeat_at(&mut self, now: i64) -> io::Result<()> {
        let inner = self.inner.as_ref().expect("unreleased lease");
        let mut record = inner
            .record
            .lock()
            .map_err(|_| io::Error::other("write lease state was poisoned"))?;
        let result = inner.refresh(&mut record, now);
        inner.remember_failure(&result);
        self.info = record.info.clone();
        result
    }

    pub fn release(self) -> LeaseRelease {
        // Drop queues cleanup; the final inner field signals after its handles close.
        LeaseRelease(
            self.inner
                .as_ref()
                .expect("unreleased lease")
                .released
                .0
                .clone(),
        )
    }

    /// Drain already queued releases at process exit, with a bounded wait.
    /// Do not call this from frame, tab-close, or ordinary drop paths.
    pub fn flush_pending_releases(timeout: std::time::Duration) -> io::Result<()> {
        maintenance::flush(timeout)
    }

    pub fn info(&self) -> &LeaseInfo {
        &self.info
    }
}

impl LeaseInner {
    fn check_owned(&self, expected: &LeaseInfo) -> io::Result<()> {
        if !self.active.load(Ordering::Acquire) {
            return Err(io::Error::other("write lease was released"));
        }
        let failure = self
            .failure
            .lock()
            .map_err(|_| io::Error::other("write lease failure state was poisoned"))?
            .clone();
        if let Some(failure) = failure {
            return Err(io::Error::other(failure));
        }
        match read_info(&self.lock_path) {
            Some(actual) if expected.same_owner(&actual) => Ok(()),
            _ => Err(io::Error::other("write lease ownership was lost")),
        }
    }

    fn refresh(&self, record: &mut Record, now: i64) -> io::Result<()> {
        if now.saturating_sub(record.info.heartbeat_at) < LEASE_HEARTBEAT_SECS {
            return Ok(());
        }
        self.check_owned(&record.info)?;
        let mut next = record.info.clone();
        next.heartbeat_at = now;
        write_info(&mut record.marker, &next)?;
        record.info = next;
        Ok(())
    }

    fn remember_failure(&self, result: &io::Result<()>) {
        if let Err(error) = result {
            if let Ok(mut failure) = self.failure.lock() {
                *failure = Some(error.to_string());
            }
        }
    }

    fn remove_if_ours(&self) {
        let Ok(record) = self.record.lock() else {
            return;
        };
        // A disconnected SMB session can lose its server-side guard even
        // though this handle is still open. Reacquire exclusive authority
        // before checking/removing a pathname; the token check alone leaves
        // a takeover window between the read and unlink. Releasing our old
        // lock first also avoids platform-dependent recursive-lock behavior.
        let _ = record._guard.unlock();
        if record._guard.try_lock().is_err() {
            return;
        }
        if read_info(&self.lock_path).is_some_and(|actual| record.info.same_owner(&actual)) {
            let _ = fs::remove_file(&self.lock_path);
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            inner.active.store(false, Ordering::Release);
            // If the worker failed, drop the handles and leave the marker for
            // stale recovery. Never perform shared-drive cleanup on this thread.
            let _ = self.worker.send(maintenance::Task::Release(inner));
        }
    }
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

enum MarkerClaim {
    Owned(File),
    Held(LeaseInfo),
}

/// Called only while the acquisition's OS guard is held. Stale takeover
/// replaces the file object, so an old handle can never write its successor.
fn claim_marker(lock_path: &Path, info: &LeaseInfo, now: i64) -> io::Result<MarkerClaim> {
    match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(lock_path)
    {
        Ok(mut file) => {
            write_info(&mut file, info)?;
            return Ok(MarkerClaim::Owned(file));
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let existing = read_info(lock_path);
    let stale = match &existing {
        Some(info) => now.saturating_sub(info.heartbeat_at) > LEASE_STALE_SECS,
        None => lock_age_secs(lock_path, now).is_some_and(|age| age > LEASE_STALE_SECS),
    };
    if !stale {
        return Ok(MarkerClaim::Held(
            existing.unwrap_or_else(LeaseInfo::unknown),
        ));
    }
    let temporary = append_suffix(lock_path, &format!(".{}.tmp", info.token));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    if let Err(error) = write_info(&mut file, info).and_then(|()| fs::rename(&temporary, lock_path))
    {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(MarkerClaim::Owned(file))
}

fn encode(info: &LeaseInfo) -> io::Result<String> {
    serde_json::to_string_pretty(info).map_err(io::Error::other)
}

fn write_info(file: &mut File, info: &LeaseInfo) -> io::Result<()> {
    let bytes = encode(info)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(bytes.as_bytes())?;
    file.set_len(bytes.len() as u64)?;
    file.flush()
}

fn read_info(lock_path: &Path) -> Option<LeaseInfo> {
    serde_json::from_slice(&fs::read(lock_path).ok()?).ok()
}

fn lock_age_secs(lock_path: &Path, now: i64) -> Option<i64> {
    let modified = fs::metadata(lock_path).ok()?.modified().ok()?;
    let secs = modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some(now.saturating_sub(secs))
}

fn env_name(keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| std::env::var(key).ok().filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| "unknown".to_string())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
