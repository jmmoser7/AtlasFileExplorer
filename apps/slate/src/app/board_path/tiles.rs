//! GPU tiles for committed brush stamps.
//!
//! Plain stamp strokes that sit next to each other in paint order share
//! world-aligned textures. The pixels are the same source-over composite as
//! painting each stamp bitmap in z-order (`vector_ink::composite_stroke`).
//! Tiles are derived: nothing here is journaled or written into the workbook.
//!
//! A stroke that is selected, faded, mid-erase, or the anchor of the live
//! brush canvas stays on the per-stroke path and splits the run so later
//! strokes still paint above it.

use super::super::board::BoardXf;
use super::super::SlateApp;
use super::{paint_path_shape, path_content_hash};
use eframe::egui::{self, Color32, Pos2};
use slate_doc::scene::{ShapeKind, ShapeNode};
use slate_doc::{Node, NodeId, NodeKind};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use vector_ink::{composite_stroke, tile_index, tile_origin, StampImage, StrokeInk, TILE_PX};

type InkCache = HashMap<(NodeId, u64, u32), Arc<StrokeInk>>;
type TileCoord = (u32, i32, i32);
type QueuedTiles = HashMap<(u64, u32, i32, i32), Vec<(NodeId, u64)>>;
type Inflight = HashMap<(u64, u32), u32>;

const CACHE_BYTES: usize = 384 * 1024 * 1024;
const UPLOADS_PER_FRAME: usize = 2;
/// Tile rasterizers. They only run while a zoom level or new strokes fill in,
/// so they may use most cores; two stay free for the frame loop and thumbnails.
fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(2))
        .unwrap_or(2)
        .clamp(2, 6)
}
/// Missing strokes painted on the CPU the same frame they commit. A longer
/// gap waits for tiles so a full board never rasterizes on the UI thread.
const IMMEDIATE_STROKES: usize = 4;

#[allow(dead_code)] // read by the ignored brush-scale bench
pub(crate) struct BrushPaintStats {
    pub gpu_bytes: usize,
    pub pending_jobs: usize,
    pub ready_tiles: usize,
    pub drew_fallback: bool,
    pub individuals: usize,
    pub settled: bool,
    /// Frame of the last paint.
    pub frame: u64,
}

struct StrokeSrc {
    key: u64,
    node: Node,
}

struct GpuTile {
    tex: egui::TextureHandle,
    /// Straight RGBA. Incremental commits stamp onto this, not a re-render.
    cpu: Vec<u8>,
    baked: Vec<(NodeId, u64)>,
    /// The job that rasterized it. Jobs are numbered in the order queued.
    job: u64,
    used: u64,
    pixel: f32,
    tx: i32,
    ty: i32,
}

struct RunCache {
    token: u64,
    /// Tab whose document the strokes belong to. Node ids repeat across
    /// documents, so a run never serves another tab.
    doc: u64,
    ids: Vec<NodeId>,
    keys: Vec<u64>,
    tiles: HashMap<TileCoord, GpuTile>,
    used: u64,
    /// Every tile in `span` at `bits` was exact for the run signature `sig`.
    /// A frame with the same strokes inside that span only draws textures.
    validated: Option<Validated>,
}

#[derive(Clone, Copy)]
struct Validated {
    sig: u64,
    bits: u32,
    span: (i32, i32, i32, i32),
}

impl Validated {
    fn covers(self, sig: u64, bits: u32, span: (i32, i32, i32, i32)) -> bool {
        self.sig == sig
            && self.bits == bits
            && span.0 >= self.span.0
            && span.1 >= self.span.1
            && span.2 <= self.span.2
            && span.3 <= self.span.3
    }
}

struct Job {
    id: u64,
    token: u64,
    pixel: f32,
    tx: i32,
    ty: i32,
    /// `Some` = source-over `strokes` onto these pixels. `None` = start empty.
    base: Option<Vec<u8>>,
    strokes: Vec<Arc<StrokeSrc>>,
    baked: Vec<(NodeId, u64)>,
    incremental: bool,
    ink: Arc<Mutex<InkCache>>,
}

struct Finished {
    id: u64,
    token: u64,
    pixel: f32,
    tx: i32,
    ty: i32,
    rgba: Vec<u8>,
    /// The texture's pixels, premultiplied on the worker so an upload on
    /// the frame thread is only a hand-off.
    image: egui::ColorImage,
    baked: Vec<(NodeId, u64)>,
    incremental: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TileFit {
    Exact,
    Prefix(usize),
    Stale,
    Missing,
}

struct Prepared {
    id: NodeId,
    key: u64,
    bounds: [f32; 4],
    src: Arc<StrokeSrc>,
}

/// What one tile coordinate showed the last frame every run there had its
/// exact tile. While any run there waits for a raster, the coordinate paints
/// this instead, so a commit, split, or merge swaps in all at once.
struct Shown {
    seen: u64,
    layers: Vec<ShownLayer>,
    /// Every stroke the layers show, sorted.
    ids: Vec<NodeId>,
}

enum ShownLayer {
    Tile {
        token: u64,
        tex: egui::TextureHandle,
        ids: Vec<NodeId>,
    },
    /// A stroke painted on its own, clipped to the coordinate on replay.
    Node(NodeId),
}

impl ShownLayer {
    fn key(&self) -> LayerKey {
        match self {
            ShownLayer::Tile { token, tex, .. } => LayerKey::Tile {
                token: *token,
                tex: tex.id(),
            },
            ShownLayer::Node(id) => LayerKey::Node(*id),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LayerKey {
    Tile { token: u64, tex: egui::TextureId },
    Node(NodeId),
}

/// One visible tile coordinate during a paint.
#[derive(Default)]
struct CoordFrame {
    /// Every run here has its exact tile this frame.
    settled: bool,
    /// The stand-in (`Shown`) was painted this frame.
    replayed: bool,
    /// What painted here, in order, while settled.
    layers: Vec<LayerKey>,
}

/// A stretch of paint order: a run of plain stamps, or one node on its own.
enum Segment {
    Run(RunPlan),
    Single(usize),
}

struct RunPlan {
    token: u64,
    start: usize,
    end: usize,
    /// Every tile in view was exact last time; only textures paint.
    fast: bool,
    prep: Vec<Prepared>,
    cells: Vec<Cell>,
    sig: u64,
    ids: Vec<NodeId>,
    keys: Vec<u64>,
    missing: bool,
}

struct Cell {
    slot: usize,
    tx: i32,
    ty: i32,
    fit: TileFit,
    desired: Vec<(NodeId, u64)>,
}

pub(crate) struct BrushTiles {
    keys: HashMap<NodeId, u64>,
    keyed_gen: u64,
    dirty_ids: Vec<NodeId>,
    dirty_all: bool,
    specified: bool,
    runs: Vec<RunCache>,
    /// Runs a paint already matched this frame. A split run's later pieces
    /// get their own caches instead of fighting over one tile set.
    claimed: Vec<u64>,
    /// What each visible tile coordinate painted when it last settled, for
    /// document `shown_doc`. It stands in while the coordinate rebuilds.
    shown: HashMap<TileCoord, Shown>,
    shown_doc: u64,
    coords: Vec<CoordFrame>,
    next_token: u64,
    next_job: u64,
    live_jobs: HashSet<u64>,
    queued: QueuedTiles,
    /// Incremental jobs still rasterizing, keyed by run token and pixel.
    inflight: Inflight,
    stash: Vec<Finished>,
    srcs: HashMap<NodeId, Arc<StrokeSrc>>,
    incoming: VecDeque<Finished>,
    job_tx: Option<Sender<Job>>,
    done_rx: Option<Receiver<Finished>>,
    workers: Vec<JoinHandle<()>>,
    ink: Arc<Mutex<InkCache>>,
    pub last: BrushPaintStats,
}

impl Default for BrushTiles {
    fn default() -> Self {
        Self {
            keys: HashMap::new(),
            keyed_gen: u64::MAX,
            dirty_ids: Vec::new(),
            dirty_all: false,
            specified: false,
            runs: Vec::new(),
            claimed: Vec::new(),
            shown: HashMap::new(),
            shown_doc: u64::MAX,
            coords: Vec::new(),
            next_token: 1,
            next_job: 1,
            live_jobs: HashSet::new(),
            queued: HashMap::new(),
            inflight: HashMap::new(),
            stash: Vec::new(),
            srcs: HashMap::new(),
            incoming: VecDeque::new(),
            job_tx: None,
            done_rx: None,
            workers: Vec::new(),
            ink: Arc::new(Mutex::new(HashMap::new())),
            last: BrushPaintStats {
                gpu_bytes: 0,
                pending_jobs: 0,
                ready_tiles: 0,
                drew_fallback: false,
                individuals: 0,
                settled: false,
                frame: 0,
            },
        }
    }
}

impl Drop for BrushTiles {
    fn drop(&mut self) {
        self.job_tx.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl BrushTiles {
    pub(crate) fn note_ids(&mut self, ids: impl IntoIterator<Item = NodeId>) {
        self.specified = true;
        self.dirty_ids.extend(ids);
    }

    /// Scene changed but the caller did not name nodes (undo, tab switch, a
    /// gesture end).
    pub(crate) fn note_unspecified(&mut self) {
        if self.specified {
            self.specified = false;
        } else {
            self.dirty_all = true;
        }
    }

    #[allow(dead_code)] // the bench resets between the legacy path and tiles
    pub(crate) fn clear(&mut self) {
        self.keys.clear();
        self.keyed_gen = u64::MAX;
        self.dirty_all = true;
        self.runs.clear();
        self.shown.clear();
        self.live_jobs.clear();
        self.queued.clear();
        self.inflight.clear();
        self.stash.clear();
        self.srcs.clear();
        self.incoming.clear();
        if let Ok(mut ink) = self.ink.lock() {
            ink.clear();
        }
    }

    fn sync_keys(&mut self, scene_gen: u64) {
        if self.keyed_gen == scene_gen {
            return;
        }
        if self.dirty_all {
            // Nobody named the nodes, so every content key is recomputed. The
            // tiles stay: each is judged against those keys, and one whose
            // strokes did not change keeps painting.
            self.keys.clear();
            self.srcs.clear();
            if let Ok(mut ink) = self.ink.lock() {
                ink.clear();
            }
        } else {
            for id in &self.dirty_ids {
                self.keys.remove(id);
                self.srcs.remove(id);
            }
        }
        // In-flight rasters stay. Each one names the strokes it baked and is
        // judged against the new keys when it lands, so a stream of commits
        // never keeps an older tile from ever landing.
        self.dirty_ids.clear();
        self.dirty_all = false;
        self.keyed_gen = scene_gen;
    }

    fn src_for(&mut self, node: &Node, key: u64) -> Arc<StrokeSrc> {
        if let Some(cached) = self.srcs.get(&node.id) {
            if cached.key == key {
                return Arc::clone(cached);
            }
        }
        let src = Arc::new(StrokeSrc {
            key,
            node: node.clone(),
        });
        self.srcs.insert(node.id, Arc::clone(&src));
        src
    }

    fn content_key(
        &mut self,
        node: &Node,
        shape: &ShapeNode,
        path: &slate_doc::scene::PathData,
    ) -> u64 {
        if let Some(key) = self.keys.get(&node.id) {
            return *key;
        }
        let key = path_content_hash(
            path,
            &shape.stroke,
            node.rect,
            node.rotation_deg,
            shape.corner,
            0,
        ) ^ 0x57A5;
        self.keys.insert(node.id, key);
        key
    }

    fn ensure_pool(&mut self) {
        if self.job_tx.is_some() {
            return;
        }
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (done_tx, done_rx) = mpsc::channel::<Finished>();
        let job_rx = Arc::new(Mutex::new(job_rx));
        for _ in 0..worker_count() {
            let job_rx = Arc::clone(&job_rx);
            let done_tx = done_tx.clone();
            self.workers
                .push(std::thread::spawn(move || worker(job_rx, done_tx)));
        }
        self.job_tx = Some(job_tx);
        self.done_rx = Some(done_rx);
    }

    fn drain_finished(&mut self) {
        let Some(rx) = self.done_rx.as_ref() else {
            return;
        };
        while let Ok(fin) = rx.try_recv() {
            self.incoming.push_back(fin);
        }
    }

    fn upload_some(&mut self, ctx: &egui::Context) {
        let mut uploaded = 0;
        while uploaded < UPLOADS_PER_FRAME {
            let Some(fin) = self.incoming.pop_front() else {
                break;
            };
            if !self.live_jobs.remove(&fin.id) {
                continue;
            }
            if fin.incremental {
                let key = (fin.token, fin.pixel.to_bits());
                if let Some(left) = self.inflight.get_mut(&key) {
                    *left = left.saturating_sub(1);
                }
                self.stash.push(fin);
                self.flush_stash(ctx);
                continue;
            }
            self.unqueue(&fin);
            self.install(ctx, fin);
            uploaded += 1;
        }
    }

    fn enqueue(&mut self, job: Job) {
        self.ensure_pool();
        let id = job.id;
        let token = job.token;
        let bits = job.pixel.to_bits();
        let tile = (token, bits, job.tx, job.ty);
        let incremental = job.incremental;
        self.queued.insert(tile, job.baked.clone());
        self.live_jobs.insert(id);
        if incremental {
            *self.inflight.entry((token, bits)).or_insert(0) += 1;
        }
        if self.job_tx.as_ref().is_some_and(|tx| tx.send(job).is_err()) {
            self.live_jobs.remove(&id);
            self.queued.remove(&tile);
            if incremental {
                if let Some(left) = self.inflight.get_mut(&(token, bits)) {
                    *left = left.saturating_sub(1);
                }
            }
        }
    }

    /// Install every held incremental tile once the whole batch has rasterized.
    /// A commit usually touches a handful of tiles, so this stays one frame.
    fn flush_stash(&mut self, ctx: &egui::Context) {
        let ready: Vec<(u64, u32)> = self
            .inflight
            .iter()
            .filter(|(_, n)| **n == 0)
            .map(|(k, _)| *k)
            .collect();
        if ready.is_empty() {
            return;
        }
        for key in &ready {
            self.inflight.remove(key);
        }
        let mut keep = Vec::new();
        let batch = std::mem::take(&mut self.stash);
        for fin in batch {
            let key = (fin.token, fin.pixel.to_bits());
            if !ready.contains(&key) {
                keep.push(fin);
                continue;
            }
            self.unqueue(&fin);
            self.install(ctx, fin);
        }
        self.stash = keep;
    }

    /// A newer job queued for the same tile stays queued.
    fn unqueue(&mut self, fin: &Finished) {
        let tile = (fin.token, fin.pixel.to_bits(), fin.tx, fin.ty);
        if self.queued.get(&tile).is_some_and(|q| *q == fin.baked) {
            self.queued.remove(&tile);
        }
    }

    fn install(&mut self, ctx: &egui::Context, fin: Finished) {
        let Some(run) = self.runs.iter_mut().find(|r| r.token == fin.token) else {
            return;
        };
        // A raster queued before the one already here must not undo it.
        if run
            .tiles
            .get(&(fin.pixel.to_bits(), fin.tx, fin.ty))
            .is_some_and(|t| t.job > fin.id)
        {
            return;
        }
        run.validated = None;
        let tex = ctx.load_texture(
            format!("brush-tile-{}-{}-{}", fin.token, fin.tx, fin.ty),
            fin.image,
            egui::TextureOptions::LINEAR,
        );
        run.tiles.insert(
            (fin.pixel.to_bits(), fin.tx, fin.ty),
            GpuTile {
                tex,
                cpu: fin.rgba,
                baked: fin.baked,
                job: fin.id,
                used: 0,
                pixel: fin.pixel,
                tx: fin.tx,
                ty: fin.ty,
            },
        );
    }

    fn gpu_bytes(&self) -> usize {
        self.runs
            .iter()
            .flat_map(|r| r.tiles.values())
            .map(|t| t.cpu.len())
            .sum()
    }

    fn evict(&mut self, frame: u64) {
        let mut total = self.gpu_bytes();
        if total <= CACHE_BYTES {
            return;
        }
        let mut old: Vec<(u64, u64, u32, i32, i32, usize)> = Vec::new();
        for run in &self.runs {
            for tile in run.tiles.values() {
                if tile.used + 1 < frame {
                    old.push((
                        tile.used,
                        run.token,
                        tile.pixel.to_bits(),
                        tile.tx,
                        tile.ty,
                        tile.cpu.len(),
                    ));
                }
            }
        }
        old.sort_by_key(|(used, _, _, _, _, _)| *used);
        for (_, token, bits, tx, ty, bytes) in old {
            if total <= CACHE_BYTES {
                break;
            }
            if let Some(run) = self.runs.iter_mut().find(|r| r.token == token) {
                run.tiles.remove(&(bits, tx, ty));
                total = total.saturating_sub(bytes);
            }
        }
    }
}

/// One draw the last board paint made: a tile or a stroke on its own, the
/// world box it covered, and the strokes it showed there.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct PaintRec {
    pub rect: [f32; 4],
    pub ids: Vec<NodeId>,
}

#[cfg(test)]
thread_local! {
    static PAINTED: std::cell::RefCell<Vec<PaintRec>> = const { std::cell::RefCell::new(Vec::new()) };
    static LOGGING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Record every draw board paints make on this thread (benches leave it off).
#[cfg(test)]
pub(crate) fn log_paints() {
    LOGGING.with(|l| l.set(true));
}

#[cfg(test)]
fn note_paint(rect: [f32; 4], ids: impl Iterator<Item = NodeId>) {
    if !LOGGING.with(|l| l.get()) {
        return;
    }
    PAINTED.with(|p| {
        p.borrow_mut().push(PaintRec {
            rect,
            ids: ids.collect(),
        })
    });
}

#[cfg(not(test))]
#[inline(always)]
fn note_paint(_rect: [f32; 4], _ids: impl Iterator<Item = NodeId>) {}

#[cfg(test)]
fn clear_paint_log() {
    PAINTED.with(|p| p.borrow_mut().clear());
}

#[cfg(not(test))]
#[inline(always)]
fn clear_paint_log() {}

/// Every draw the last board paint on this thread made.
#[cfg(test)]
pub(crate) fn painted_last_frame() -> Vec<PaintRec> {
    PAINTED.with(|p| p.borrow().clone())
}

/// The world box a stroke's ink can reach, as the tiles judge it.
#[cfg(test)]
pub(crate) fn stroke_ink_rect(node: &Node) -> Option<[f32; 4]> {
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    Some(ink_rect(node, shape))
}

/// How often the last paint drew stroke `id` over samples of `region`:
/// `(samples, gaps, doubles)`, where a gap is a sample no draw covered and a
/// double is one two draws covered.
#[cfg(test)]
pub(crate) fn coverage(id: NodeId, region: [f32; 4], steps: u32) -> (u32, u32, u32) {
    let log = painted_last_frame();
    let mut out = (0, 0, 0);
    let steps = steps.max(1);
    for j in 0..steps {
        for i in 0..steps {
            let x = region[0] + (region[2] - region[0]) * (i as f32 + 0.5) / steps as f32;
            let y = region[1] + (region[3] - region[1]) * (j as f32 + 0.5) / steps as f32;
            let hits = log
                .iter()
                .filter(|r| x >= r.rect[0] && x < r.rect[2] && y >= r.rect[1] && y < r.rect[3])
                .filter(|r| r.ids.contains(&id))
                .count();
            out.0 += 1;
            if hits == 0 {
                out.1 += 1;
            } else if hits > 1 {
                out.2 += 1;
            }
        }
    }
    out
}

#[cfg(test)]
impl BrushTiles {
    /// Strokes the last paint drew, from any tile it painted or on their own.
    pub(crate) fn drawn_ids(&self) -> HashSet<NodeId> {
        painted_last_frame()
            .into_iter()
            .flat_map(|r| r.ids.into_iter())
            .collect()
    }

    /// Every cached tile's texture, keyed by run token and tile.
    pub(crate) fn tile_textures(&self) -> HashMap<(u64, TileCoord), egui::TextureId> {
        self.runs
            .iter()
            .flat_map(|r| r.tiles.iter().map(|(k, t)| ((r.token, *k), t.tex.id())))
            .collect()
    }

    /// Tiles whose baked strokes include `id`.
    pub(crate) fn tiles_with(&self, id: NodeId) -> HashSet<(u64, TileCoord)> {
        self.runs
            .iter()
            .flat_map(|r| {
                r.tiles
                    .iter()
                    .filter(|(_, t)| t.baked.iter().any(|(b, _)| *b == id))
                    .map(|(k, _)| (r.token, *k))
            })
            .collect()
    }
}

fn worker(jobs: Arc<Mutex<Receiver<Job>>>, done: Sender<Finished>) {
    loop {
        let job = {
            let rx = jobs.lock().unwrap_or_else(|p| p.into_inner());
            match rx.recv() {
                Ok(job) => job,
                Err(_) => break,
            }
        };
        let origin = tile_origin(job.tx, job.ty, job.pixel, TILE_PX);
        let mut img = StampImage {
            width: TILE_PX,
            height: TILE_PX,
            origin,
            pixel: job.pixel,
            rgba: job
                .base
                .unwrap_or_else(|| vec![0u8; (TILE_PX as usize) * (TILE_PX as usize) * 4]),
        };
        if img.rgba.len() != (TILE_PX as usize) * (TILE_PX as usize) * 4 {
            img.rgba
                .resize((TILE_PX as usize) * (TILE_PX as usize) * 4, 0);
        }
        let mut layer = StampImage {
            width: TILE_PX,
            height: TILE_PX,
            origin,
            pixel: job.pixel,
            rgba: vec![0u8; img.rgba.len()],
        };
        for src in &job.strokes {
            let ink = cached_ink(&job.ink, src, job.pixel);
            composite_stroke(&mut img, &mut layer, ink.as_ref());
        }
        let image = egui::ColorImage::from_rgba_premultiplied(
            [TILE_PX as usize, TILE_PX as usize],
            &super::premultiplied(&img.rgba),
        );
        if done
            .send(Finished {
                id: job.id,
                token: job.token,
                pixel: job.pixel,
                tx: job.tx,
                ty: job.ty,
                rgba: img.rgba,
                image,
                baked: job.baked,
                incremental: job.incremental,
            })
            .is_err()
        {
            break;
        }
    }
}

fn cached_ink(cache: &Mutex<InkCache>, src: &StrokeSrc, pixel: f32) -> Arc<StrokeInk> {
    let bits = pixel.to_bits();
    if let Ok(guard) = cache.lock() {
        if let Some(hit) = guard.get(&(src.node.id, src.key, bits)) {
            return Arc::clone(hit);
        }
    }
    let built = Arc::new(ink_for(&src.node, pixel));
    if let Ok(mut guard) = cache.lock() {
        guard.insert((src.node.id, src.key, bits), Arc::clone(&built));
    }
    built
}

fn ink_for(node: &Node, pixel: f32) -> StrokeInk {
    let NodeKind::Shape(shape) = &node.kind else {
        return StrokeInk {
            contours: Vec::new(),
            erase: Vec::new(),
        };
    };
    let Some(path) = shape.path.as_ref() else {
        return StrokeInk {
            contours: Vec::new(),
            erase: Vec::new(),
        };
    };
    let tolerance = (pixel as f64 * 0.5).max(0.05);
    StrokeInk {
        contours: super::stamped_contours(node, shape, path, tolerance),
        erase: super::stamped_erase_marks(node, shape, path),
    }
}

fn plain_stamp<'a>(
    app: &SlateApp,
    node: &'a Node,
) -> Option<(&'a ShapeNode, &'a slate_doc::scene::PathData)> {
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    if shape.shape != ShapeKind::Path || slate_doc::scene::shape_hosts_text(shape) {
        return None;
    }
    let path = shape.path.as_deref()?;
    if !shape.stroke.paints_as_stamp() || shape.stroke.is_none() {
        return None;
    }
    // Tiles stamp without a blur pass. A blurred stroke paints on its own,
    // through the same blur the HTML artifact uses.
    if shape.stroke.gaussian_blur > 0.0 {
        return None;
    }
    if (node.opacity - 1.0).abs() > 0.001 {
        return None;
    }
    // A stroke the eraser reached leaves the tiles once its live preview
    // exists (`ensure_erase_live`), not before, so it never goes undrawn.
    if app.board_sel.contains(&node.id) || app.erase_live.contains_key(&node.id) {
        return None;
    }
    if app.shape_properties.preview.iter().any(|p| p.id == node.id) {
        return None;
    }
    if app.brush_live.as_ref().and_then(|c| c.anchor) == Some(node.id) {
        return None;
    }
    Some((shape, path))
}

/// A stamped stroke of any kind, tiled or painted on its own.
fn stamp_stroke(node: &Node) -> Option<&ShapeNode> {
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    (shape.shape == ShapeKind::Path
        && shape.path.is_some()
        && !slate_doc::scene::shape_hosts_text(shape)
        && shape.stroke.paints_as_stamp()
        && !shape.stroke.is_none())
    .then_some(shape)
}

fn ink_rect(node: &Node, shape: &ShapeNode) -> [f32; 4] {
    let pad = shape.stroke.width.max(1.0) * 0.5 + 4.0;
    if node.rotation_deg.abs() < 0.01 {
        let r = node.rect.normalized();
        return [r.x - pad, r.y - pad, r.x + r.w + pad, r.y + r.h + pad];
    }
    let corners = node.rect.corners_rotated(node.rotation_deg);
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (x, y) in corners {
        b[0] = b[0].min(x);
        b[1] = b[1].min(y);
        b[2] = b[2].max(x);
        b[3] = b[3].max(y);
    }
    [b[0] - pad, b[1] - pad, b[2] + pad, b[3] + pad]
}

fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

fn tile_bounds(tx: i32, ty: i32, pixel: f32) -> [f32; 4] {
    let origin = tile_origin(tx, ty, pixel, TILE_PX);
    let span = TILE_PX as f32 * pixel;
    [origin[0], origin[1], origin[0] + span, origin[1] + span]
}

fn view_bounds(screen: egui::Rect, xf: &BoardXf) -> [f32; 4] {
    let a = xf.s2w(screen.min);
    let b = xf.s2w(screen.max);
    [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)]
}

fn tile_span(view: [f32; 4], pixel: f32) -> (i32, i32, i32, i32) {
    (
        tile_index(view[0], pixel, TILE_PX),
        tile_index(view[1], pixel, TILE_PX),
        tile_index(view[2], pixel, TILE_PX),
        tile_index(view[3], pixel, TILE_PX),
    )
}

fn coarser(pixel: f32) -> Option<f32> {
    let c = pixel * 8.0;
    if c > 256.0 || (c - pixel).abs() < f32::EPSILON {
        None
    } else {
        Some(c)
    }
}

/// Paint everything that is not a frame or a connector, batching plain
/// stamp strokes into tiles.
pub(crate) fn paint_rest(
    app: &mut SlateApp,
    ui: &egui::Ui,
    painter: &egui::Painter,
    xf: &BoardXf,
    screen: egui::Rect,
    nodes: &[Node],
) {
    if !app.brush_tiles_enabled {
        for n in nodes
            .iter()
            .filter(|n| !n.is_frame() && !matches!(n.kind, NodeKind::Connector(_)))
        {
            app.paint_board_node(ui, painter, xf, n, true);
        }
        return;
    }

    super::ensure_erase_live(app, painter, xf);
    clear_paint_log();
    let scene_gen = app.scene_gen;
    app.brush_tiles.last.drew_fallback = false;
    app.brush_tiles.last.individuals = 0;
    app.brush_tiles.claimed.clear();
    app.brush_tiles.sync_keys(scene_gen);
    app.brush_tiles.drain_finished();
    crate::app::board::brush_prof::lap("tiles.keys");
    app.brush_tiles.upload_some(painter.ctx());
    crate::app::board::brush_prof::lap("tiles.sync");

    let want = super::stamp_pixel_for_zoom(xf.z, painter.ctx().pixels_per_point());
    let view = view_bounds(screen, xf);
    let frame = app.frame_no;
    let span = tile_span(view, want);
    let doc = app.tab().id;
    if app.brush_tiles.shown_doc != doc {
        app.brush_tiles.shown.clear();
        app.brush_tiles.shown_doc = doc;
    }
    let mut coords = std::mem::take(&mut app.brush_tiles.coords);
    let cells = ((span.2 - span.0 + 1) * (span.3 - span.1 + 1)).max(0) as usize;
    coords.resize_with(cells, CoordFrame::default);
    for c in &mut coords {
        c.settled = true;
        c.replayed = false;
        c.layers.clear();
    }

    // Plan every run first: a coordinate is settled only when every run
    // that reaches it has its exact tile.
    let mut segments = Vec::new();
    let mut index = 0;
    while index < nodes.len() {
        let node = &nodes[index];
        if node.is_frame() || matches!(node.kind, NodeKind::Connector(_)) {
            index += 1;
            continue;
        }
        if plain_stamp(app, node).is_none() {
            segments.push(Segment::Single(index));
            index += 1;
            continue;
        }
        let start = index;
        while index < nodes.len() {
            let n = &nodes[index];
            if n.is_frame() || matches!(n.kind, NodeKind::Connector(_)) {
                index += 1;
                continue;
            }
            if plain_stamp(app, n).is_some() {
                index += 1;
            } else {
                break;
            }
        }
        segments.push(Segment::Run(plan_run(
            app,
            want,
            span,
            nodes,
            start,
            index,
            &mut coords,
        )));
    }

    let pass = Pass {
        ui,
        painter,
        xf,
        view,
        pixel: want,
        frame,
        span,
        nodes,
    };
    for segment in segments {
        match segment {
            Segment::Run(plan) => paint_plan(app, &pass, plan, &mut coords),
            Segment::Single(i) => paint_single(app, &pass, i, &mut coords),
        }
    }
    record_shown(&mut app.brush_tiles, &pass, &coords);
    app.brush_tiles.coords = coords;

    app.brush_tiles.evict(frame);
    let pending = app.brush_tiles.live_jobs.len() + app.brush_tiles.incoming.len();
    let ready = app.brush_tiles.runs.iter().map(|r| r.tiles.len()).sum();
    let gpu_bytes = app.brush_tiles.gpu_bytes();
    let drew_fallback = app.brush_tiles.last.drew_fallback;
    let individuals = app.brush_tiles.last.individuals;
    app.brush_tiles.last = BrushPaintStats {
        gpu_bytes,
        pending_jobs: pending,
        ready_tiles: ready,
        drew_fallback,
        individuals,
        settled: pending == 0 && !drew_fallback && individuals == 0,
        frame,
    };
    if pending > 0 {
        painter.ctx().request_repaint();
    }
}

/// Borrowed state for one board paint.
struct Pass<'a> {
    ui: &'a egui::Ui,
    painter: &'a egui::Painter,
    xf: &'a BoardXf,
    view: [f32; 4],
    pixel: f32,
    frame: u64,
    span: (i32, i32, i32, i32),
    nodes: &'a [Node],
}

impl Pass<'_> {
    fn bits(&self) -> u32 {
        self.pixel.to_bits()
    }

    fn screen_rect(&self, rect: [f32; 4]) -> egui::Rect {
        egui::Rect::from_min_max(
            self.xf.w2s(Pos2::new(rect[0], rect[1])),
            self.xf.w2s(Pos2::new(rect[2], rect[3])),
        )
    }

    /// The board painter, clipped to one tile.
    fn clipped(&self, rect: [f32; 4]) -> egui::Painter {
        self.painter
            .with_clip_rect(self.painter.clip_rect().intersect(self.screen_rect(rect)))
    }
}

/// Visible tile coordinates a world box reaches, inclusive (empty when
/// `x0 > x1`). A hair wider than the box, so `overlaps` stays the judge.
fn reach(span: (i32, i32, i32, i32), pixel: f32, rect: [f32; 4]) -> (i32, i32, i32, i32) {
    let eps = pixel * 0.5;
    (
        tile_index(rect[0] - eps, pixel, TILE_PX).max(span.0),
        tile_index(rect[1] - eps, pixel, TILE_PX).max(span.1),
        tile_index(rect[2] + eps, pixel, TILE_PX).min(span.2),
        tile_index(rect[3] + eps, pixel, TILE_PX).min(span.3),
    )
}

fn slot_coord(span: (i32, i32, i32, i32), slot: usize) -> (i32, i32) {
    let cols = (span.2 - span.0 + 1) as usize;
    (span.0 + (slot % cols) as i32, span.1 + (slot / cols) as i32)
}

/// Match a run of plain stamps to its cache and judge each visible tile.
/// Tiles that are not exact are queued here and unsettle their coordinate.
fn plan_run(
    app: &mut SlateApp,
    pixel: f32,
    span: (i32, i32, i32, i32),
    nodes: &[Node],
    start: usize,
    end: usize,
    coords: &mut [CoordFrame],
) -> RunPlan {
    let mut prep = Vec::with_capacity(end - start);
    for node in &nodes[start..end] {
        if node.is_frame() || matches!(node.kind, NodeKind::Connector(_)) {
            continue;
        }
        let Some((shape, path)) = plain_stamp(app, node) else {
            continue;
        };
        let key = app.brush_tiles.content_key(node, shape, path);
        let bounds = ink_rect(node, shape);
        let src = app.brush_tiles.src_for(node, key);
        prep.push(Prepared {
            id: node.id,
            key,
            bounds,
            src,
        });
    }
    let ids: Vec<NodeId> = prep.iter().map(|p| p.id).collect();
    let keys: Vec<u64> = prep.iter().map(|p| p.key).collect();
    crate::app::board::brush_prof::lap("tiles.prep");

    let doc = app.tab().id;
    let token = match_run(&mut app.brush_tiles, doc, &ids, &keys);
    crate::app::board::brush_prof::lap("tiles.match");
    let bits = pixel.to_bits();
    let sig = run_signature(&ids, &keys);
    let fast = app
        .brush_tiles
        .runs
        .iter()
        .find(|r| r.token == token)
        .is_some_and(|r| r.validated.is_some_and(|v| v.covers(sig, bits, span)));
    let mut plan = RunPlan {
        token,
        start,
        end,
        fast,
        prep: Vec::new(),
        cells: Vec::new(),
        sig,
        ids,
        keys,
        missing: false,
    };
    if fast {
        return plan;
    }
    let bins = bin_by_tile(&prep, pixel, span);
    for ty in span.1..=span.3 {
        for tx in span.0..=span.2 {
            let bounds = tile_bounds(tx, ty, pixel);
            let slot = bin_slot(span, tx, ty);
            let bin = &bins[slot];
            let desired: Vec<(NodeId, u64)> = bin
                .iter()
                .map(|&i| &prep[i])
                .filter(|p| overlaps(p.bounds, bounds))
                .map(|p| (p.id, p.key))
                .collect();
            if desired.is_empty() {
                continue;
            }
            let fit = app
                .brush_tiles
                .runs
                .iter()
                .find(|r| r.token == token)
                .and_then(|r| r.tiles.get(&(bits, tx, ty)))
                .map(|t| {
                    if t.baked == desired {
                        TileFit::Exact
                    } else if desired.starts_with(&t.baked) {
                        TileFit::Prefix(t.baked.len())
                    } else {
                        TileFit::Stale
                    }
                })
                .unwrap_or(TileFit::Missing);
            if fit != TileFit::Exact {
                plan.missing = true;
                coords[slot].settled = false;
                let queued = app
                    .brush_tiles
                    .queued
                    .get(&(token, bits, tx, ty))
                    .is_some_and(|q| q == &desired);
                if !queued {
                    let skip = match fit {
                        TileFit::Prefix(n) => n,
                        _ => 0,
                    };
                    let base = if skip > 0 {
                        app.brush_tiles
                            .runs
                            .iter()
                            .find(|r| r.token == token)
                            .and_then(|r| r.tiles.get(&(bits, tx, ty)))
                            .map(|t| t.cpu.clone())
                    } else {
                        None
                    };
                    let incremental = base.is_some();
                    let strokes = bin
                        .iter()
                        .map(|&i| &prep[i])
                        .filter(|p| overlaps(p.bounds, bounds))
                        .skip(skip)
                        .map(|p| Arc::clone(&p.src))
                        .collect();
                    let id = app.brush_tiles.next_job;
                    app.brush_tiles.next_job = app.brush_tiles.next_job.wrapping_add(1);
                    let ink = Arc::clone(&app.brush_tiles.ink);
                    app.brush_tiles.enqueue(Job {
                        id,
                        token,
                        pixel,
                        tx,
                        ty,
                        base,
                        strokes,
                        baked: desired.clone(),
                        incremental,
                        ink,
                    });
                }
            }
            plan.cells.push(Cell {
                slot,
                tx,
                ty,
                fit,
                desired,
            });
        }
    }
    plan.prep = prep;
    plan
}

/// Paint a run's own tile at `key`, recording it when the coordinate is
/// settled. Returns false when the run has no tile there.
fn paint_run_tile(
    app: &mut SlateApp,
    pass: &Pass,
    token: u64,
    key: TileCoord,
    slot: usize,
    coords: &mut [CoordFrame],
) -> bool {
    let Some(run) = app.brush_tiles.runs.iter_mut().find(|r| r.token == token) else {
        return false;
    };
    let Some(tile) = run.tiles.get_mut(&key) else {
        return false;
    };
    tile.used = pass.frame;
    paint_tile(pass.painter, pass.xf, tile);
    if coords[slot].settled {
        coords[slot].layers.push(LayerKey::Tile {
            token,
            tex: tile.tex.id(),
        });
    }
    true
}

/// Paint what `key` showed when it last settled, once per frame, at the
/// first thing painted there.
fn replay(app: &mut SlateApp, pass: &Pass, key: TileCoord, slot: usize, coords: &mut [CoordFrame]) {
    if coords[slot].replayed {
        return;
    }
    coords[slot].replayed = true;
    let Some(shown) = app.brush_tiles.shown.remove(&key) else {
        return;
    };
    let rect = tile_bounds(key.1, key.2, pass.pixel);
    for layer in &shown.layers {
        match layer {
            ShownLayer::Tile { tex, ids, .. } => {
                note_paint(rect, ids.iter().copied());
                pass.painter.image(
                    tex.id(),
                    pass.screen_rect(rect),
                    egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            ShownLayer::Node(id) => {
                if let Some(node) = pass.nodes.iter().find(|n| n.id == *id) {
                    note_paint(rect, std::iter::once(*id));
                    app.paint_board_node(pass.ui, &pass.clipped(rect), pass.xf, node, true);
                }
            }
        }
    }
    app.brush_tiles.shown.insert(key, shown);
}

/// The stand-in at `key` is live this frame and already shows `id`.
fn stand_in_shows(
    app: &SlateApp,
    coords: &[CoordFrame],
    slot: usize,
    key: TileCoord,
    id: NodeId,
) -> bool {
    !coords[slot].settled
        && app
            .brush_tiles
            .shown
            .get(&key)
            .is_some_and(|s| s.ids.binary_search(&id).is_ok())
}

/// Paint one planned run in place. A settled coordinate paints the run's
/// exact tile. An unsettled one paints its stand-in once, then only the
/// strokes the stand-in does not show yet, clipped to that coordinate.
fn paint_plan(app: &mut SlateApp, pass: &Pass, plan: RunPlan, coords: &mut [CoordFrame]) {
    let bits = pass.bits();
    let token = plan.token;
    let mut fresh: Vec<(NodeId, usize)> = Vec::new();
    let mut absent: Vec<usize> = Vec::new();
    if plan.fast {
        for ty in pass.span.1..=pass.span.3 {
            for tx in pass.span.0..=pass.span.2 {
                let key = (bits, tx, ty);
                let slot = bin_slot(pass.span, tx, ty);
                if coords[slot].settled || !app.brush_tiles.shown.contains_key(&key) {
                    paint_run_tile(app, pass, token, key, slot, coords);
                    continue;
                }
                let Some(baked) = app
                    .brush_tiles
                    .runs
                    .iter()
                    .find(|r| r.token == token)
                    .and_then(|r| r.tiles.get(&key))
                    .map(|t| t.baked.iter().map(|(id, _)| *id).collect::<Vec<_>>())
                else {
                    continue;
                };
                replay(app, pass, key, slot, coords);
                for id in baked {
                    if !stand_in_shows(app, coords, slot, key, id) {
                        fresh.push((id, slot));
                    }
                }
            }
        }
        crate::app::board::brush_prof::lap("tiles.fast");
    } else {
        for (ci, cell) in plan.cells.iter().enumerate() {
            let key = (bits, cell.tx, cell.ty);
            if coords[cell.slot].settled {
                paint_run_tile(app, pass, token, key, cell.slot, coords);
            } else if app.brush_tiles.shown.contains_key(&key) {
                replay(app, pass, key, cell.slot, coords);
                for (id, _) in &cell.desired {
                    if !stand_in_shows(app, coords, cell.slot, key, *id) {
                        fresh.push((*id, cell.slot));
                    }
                }
            } else if cell.fit == TileFit::Missing {
                absent.push(ci);
            } else {
                // First sight of this coordinate at this zoom: the run's own
                // older tile, plus the strokes it does not show.
                paint_run_tile(app, pass, token, key, cell.slot, coords);
                let shown: Vec<(NodeId, u64)> = app
                    .brush_tiles
                    .runs
                    .iter()
                    .find(|r| r.token == token)
                    .and_then(|r| r.tiles.get(&key))
                    .map(|t| t.baked.clone())
                    .unwrap_or_default();
                for pair in &cell.desired {
                    if !shown.contains(pair) {
                        fresh.push((pair.0, cell.slot));
                    }
                }
            }
        }
    }

    // Another zoom level stands in only for coordinates with nothing to show.
    let mut drew_fallback = false;
    if !absent.is_empty() {
        for fallback in fallback_pixels(app, token, pass.pixel) {
            if draw_level(
                app,
                pass.painter,
                pass.xf,
                pass.view,
                fallback,
                pass.frame,
                &plan.prep,
                token,
                true,
            ) {
                drew_fallback = true;
                break;
            }
        }
        if let Some(coarse) = coarser(pass.pixel) {
            ensure_level(app, pass.view, coarse, token, &plan.prep);
        }
        if !drew_fallback {
            for &ci in &absent {
                let cell = &plan.cells[ci];
                fresh.extend(cell.desired.iter().map(|(id, _)| (*id, cell.slot)));
            }
        }
    }
    let individuals = if fresh.is_empty() {
        0
    } else {
        paint_fresh(app, pass, &plan, &fresh)
    };
    app.brush_tiles.last.drew_fallback |= drew_fallback;
    app.brush_tiles.last.individuals += individuals;
    if let Some(run) = app.brush_tiles.runs.iter_mut().find(|r| r.token == token) {
        // Grow the cached stroke list when new strokes appear. A cull is the
        // other way around and must not throw the list away. A restyle keeps
        // the strokes and takes their new keys.
        run.validated = (!plan.missing).then_some(Validated {
            sig: plan.sig,
            bits,
            span: pass.span,
        });
        if id_subsequence(&plan.ids, &run.ids) {
            run.ids = plan.ids;
            run.keys = plan.keys;
        }
        run.used = pass.frame;
    }
}

/// Paint strokes no tile here shows yet, each clipped to the coordinates
/// that need it, newest first. A stroke whose stamp bitmap is current always
/// paints; at most `IMMEDIATE_STROKES` new bitmaps build per run and frame,
/// so a full board never rasterizes on the frame loop. Returns how many
/// strokes painted.
fn paint_fresh(
    app: &mut SlateApp,
    pass: &Pass,
    plan: &RunPlan,
    fresh: &[(NodeId, usize)],
) -> usize {
    let mut slots: HashMap<NodeId, Vec<usize>> = HashMap::new();
    for (id, slot) in fresh {
        let at = slots.entry(*id).or_default();
        if !at.contains(slot) {
            at.push(*slot);
        }
    }
    let mut built = 0;
    let mut painted = 0;
    for node in pass.nodes[plan.start..plan.end].iter().rev() {
        let Some(at) = slots.get(&node.id) else {
            continue;
        };
        let NodeKind::Shape(shape) = &node.kind else {
            continue;
        };
        let Some(path) = shape.path.as_ref() else {
            continue;
        };
        let key = app.brush_tiles.keys.get(&node.id).copied();
        let current = key.is_some_and(|k| {
            app.brush_stamps
                .get(&node.id)
                .is_some_and(|(cached, gpu)| *cached == k && gpu.wanted_pixel == pass.pixel)
        });
        if !current {
            if built >= IMMEDIATE_STROKES {
                continue;
            }
            built += 1;
        }
        painted += 1;
        let fade = fade_of(node);
        for &slot in at {
            let (tx, ty) = slot_coord(pass.span, slot);
            let rect = tile_bounds(tx, ty, pass.pixel);
            note_paint(rect, std::iter::once(node.id));
            paint_path_shape(app, &pass.clipped(rect), pass.xf, node, shape, path, &fade);
        }
    }
    painted
}

/// A node outside the tiles. A stamped stroke skips coordinates whose
/// stand-in already shows it, so it is never drawn twice or dropped.
fn paint_single(app: &mut SlateApp, pass: &Pass, index: usize, coords: &mut [CoordFrame]) {
    let node = &pass.nodes[index];
    let Some(shape) = stamp_stroke(node) else {
        app.paint_board_node(pass.ui, pass.painter, pass.xf, node, true);
        return;
    };
    let ink = ink_rect(node, shape);
    let bits = pass.bits();
    let (x0, y0, x1, y1) = reach(pass.span, pass.pixel, ink);
    let mut blocked = false;
    for ty in y0..=y1 {
        for tx in x0..=x1 {
            let slot = bin_slot(pass.span, tx, ty);
            let key = (bits, tx, ty);
            if coords[slot].settled
                || !overlaps(ink, tile_bounds(tx, ty, pass.pixel))
                || !app.brush_tiles.shown.contains_key(&key)
            {
                continue;
            }
            replay(app, pass, key, slot, coords);
            blocked |= stand_in_shows(app, coords, slot, key, node.id);
        }
    }
    if blocked {
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let slot = bin_slot(pass.span, tx, ty);
                let bounds = tile_bounds(tx, ty, pass.pixel);
                if !overlaps(ink, bounds)
                    || stand_in_shows(app, coords, slot, (bits, tx, ty), node.id)
                {
                    continue;
                }
                note_paint(bounds, std::iter::once(node.id));
                app.paint_board_node(pass.ui, &pass.clipped(bounds), pass.xf, node, true);
            }
        }
    } else {
        note_paint(ink, std::iter::once(node.id));
        app.paint_board_node(pass.ui, pass.painter, pass.xf, node, true);
    }
    for ty in y0..=y1 {
        for tx in x0..=x1 {
            let slot = bin_slot(pass.span, tx, ty);
            if coords[slot].settled && overlaps(ink, tile_bounds(tx, ty, pass.pixel)) {
                coords[slot].layers.push(LayerKey::Node(node.id));
            }
        }
    }
}

/// Remember what each settled coordinate painted, for the frames after the
/// next change there. Coordinates that left the view are forgotten.
fn record_shown(cache: &mut BrushTiles, pass: &Pass, coords: &[CoordFrame]) {
    let bits = pass.bits();
    for ty in pass.span.1..=pass.span.3 {
        for tx in pass.span.0..=pass.span.2 {
            let slot = bin_slot(pass.span, tx, ty);
            let key = (bits, tx, ty);
            let now = &coords[slot];
            if let Some(shown) = cache.shown.get_mut(&key) {
                let same = shown.layers.len() == now.layers.len()
                    && shown
                        .layers
                        .iter()
                        .zip(&now.layers)
                        .all(|(a, b)| a.key() == *b);
                if !now.settled || same {
                    shown.seen = pass.frame;
                    continue;
                }
            } else if !now.settled {
                continue;
            }
            let mut layers = Vec::with_capacity(now.layers.len());
            let mut ids = Vec::new();
            for layer in &now.layers {
                match *layer {
                    LayerKey::Tile { token, .. } => {
                        let Some(tile) = cache
                            .runs
                            .iter()
                            .find(|r| r.token == token)
                            .and_then(|r| r.tiles.get(&key))
                        else {
                            continue;
                        };
                        let tile_ids: Vec<NodeId> = tile.baked.iter().map(|(id, _)| *id).collect();
                        ids.extend_from_slice(&tile_ids);
                        layers.push(ShownLayer::Tile {
                            token,
                            tex: tile.tex.clone(),
                            ids: tile_ids,
                        });
                    }
                    LayerKey::Node(id) => {
                        ids.push(id);
                        layers.push(ShownLayer::Node(id));
                    }
                }
            }
            ids.sort_unstable();
            ids.dedup();
            cache.shown.insert(
                key,
                Shown {
                    seen: pass.frame,
                    layers,
                    ids,
                },
            );
        }
    }
    cache.shown.retain(|_, s| s.seen == pass.frame);
}

fn run_signature(ids: &[NodeId], keys: &[u64]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ids.hash(&mut h);
    keys.hash(&mut h);
    h.finish()
}

fn bin_slot(span: (i32, i32, i32, i32), tx: i32, ty: i32) -> usize {
    let cols = (span.2 - span.0 + 1) as usize;
    (ty - span.1) as usize * cols + (tx - span.0) as usize
}

/// Candidate strokes per visible tile, in paint order. Each stroke lands in
/// every tile its bounds reach (a hair wider, so `overlaps` stays the judge).
fn bin_by_tile(prep: &[Prepared], pixel: f32, span: (i32, i32, i32, i32)) -> Vec<Vec<usize>> {
    let cols = (span.2 - span.0 + 1) as usize;
    let rows = (span.3 - span.1 + 1) as usize;
    let mut bins = vec![Vec::new(); cols * rows];
    let eps = pixel * 0.5;
    for (i, p) in prep.iter().enumerate() {
        let x0 = tile_index(p.bounds[0] - eps, pixel, TILE_PX).max(span.0);
        let y0 = tile_index(p.bounds[1] - eps, pixel, TILE_PX).max(span.1);
        let x1 = tile_index(p.bounds[2] + eps, pixel, TILE_PX).min(span.2);
        let y1 = tile_index(p.bounds[3] + eps, pixel, TILE_PX).min(span.3);
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                bins[bin_slot(span, tx, ty)].push(i);
            }
        }
    }
    bins
}

fn fallback_pixels(app: &SlateApp, token: u64, current: f32) -> Vec<f32> {
    let mut pixels = Vec::new();
    let Some(run) = app.brush_tiles.runs.iter().find(|r| r.token == token) else {
        return pixels;
    };
    for tile in run.tiles.values() {
        if tile.pixel.to_bits() != current.to_bits()
            && !pixels
                .iter()
                .any(|p: &f32| p.to_bits() == tile.pixel.to_bits())
        {
            pixels.push(tile.pixel);
        }
    }
    pixels.sort_by(|a, b| {
        (*a - current)
            .abs()
            .partial_cmp(&(*b - current).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    pixels
}

fn fade_of(node: &Node) -> impl Fn(Color32) -> Color32 {
    let alpha = node.opacity.clamp(0.0, 1.0);
    move |c: Color32| c.gamma_multiply(alpha)
}

/// `needle` appears inside `hay` in order, with the same content keys.
fn subsequence(
    hay_ids: &[NodeId],
    hay_keys: &[u64],
    needle_ids: &[NodeId],
    needle_keys: &[u64],
) -> bool {
    let mut i = 0;
    for (id, key) in hay_ids.iter().zip(hay_keys) {
        if i < needle_ids.len() && *id == needle_ids[i] && *key == needle_keys[i] {
            i += 1;
        }
    }
    i == needle_ids.len()
}

fn covers(baked: &[(NodeId, u64)], desired: &[(NodeId, u64)]) -> bool {
    let mut i = 0;
    for pair in baked {
        if i < desired.len() && *pair == desired[i] {
            i += 1;
        }
    }
    i == desired.len()
}

/// `needle` appears inside `hay` in order, whatever the content keys.
fn id_subsequence(hay: &[NodeId], needle: &[NodeId]) -> bool {
    let mut i = 0;
    for id in hay {
        if i < needle.len() && *id == needle[i] {
            i += 1;
        }
    }
    i == needle.len()
}

/// The run cache for these strokes. Same strokes with the same content come
/// first; failing that, the same strokes in order whose content changed (a
/// restyle), so their tiles keep painting until the new raster lands.
fn match_run(cache: &mut BrushTiles, doc: u64, ids: &[NodeId], keys: &[u64]) -> u64 {
    let open = |r: &&RunCache| r.doc == doc && !cache.claimed.contains(&r.token);
    let found = cache
        .runs
        .iter()
        .filter(open)
        .find(|r| {
            subsequence(&r.ids, &r.keys, ids, keys) || subsequence(ids, keys, &r.ids, &r.keys)
        })
        .or_else(|| {
            cache
                .runs
                .iter()
                .filter(open)
                .find(|r| id_subsequence(&r.ids, ids) || id_subsequence(ids, &r.ids))
        })
        .map(|r| r.token);
    if let Some(token) = found {
        cache.claimed.push(token);
        return token;
    }
    let token = cache.next_token;
    cache.next_token = cache.next_token.wrapping_add(1);
    cache.claimed.push(token);
    cache.runs.push(RunCache {
        token,
        doc,
        ids: ids.to_vec(),
        keys: keys.to_vec(),
        tiles: HashMap::new(),
        used: 0,
        validated: None,
    });
    token
}

#[allow(clippy::too_many_arguments)]
fn draw_level(
    app: &mut SlateApp,
    painter: &egui::Painter,
    xf: &BoardXf,
    view: [f32; 4],
    pixel: f32,
    frame: u64,
    prep: &[Prepared],
    token: u64,
    allow_superset: bool,
) -> bool {
    let (tx0, ty0, tx1, ty1) = tile_span(view, pixel);
    let mut any = false;
    let mut all = true;
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let bounds = tile_bounds(tx, ty, pixel);
            let desired: Vec<(NodeId, u64)> = prep
                .iter()
                .filter(|p| overlaps(p.bounds, bounds))
                .map(|p| (p.id, p.key))
                .collect();
            if desired.is_empty() {
                continue;
            }
            let exact = app
                .brush_tiles
                .runs
                .iter()
                .find(|r| r.token == token)
                .and_then(|r| r.tiles.get(&(pixel.to_bits(), tx, ty)))
                .is_some_and(|t| {
                    t.baked == desired || (allow_superset && covers(&t.baked, &desired))
                });
            if !exact {
                all = false;
            }
        }
    }
    if !all {
        return false;
    }
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let Some(run) = app.brush_tiles.runs.iter_mut().find(|r| r.token == token) else {
                continue;
            };
            let Some(tile) = run.tiles.get_mut(&(pixel.to_bits(), tx, ty)) else {
                continue;
            };
            tile.used = frame;
            paint_tile(painter, xf, tile);
            any = true;
        }
    }
    any
}

fn ensure_level(app: &mut SlateApp, view: [f32; 4], pixel: f32, token: u64, prep: &[Prepared]) {
    let (tx0, ty0, tx1, ty1) = tile_span(view, pixel);
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let bounds = tile_bounds(tx, ty, pixel);
            let mut desired = Vec::new();
            let mut srcs = Vec::new();
            for p in prep {
                if overlaps(p.bounds, bounds) {
                    desired.push((p.id, p.key));
                    srcs.push(Arc::clone(&p.src));
                }
            }
            if desired.is_empty() {
                continue;
            }
            let have = app
                .brush_tiles
                .runs
                .iter()
                .find(|r| r.token == token)
                .and_then(|r| r.tiles.get(&(pixel.to_bits(), tx, ty)))
                .is_some_and(|t| t.baked == desired);
            let queued = app
                .brush_tiles
                .queued
                .get(&(token, pixel.to_bits(), tx, ty))
                .is_some_and(|q| q == &desired);
            if have || queued {
                continue;
            }
            let id = app.brush_tiles.next_job;
            app.brush_tiles.next_job = app.brush_tiles.next_job.wrapping_add(1);
            let ink = Arc::clone(&app.brush_tiles.ink);
            app.brush_tiles.enqueue(Job {
                id,
                token,
                pixel,
                tx,
                ty,
                base: None,
                strokes: srcs,
                baked: desired,
                incremental: false,
                ink,
            });
        }
    }
}

fn paint_tile(painter: &egui::Painter, xf: &BoardXf, tile: &GpuTile) {
    note_paint(
        tile_bounds(tile.tx, tile.ty, tile.pixel),
        tile.baked.iter().map(|(id, _)| *id),
    );
    let origin = tile_origin(tile.tx, tile.ty, tile.pixel, TILE_PX);
    let span = TILE_PX as f32 * tile.pixel;
    let min = xf.w2s(Pos2::new(origin[0], origin[1]));
    let max = xf.w2s(Pos2::new(origin[0] + span, origin[1] + span));
    painter.image(
        tile.tex.id(),
        egui::Rect::from_min_max(min, max),
        egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
}
