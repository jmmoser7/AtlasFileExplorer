//! Bounded retention for board path geometry; every miss still paints.
use super::*;

/// The path shape a styled closed form paints as, with the shape and box
/// it was derived from.
pub(super) struct ClosedFormPaint {
    source: ShapeNode,
    rect: WorldRect,
    styled: Option<Shared<ShapeNode>>,
}

pub struct PathMeshCache {
    pub(super) map: HashMap<GeometryKey, GeometryEntry>,
    pub(super) clock: VecDeque<GeometryKey>,
    pub(super) resident_bytes: usize,
    pub(super) budget_bytes: usize,
    pub(super) entry_limit: usize,
    pub(super) frame: Option<u64>,
    pub(super) sweep_remaining: usize,
    /// Keyed by (board scope, id): every board numbers its nodes from one.
    pub(super) closed_forms: HashMap<(u64, NodeId), ClosedFormPaint>,
    /// Cache misses this paint — reset at the start of `board_canvas`.
    pub tess_misses: u32,
    #[cfg(test)]
    pub(super) probe: Option<cache_probe::Probe>,
}

impl Default for PathMeshCache {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            clock: VecDeque::new(),
            resident_bytes: 0,
            budget_bytes: CACHE_BYTES,
            entry_limit: CACHE_ENTRIES,
            frame: None,
            sweep_remaining: 0,
            closed_forms: HashMap::new(),
            tess_misses: 0,
            #[cfg(test)]
            probe: None,
        }
    }
}

impl PathMeshCache {
    /// One epoch for every painter in an app update, including presentation
    /// and nested boards. Repeated calls cannot replenish eviction work.
    pub(crate) fn begin_frame(&mut self, frame: u64) {
        if self.frame == Some(frame) {
            return;
        }
        self.frame = Some(frame);
        self.sweep_remaining = self.clock.len();
    }

    /// The path shape closed form `id` paints as while it stores per-vertex
    /// style (`vertex_style::closed_form_paint_shape`), derived again only
    /// when the shape or its box changes, so a steady frame allocates
    /// nothing (Art. II). `scope` names the board being painted
    /// ([`SlateApp::closed_form_scope`]).
    pub(crate) fn closed_form_paint_shape(
        &mut self,
        scope: u64,
        id: NodeId,
        shape: &ShapeNode,
        rect: WorldRect,
    ) -> Option<Shared<ShapeNode>> {
        if shape.path.is_none()
            || slate_doc::vertex_style::closed_form_vertex_count(shape).is_none()
        {
            return None;
        }
        let id = (scope, id);
        if let Some(entry) = self.closed_forms.get(&id) {
            if entry.rect == rect && entry.source == *shape {
                return entry.styled.clone();
            }
        }
        note_closed_form_derive();
        let styled = slate_doc::vertex_style::closed_form_paint_shape(shape, rect).map(Shared::new);
        if self.closed_forms.len() >= CACHE_ENTRIES && !self.closed_forms.contains_key(&id) {
            self.closed_forms.clear();
        }
        self.closed_forms.insert(
            id,
            ClosedFormPaint {
                source: shape.clone(),
                rect,
                styled: styled.clone(),
            },
        );
        styled
    }

    fn get(&mut self, key: GeometryKey) -> Option<CachedGeometry> {
        #[cfg(test)]
        self.probe_request(key);
        let entry = self.map.get_mut(&key)?;
        entry.last_seen_frame = self.frame.unwrap_or(0);
        #[cfg(test)]
        if let Some(probe) = self.probe.as_mut() {
            probe.hit(key, entry.geometry.bytes());
        }
        Some(entry.geometry.clone())
    }

    fn insert(&mut self, key: GeometryKey, geometry: CachedGeometry) {
        let bytes = geometry.bytes();
        // One exceptionally complex shape must not flush everything else.
        // Its caller still owns and paints the result for this frame.
        if bytes > self.budget_bytes || self.entry_limit == 0 {
            #[cfg(test)]
            self.probe_rejected(bytes, bytes > self.budget_bytes);
            return;
        }
        // Protect this paint and the preceding one, regardless of traversal
        // order. An oversized visible set must not evict each next-needed mesh.
        // Only slots present at frame start are scanned, at most once total;
        // new entries are already protected and cannot provide eviction space.
        let protected_since = self.frame.unwrap_or(0).saturating_sub(1);
        while !self.fits(bytes) && self.sweep_remaining > 0 {
            self.sweep_remaining -= 1;
            #[cfg(test)]
            self.probe_scanned();
            let old = self
                .clock
                .pop_front()
                .expect("cache entry has a clock slot");
            let entry = self
                .map
                .get_mut(&old)
                .expect("clock slot has a cache entry");
            if entry.last_seen_frame >= protected_since {
                self.clock.push_back(old);
            } else if let Some(entry) = self.map.remove(&old) {
                self.resident_bytes -= entry.geometry.bytes();
                #[cfg(test)]
                self.probe_evicted(entry.geometry.bytes());
            }
        }
        // A later, smaller request may still fit the space freed above. Do
        // not latch all admission off when one large candidate cannot fit.
        if !self.fits(bytes) {
            #[cfg(test)]
            self.probe_bypassed(bytes);
            return;
        }
        self.resident_bytes += bytes;
        #[cfg(test)]
        self.probe_admitted();
        self.map.insert(
            key,
            GeometryEntry {
                geometry,
                last_seen_frame: self.frame.unwrap_or(0),
            },
        );
        self.clock.push_back(key);
    }

    fn fits(&self, bytes: usize) -> bool {
        self.resident_bytes + bytes <= self.budget_bytes && self.map.len() < self.entry_limit
    }

    pub(crate) fn get_or_tessellate(
        &mut self,
        node_id: NodeId,
        key: u64,
        build: impl FnOnce() -> InkMesh,
    ) -> Shared<CachedInkMesh> {
        let key = (node_id, key, false);
        if let Some(CachedGeometry::Stroke(c)) = self.get(key) {
            return c;
        }
        self.tess_misses = self.tess_misses.saturating_add(1);
        #[cfg(test)]
        let probe_started = self.probe_started();
        let cached = Shared::new(CachedInkMesh::from(build()));
        #[cfg(test)]
        self.probe_built(
            key,
            CachedGeometry::Stroke(cached.clone()).bytes(),
            probe_started,
        );
        self.insert(key, CachedGeometry::Stroke(cached.clone()));
        cached
    }

    pub(crate) fn get_or_fill_tris(
        &mut self,
        node_id: NodeId,
        key: u64,
        build: impl FnOnce() -> (Vec<[f32; 2]>, Vec<u32>),
    ) -> Shared<FillTriangles> {
        let key = (node_id, key, true);
        if let Some(CachedGeometry::Fill(t)) = self.get(key) {
            return t;
        }
        self.tess_misses = self.tess_misses.saturating_add(1);
        #[cfg(test)]
        let probe_started = self.probe_started();
        let t = Shared::new(build());
        #[cfg(test)]
        self.probe_built(key, CachedGeometry::Fill(t.clone()).bytes(), probe_started);
        self.insert(key, CachedGeometry::Fill(t.clone()));
        t
    }

    #[cfg(test)]
    pub(crate) fn stroke_len(&self) -> usize {
        self.map.keys().filter(|(_, _, fill)| !fill).count()
    }
}

#[cfg(test)]
mod policy_tests;
