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

use super::super::board::{BoardDrag, BoardXf};
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
    used: u64,
    pixel: f32,
    tx: i32,
    ty: i32,
}

struct RunCache {
    token: u64,
    ids: Vec<NodeId>,
    keys: Vec<u64>,
    tiles: HashMap<TileCoord, GpuTile>,
    used: u64,
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

pub(crate) struct BrushTiles {
    keys: HashMap<NodeId, u64>,
    keyed_gen: u64,
    dirty_ids: Vec<NodeId>,
    dirty_all: bool,
    specified: bool,
    runs: Vec<RunCache>,
    next_token: u64,
    next_job: u64,
    live_jobs: HashSet<u64>,
    queued: QueuedTiles,
    /// Incremental jobs still rasterizing, keyed by run token and pixel.
    inflight: Inflight,
    stash: Vec<Finished>,
    srcs: HashMap<NodeId, Arc<StrokeSrc>>,
    scratch_desired: Vec<(NodeId, u64)>,
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
            next_token: 1,
            next_job: 1,
            live_jobs: HashSet::new(),
            queued: HashMap::new(),
            inflight: HashMap::new(),
            stash: Vec::new(),
            srcs: HashMap::new(),
            scratch_desired: Vec::new(),
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

    /// Scene changed but the caller did not name nodes (undo, tab switch).
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
            self.keys.clear();
            self.srcs.clear();
            self.runs.clear();
            self.live_jobs.clear();
            self.queued.clear();
            self.inflight.clear();
            self.stash.clear();
            self.incoming.clear();
            if let Ok(mut ink) = self.ink.lock() {
                ink.clear();
            }
        } else {
            for id in &self.dirty_ids {
                self.keys.remove(id);
                self.srcs.remove(id);
            }
            // In-flight rasters still show the previous strokes. Drop them.
            self.live_jobs.clear();
            self.queued.clear();
            self.inflight.clear();
            self.stash.clear();
        }
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
        let key = path_content_hash(path, &shape.stroke, node.rect, node.rotation_deg, 0) ^ 0x57A5;
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
            self.queued
                .remove(&(fin.token, fin.pixel.to_bits(), fin.tx, fin.ty));
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
            self.queued
                .remove(&(fin.token, fin.pixel.to_bits(), fin.tx, fin.ty));
            self.install(ctx, fin);
        }
        self.stash = keep;
    }

    fn install(&mut self, ctx: &egui::Context, fin: Finished) {
        let Some(run) = self.runs.iter_mut().find(|r| r.token == fin.token) else {
            return;
        };
        let image = egui::ColorImage::from_rgba_premultiplied(
            [TILE_PX as usize, TILE_PX as usize],
            &super::premultiplied(&fin.rgba),
        );
        let tex = ctx.load_texture(
            format!("brush-tile-{}-{}-{}", fin.token, fin.tx, fin.ty),
            image,
            egui::TextureOptions::LINEAR,
        );
        run.tiles.insert(
            (fin.pixel.to_bits(), fin.tx, fin.ty),
            GpuTile {
                tex,
                cpu: fin.rgba,
                baked: fin.baked,
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
        if done
            .send(Finished {
                id: job.id,
                token: job.token,
                pixel: job.pixel,
                tx: job.tx,
                ty: job.ty,
                rgba: img.rgba,
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
    let Some(path) = shape.path.as_ref() else {
        return None;
    };
    if !shape.stroke.paints_as_stamp() || shape.stroke.is_none() {
        return None;
    }
    if (node.opacity - 1.0).abs() > 0.001 {
        return None;
    }
    if app.board_sel.contains(&node.id) || app.erase_live.contains_key(&node.id) {
        return None;
    }
    if let Some(BoardDrag::Erase { spot, .. }) = &app.board_drag {
        if spot.contains(&node.id) {
            return None;
        }
    }
    if app.shape_properties.preview.iter().any(|p| p.id == node.id) {
        return None;
    }
    if app.brush_live.as_ref().and_then(|c| c.anchor) == Some(node.id) {
        return None;
    }
    Some((shape, path))
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

    let scene_gen = app.scene_gen;
    app.brush_tiles.last.drew_fallback = false;
    app.brush_tiles.last.individuals = 0;
    app.brush_tiles.sync_keys(scene_gen);
    app.brush_tiles.drain_finished();
    app.brush_tiles.upload_some(painter.ctx());

    let want = super::stamp_pixel_for_zoom(xf.z, painter.ctx().pixels_per_point());
    let view = view_bounds(screen, xf);
    let frame = app.frame_no;

    let mut index = 0;
    while index < nodes.len() {
        let node = &nodes[index];
        if node.is_frame() || matches!(node.kind, NodeKind::Connector(_)) {
            index += 1;
            continue;
        }
        if plain_stamp(app, node).is_none() {
            app.paint_board_node(ui, painter, xf, node, true);
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
        paint_run(
            app,
            ui,
            painter,
            xf,
            view,
            want,
            frame,
            &nodes[start..index],
        );
    }

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
    };
    if pending > 0 {
        painter.ctx().request_repaint();
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_run(
    app: &mut SlateApp,
    _ui: &egui::Ui,
    painter: &egui::Painter,
    xf: &BoardXf,
    view: [f32; 4],
    pixel: f32,
    frame: u64,
    nodes: &[Node],
) {
    let mut prep = Vec::with_capacity(nodes.len());
    for node in nodes {
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

    let token = match_run(&mut app.brush_tiles, &ids, &keys);
    let mut drew_fallback = false;
    let mut individuals = 0usize;

    let (tx0, ty0, tx1, ty1) = tile_span(view, pixel);
    let mut missing = false;
    let mut covered: HashSet<NodeId> = HashSet::new();
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let bounds = tile_bounds(tx, ty, pixel);
            let mut desired = std::mem::take(&mut app.brush_tiles.scratch_desired);
            desired.clear();
            for p in &prep {
                if overlaps(p.bounds, bounds) {
                    desired.push((p.id, p.key));
                }
            }
            if desired.is_empty() {
                app.brush_tiles.scratch_desired = desired;
                continue;
            }
            let bits = pixel.to_bits();
            let kind = app
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
            if kind == TileFit::Exact {
                for (id, _) in &desired {
                    covered.insert(*id);
                }
                if let Some(run) = app.brush_tiles.runs.iter_mut().find(|r| r.token == token) {
                    if let Some(tile) = run.tiles.get_mut(&(bits, tx, ty)) {
                        tile.used = frame;
                        paint_tile(painter, xf, tile);
                    }
                }
                app.brush_tiles.scratch_desired = desired;
                continue;
            }
            missing = true;
            if let TileFit::Prefix(_) = kind {
                if let Some(run) = app.brush_tiles.runs.iter_mut().find(|r| r.token == token) {
                    if let Some(tile) = run.tiles.get_mut(&(bits, tx, ty)) {
                        tile.used = frame;
                        paint_tile(painter, xf, tile);
                    }
                }
            }
            let queued = app
                .brush_tiles
                .queued
                .get(&(token, bits, tx, ty))
                .is_some_and(|q| q == &desired);
            if queued {
                app.brush_tiles.scratch_desired = desired;
                continue;
            }
            {
                let skip = match kind {
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
                let strokes = prep
                    .iter()
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
                    baked: desired,
                    incremental,
                    ink,
                });
            }
        }
    }

    if missing {
        for fallback in fallback_pixels(app, token, pixel) {
            if draw_level(app, painter, xf, view, fallback, frame, &prep, token, true) {
                drew_fallback = true;
                break;
            }
        }
        if let Some(coarse) = coarser(pixel) {
            ensure_level(app, view, coarse, token, &prep);
        }
    }

    let fresh: Vec<NodeId> = if missing && !drew_fallback {
        prep.iter()
            .rev()
            .filter(|p| !covered.contains(&p.id))
            .take(IMMEDIATE_STROKES)
            .map(|p| p.id)
            .collect()
    } else {
        Vec::new()
    };
    if !fresh.is_empty() && fresh.len() <= IMMEDIATE_STROKES {
        individuals = fresh.len();
        for node in nodes {
            if fresh.contains(&node.id) {
                let fade = fade_of(node);
                if let NodeKind::Shape(shape) = &node.kind {
                    if let Some(path) = shape.path.as_ref() {
                        paint_path_shape(app, painter, xf, node, shape, path, &fade);
                    }
                }
            }
        }
    }

    if drew_fallback {
        individuals = 0;
    }
    app.brush_tiles.last.drew_fallback |= drew_fallback;
    app.brush_tiles.last.individuals += individuals;
    if let Some(run) = app.brush_tiles.runs.iter_mut().find(|r| r.token == token) {
        // Grow the cached stroke list when new strokes appear. A cull is the
        // other way around and must not throw the list away.
        if subsequence(&ids, &keys, &run.ids, &run.keys) {
            run.ids = ids;
            run.keys = keys;
        }
        run.used = frame;
    }
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

fn match_run(cache: &mut BrushTiles, ids: &[NodeId], keys: &[u64]) -> u64 {
    if let Some(run) = cache.runs.iter().find(|r| {
        subsequence(&r.ids, &r.keys, ids, keys) || subsequence(ids, keys, &r.ids, &r.keys)
    }) {
        return run.token;
    }
    let token = cache.next_token;
    cache.next_token = cache.next_token.wrapping_add(1);
    cache.runs.push(RunCache {
        token,
        ids: ids.to_vec(),
        keys: keys.to_vec(),
        tiles: HashMap::new(),
        used: 0,
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
