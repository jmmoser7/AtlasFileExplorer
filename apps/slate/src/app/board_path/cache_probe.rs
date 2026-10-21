//! Opt-in, test-only measurements of the existing cache and mesh conversion.
//! No path data is retained, and identity tracking shares the cache entry cap.

use super::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::time::Instant;

#[derive(Clone, Default, serde::Serialize)]
pub(crate) struct FrameStats {
    pub requests: u64,
    pub hits: u64,
    pub builds: u64,
    pub requested_bytes: u64,
    pub unique_requested_bytes: u64,
    pub built_bytes: u64,
    pub largest_request_bytes: usize,
    pub build_cpu_ns: u64,
    pub admitted: u64,
    pub evictions: u64,
    pub evicted_bytes: u64,
    pub eviction_scans: u64,
    pub admission_bypasses: u64,
    pub bypassed_bytes: u64,
    pub oversized_rejections: u64,
    pub oversized_bytes: u64,
    pub disabled_admission_rejections: u64,
    pub key_changes: u64,
    pub identity_tracking_saturated: bool,
    pub request_tracking_saturated: bool,
}

#[derive(Clone, Default, serde::Serialize)]
pub(crate) struct ConversionStats {
    pub calls: u64,
    pub vertices: u64,
    pub indices: u64,
    pub output_bytes: u64,
    pub cpu_ns: u64,
}

#[derive(Clone, serde::Serialize)]
pub(crate) struct Snapshot {
    pub cache: FrameStats,
    pub conversion: ConversionStats,
    pub resident_bytes: usize,
    pub resident_entries: usize,
    pub budget_bytes: usize,
    pub entry_limit: usize,
    pub tracked_identities: usize,
    pub tracked_requests: usize,
}

#[derive(Default)]
pub(super) struct Probe {
    frame: FrameStats,
    last_keys: HashMap<(NodeId, bool), u64>,
    requested: HashSet<GeometryKey>,
}

impl Probe {
    fn bytes(&mut self, key: GeometryKey, bytes: usize) {
        self.frame.requested_bytes = self.frame.requested_bytes.saturating_add(bytes as u64);
        self.frame.largest_request_bytes = self.frame.largest_request_bytes.max(bytes);
        if self.requested.contains(&key) {
            return;
        }
        if self.requested.len() >= CACHE_ENTRIES {
            self.frame.request_tracking_saturated = true;
            return;
        }
        self.requested.insert(key);
        self.frame.unique_requested_bytes = self
            .frame
            .unique_requested_bytes
            .saturating_add(bytes as u64);
    }

    pub(super) fn hit(&mut self, key: GeometryKey, bytes: usize) {
        self.frame.hits += 1;
        self.bytes(key, bytes);
    }
}

thread_local! {
    static CONVERSIONS: RefCell<Option<ConversionStats>> = const { RefCell::new(None) };
}

/// Hold this guard for a probe run; unwind and normal completion restore the
/// thread's previous conversion collection without changing cache behavior.
pub(crate) struct Scope(Option<ConversionStats>);

impl Drop for Scope {
    fn drop(&mut self) {
        CONVERSIONS.with(|stats| *stats.borrow_mut() = self.0.take());
    }
}

pub(super) struct Conversion {
    started: Instant,
    vertices: usize,
    indices: usize,
}

impl Conversion {
    pub(super) fn begin(mesh: &CachedInkMesh) -> Option<Self> {
        CONVERSIONS.with(|stats| {
            stats.borrow().is_some().then(|| Self {
                started: Instant::now(),
                vertices: mesh.vertices.len(),
                indices: mesh.indices.len(),
            })
        })
    }
}

impl Drop for Conversion {
    fn drop(&mut self) {
        CONVERSIONS.with(|stats| {
            let mut stats = stats.borrow_mut();
            let Some(stats) = stats.as_mut() else { return };
            stats.calls += 1;
            stats.vertices += self.vertices as u64;
            stats.indices += self.indices as u64;
            stats.output_bytes += (self.vertices * std::mem::size_of::<egui::epaint::Vertex>()
                + self.indices * std::mem::size_of::<u32>())
                as u64;
            stats.cpu_ns = stats
                .cpu_ns
                .saturating_add(self.started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
        });
    }
}

impl PathMeshCache {
    pub(crate) fn enable_probe(&mut self) -> Scope {
        self.probe = Some(Probe::default());
        Scope(CONVERSIONS.with(|stats| stats.replace(Some(ConversionStats::default()))))
    }

    pub(crate) fn disable_probe(&mut self) {
        self.probe = None;
    }

    pub(crate) fn begin_probe_frame(&mut self) {
        if let Some(probe) = &mut self.probe {
            probe.frame = FrameStats::default();
            probe.requested.clear();
            CONVERSIONS.with(|stats| {
                if stats.borrow().is_some() {
                    *stats.borrow_mut() = Some(ConversionStats::default());
                }
            });
        }
    }

    pub(crate) fn probe_snapshot(&self) -> Option<Snapshot> {
        let probe = self.probe.as_ref()?;
        Some(Snapshot {
            cache: probe.frame.clone(),
            conversion: CONVERSIONS.with(|stats| stats.borrow().clone().unwrap_or_default()),
            resident_bytes: self.resident_bytes,
            resident_entries: self.map.len(),
            budget_bytes: self.budget_bytes,
            entry_limit: self.entry_limit,
            tracked_identities: probe.last_keys.len(),
            tracked_requests: probe.requested.len(),
        })
    }

    pub(super) fn probe_request(&mut self, key: GeometryKey) {
        let Some(probe) = self.probe.as_mut() else {
            return;
        };
        probe.frame.requests += 1;
        let id = (key.0, key.2);
        if let Some(previous) = probe.last_keys.get_mut(&id) {
            probe.frame.key_changes += u64::from(*previous != key.1);
            *previous = key.1;
        } else if probe.last_keys.len() < CACHE_ENTRIES {
            probe.last_keys.insert(id, key.1);
        } else {
            probe.frame.identity_tracking_saturated = true;
        }
    }

    pub(super) fn probe_started(&self) -> Option<Instant> {
        self.probe.as_ref().map(|_| Instant::now())
    }

    pub(super) fn probe_built(&mut self, key: GeometryKey, bytes: usize, started: Option<Instant>) {
        let Some(probe) = self.probe.as_mut() else {
            return;
        };
        probe.frame.builds += 1;
        probe.frame.built_bytes = probe.frame.built_bytes.saturating_add(bytes as u64);
        if let Some(started) = started {
            probe.frame.build_cpu_ns = probe
                .frame
                .build_cpu_ns
                .saturating_add(started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
        }
        probe.bytes(key, bytes);
    }

    pub(super) fn probe_admitted(&mut self) {
        if let Some(probe) = self.probe.as_mut() {
            probe.frame.admitted += 1;
        }
    }

    pub(super) fn probe_evicted(&mut self, bytes: usize) {
        if let Some(probe) = self.probe.as_mut() {
            probe.frame.evictions += 1;
            probe.frame.evicted_bytes = probe.frame.evicted_bytes.saturating_add(bytes as u64);
        }
    }

    pub(super) fn probe_scanned(&mut self) {
        if let Some(probe) = self.probe.as_mut() {
            probe.frame.eviction_scans += 1;
        }
    }

    pub(super) fn probe_bypassed(&mut self, bytes: usize) {
        if let Some(probe) = self.probe.as_mut() {
            probe.frame.admission_bypasses += 1;
            probe.frame.bypassed_bytes = probe.frame.bypassed_bytes.saturating_add(bytes as u64);
        }
    }

    pub(super) fn probe_rejected(&mut self, bytes: usize, oversized: bool) {
        if let Some(probe) = self.probe.as_mut() {
            if oversized {
                probe.frame.oversized_rejections += 1;
                probe.frame.oversized_bytes =
                    probe.frame.oversized_bytes.saturating_add(bytes as u64);
            } else {
                probe.frame.disabled_admission_rejections += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests;
