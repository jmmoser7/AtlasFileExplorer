//! Cooperative write lease over a workbook file.
//!
//! A `.slate` workbook on a shared drive is opened by more than one person on
//! the same day. Without a lease the second writer's save silently discards
//! the first writer's work, so every open takes a lease and an open that
//! cannot take one becomes read-only.
//!
//! The lease is a lock file beside the workbook (`board.slate` →
//! `board.slate.lock`) holding [`LeaseInfo`] as JSON. It is deliberately
//! visible and manually deletable: a lock nobody can find is worse than no
//! lock at all.
//!
//! Acquisition is [`std::fs::OpenOptions::create_new`] — the one primitive
//! that is atomic across SMB, not a check followed by a write. The holder
//! refreshes `heartbeat_at` while it runs; a lease whose heartbeat is older
//! than [`LEASE_STALE_SECS`] belonged to a process that crashed and is stolen.
//!
//! Staleness compares the holder's clock to ours, so two machines whose clocks
//! disagree by more than [`LEASE_STALE_SECS`] can steal a live lease. Firm
//! machines share a domain time source; anything stronger needs a server.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// A lease whose heartbeat is older than this is treated as abandoned.
pub const LEASE_STALE_SECS: i64 = 30;
/// Shortest interval between two heartbeat writes.
pub const LEASE_HEARTBEAT_SECS: i64 = 10;

/// Suffix appended to the workbook's file name to form the lock file.
const LOCK_SUFFIX: &str = ".lock";

/// Who holds a workbook, as recorded in its lock file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseInfo {
    /// OS user name, best effort; `"unknown"` if unavailable.
    pub holder: String,
    /// Machine name, best effort; `"unknown"` if unavailable.
    pub host: String,
    pub pid: u32,
    /// Unix seconds when the lease was first taken.
    pub acquired_at: i64,
    /// Unix seconds of the most recent heartbeat.
    pub heartbeat_at: i64,
}

impl LeaseInfo {
    /// The holder of a lock file we could not read or parse. Fresh but
    /// anonymous locks are honoured rather than stolen, so this is what the
    /// caller shows instead of a name.
    pub fn unknown() -> LeaseInfo {
        LeaseInfo {
            holder: "unknown".to_string(),
            host: "unknown".to_string(),
            pid: 0,
            acquired_at: 0,
            heartbeat_at: 0,
        }
    }

    fn for_this_process(now: i64) -> LeaseInfo {
        LeaseInfo {
            holder: env_name(&["USERNAME", "USER", "LOGNAME"]),
            host: env_name(&["COMPUTERNAME", "HOSTNAME"]),
            pid: std::process::id(),
            acquired_at: now,
            heartbeat_at: now,
        }
    }

    /// `"jmoser on WS-014"` — the phrasing the app puts in front of the user.
    pub fn describe(&self) -> String {
        format!("{} on {}", self.holder, self.host)
    }
}

/// The outcome of [`Lease::acquire`].
#[derive(Debug)]
pub enum LeaseState {
    /// This process owns the lease.
    Acquired(Lease),
    /// Someone else holds a live lease; open read-only.
    Held(LeaseInfo),
}

/// An owned write lease. Released on [`Lease::release`] and, best effort, on
/// drop.
#[derive(Debug)]
pub struct Lease {
    lock_path: PathBuf,
    info: LeaseInfo,
}

impl Lease {
    /// The lock file for `doc_path`: the workbook's own name plus `.lock`, in
    /// the workbook's directory.
    pub fn lock_path(doc_path: &Path) -> PathBuf {
        let mut name = doc_path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        name.push(LOCK_SUFFIX);
        doc_path.with_file_name(name)
    }

    /// Attempts to take the lease for `doc_path`. A lease whose heartbeat is
    /// older than [`LEASE_STALE_SECS`] is stolen (the holder crashed).
    pub fn acquire(doc_path: &Path) -> io::Result<LeaseState> {
        let lock_path = Lease::lock_path(doc_path);
        if let Some(lease) = create_lock(&lock_path)? {
            return Ok(LeaseState::Acquired(lease));
        }
        let now = unix_now();
        let existing = read_info(&lock_path);
        let stale = match &existing {
            Some(info) => now.saturating_sub(info.heartbeat_at) > LEASE_STALE_SECS,
            // Unreadable or unparsable: fall back to the file's own age. An
            // age we cannot determine counts as fresh — never steal blind.
            None => lock_age_secs(&lock_path, now).is_some_and(|age| age > LEASE_STALE_SECS),
        };
        if !stale {
            return Ok(LeaseState::Held(
                existing.unwrap_or_else(LeaseInfo::unknown),
            ));
        }
        // One steal attempt, never a retry loop: if another process wins the
        // race between the remove and the create, we open read-only.
        let _ = fs::remove_file(&lock_path);
        match create_lock(&lock_path)? {
            Some(lease) => Ok(LeaseState::Acquired(lease)),
            None => Ok(LeaseState::Held(
                read_info(&lock_path).unwrap_or_else(LeaseInfo::unknown),
            )),
        }
    }

    /// Refreshes `heartbeat_at`; cheap enough to call every frame, writes at
    /// most once per [`LEASE_HEARTBEAT_SECS`].
    pub fn heartbeat(&mut self) -> io::Result<()> {
        self.heartbeat_at(unix_now())
    }

    /// Deletes the lock file. Also runs on `Drop`, best effort.
    pub fn release(self) {
        self.remove_if_ours();
    }

    /// Who this process recorded itself as when it took the lease.
    pub fn info(&self) -> &LeaseInfo {
        &self.info
    }

    fn heartbeat_at(&mut self, now: i64) -> io::Result<()> {
        if now.saturating_sub(self.info.heartbeat_at) < LEASE_HEARTBEAT_SECS {
            return Ok(());
        }
        let mut next = self.info.clone();
        next.heartbeat_at = now;
        fs::write(&self.lock_path, encode(&next)?)?;
        self.info = next;
        Ok(())
    }

    /// Removes the lock file unless another process has since taken it — a
    /// stolen lease belongs to its new holder, not to us.
    fn remove_if_ours(&self) {
        match read_info(&self.lock_path) {
            Some(info)
                if info.pid != self.info.pid || info.acquired_at != self.info.acquired_at => {}
            _ => {
                let _ = fs::remove_file(&self.lock_path);
            }
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.remove_if_ours();
    }
}

/// `create_new` is the atomic primitive: it either makes the file or reports
/// that someone else already did. `Ok(None)` means the lock already exists.
fn create_lock(lock_path: &Path) -> io::Result<Option<Lease>> {
    let info = LeaseInfo::for_this_process(unix_now());
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
    {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => return Ok(None),
        Err(err) => return Err(err),
    };
    if let Err(err) = encode(&info).and_then(|json| file.write_all(json.as_bytes())) {
        // Don't leave an empty lock behind that nobody can attribute.
        drop(file);
        let _ = fs::remove_file(lock_path);
        return Err(err);
    }
    Ok(Some(Lease {
        lock_path: lock_path.to_path_buf(),
        info,
    }))
}

fn encode(info: &LeaseInfo) -> io::Result<String> {
    serde_json::to_string_pretty(info).map_err(io::Error::other)
}

fn read_info(lock_path: &Path) -> Option<LeaseInfo> {
    let bytes = fs::read(lock_path).ok()?;
    serde_json::from_slice(&bytes).ok()
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
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}-{n}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn workbook(prefix: &str) -> PathBuf {
        let path = unique_temp_dir(prefix).join("board.slate");
        fs::write(&path, "{}").expect("write workbook");
        path
    }

    fn write_lock(doc: &Path, info: &LeaseInfo) {
        fs::write(Lease::lock_path(doc), encode(info).expect("encode")).expect("write lock");
    }

    #[test]
    fn lock_path_sits_beside_the_workbook() {
        let doc = Path::new("/share/projects/board.slate");
        assert_eq!(
            Lease::lock_path(doc),
            PathBuf::from("/share/projects/board.slate.lock")
        );
    }

    #[test]
    fn lease_acquire_then_second_acquire_is_held() {
        let doc = workbook("slate-lease-held");
        let first = match Lease::acquire(&doc).expect("acquire") {
            LeaseState::Acquired(lease) => lease,
            LeaseState::Held(info) => panic!("first acquire was held by {info:?}"),
        };
        match Lease::acquire(&doc).expect("second acquire") {
            LeaseState::Held(info) => {
                assert_eq!(info.pid, std::process::id());
                assert_eq!(&info, first.info());
            }
            LeaseState::Acquired(_) => panic!("second acquire stole a live lease"),
        }
    }

    #[test]
    fn stale_lease_is_stolen() {
        let doc = workbook("slate-lease-stale");
        let now = unix_now();
        write_lock(
            &doc,
            &LeaseInfo {
                holder: "ghost".to_string(),
                host: "WS-000".to_string(),
                pid: 4242,
                acquired_at: now - 600,
                heartbeat_at: now - 60,
            },
        );
        match Lease::acquire(&doc).expect("acquire") {
            LeaseState::Acquired(lease) => {
                assert_eq!(lease.info().pid, std::process::id());
                let on_disk = read_info(&Lease::lock_path(&doc)).expect("lock is ours now");
                assert_eq!(&on_disk, lease.info());
            }
            LeaseState::Held(info) => panic!("stale lease was not stolen: {info:?}"),
        }
    }

    #[test]
    fn heartbeat_keeps_lease_fresh() {
        let doc = workbook("slate-lease-heartbeat");
        let LeaseState::Acquired(mut lease) = Lease::acquire(&doc).expect("acquire") else {
            panic!("expected to acquire");
        };
        let taken_at = lease.info().heartbeat_at;

        // Inside the interval: no write, no advance.
        lease
            .heartbeat_at(taken_at + LEASE_HEARTBEAT_SECS - 1)
            .expect("heartbeat");
        assert_eq!(lease.info().heartbeat_at, taken_at);
        let on_disk = read_info(&Lease::lock_path(&doc)).expect("lock");
        assert_eq!(on_disk.heartbeat_at, taken_at);

        // Past the interval: one write, and the file reflects it.
        let later = taken_at + LEASE_HEARTBEAT_SECS;
        lease.heartbeat_at(later).expect("heartbeat");
        assert_eq!(lease.info().heartbeat_at, later);
        let on_disk = read_info(&Lease::lock_path(&doc)).expect("lock");
        assert_eq!(on_disk.heartbeat_at, later);
        assert_eq!(on_disk.acquired_at, taken_at);
    }

    #[test]
    fn release_removes_lock_file() {
        let doc = workbook("slate-lease-release");
        let LeaseState::Acquired(lease) = Lease::acquire(&doc).expect("acquire") else {
            panic!("expected to acquire");
        };
        let lock = Lease::lock_path(&doc);
        assert!(lock.is_file());
        lease.release();
        assert!(!lock.exists());
        // And the workbook is free again.
        assert!(matches!(
            Lease::acquire(&doc).expect("re-acquire"),
            LeaseState::Acquired(_)
        ));
    }

    #[test]
    fn drop_releases_lease() {
        let doc = workbook("slate-lease-drop");
        let lock = Lease::lock_path(&doc);
        {
            let LeaseState::Acquired(_lease) = Lease::acquire(&doc).expect("acquire") else {
                panic!("expected to acquire");
            };
            assert!(lock.is_file());
        }
        assert!(!lock.exists());
    }

    #[test]
    fn release_leaves_a_lock_another_process_now_owns() {
        let doc = workbook("slate-lease-stolen");
        let LeaseState::Acquired(lease) = Lease::acquire(&doc).expect("acquire") else {
            panic!("expected to acquire");
        };
        let now = unix_now();
        let successor = LeaseInfo {
            holder: "someone".to_string(),
            host: "WS-001".to_string(),
            pid: lease.info().pid + 1,
            acquired_at: now,
            heartbeat_at: now,
        };
        write_lock(&doc, &successor);
        lease.release();
        assert_eq!(read_info(&Lease::lock_path(&doc)), Some(successor));
    }

    #[test]
    fn unparsable_fresh_lock_is_held_not_stolen() {
        let doc = workbook("slate-lease-garbage");
        let lock = Lease::lock_path(&doc);
        fs::write(&lock, "not json at all").expect("write lock");
        match Lease::acquire(&doc).expect("acquire") {
            LeaseState::Held(info) => assert_eq!(info, LeaseInfo::unknown()),
            LeaseState::Acquired(_) => panic!("a fresh unparsable lock was stolen"),
        }
        assert_eq!(fs::read_to_string(&lock).expect("lock"), "not json at all");
    }
}
