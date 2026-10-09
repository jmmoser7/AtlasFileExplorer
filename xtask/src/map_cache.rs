//! Per-file cache so `cargo xtask map` skips unchanged sources.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::map::{FileMap, MapItem};
use crate::MetricsError;

pub const CACHE_PATH: &str = "docs/metrics/.map-cache.json";

pub(crate) type EntryStore = BTreeMap<String, CacheEntry>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CacheEntry {
    mtime: u64,
    size: u64,
    lines: usize,
    purpose: String,
    items: Vec<MapItem>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    entries: EntryStore,
}

pub fn empty_store() -> EntryStore {
    EntryStore::new()
}

pub fn load(root: &Path) -> EntryStore {
    let path = root.join(CACHE_PATH);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    serde_json::from_str::<CacheFile>(&text)
        .map(|c| c.entries)
        .unwrap_or_default()
}

pub fn save(root: &Path, entries: &EntryStore) -> Result<(), MetricsError> {
    let path = root.join(CACHE_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| MetricsError::io(parent, e))?;
    }
    let body = CacheFile {
        entries: entries.clone(),
    };
    let text = serde_json::to_string_pretty(&body).expect("cache serializes");
    std::fs::write(&path, text).map_err(|e| MetricsError::io(&path, e))
}

fn stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some((mtime, meta.len()))
}

pub fn try_hit(store: &EntryStore, rel: &str, path: &Path) -> Option<FileMap> {
    let (mtime, size) = stamp(path)?;
    let entry = store.get(rel)?;
    if entry.mtime == mtime && entry.size == size {
        Some(FileMap {
            path: rel.to_string(),
            lines: entry.lines,
            purpose: entry.purpose.clone(),
            items: entry.items.clone(),
        })
    } else {
        None
    }
}

pub fn insert(store: &mut EntryStore, rel: String, fm: &FileMap, path: &Path) {
    let Some((mtime, size)) = stamp(path) else {
        return;
    };
    store.insert(
        rel,
        CacheEntry {
            mtime,
            size,
            lines: fm.lines,
            purpose: fm.purpose.clone(),
            items: fm.items.clone(),
        },
    );
}

pub fn retain_existing(root: &Path, entries: &mut EntryStore) {
    entries.retain(|rel, _| {
        root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))
            .is_file()
    });
}
