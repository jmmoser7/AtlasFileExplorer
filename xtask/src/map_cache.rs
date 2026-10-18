//! Per-file cache so `cargo xtask map` skips unchanged sources.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::map::{FileMap, MapItem};
use crate::MetricsError;

pub const CACHE_PATH: &str = "docs/metrics/.map-cache.json";

pub(crate) type EntryStore = BTreeMap<String, CacheEntry>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CacheEntry {
    fingerprint: u64,
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

fn fingerprint(source: &str) -> u64 {
    // Deterministic FNV-1a: cache freshness depends on the bytes, never on
    // timestamp granularity. The collector hashes the same source it parses.
    source
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}

pub fn try_hit(store: &EntryStore, rel: &str, source: &str) -> Option<FileMap> {
    let entry = store.get(rel)?;
    if entry.size == source.len() as u64 && entry.fingerprint == fingerprint(source) {
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

pub fn insert(store: &mut EntryStore, rel: String, fm: &FileMap, source: &str) {
    store.insert(
        rel,
        CacheEntry {
            fingerprint: fingerprint(source),
            size: source.len() as u64,
            lines: fm.lines,
            purpose: fm.purpose.clone(),
            items: fm.items.clone(),
        },
    );
}
