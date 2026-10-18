//! Durable local recovery generations, including workbooks never saved by name.
//!
//! A run holds its OS guard for its entire lifetime. Other processes discover
//! only unlocked runs; clocks and process identifiers never decide ownership.
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::SlateDoc;

mod discovery;
#[cfg(test)]
mod tests;

const SCHEMA: u32 = 1;
const GUARD: &str = ".guard";
static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);

/// A single workbook's newest valid generation. Paths used for storage are
/// private so callers cannot turn a recovery dismissal into arbitrary deletion.
#[derive(Clone, Debug)]
pub struct RecoveryEntry {
    pub session: String,
    pub tab: u64,
    pub name: String,
    pub saved_at_ms: u64,
    pub original_path: Option<PathBuf>,
    pub warning: Option<String>,
    metadata: Metadata,
}

/// Corrupt or unreadable files remain on disk and are reported independently.
#[derive(Debug, Default)]
pub struct RecoveryScan {
    pub entries: Vec<RecoveryEntry>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RecoveryStore {
    root: PathBuf,
}

/// Move this owner to the persistence worker; dropping it releases the run.
#[derive(Debug)]
pub struct RecoverySession {
    directory: PathBuf,
    session: String,
    sequence: AtomicU64,
    _guard: File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Metadata {
    session: String,
    tab: u64,
    generation: String,
    name: String,
    saved_at_ms: u64,
    original_path: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct Snapshot {
    schema: u32,
    metadata: Metadata,
    document: serde_json::Value,
}

impl RecoveryStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn begin_session(&self) -> io::Result<RecoverySession> {
        fs::create_dir_all(&self.root)?;
        let root = fs::canonicalize(&self.root)?;
        // create_dir, rather than create_dir_all, reserves a fresh run name.
        for _ in 0..100 {
            let session = format!(
                "{}-{}-{}",
                unix_millis(),
                std::process::id(),
                NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
            );
            let directory = root.join(&session);
            match fs::create_dir(&directory) {
                Ok(()) => {
                    let guard = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create_new(true)
                        .open(directory.join(GUARD))?;
                    match guard.try_lock() {
                        Ok(()) => {}
                        // A discovery scan may briefly lock the just-created
                        // empty run. Reserve another run instead of disabling
                        // recovery because of that startup race.
                        Err(TryLockError::WouldBlock) => continue,
                        Err(TryLockError::Error(error)) => return Err(error),
                    }
                    return Ok(RecoverySession {
                        directory,
                        session,
                        sequence: AtomicU64::new(0),
                        _guard: guard,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "cannot reserve recovery session",
        ))
    }

    /// Read only an inactive run. A changed generation must pass validation
    /// again, including the canonical workbook decoder and future-version gate.
    pub fn load(&self, entry: &RecoveryEntry) -> io::Result<SlateDoc> {
        let directory = self.session_directory(&entry.metadata.session)?;
        let _guard = inactive_guard(&directory)?.ok_or_else(live_session)?;
        let path = generation_path(&directory, &entry.metadata.generation)?;
        let (metadata, doc) = read_snapshot(&path, &entry.metadata.session)?;
        if metadata.tab != entry.metadata.tab {
            return Err(invalid("recovery tab identity changed"));
        }
        Ok(doc)
    }

    /// Explicit dismissal removes this tab's generations, leaving corrupt
    /// files in other tabs and every live run untouched.
    pub fn discard(&self, entry: &RecoveryEntry) -> io::Result<()> {
        self.discard_group(&entry.metadata.session, entry.metadata.tab)
    }

    /// Dismiss a group even when all its generations are unreadable.
    pub fn discard_group(&self, session: &str, tab: u64) -> io::Result<()> {
        let directory = self.session_directory(session)?;
        let _guard = inactive_guard(&directory)?.ok_or_else(live_session)?;
        discard_tab(&directory, tab)
    }

    fn session_directory(&self, session: &str) -> io::Result<PathBuf> {
        if session.is_empty() || !session.bytes().all(|c| c.is_ascii_digit() || c == b'-') {
            return Err(invalid("invalid recovery session name"));
        }
        let directory = fs::canonicalize(&self.root)?.join(session);
        ensure_regular(&directory, true)?;
        Ok(directory)
    }
}

impl RecoverySession {
    /// Commit a complete immutable generation before pruning older valid ones.
    /// This writes document JSON only and never opens a linked source file.
    pub fn write(
        &self,
        tab: u64,
        original_path: Option<PathBuf>,
        doc: &SlateDoc,
        keep: usize,
    ) -> io::Result<RecoveryEntry> {
        let original_path = match original_path {
            Some(path) if !path.is_absolute() => Some(std::env::current_dir()?.join(path)),
            path => path,
        };
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let metadata = Metadata {
            session: self.session.clone(),
            tab,
            generation: format!("{tab}-{sequence:020}.json"),
            name: doc.name.clone(),
            saved_at_ms: unix_millis(),
            original_path: original_path.clone(),
        };
        let mut document = doc.clone();
        crate::locators::absolutize_document(&mut document, original_path.as_deref())?;
        let snapshot = Snapshot {
            schema: SCHEMA,
            metadata: metadata.clone(),
            document: serde_json::to_value(&document).map_err(io::Error::other)?,
        };
        let bytes = serde_json::to_vec(&snapshot).map_err(io::Error::other)?;
        let path = generation_path(&self.directory, &metadata.generation)?;
        persist_generation(&path, &bytes)?;
        let mut entry = RecoveryEntry::from(metadata);
        // A pruning failure must not conceal a successful durable checkpoint.
        if let Err(error) = self.prune(tab, keep.max(1)) {
            entry.warning = Some(format!(
                "Checkpoint saved; older versions could not be pruned: {error}"
            ));
        }
        Ok(entry)
    }

    pub fn discard(&self, tab: u64) -> io::Result<()> {
        discard_tab(&self.directory, tab)
    }

    fn prune(&self, tab: u64, keep: usize) -> io::Result<()> {
        let mut paths = tab_paths(&self.directory, tab)?;
        paths.sort();
        let valid: Vec<_> = paths
            .into_iter()
            .rev()
            .filter(|path| read_snapshot(path, &self.session).is_ok())
            .collect();
        for path in valid.into_iter().skip(keep) {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

impl From<Metadata> for RecoveryEntry {
    fn from(metadata: Metadata) -> Self {
        Self {
            session: metadata.session.clone(),
            tab: metadata.tab,
            name: metadata.name.clone(),
            saved_at_ms: metadata.saved_at_ms,
            original_path: metadata.original_path.clone(),
            warning: None,
            metadata,
        }
    }
}

fn persist_generation(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)
}

fn read_snapshot(path: &Path, session: &str) -> io::Result<(Metadata, SlateDoc)> {
    ensure_regular(path, false)?;
    let snapshot: Snapshot = serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
    if snapshot.schema != SCHEMA || snapshot.metadata.session != session {
        return Err(invalid("unsupported recovery schema or mismatched session"));
    }
    let metadata = snapshot.metadata;
    if path.file_name() != Some(std::ffi::OsStr::new(&metadata.generation))
        || generation_tab(&metadata.generation) != Some(metadata.tab)
    {
        return Err(invalid("mismatched recovery generation identity"));
    }
    let bytes = serde_json::to_vec(&snapshot.document).map_err(io::Error::other)?;
    let doc = SlateDoc::decode_bytes(&bytes, path).map_err(io::Error::other)?;
    if metadata.name != doc.name
        || metadata
            .original_path
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
    {
        return Err(invalid("invalid recovery workbook metadata"));
    }
    Ok((metadata, doc))
}

fn generation_tab(name: &str) -> Option<u64> {
    let stem = name.strip_suffix(".json")?;
    let (tab, sequence) = stem.split_once('-')?;
    if sequence.len() != 20 || !sequence.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    tab.parse().ok()
}

fn generation_path(directory: &Path, generation: &str) -> io::Result<PathBuf> {
    generation_tab(generation).ok_or_else(|| invalid("invalid recovery generation name"))?;
    Ok(directory.join(generation))
}

fn ensure_regular(path: &Path, directory: bool) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err(invalid(
            "recovery path is not an ordinary file or directory",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("recovery path cannot be a reparse point"));
        }
    }
    Ok(())
}

fn inactive_guard(directory: &Path) -> io::Result<Option<File>> {
    let path = directory.join(GUARD);
    ensure_regular(&path, false)?;
    let guard = OpenOptions::new().read(true).write(true).open(path)?;
    match guard.try_lock() {
        Ok(()) => Ok(Some(guard)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn tab_paths(directory: &Path, tab: u64) -> io::Result<Vec<PathBuf>> {
    fs::read_dir(directory)?
        .filter_map(|entry| match entry {
            Ok(entry) if generation_tab(&entry.file_name().to_string_lossy()) == Some(tab) => {
                Some(Ok(entry.path()))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

fn discard_tab(directory: &Path, tab: u64) -> io::Result<()> {
    for path in tab_paths(directory, tab)? {
        ensure_regular(&path, false)?;
        fs::remove_file(path)?;
    }
    Ok(())
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn live_session() -> io::Error {
    io::Error::new(
        io::ErrorKind::WouldBlock,
        "recovery session is still open in Slate",
    )
}
