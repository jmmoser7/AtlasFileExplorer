//! Interactive 3D viewports for placed Rhino models (`MediaKind::Model`).
//!
//! A `.3dm` item placed on the board is a **viewport node**: its saved
//! [`ModelCamera`] pose (journaled document state on the `ImageNode`) decides
//! which view of the model the node shows. The lifecycle keeps big models
//! cheap by default:
//!
//! - **Locked (default).** The node paints a *poster* — a PNG rendered from
//!   the saved camera pose, cached on disk next to the thumbnail cache. No
//!   mesh, no GPU buffers, no per-frame work. Duplicating the node and
//!   changing each copy's camera is how one model appears from several
//!   perspectives across slides.
//! - **Unlocked (double-click the node, or hover → padlock).** The mesh is
//!   parsed off-thread
//!   (`rhino-mesh` reads the render meshes Rhino cached into the file),
//!   uploaded to the GPU, and rendered live with Rhino-style controls:
//!   drag = orbit, Shift+drag = pan, scroll = zoom. At most [`MAX_LIVE`]
//!   viewports stay live; unlocking more locks the least-recently-used one.
//! - **Auto-lock.** A live viewport idle for [`AUTO_LOCK`] locks itself:
//!   the current framebuffer pose is written out as the new poster, the
//!   camera is committed to the document as one undoable patch, and the GPU
//!   resources are released.
//!
//! Rendering happens **offscreen inside `update`** (the glow GL context is
//! current there): the scene is drawn into an MSAA framebuffer, resolved,
//! read back, and handed to egui as an ordinary texture — so model nodes go
//! through the exact same `textured_polygon` path as every other board
//! image (stroke, corners, opacity all apply), and the headless test
//! harness (no GL) simply never sees a live viewport.
//!
//! Files without cached render meshes ("Save Small", wireframe-only saves)
//! degrade to the embedded-preview thumbnail the thumb pool already
//! extracts — same look as before this feature existed.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, TextureHandle};
use eframe::glow::{self, HasContext};
use slate_doc::scene::{ModelCamera, NodeKind};
use slate_doc::NodeId;

use super::SlateApp;

/// Idle time after which a live viewport locks itself back down.
pub const AUTO_LOCK: Duration = Duration::from_secs(30);
/// Maximum simultaneously-live viewports; unlocking more locks the oldest.
pub const MAX_LIVE: usize = 3;
/// Parsed CPU meshes kept around (beyond those needed by live viewports).
const MAX_CPU_MODELS: usize = 4;
/// GPU buffers for files with no live viewport are freed after this.
const GPU_LINGER: Duration = Duration::from_secs(10);
/// MSAA samples for the offscreen render (presentation-quality edges).
const MSAA_SAMPLES: i32 = 4;
/// Live render resolution cap (long edge, physical px).
const MAX_RENDER_PX: u32 = 1920;
/// Poster render resolution (long edge, physical px).
const POSTER_LONG_EDGE: u32 = 1600;
/// Vertical field of view, radians (≈ Rhino's default perspective lens).
pub const FOV_Y: f32 = 0.6108652; // 35°
/// Orbit sensitivity, radians per screen px.
const ORBIT_PER_PX: f32 = 0.008;

// ---------- viewport tools ----------

/// Active tool inside a live 3D viewport. Navigation matches Rhino's default
/// perspective viewport; measure tools own primary clicks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ModelViewportTool {
    #[default]
    Navigate,
    /// Rhino `Distance` — direct line between two picked surface points.
    MeasureDistance,
}

/// One completed point-to-point measurement (session-local until lock).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistanceMeasurement {
    pub a: [f32; 3],
    pub b: [f32; 3],
}

impl DistanceMeasurement {
    pub fn length(&self) -> f32 {
        distance_3d(self.a, self.b)
    }
}

fn distance_3d(a: [f32; 3], b: [f32; 3]) -> f32 {
    v_sub(b, a).map(|x| x * x).into_iter().sum::<f32>().sqrt()
}

// ---------- camera math (pure, unit-tested) ----------

fn v_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn v_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn v_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn v_norm(a: [f32; 3]) -> [f32; 3] {
    let l = v_dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// Eye position for a (resolved) camera: Z-up spherical orbit around target.
pub fn eye_of(cam: &ModelCamera) -> [f32; 3] {
    let (cy, sy) = (cam.yaw.cos(), cam.yaw.sin());
    let (cp, sp) = (cam.pitch.cos(), cam.pitch.sin());
    [
        cam.target[0] + cp * cy * cam.distance,
        cam.target[1] + cp * sy * cam.distance,
        cam.target[2] + sp * cam.distance,
    ]
}

/// Column-major look-at view matrix (up = +Z world).
fn look_at(eye: [f32; 3], target: [f32; 3]) -> [f32; 16] {
    let f = v_norm(v_sub(target, eye));
    // Degenerate straight-up/down views are prevented by the pitch clamp.
    let s = v_norm(v_cross(f, [0.0, 0.0, 1.0]));
    let u = v_cross(s, f);
    [
        s[0],
        u[0],
        -f[0],
        0.0,
        s[1],
        u[1],
        -f[1],
        0.0,
        s[2],
        u[2],
        -f[2],
        0.0,
        -v_dot(s, eye),
        -v_dot(u, eye),
        v_dot(f, eye),
        1.0,
    ]
}

/// Column-major perspective projection.
fn perspective(aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (FOV_Y * 0.5).tan();
    let mut m = [0.0f32; 16];
    m[0] = f / aspect.max(1e-4);
    m[5] = f;
    m[10] = (far + near) / (near - far);
    m[11] = -1.0;
    m[14] = 2.0 * far * near / (near - far);
    m
}

fn mat_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut acc = 0.0;
            for k in 0..4 {
                acc += a[k * 4 + row] * b[col * 4 + k];
            }
            out[col * 4 + row] = acc;
        }
    }
    out
}

/// Bounds center + radius (half diagonal, floored to keep math finite).
pub fn bounds_sphere(min: [f32; 3], max: [f32; 3]) -> ([f32; 3], f32) {
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let half = v_sub(max, min);
    let radius = (v_dot(half, half).sqrt() * 0.5).max(1e-4);
    (center, radius)
}

/// Fills in the auto-fit parts of a camera (`distance <= 0` = fresh node):
/// aim at the bounds center from the saved yaw/pitch, far enough back that
/// the whole model fits the vertical field of view with some margin.
pub fn resolve_camera(cam: &ModelCamera, min: [f32; 3], max: [f32; 3]) -> ModelCamera {
    let mut out = *cam;
    out.pitch = out.pitch.clamp(-1.55, 1.55);
    if out.distance <= 0.0 || !out.distance.is_finite() || !out.target.iter().all(|v| v.is_finite())
    {
        let (center, radius) = bounds_sphere(min, max);
        out.target = center;
        out.distance = radius / (FOV_Y * 0.5).tan() * 1.25;
    }
    out
}

/// Rhino-style orbit: dragging moves the *camera* around the target (drag
/// right = your head moves right; the model appears to swing left).
pub fn orbit(cam: &mut ModelCamera, dx: f32, dy: f32) {
    cam.yaw += dx * ORBIT_PER_PX;
    cam.pitch = (cam.pitch - dy * ORBIT_PER_PX).clamp(-1.55, 1.55);
    // Keep yaw bounded so poses stay serialization-friendly.
    if cam.yaw.abs() > std::f32::consts::TAU {
        cam.yaw %= std::f32::consts::TAU;
    }
}

/// Rhino-style pan (Shift+drag): the model follows the cursor, i.e. the
/// target slides opposite to the drag in the view plane. `world_per_px`
/// converts screen pixels to world units at the target depth.
pub fn pan(cam: &mut ModelCamera, dx: f32, dy: f32, world_per_px: f32) {
    let eye = eye_of(cam);
    let f = v_norm(v_sub(cam.target, eye));
    let s = v_norm(v_cross(f, [0.0, 0.0, 1.0]));
    let u = v_cross(s, f);
    for i in 0..3 {
        cam.target[i] -= s[i] * dx * world_per_px;
        cam.target[i] += u[i] * dy * world_per_px;
    }
}

/// Scroll zoom: shrink/grow the orbit distance toward the target.
pub fn zoom(cam: &mut ModelCamera, factor: f32, radius_hint: f32) {
    let min = (radius_hint * 0.02).max(1e-4);
    let max = (radius_hint * 80.0).max(1.0);
    cam.distance = (cam.distance * factor).clamp(min, max);
}

/// World units per screen pixel at the target depth (for pan).
pub fn world_per_px(cam: &ModelCamera, viewport_px_h: f32) -> f32 {
    2.0 * cam.distance * (FOV_Y * 0.5).tan() / viewport_px_h.max(1.0)
}

// ---------- picking (CPU raycast against render meshes) ----------

/// Normalized viewport coordinates (0..1, origin top-left) → world-space ray.
pub fn ray_from_viewport_uv(
    u: f32,
    v: f32,
    aspect: f32,
    cam: &ModelCamera,
    bounds: ([f32; 3], [f32; 3]),
) -> ([f32; 3], [f32; 3]) {
    let cam = resolve_camera(cam, bounds.0, bounds.1);
    let eye = eye_of(&cam);
    let f = v_norm(v_sub(cam.target, eye));
    let s = v_norm(v_cross(f, [0.0, 0.0, 1.0]));
    let up = v_cross(s, f);
    let tan_half = (FOV_Y * 0.5).tan();
    let nx = (u - 0.5) * 2.0;
    let ny = (0.5 - v) * 2.0;
    let dir = v_norm([
        f[0] + s[0] * (nx * tan_half * aspect) + up[0] * (ny * tan_half),
        f[1] + s[1] * (nx * tan_half * aspect) + up[1] * (ny * tan_half),
        f[2] + s[2] * (nx * tan_half * aspect) + up[2] * (ny * tan_half),
    ]);
    (eye, dir)
}

/// Closest triangle hit along a ray (Möller–Trumbore). Returns world hit point.
pub fn raycast_model(
    model: &rhino_mesh::Model,
    origin: [f32; 3],
    dir: [f32; 3],
) -> Option<[f32; 3]> {
    let mut best_t = f32::INFINITY;
    let mut best = None;
    for part in &model.parts {
        let idx = &part.indices;
        let pos = &part.positions;
        for tri in idx.chunks_exact(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;
            if i0 >= pos.len() || i1 >= pos.len() || i2 >= pos.len() {
                continue;
            }
            if let Some(t) = ray_triangle(origin, dir, pos[i0], pos[i1], pos[i2]) {
                if t > 1e-4 && t < best_t {
                    best_t = t;
                    best = Some([
                        origin[0] + dir[0] * t,
                        origin[1] + dir[1] * t,
                        origin[2] + dir[2] * t,
                    ]);
                }
            }
        }
    }
    best
}

fn ray_triangle(
    origin: [f32; 3],
    dir: [f32; 3],
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
) -> Option<f32> {
    const EPS: f32 = 1e-7;
    let e1 = v_sub(v1, v0);
    let e2 = v_sub(v2, v0);
    let p = v_cross(dir, e2);
    let det = v_dot(e1, p);
    if det.abs() < EPS {
        return None;
    }
    let inv = 1.0 / det;
    let tvec = v_sub(origin, v0);
    let u = v_dot(tvec, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = v_cross(tvec, e1);
    let v = v_dot(dir, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = v_dot(e2, q) * inv;
    (t > EPS).then_some(t)
}

/// Project a model-space point to normalized viewport UV (0..1, top-left).
/// Returns `None` when behind the camera.
pub fn project_model_point(
    point: [f32; 3],
    aspect: f32,
    cam: &ModelCamera,
    bounds: ([f32; 3], [f32; 3]),
) -> Option<(f32, f32)> {
    let cam = resolve_camera(cam, bounds.0, bounds.1);
    let eye = eye_of(&cam);
    let view = look_at(eye, cam.target);
    let (_, radius) = bounds_sphere(bounds.0, bounds.1);
    let near = (cam.distance - radius * 2.0)
        .max(cam.distance * 0.01)
        .max(radius * 1e-3);
    let far = cam.distance + radius * 4.0;
    let proj = perspective(aspect, near, far);
    let mvp = mat_mul(&proj, &view);
    let clip = [
        mvp[0] * point[0] + mvp[4] * point[1] + mvp[8] * point[2] + mvp[12],
        mvp[1] * point[0] + mvp[5] * point[1] + mvp[9] * point[2] + mvp[13],
        mvp[2] * point[0] + mvp[6] * point[1] + mvp[10] * point[2] + mvp[14],
        mvp[3] * point[0] + mvp[7] * point[1] + mvp[11] * point[2] + mvp[15],
    ];
    if clip[3].abs() < 1e-8 {
        return None;
    }
    let ndc_x = clip[0] / clip[3];
    let ndc_y = clip[1] / clip[3];
    let ndc_z = clip[2] / clip[3];
    if ndc_z < -1.0 || ndc_z > 1.0 {
        return None;
    }
    Some(((ndc_x + 1.0) * 0.5, (1.0 - ndc_y) * 0.5))
}

// ---------- poster cache (pure path helpers, unit-tested) ----------

/// Disk cache for frozen-viewport posters, beside the shared thumb cache.
pub fn poster_dir() -> PathBuf {
    atlas_core::index::data_dir().join("model-posters")
}

/// Aspect quantized to keep poster keys stable across sub-pixel resizes.
pub fn aspect_q(w: f32, h: f32) -> u32 {
    ((w / h.max(1.0)) * 100.0).round().clamp(10.0, 1000.0) as u32
}

/// One poster per (model file, camera pose, node aspect).
pub fn poster_file_name(cache_key: &str, cam: &ModelCamera, aspect_q: u32) -> String {
    format!("{cache_key}-{:016x}-a{aspect_q}.png", cam.cache_hash())
}

pub fn poster_path(cache_key: &str, cam: &ModelCamera, aspect_q: u32) -> PathBuf {
    poster_dir().join(poster_file_name(cache_key, cam, aspect_q))
}

/// Poster pixel size for a node aspect (long edge = [`POSTER_LONG_EDGE`]).
fn poster_size(aspect_q: u32) -> (u32, u32) {
    let aspect = aspect_q as f32 / 100.0;
    if aspect >= 1.0 {
        (POSTER_LONG_EDGE, (POSTER_LONG_EDGE as f32 / aspect) as u32)
    } else {
        ((POSTER_LONG_EDGE as f32 * aspect) as u32, POSTER_LONG_EDGE)
    }
}

/// Quantize a live render size (steps of 32 px avoid re-render churn while
/// the board camera zooms).
fn quantize_px(v: f32) -> u32 {
    (((v / 32.0).ceil() * 32.0) as u32).clamp(64, MAX_RENDER_PX)
}

// ---------- parse progress ----------

/// Coarse stage of an off-thread model parse, for the in-viewport load bar.
const STAGE_READING: u8 = 0;
const STAGE_PARSING: u8 = 1;
const STAGE_DONE: u8 = 2;

/// Reading the file is the measurable part; parsing gets the tail slice.
const READ_SPAN: f32 = 0.75;
const PARSE_CHECKPOINT: f32 = 0.9;

/// Progress shared between the parse worker and the paint pass. The file
/// read is measured exactly (bytes copied vs file size); the mesh parse has
/// no incremental hook in `rhino-mesh`, so it reports a fixed checkpoint and
/// the UI eases the bar between checkpoints.
pub struct ParseProgress {
    /// File size in bytes (0 until stat'd).
    total: AtomicU64,
    /// Bytes read from disk so far.
    read: AtomicU64,
    /// One of `STAGE_READING` / `STAGE_PARSING` / `STAGE_DONE`.
    stage: AtomicU8,
}

impl ParseProgress {
    fn new() -> Self {
        ParseProgress {
            total: AtomicU64::new(0),
            read: AtomicU64::new(0),
            stage: AtomicU8::new(STAGE_READING),
        }
    }

    fn set_total(&self, bytes: u64) {
        self.total.store(bytes, Ordering::Relaxed);
    }

    fn add_read(&self, bytes: u64) {
        self.read.fetch_add(bytes, Ordering::Relaxed);
    }

    fn set_stage(&self, stage: u8) {
        self.stage.store(stage, Ordering::Relaxed);
    }

    /// Monotonic 0..=1 checkpoint for the load bar: the byte-accurate read
    /// fills `0..READ_SPAN`, parsing sits at `PARSE_CHECKPOINT`, done = 1.
    pub fn fraction(&self) -> f32 {
        match self.stage.load(Ordering::Relaxed) {
            STAGE_READING => {
                let total = self.total.load(Ordering::Relaxed);
                let read = self.read.load(Ordering::Relaxed);
                if total == 0 {
                    0.0
                } else {
                    READ_SPAN * (read as f32 / total as f32).clamp(0.0, 1.0)
                }
            }
            STAGE_PARSING => PARSE_CHECKPOINT,
            _ => 1.0,
        }
    }
}

/// Worker-side parse: read the file in chunks (updating `progress` so the
/// UI bar tracks real bytes), then hand the buffer to the mesh parser.
fn parse_with_progress(path: &Path, progress: &ParseProgress) -> Result<rhino_mesh::Model, String> {
    let bytes = read_counted(path, progress).map_err(|e| e.to_string())?;
    progress.set_stage(STAGE_PARSING);
    let result = rhino_mesh::read_render_meshes_from(&bytes).map_err(|e| e.to_string());
    progress.set_stage(STAGE_DONE);
    result
}

fn read_counted(path: &Path, progress: &ParseProgress) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let total = file.metadata()?.len();
    progress.set_total(total);
    let mut buf = Vec::with_capacity(total as usize);
    let mut chunk = vec![0u8; 512 * 1024];
    loop {
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        progress.add_read(n as u64);
    }
    Ok(buf)
}

// ---------- state ----------

/// Parse status of one model file (keyed by the item's thumbnail cache key,
/// which already encodes path + size + mtime).
pub enum ModelState {
    Loading,
    Ready(Arc<rhino_mesh::Model>),
    /// Message shown in the node placeholder ("no cached render meshes…").
    Failed(String),
}

/// One live (unlocked) viewport.
pub struct LiveViewport {
    /// The item cache key of the model file this node shows.
    pub cache_key: String,
    /// Live camera (resolved: `distance > 0` once the mesh is known).
    pub cam: ModelCamera,
    /// Document pose at unlock — the `before` of the single patch on lock.
    pub before: ModelCamera,
    pub last_interact: Instant,
    tex: Option<TextureHandle>,
    rendered: Option<(u64, u32, u32)>,
    /// Bounds radius once known (zoom clamps, pan scale).
    pub radius: f32,
    /// Left-edge tool palette (Miro-style expandable strip).
    pub toolbar_expanded: bool,
    pub tool: ModelViewportTool,
    /// In-progress first point for point-to-point measure.
    pub measure_first: Option<[f32; 3]>,
    /// Hover/drag preview endpoint.
    pub measure_preview: Option<[f32; 3]>,
    /// Completed measurements this live session (cleared on lock).
    pub measures: Vec<DistanceMeasurement>,
}

struct GpuEntry {
    model: GpuModel,
    last_used: Instant,
}

struct CpuEntry {
    state: ModelState,
    last_used: Instant,
    /// Load-bar state while `state` is `Loading`.
    progress: Arc<ParseProgress>,
}

enum EngineSlot {
    Untried,
    Ready(Box<ModelEngine>),
    Failed,
}

/// All 3D-viewport state, owned by [`SlateApp`].
pub struct ModelSpace {
    engine: EngineSlot,
    /// Parsed meshes by item cache key.
    models: HashMap<String, CpuEntry>,
    /// Uploaded GPU buffers by item cache key.
    gpu: HashMap<String, GpuEntry>,
    /// Live viewports by node (active tab only; locked on tab switch).
    pub live: HashMap<NodeId, LiveViewport>,
    /// Poster textures by poster file name.
    posters: HashMap<String, TextureHandle>,
    /// Nodes whose poster needs (re)generation once mesh + GL are ready.
    want_poster: std::collections::HashSet<NodeId>,
    /// Bounds by cache key (kept even after CPU mesh eviction — needed to
    /// resolve auto-fit cameras cheaply, e.g. for artifact export).
    pub bounds: HashMap<String, ([f32; 3], [f32; 3])>,
    parse_tx: Sender<(String, Result<rhino_mesh::Model, String>)>,
    parse_rx: Receiver<(String, Result<rhino_mesh::Model, String>)>,
    engine_toast_shown: bool,
}

impl Default for ModelSpace {
    fn default() -> Self {
        let (parse_tx, parse_rx) = unbounded();
        ModelSpace {
            engine: EngineSlot::Untried,
            models: HashMap::new(),
            gpu: HashMap::new(),
            live: HashMap::new(),
            posters: HashMap::new(),
            want_poster: std::collections::HashSet::new(),
            bounds: HashMap::new(),
            parse_tx,
            parse_rx,
            engine_toast_shown: false,
        }
    }
}

impl ModelSpace {
    /// Kick off (or re-poll) the off-thread parse of a model file.
    fn request_model(&mut self, cache_key: &str, path: &Path) {
        if self.models.contains_key(cache_key) {
            return;
        }
        let progress = Arc::new(ParseProgress::new());
        self.models.insert(
            cache_key.to_string(),
            CpuEntry {
                state: ModelState::Loading,
                last_used: Instant::now(),
                progress: progress.clone(),
            },
        );
        let tx = self.parse_tx.clone();
        let key = cache_key.to_string();
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let result = parse_with_progress(&path, &progress);
            let _ = tx.send((key, result));
        });
    }

    fn drain_parses(&mut self) -> bool {
        let mut any = false;
        while let Ok((key, result)) = self.parse_rx.try_recv() {
            any = true;
            let state = match result {
                Ok(model) => {
                    self.bounds
                        .insert(key.clone(), (model.bounds_min, model.bounds_max));
                    ModelState::Ready(Arc::new(model))
                }
                Err(e) => ModelState::Failed(e),
            };
            if let Some(entry) = self.models.get_mut(&key) {
                entry.state = state;
                entry.last_used = Instant::now();
            }
        }
        any
    }

    /// Parsed CPU mesh when ready (for picking / measurement).
    pub fn mesh_for_key(&mut self, cache_key: &str) -> Option<Arc<rhino_mesh::Model>> {
        self.ready_model(cache_key)
    }

    fn ready_model(&mut self, cache_key: &str) -> Option<Arc<rhino_mesh::Model>> {
        let entry = self.models.get_mut(cache_key)?;
        entry.last_used = Instant::now();
        match &entry.state {
            ModelState::Ready(m) => Some(m.clone()),
            _ => None,
        }
    }

    fn engine(&mut self, gl: &Arc<glow::Context>) -> Option<&ModelEngine> {
        if matches!(self.engine, EngineSlot::Untried) {
            self.engine = match ModelEngine::new(gl.clone()) {
                Some(e) => EngineSlot::Ready(Box::new(e)),
                None => EngineSlot::Failed,
            };
        }
        match &self.engine {
            EngineSlot::Ready(e) => Some(e),
            _ => None,
        }
    }

    /// GPU buffers for a file, uploading if the CPU mesh is ready.
    fn ensure_gpu(&mut self, gl: &Arc<glow::Context>, cache_key: &str) -> bool {
        if self.gpu.contains_key(cache_key) {
            if let Some(e) = self.gpu.get_mut(cache_key) {
                e.last_used = Instant::now();
            }
            return true;
        }
        let Some(model) = self.ready_model(cache_key) else {
            return false;
        };
        if self.engine(gl).is_none() {
            return false;
        }
        let EngineSlot::Ready(engine) = &self.engine else {
            return false;
        };
        match engine.upload(&model) {
            Some(gpu) => {
                self.gpu.insert(
                    cache_key.to_string(),
                    GpuEntry {
                        model: gpu,
                        last_used: Instant::now(),
                    },
                );
                true
            }
            None => false,
        }
    }

    /// Render a pose into an egui image (offscreen MSAA pass + readback).
    fn render_image(
        &mut self,
        gl: &Arc<glow::Context>,
        cache_key: &str,
        cam: &ModelCamera,
        w: u32,
        h: u32,
    ) -> Option<egui::ColorImage> {
        if !self.ensure_gpu(gl, cache_key) {
            return None;
        }
        let EngineSlot::Ready(engine) = &self.engine else {
            return None;
        };
        let gpu = self.gpu.get(cache_key)?;
        engine.render(&gpu.model, cam, w, h)
    }

    /// Free GPU/CPU entries nothing is using (called once per frame).
    fn evict(&mut self) {
        let live_keys: std::collections::HashSet<&String> =
            self.live.values().map(|v| &v.cache_key).collect();
        let mut dead_gpu: Vec<String> = self
            .gpu
            .iter()
            .filter(|(k, e)| !live_keys.contains(k) && e.last_used.elapsed() > GPU_LINGER)
            .map(|(k, _)| k.clone())
            .collect();
        for key in dead_gpu.drain(..) {
            if let (Some(entry), EngineSlot::Ready(engine)) = (self.gpu.remove(&key), &self.engine)
            {
                engine.free(entry.model);
            }
        }

        // CPU meshes: keep the ones live viewports need plus a small LRU.
        let spare: Vec<(String, Instant)> = self
            .models
            .iter()
            .filter(|(k, e)| !live_keys.contains(k) && matches!(e.state, ModelState::Ready(_)))
            .map(|(k, e)| (k.clone(), e.last_used))
            .collect();
        if spare.len() > MAX_CPU_MODELS {
            let mut by_age = spare;
            by_age.sort_by_key(|(_, t)| *t);
            for (key, _) in by_age.iter().take(by_age.len() - MAX_CPU_MODELS) {
                self.models.remove(key);
            }
        }

        if self.posters.len() > 64 {
            self.posters.clear();
        }
    }
}

// ---------- SlateApp integration ----------

/// Data snapshot of one model node used by paint/interaction passes.
pub struct ModelNodeInfo {
    pub node: NodeId,
    pub cache_key: String,
    pub path: PathBuf,
    pub cam: ModelCamera,
    pub rect: slate_doc::scene::WorldRect,
}

impl SlateApp {
    /// Snapshot of the node when it's a placed 3D model.
    pub fn model_node_info(&self, id: NodeId) -> Option<ModelNodeInfo> {
        let node = self.doc().scene.node(id)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        let item = self.doc().item(img.item)?;
        if slate_doc::media_kind(&item.path) != slate_doc::MediaKind::Model
            || item.cache_key.is_empty()
        {
            return None;
        }
        Some(ModelNodeInfo {
            node: id,
            cache_key: item.cache_key.clone(),
            path: item.path.clone(),
            cam: img.model,
            rect: node.rect,
        })
    }

    /// All placed model nodes in the active document.
    pub fn model_nodes(&self) -> Vec<ModelNodeInfo> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| self.model_node_info(n.id))
            .collect()
    }

    /// The topmost node at a world point when it's an *unlocked* viewport.
    pub fn live_model_at(&self, wx: f32, wy: f32) -> Option<NodeId> {
        let id = self.doc().scene.node_at(wx, wy)?;
        (self.model3d.live.contains_key(&id) && self.model_node_info(id).is_some()).then_some(id)
    }

    /// Unlock a model node into a live viewport (parses + uploads lazily).
    pub fn unlock_model(&mut self, id: NodeId) {
        if self.model3d.live.contains_key(&id) {
            return;
        }
        let Some(info) = self.model_node_info(id) else {
            return;
        };
        if self.gl.is_none() {
            self.toast("3D viewports need GPU rendering (unavailable here)");
            return;
        }
        // Budget: lock the least-recently-touched live viewport first.
        while self.model3d.live.len() >= MAX_LIVE {
            let Some(oldest) = self
                .model3d
                .live
                .iter()
                .min_by_key(|(_, v)| v.last_interact)
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.lock_model(oldest);
        }
        self.model3d.request_model(&info.cache_key, &info.path);
        // Resolve now if bounds are already known; otherwise the first
        // rendered frame resolves it.
        let cam = match self.model3d.bounds.get(&info.cache_key) {
            Some((min, max)) => resolve_camera(&info.cam, *min, *max),
            None => info.cam,
        };
        let radius = self
            .model3d
            .bounds
            .get(&info.cache_key)
            .map(|(min, max)| bounds_sphere(*min, *max).1)
            .unwrap_or(0.0);
        self.model3d.live.insert(
            id,
            LiveViewport {
                cache_key: info.cache_key,
                cam,
                before: info.cam,
                last_interact: Instant::now(),
                tex: None,
                rendered: None,
                radius,
                toolbar_expanded: false,
                tool: ModelViewportTool::Navigate,
                measure_first: None,
                measure_preview: None,
                measures: Vec::new(),
            },
        );
    }

    /// Lock a live viewport: freeze the current pose as the poster, commit
    /// the camera to the document (one undo step), release GPU work.
    pub fn lock_model(&mut self, id: NodeId) {
        let Some(vp) = self.model3d.live.remove(&id) else {
            return;
        };
        let Some(info) = self.model_node_info(id) else {
            return; // node deleted while live — nothing to persist
        };

        // Render the final pose at poster quality and cache it on disk.
        let cam = vp.cam;
        if cam.distance > 0.0 {
            if let Some(gl) = self.gl.clone() {
                let aq = aspect_q(info.rect.w, info.rect.h);
                let (pw, ph) = poster_size(aq);
                if let Some(img) = self
                    .model3d
                    .render_image(&gl, &info.cache_key, &cam, pw, ph)
                {
                    save_poster(&poster_path(&info.cache_key, &cam, aq), &img);
                    self.model3d.want_poster.remove(&id);
                }
            }
            // Commit the pose (skip when untouched, e.g. unlock → instant
            // relock before the mesh even loaded).
            if cam != vp.before {
                self.last_board_edit = None;
                self.patch_nodes(&[id], |n| {
                    if let NodeKind::Image(img) = &mut n.kind {
                        img.model = cam;
                    }
                });
                self.last_board_edit = None;
            }
        }
    }

    /// Lock every live viewport (tab switches, presentation start, exit).
    pub fn lock_all_models(&mut self) {
        let ids: Vec<NodeId> = self.model3d.live.keys().copied().collect();
        for id in ids {
            self.lock_model(id);
        }
    }

    /// Per-frame upkeep: parse results, auto-lock, eviction, repaint ticks.
    pub fn model3d_frame(&mut self, ctx: &egui::Context) {
        if self.model3d.drain_parses() {
            ctx.request_repaint();
        }

        // Live viewports whose node vanished (undo, delete) just drop.
        let dead: Vec<NodeId> = self
            .model3d
            .live
            .keys()
            .filter(|id| self.model_node_info(**id).is_none())
            .copied()
            .collect();
        for id in dead {
            self.model3d.live.remove(&id);
        }

        // Auto-lock idle viewports.
        let idle: Vec<NodeId> = self
            .model3d
            .live
            .iter()
            .filter(|(_, v)| v.last_interact.elapsed() >= AUTO_LOCK)
            .map(|(id, _)| *id)
            .collect();
        for id in idle {
            self.lock_model(id);
        }

        // Regenerate posters requested by the paint pass (mesh may have
        // finished parsing this frame).
        if self.gl.is_some() && !self.model3d.want_poster.is_empty() {
            let wanted: Vec<NodeId> = self.model3d.want_poster.iter().copied().collect();
            for id in wanted {
                if self.model3d.live.contains_key(&id) {
                    self.model3d.want_poster.remove(&id);
                    continue;
                }
                if self.generate_poster(id) {
                    self.model3d.want_poster.remove(&id);
                    ctx.request_repaint();
                }
            }
        }

        self.model3d.evict();

        if !self.model3d.live.is_empty() {
            // Keep ticking so the auto-lock countdown fires without input.
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    /// Render + store the poster for a locked node. `true` when done (or
    /// permanently impossible, so the request should be dropped).
    fn generate_poster(&mut self, id: NodeId) -> bool {
        let Some(info) = self.model_node_info(id) else {
            return true;
        };
        let Some(gl) = self.gl.clone() else {
            return true;
        };
        self.model3d.request_model(&info.cache_key, &info.path);
        match self.model3d.models.get(&info.cache_key).map(|e| &e.state) {
            Some(ModelState::Ready(_)) => {}
            Some(ModelState::Failed(_)) => return true, // fallback thumb stays
            _ => return false,                          // still parsing
        }
        let Some((min, max)) = self.model3d.bounds.get(&info.cache_key).copied() else {
            return true;
        };
        let cam = resolve_camera(&info.cam, min, max);
        let aq = aspect_q(info.rect.w, info.rect.h);
        let (pw, ph) = poster_size(aq);
        match self
            .model3d
            .render_image(&gl, &info.cache_key, &cam, pw, ph)
        {
            Some(img) => {
                save_poster(&poster_path(&info.cache_key, &cam, aq), &img);
                true
            }
            None => matches!(self.model3d.engine, EngineSlot::Failed),
        }
    }

    /// Texture for a locked model node: the cached frozen-camera poster.
    /// `None` = not available yet (caller falls back to the item thumbnail
    /// and the poster is queued for generation).
    pub fn model_poster_texture(
        &mut self,
        ctx: &egui::Context,
        info: &ModelNodeInfo,
    ) -> Option<TextureHandle> {
        // Auto-fit cameras can only be resolved once bounds are known.
        let cam = if info.cam.distance > 0.0 {
            info.cam
        } else {
            let (min, max) = self.model3d.bounds.get(&info.cache_key).copied()?;
            resolve_camera(&info.cam, min, max)
        };
        let aq = aspect_q(info.rect.w, info.rect.h);
        let name = poster_file_name(&info.cache_key, &cam, aq);
        if let Some(tex) = self.model3d.posters.get(&name) {
            return Some(tex.clone());
        }
        let path = poster_dir().join(&name);
        let img = image::open(&path).ok()?.to_rgba8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let color = egui::ColorImage::from_rgba_unmultiplied([w, h], img.as_raw());
        let tex = ctx.load_texture(
            format!("slate-model-poster-{name}"),
            color,
            egui::TextureOptions::LINEAR,
        );
        self.model3d.posters.insert(name, tex.clone());
        Some(tex)
    }

    /// Queue poster generation for a locked node (paint pass found none).
    pub fn request_model_poster(&mut self, id: NodeId) {
        if self.gl.is_none() {
            return;
        }
        if let Some(info) = self.model_node_info(id) {
            self.model3d.request_model(&info.cache_key, &info.path);
            self.model3d.want_poster.insert(id);
        }
    }

    /// Texture for a live viewport, re-rendered when the camera or the
    /// on-screen size changed. Falls back to `None` while the mesh parses.
    pub fn model_live_texture(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<TextureHandle> {
        let gl = self.gl.clone()?;
        // Resolve the camera as soon as bounds exist.
        let (cache_key, mut cam) = {
            let vp = self.model3d.live.get(&id)?;
            (vp.cache_key.clone(), vp.cam)
        };
        if cam.distance <= 0.0 {
            let (min, max) = self.model3d.bounds.get(&cache_key).copied()?;
            cam = resolve_camera(&cam, min, max);
            let radius = bounds_sphere(min, max).1;
            if let Some(vp) = self.model3d.live.get_mut(&id) {
                vp.cam = cam;
                vp.radius = radius;
            }
        }

        let ppp = ctx.pixels_per_point();
        let w = quantize_px(screen_w * ppp);
        let h = quantize_px(screen_h * ppp);
        let stamp = (cam.cache_hash(), w, h);
        let up_to_date = self
            .model3d
            .live
            .get(&id)
            .is_some_and(|vp| vp.rendered == Some(stamp) && vp.tex.is_some());
        if !up_to_date {
            let img = self.model3d.render_image(&gl, &cache_key, &cam, w, h)?;
            let vp = self.model3d.live.get_mut(&id)?;
            match &mut vp.tex {
                Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
                None => {
                    vp.tex = Some(ctx.load_texture(
                        format!("slate-model-live-{}", id.0),
                        img,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
            vp.rendered = Some(stamp);
        }
        self.model3d.live.get(&id).and_then(|vp| vp.tex.clone())
    }

    /// Raycast against a live viewport at a screen point inside the node rect.
    pub fn model_pick_at_screen(
        &mut self,
        id: NodeId,
        screen: egui::Pos2,
        srect: egui::Rect,
    ) -> Option<[f32; 3]> {
        let (cache_key, cam) = {
            let vp = self.model3d.live.get(&id)?;
            (vp.cache_key.clone(), vp.cam)
        };
        let bounds = self.model3d.bounds.get(&cache_key).copied()?;
        let u = (screen.x - srect.min.x) / srect.width().max(1.0);
        let v = (screen.y - srect.min.y) / srect.height().max(1.0);
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return None;
        }
        let aspect = srect.width() / srect.height().max(1.0);
        let (origin, dir) = ray_from_viewport_uv(u, v, aspect, &cam, bounds);
        let model = self.model3d.mesh_for_key(&cache_key)?;
        raycast_model(&model, origin, dir)
    }

    /// Update the hover preview while measuring.
    pub fn model_measure_preview(&mut self, id: NodeId, screen: egui::Pos2, srect: egui::Rect) {
        let hit = self.model_pick_at_screen(id, screen, srect);
        if let Some(vp) = self.model3d.live.get_mut(&id) {
            vp.measure_preview = hit;
            if hit.is_some() {
                vp.last_interact = Instant::now();
            }
        }
    }

    /// Commit a measure pick (first or second point).
    pub fn model_measure_pick(&mut self, id: NodeId, screen: egui::Pos2, srect: egui::Rect) {
        let Some(hit) = self.model_pick_at_screen(id, screen, srect) else {
            return;
        };
        let Some(vp) = self.model3d.live.get_mut(&id) else {
            return;
        };
        vp.last_interact = Instant::now();
        match vp.measure_first {
            None => vp.measure_first = Some(hit),
            Some(a) => {
                vp.measures.push(DistanceMeasurement { a, b: hit });
                vp.measure_first = None;
            }
        }
        vp.measure_preview = None;
    }

    /// Clear in-progress measure picks.
    pub fn model_measure_cancel(&mut self, id: NodeId) {
        if let Some(vp) = self.model3d.live.get_mut(&id) {
            vp.measure_first = None;
            vp.measure_preview = None;
        }
    }

    /// Route an orbit/pan drag into a live viewport. `pan_mode` = Shift.
    pub fn model_drag(&mut self, id: NodeId, dx: f32, dy: f32, pan_mode: bool, viewport_h: f32) {
        let Some(vp) = self.model3d.live.get_mut(&id) else {
            return;
        };
        vp.last_interact = Instant::now();
        if vp.cam.distance <= 0.0 {
            return; // mesh not in yet — nothing sensible to move
        }
        if pan_mode {
            let wpp = world_per_px(&vp.cam, viewport_h);
            pan(&mut vp.cam, dx, dy, wpp);
        } else {
            orbit(&mut vp.cam, dx, dy);
        }
    }

    /// Route scroll zoom into a live viewport.
    pub fn model_scroll(&mut self, id: NodeId, scroll: f32) {
        let Some(vp) = self.model3d.live.get_mut(&id) else {
            return;
        };
        vp.last_interact = Instant::now();
        if vp.cam.distance <= 0.0 {
            return;
        }
        let factor = (1.0 - scroll * 0.0015).clamp(0.5, 2.0);
        let radius = if vp.radius > 0.0 {
            vp.radius
        } else {
            vp.cam.distance
        };
        zoom(&mut vp.cam, factor, radius);
    }

    /// Reset a model node's camera to the auto-fit default (journaled).
    pub fn reset_model_camera(&mut self, id: NodeId) {
        if let Some(vp) = self.model3d.live.get_mut(&id) {
            let mut cam = ModelCamera::default();
            if let Some((min, max)) = self.model3d.bounds.get(&vp.cache_key) {
                cam = resolve_camera(&cam, *min, *max);
            }
            vp.cam = cam;
            vp.last_interact = Instant::now();
            return;
        }
        self.last_board_edit = None;
        self.patch_nodes(&[id], |n| {
            if let NodeKind::Image(img) = &mut n.kind {
                img.model = ModelCamera::default();
            }
        });
        self.last_board_edit = None;
    }

    /// Load-bar checkpoint (0..=1) while a model file is still parsing.
    /// `None` once the parse finished (ready or failed) or never started.
    pub fn model_parse_progress(&self, cache_key: &str) -> Option<f32> {
        let entry = self.model3d.models.get(cache_key)?;
        matches!(entry.state, ModelState::Loading).then(|| entry.progress.fraction())
    }

    /// Parse-failure message for a model file, if it failed.
    pub fn model_failure(&self, cache_key: &str) -> Option<&str> {
        match self.model3d.models.get(cache_key).map(|e| &e.state) {
            Some(ModelState::Failed(msg)) => Some(msg.as_str()),
            _ => None,
        }
    }

    /// One-time toast when shader setup failed (very old GPUs).
    pub fn note_engine_failure(&mut self) {
        if matches!(self.model3d.engine, EngineSlot::Failed) && !self.model3d.engine_toast_shown {
            self.model3d.engine_toast_shown = true;
            self.toast("3D viewport unavailable — GPU shader setup failed");
        }
    }
}

fn save_poster(path: &Path, img: &egui::ColorImage) {
    let _ = std::fs::create_dir_all(poster_dir());
    let (w, h) = (img.size[0] as u32, img.size[1] as u32);
    let mut rgba = Vec::with_capacity(img.pixels.len() * 4);
    for p in &img.pixels {
        rgba.extend_from_slice(&p.to_srgba_unmultiplied());
    }
    let _ = image::save_buffer_with_format(
        path,
        &rgba,
        w,
        h,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    );
}

// ---------- GL renderer ----------

const MODEL_VS: &str = r#"#version 330 core
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_nrm;
uniform mat4 u_mvp;
uniform mat4 u_view;
out vec3 v_nrm;
out vec3 v_pos;
void main() {
    gl_Position = u_mvp * vec4(a_pos, 1.0);
    v_nrm = mat3(u_view) * a_nrm;
    v_pos = (u_view * vec4(a_pos, 1.0)).xyz;
}
"#;

// Two-sided Blinn-Phong with a camera headlight plus a hemisphere fill,
// computed in linear space and encoded back to sRGB — reads like Rhino's
// shaded display mode (minus isocurves/edges, which need edge extraction).
const MODEL_FS: &str = r#"#version 330 core
in vec3 v_nrm;
in vec3 v_pos;
uniform vec3 u_color;
out vec4 frag;
void main() {
    vec3 n = normalize(v_nrm);
    vec3 v = normalize(-v_pos);
    if (dot(n, v) < 0.0) n = -n;
    vec3 l = normalize(vec3(0.25, 0.4, 1.0));
    vec3 base = pow(u_color, vec3(2.2));
    float ndl = max(dot(n, l), 0.0);
    float hemi = 0.5 + 0.5 * n.y;
    vec3 col = base * (0.22 + 0.16 * hemi) + base * ndl * 0.72;
    vec3 hv = normalize(l + v);
    col += vec3(0.18) * pow(max(dot(n, hv), 0.0), 48.0);
    frag = vec4(pow(col, vec3(1.0 / 2.2)), 1.0);
}
"#;

const BG_VS: &str = r#"#version 330 core
out vec2 v_uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

const BG_FS: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 frag;
void main() {
    vec3 top = vec3(0.792, 0.831, 0.874);
    vec3 bottom = vec3(0.545, 0.569, 0.600);
    frag = vec4(mix(bottom, top, v_uv.y), 1.0);
}
"#;

/// Default surface color for parts without an object color (Rhino files
/// usually color by layer, which the reader doesn't resolve — see
/// `rhino-mesh` docs).
const DEFAULT_PART_COLOR: [f32; 3] = [0.78, 0.78, 0.76];

/// One `glDrawElements` range with its uniform color.
struct DrawRange {
    /// Byte offset into the index buffer.
    offset: i32,
    count: i32,
    color: [f32; 3],
}

pub struct GpuModel {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    draws: Vec<DrawRange>,
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
}

/// Shared GL program + offscreen pipeline for all model viewports.
pub struct ModelEngine {
    gl: Arc<glow::Context>,
    program: glow::Program,
    u_mvp: glow::UniformLocation,
    u_view: glow::UniformLocation,
    u_color: glow::UniformLocation,
    bg_program: glow::Program,
    bg_vao: glow::VertexArray,
}

fn compile_program(gl: &glow::Context, vs_src: &str, fs_src: &str) -> Option<glow::Program> {
    unsafe {
        let program = gl.create_program().ok()?;
        let mut shaders = Vec::new();
        for (kind, src) in [
            (glow::VERTEX_SHADER, vs_src),
            (glow::FRAGMENT_SHADER, fs_src),
        ] {
            let shader = gl.create_shader(kind).ok()?;
            gl.shader_source(shader, src);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                log_gl_error(&gl.get_shader_info_log(shader));
                gl.delete_shader(shader);
                gl.delete_program(program);
                return None;
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.link_program(program);
        for s in shaders {
            gl.detach_shader(program, s);
            gl.delete_shader(s);
        }
        if !gl.get_program_link_status(program) {
            log_gl_error(&gl.get_program_info_log(program));
            gl.delete_program(program);
            return None;
        }
        Some(program)
    }
}

fn log_gl_error(msg: &str) {
    eprintln!("slate model3d: GL setup failed: {msg}");
}

impl ModelEngine {
    pub fn new(gl: Arc<glow::Context>) -> Option<Self> {
        let program = compile_program(&gl, MODEL_VS, MODEL_FS)?;
        let bg_program = compile_program(&gl, BG_VS, BG_FS)?;
        unsafe {
            let u_mvp = gl.get_uniform_location(program, "u_mvp")?;
            let u_view = gl.get_uniform_location(program, "u_view")?;
            let u_color = gl.get_uniform_location(program, "u_color")?;
            // Core profiles need a bound VAO even for bufferless draws.
            let bg_vao = gl.create_vertex_array().ok()?;
            Some(ModelEngine {
                gl,
                program,
                u_mvp,
                u_view,
                u_color,
                bg_program,
                bg_vao,
            })
        }
    }

    /// Upload a parsed model: one interleaved (pos, normal) vertex buffer,
    /// one index buffer, per-color draw ranges (brep faces usually share a
    /// color, so most files collapse to a single draw call).
    pub fn upload(&self, model: &rhino_mesh::Model) -> Option<GpuModel> {
        let gl = &self.gl;

        // Group parts by color to minimize draw calls.
        let mut order: Vec<usize> = (0..model.parts.len()).collect();
        let color_of = |p: &rhino_mesh::MeshPart| -> [u8; 3] { p.color.unwrap_or([255, 255, 255]) };
        order.sort_by_key(|i| (model.parts[*i].color.is_some(), color_of(&model.parts[*i])));

        let total_verts: usize = model.parts.iter().map(|p| p.positions.len()).sum();
        let total_idx: usize = model.parts.iter().map(|p| p.indices.len()).sum();
        let mut verts: Vec<f32> = Vec::with_capacity(total_verts * 6);
        let mut indices: Vec<u32> = Vec::with_capacity(total_idx);
        let mut draws: Vec<DrawRange> = Vec::new();
        let mut base_vertex: u32 = 0;

        for i in order {
            let part = &model.parts[i];
            if part.positions.is_empty() || part.indices.is_empty() {
                continue;
            }
            let color = part
                .color
                .map(|c| {
                    [
                        c[0] as f32 / 255.0,
                        c[1] as f32 / 255.0,
                        c[2] as f32 / 255.0,
                    ]
                })
                .unwrap_or(DEFAULT_PART_COLOR);
            let start_index = indices.len();
            for (p, n) in part.positions.iter().zip(part.normals.iter()) {
                verts.extend_from_slice(p);
                verts.extend_from_slice(n);
            }
            for idx in &part.indices {
                indices.push(idx + base_vertex);
            }
            base_vertex += part.positions.len() as u32;

            // Extend the previous range when the color repeats.
            let count = (indices.len() - start_index) as i32;
            match draws.last_mut() {
                Some(last) if last.color == color => last.count += count,
                _ => draws.push(DrawRange {
                    offset: (start_index * 4) as i32,
                    count,
                    color,
                }),
            }
        }
        if indices.is_empty() {
            return None;
        }

        unsafe {
            let vao = gl.create_vertex_array().ok()?;
            let vbo = gl.create_buffer().ok()?;
            let ebo = gl.create_buffer().ok()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck_f32_slice(&verts),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytemuck_u32_slice(&indices),
                glow::STATIC_DRAW,
            );
            let stride = 6 * 4;
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 3 * 4);
            gl.enable_vertex_attrib_array(1);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);

            Some(GpuModel {
                vao,
                vbo,
                ebo,
                draws,
                bounds_min: model.bounds_min,
                bounds_max: model.bounds_max,
            })
        }
    }

    pub fn free(&self, model: GpuModel) {
        let gl = &self.gl;
        unsafe {
            gl.delete_vertex_array(model.vao);
            gl.delete_buffer(model.vbo);
            gl.delete_buffer(model.ebo);
        }
    }

    /// Offscreen render: MSAA color+depth renderbuffers → resolve blit →
    /// readback. Returns straight-alpha RGBA (alpha is 1 everywhere — the
    /// gradient background makes MSAA resolve fringe-free).
    pub fn render(
        &self,
        model: &GpuModel,
        cam: &ModelCamera,
        w: u32,
        h: u32,
    ) -> Option<egui::ColorImage> {
        let gl = &self.gl;
        let (w, h) = (w.clamp(16, 4096) as i32, h.clamp(16, 4096) as i32);

        let cam = resolve_camera(cam, model.bounds_min, model.bounds_max);
        let (_, radius) = bounds_sphere(model.bounds_min, model.bounds_max);
        let eye = eye_of(&cam);
        let view = look_at(eye, cam.target);
        let near = (cam.distance - radius * 2.0)
            .max(cam.distance * 0.01)
            .max(radius * 1e-3);
        let far = cam.distance + radius * 4.0;
        let proj = perspective(w as f32 / h as f32, near, far);
        let mvp = mat_mul(&proj, &view);

        unsafe {
            // MSAA target.
            let fbo = gl.create_framebuffer().ok()?;
            let color_rb = gl.create_renderbuffer().ok()?;
            let depth_rb = gl.create_renderbuffer().ok()?;
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color_rb));
            gl.renderbuffer_storage_multisample(
                glow::RENDERBUFFER,
                MSAA_SAMPLES,
                glow::RGBA8,
                w,
                h,
            );
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth_rb));
            gl.renderbuffer_storage_multisample(
                glow::RENDERBUFFER,
                MSAA_SAMPLES,
                glow::DEPTH_COMPONENT24,
                w,
                h,
            );
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::RENDERBUFFER,
                Some(color_rb),
            );
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(depth_rb),
            );
            let complete =
                gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;

            let mut pixels = None;
            if complete {
                gl.viewport(0, 0, w, h);
                gl.disable(glow::SCISSOR_TEST);
                gl.disable(glow::BLEND);
                gl.disable(glow::CULL_FACE);
                gl.clear_color(0.0, 0.0, 0.0, 1.0);
                gl.clear_depth_f64(1.0);
                gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

                // Background gradient (no depth).
                gl.disable(glow::DEPTH_TEST);
                gl.use_program(Some(self.bg_program));
                gl.bind_vertex_array(Some(self.bg_vao));
                gl.draw_arrays(glow::TRIANGLES, 0, 3);

                // Model.
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LESS);
                gl.use_program(Some(self.program));
                gl.uniform_matrix_4_f32_slice(Some(&self.u_mvp), false, &mvp);
                gl.uniform_matrix_4_f32_slice(Some(&self.u_view), false, &view);
                gl.bind_vertex_array(Some(model.vao));
                for draw in &model.draws {
                    gl.uniform_3_f32(
                        Some(&self.u_color),
                        draw.color[0],
                        draw.color[1],
                        draw.color[2],
                    );
                    gl.draw_elements(glow::TRIANGLES, draw.count, glow::UNSIGNED_INT, draw.offset);
                }
                gl.bind_vertex_array(None);
                gl.use_program(None);
                gl.disable(glow::DEPTH_TEST);

                // Resolve MSAA into a readable texture.
                let resolve_fbo = gl.create_framebuffer().ok()?;
                let resolve_tex = gl.create_texture().ok()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(resolve_tex));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w,
                    h,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::LINEAR as i32,
                );
                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(resolve_fbo));
                gl.framebuffer_texture_2d(
                    glow::DRAW_FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(resolve_tex),
                    0,
                );
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
                gl.blit_framebuffer(
                    0,
                    0,
                    w,
                    h,
                    0,
                    0,
                    w,
                    h,
                    glow::COLOR_BUFFER_BIT,
                    glow::NEAREST,
                );

                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(resolve_fbo));
                let mut buf = vec![0u8; (w * h * 4) as usize];
                gl.read_pixels(
                    0,
                    0,
                    w,
                    h,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut buf)),
                );
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
                gl.delete_framebuffer(resolve_fbo);
                gl.delete_texture(resolve_tex);

                // GL reads bottom-up; egui wants top-down.
                let row = (w * 4) as usize;
                let mut flipped = vec![0u8; buf.len()];
                for y in 0..h as usize {
                    let src = (h as usize - 1 - y) * row;
                    flipped[y * row..(y + 1) * row].copy_from_slice(&buf[src..src + row]);
                }
                pixels = Some(egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    &flipped,
                ));
            }

            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.delete_framebuffer(fbo);
            gl.delete_renderbuffer(color_rb);
            gl.delete_renderbuffer(depth_rb);
            pixels
        }
    }
}

fn bytemuck_f32_slice(v: &[f32]) -> &[u8] {
    // Plain-old-data reinterpretation; f32 has no invalid byte patterns.
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

fn bytemuck_u32_slice(v: &[u32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam(yaw: f32, pitch: f32, distance: f32) -> ModelCamera {
        ModelCamera {
            target: [0.0, 0.0, 0.0],
            yaw,
            pitch,
            distance,
        }
    }

    #[test]
    fn auto_fit_resolves_target_and_distance() {
        let fresh = ModelCamera::default();
        assert!(fresh.distance <= 0.0);
        let resolved = resolve_camera(&fresh, [-1.0, -1.0, -1.0], [3.0, 3.0, 3.0]);
        assert_eq!(resolved.target, [1.0, 1.0, 1.0]);
        assert!(resolved.distance > 0.0);
        // Whole model within the vertical FOV: distance > radius / tan(fov/2).
        let radius = (12.0f32).sqrt() * 0.5;
        assert!(resolved.distance > radius / (FOV_Y * 0.5).tan());
        // Already-resolved cameras pass through untouched.
        let again = resolve_camera(&resolved, [-1.0, -1.0, -1.0], [3.0, 3.0, 3.0]);
        assert_eq!(resolved, again);
    }

    #[test]
    fn eye_orbits_z_up() {
        // Yaw 0, pitch 0: eye sits on +X looking back at the target.
        let e = eye_of(&cam(0.0, 0.0, 10.0));
        assert!((e[0] - 10.0).abs() < 1e-4 && e[1].abs() < 1e-4 && e[2].abs() < 1e-4);
        // Pitch straight up-ish raises the eye.
        let e = eye_of(&cam(0.0, 1.0, 10.0));
        assert!(e[2] > 8.0);
    }

    #[test]
    fn look_at_places_target_on_view_axis() {
        let eye = [5.0, 5.0, 5.0];
        let target = [0.0, 0.0, 0.0];
        let m = look_at(eye, target);
        // Transform the target: should land on the -Z view axis.
        let x = m[0] * target[0] + m[4] * target[1] + m[8] * target[2] + m[12];
        let y = m[1] * target[0] + m[5] * target[1] + m[9] * target[2] + m[13];
        let z = m[2] * target[0] + m[6] * target[1] + m[10] * target[2] + m[14];
        assert!(x.abs() < 1e-4 && y.abs() < 1e-4);
        let dist = (75.0f32).sqrt();
        assert!((z + dist).abs() < 1e-3);
    }

    #[test]
    fn orbit_clamps_pitch_and_wraps_yaw() {
        let mut c = cam(0.0, 0.0, 10.0);
        orbit(&mut c, 0.0, -10_000.0);
        assert!(c.pitch <= 1.55);
        orbit(&mut c, 0.0, 10_000.0);
        assert!(c.pitch >= -1.55);
        orbit(&mut c, 100_000.0, 0.0);
        assert!(c.yaw.abs() <= std::f32::consts::TAU);
    }

    #[test]
    fn zoom_clamps_to_radius_range() {
        let mut c = cam(0.0, 0.3, 10.0);
        zoom(&mut c, 1e-9, 5.0);
        assert!(c.distance >= 5.0 * 0.02 - 1e-6);
        zoom(&mut c, 1e9, 5.0);
        assert!(c.distance <= 5.0 * 80.0 + 1e-3);
    }

    #[test]
    fn pan_moves_target_in_view_plane() {
        let mut c = cam(0.0, 0.0, 10.0);
        let before = c.target;
        pan(&mut c, 100.0, 0.0, 0.01);
        // Eye on +X: screen-right is world -Y … the target must move, and
        // stay at the same height for a horizontal pan.
        assert_ne!(before, c.target);
        assert!((c.target[2] - before[2]).abs() < 1e-4);
        assert_eq!(c.distance, 10.0);
    }

    #[test]
    fn poster_keys_are_pose_and_aspect_specific() {
        let a = poster_file_name("k1", &cam(0.0, 0.3, 10.0), 133);
        let b = poster_file_name("k1", &cam(0.1, 0.3, 10.0), 133);
        let c = poster_file_name("k1", &cam(0.0, 0.3, 10.0), 178);
        let d = poster_file_name("k2", &cam(0.0, 0.3, 10.0), 133);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert_eq!(a, poster_file_name("k1", &cam(0.0, 0.3, 10.0), 133));
        assert!(a.ends_with(".png"));
    }

    #[test]
    fn aspect_quantization_is_stable() {
        assert_eq!(aspect_q(240.0, 180.0), aspect_q(240.4, 180.1));
        assert_eq!(aspect_q(160.0, 90.0), 178);
        assert_eq!(aspect_q(100.0, 0.0), aspect_q(100.0, 1.0));
    }

    #[test]
    fn poster_size_honors_aspect() {
        let (w, h) = poster_size(178);
        assert_eq!(w, POSTER_LONG_EDGE);
        assert!((w as f32 / h as f32 - 1.78).abs() < 0.02);
        let (w, h) = poster_size(50);
        assert_eq!(h, POSTER_LONG_EDGE);
        assert_eq!(w, POSTER_LONG_EDGE / 2);
    }

    #[test]
    fn parse_progress_checkpoints_are_monotonic() {
        let p = ParseProgress::new();
        // No file size yet: nothing meaningful to report.
        assert_eq!(p.fraction(), 0.0);
        p.set_total(1000);
        assert_eq!(p.fraction(), 0.0);
        p.add_read(500);
        let half = p.fraction();
        assert!(half > 0.0 && half < READ_SPAN);
        p.add_read(500);
        assert!((p.fraction() - READ_SPAN).abs() < 1e-6);
        p.set_stage(STAGE_PARSING);
        assert!(p.fraction() >= READ_SPAN);
        assert_eq!(p.fraction(), PARSE_CHECKPOINT);
        p.set_stage(STAGE_DONE);
        assert_eq!(p.fraction(), 1.0);
    }

    #[test]
    fn parse_progress_clamps_overshoot() {
        let p = ParseProgress::new();
        p.set_total(100);
        p.add_read(1_000_000); // short files can over-report via chunking
        assert!(p.fraction() <= READ_SPAN + 1e-6);
    }

    #[test]
    fn quantize_px_steps_and_clamps() {
        assert_eq!(quantize_px(1.0), 64);
        assert_eq!(quantize_px(100.0), 128);
        assert_eq!(quantize_px(128.0), 128);
        assert_eq!(quantize_px(1e9), MAX_RENDER_PX);
    }

    #[test]
    fn raycast_hits_a_simple_triangle() {
        let model = rhino_mesh::Model {
            parts: vec![rhino_mesh::MeshPart {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: vec![[0.0, 0.0, 1.0]; 3],
                indices: vec![0, 1, 2],
                color: None,
            }],
            bounds_min: [0.0, 0.0, 0.0],
            bounds_max: [1.0, 1.0, 0.0],
        };
        let origin = [0.2, 0.2, 5.0];
        let dir = [0.0, 0.0, -1.0];
        let hit = raycast_model(&model, origin, dir).unwrap();
        assert!((hit[0] - 0.2).abs() < 1e-3);
        assert!((hit[1] - 0.2).abs() < 1e-3);
        assert!(hit[2].abs() < 1e-3);
    }

    #[test]
    fn distance_measurement_length() {
        let m = DistanceMeasurement {
            a: [0.0, 0.0, 0.0],
            b: [3.0, 4.0, 0.0],
        };
        assert!((m.length() - 5.0).abs() < 1e-4);
    }
}
