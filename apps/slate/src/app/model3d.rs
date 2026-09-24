//! Interactive 3D viewports for placed models (`MediaKind::Model`).
//!
//! Any recognized 3D file placed on the board is a **viewport node**: its saved
//! [`ModelCamera`] pose (journaled document state on the `ImageNode`) decides
//! which view of the model the node shows. Format readers live in
//! `model-preview` and all return one [`model_preview::PreviewScene`]. The
//! lifecycle keeps big models cheap by default:
//!
//! - **Locked (default).** The node paints a *poster* — a PNG rendered from
//!   the saved camera pose, cached on disk next to the thumbnail cache. No
//!   mesh, no GPU buffers, no per-frame work. Duplicating the node and
//!   changing each copy's camera is how one model appears from several
//!   perspectives across slides.
//! - **Unlocked (double-click the node, or hover → padlock).** The mesh is
//!   parsed off-thread
//!   (`model-preview` reads the file), uploaded to the GPU, and rendered
//!   live with orbit / pan / zoom:
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
use model_preview::{PreviewMesh, PreviewScene};
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

/// Nearest and farthest view depth of the model's bounding box, for the depth pass.
fn view_depth_range(view: &[f32; 16], min: [f32; 3], max: [f32; 3]) -> (f32, f32) {
    let mut near = f32::INFINITY;
    let mut far = 0.0f32;
    for i in 0..8 {
        let p = [
            if i & 1 == 0 { min[0] } else { max[0] },
            if i & 2 == 0 { min[1] } else { max[1] },
            if i & 4 == 0 { min[2] } else { max[2] },
        ];
        // Column-major: view-space z is row 2.
        let z = view[2] * p[0] + view[6] * p[1] + view[10] * p[2] + view[14];
        let d = -z;
        near = near.min(d);
        far = far.max(d);
    }
    let far = far.max(1e-3);
    let near = near.clamp(far * 1e-3, far * 0.999);
    (near, far)
}

/// Capture size near 512² at the node's aspect, in multiples of 8.
pub fn capture_size(w: f32, h: f32) -> (u32, u32) {
    let aspect = (w / h.max(1.0)).clamp(0.5, 2.0);
    let area = 512.0 * 512.0;
    let cw = (area * aspect).sqrt();
    let ch = cw / aspect;
    let snap = |v: f32| ((v / 8.0).round() as u32 * 8).clamp(256, 1024);
    (snap(cw), snap(ch))
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

/// Rhino-style orbit: the model turns with the drag (drag right = the model
/// swings right; drag down = its top tips toward you).
pub fn orbit(cam: &mut ModelCamera, dx: f32, dy: f32) {
    cam.yaw -= dx * ORBIT_PER_PX;
    cam.pitch = (cam.pitch + dy * ORBIT_PER_PX).clamp(-1.55, 1.55);
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
pub fn raycast_model(model: &PreviewScene, origin: [f32; 3], dir: [f32; 3]) -> Option<[f32; 3]> {
    let mut best_t = f32::INFINITY;
    let mut best = None;
    for part in &model.meshes {
        let idx = &part.indices;
        let pos = &part.positions;
        for tri in idx.as_chunks::<3>().0 {
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
    if !(-1.0..=1.0).contains(&ndc_z) {
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
    // Mesh extraction has its own cache version: adding TL_Brep support
    // must invalidate partial-model posters without flushing file thumbnails.
    const CACHE_KEY_VERSION: u32 = 1;
    format!(
        "v{CACHE_KEY_VERSION}-{cache_key}-{:016x}-a{aspect_q}.png",
        cam.cache_hash()
    )
}

pub fn poster_path(cache_key: &str, cam: &ModelCamera, aspect_q: u32) -> PathBuf {
    poster_dir().join(poster_file_name(cache_key, cam, aspect_q))
}

fn enscape_poster_name(cache_key: &str) -> String {
    format!("enscape-{cache_key}")
}

fn enscape_poster_path(cache_key: &str) -> PathBuf {
    poster_dir().join(format!("{}.png", enscape_poster_name(cache_key)))
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
/// no incremental hook in `model-preview`, so it reports a fixed checkpoint and
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
fn parse_with_progress(path: &Path, progress: &ParseProgress) -> Result<PreviewScene, String> {
    if let Some(msg) = model_preview::gap_message(path) {
        return Err(msg.to_string());
    }
    let bytes = read_counted(path, progress).map_err(|e| e.to_string())?;
    progress.set_stage(STAGE_PARSING);
    let result = model_preview::load_preview(path, &bytes).map_err(|e| e.to_string());
    progress.set_stage(STAGE_DONE);
    result
}

fn read_counted(path: &Path, progress: &ParseProgress) -> std::io::Result<Vec<u8>> {
    if atlas_core::cloud::is_dehydrated(path) {
        return Err(std::io::Error::other(
            "Model is cloud-only or unavailable. Make it available locally, then unlock again.",
        ));
    }
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
    Ready(Arc<PreviewScene>),
    /// Message shown in the node placeholder.
    Failed(String),
    /// Enscape standalone (or another external runtime). The message is the
    /// card; opening the program is a separate explicit action.
    External(String),
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
    pub(crate) rendered: Option<(u64, u32, u32)>,
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

impl LiveViewport {
    fn idle(&self) -> bool {
        // Loading is not inactivity. Give the user a full idle interval
        // after the first successful frame, even on a slow source.
        self.tex.is_some() && self.last_interact.elapsed() >= AUTO_LOCK
    }
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
    parse_tx: Sender<(String, Result<PreviewScene, String>)>,
    parse_rx: Receiver<(String, Result<PreviewScene, String>)>,
    /// Confirmed Enscape standalones, by item cache key. Never written into
    /// the workbook. A received file stays a normal card until this sniff
    /// finishes, and nothing here starts the program.
    pub(crate) external: std::collections::HashSet<String>,
    sniffed: std::collections::HashSet<String>,
    sniff_tx: Sender<(String, bool)>,
    sniff_rx: Receiver<(String, bool)>,
    engine_toast_shown: bool,
    /// The one Enscape process for this Slate session. Hidden while standby
    /// so the next click shows the same window instead of launching again.
    pub(crate) enscape: Option<EnscapeSession>,
    /// Last grabbed frame, so the card and the image generator don't reread
    /// the PNG on the UI thread.
    enscape_grabs: HashMap<String, EnscapeGrab>,
    pub(crate) enscape_stamps: HashMap<String, u64>,
}

struct EnscapeGrab {
    w: u32,
    h: u32,
    rgba: std::sync::Arc<Vec<u8>>,
}

pub struct ModelCapture {
    pub view: PathBuf,
    pub depth: Option<PathBuf>,
}

enum EnscapePhase {
    #[cfg(windows)]
    Starting {
        since: Instant,
        rx: Receiver<Result<super::enscape_host::Live, super::enscape_host::LaunchError>>,
    },
    #[cfg(windows)]
    Ready(super::enscape_host::Live),
    #[cfg(not(windows))]
    Absent,
}

pub(crate) struct EnscapeSession {
    key: String,
    node: NodeId,
    /// Window is on the card. False keeps the process and shows the grab.
    shown: bool,
    phase: EnscapePhase,
}

impl Default for ModelSpace {
    fn default() -> Self {
        let (parse_tx, parse_rx) = unbounded();
        let (sniff_tx, sniff_rx) = unbounded();
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
            external: std::collections::HashSet::new(),
            sniffed: std::collections::HashSet::new(),
            sniff_tx,
            sniff_rx,
            engine_toast_shown: false,
            enscape: None,
            enscape_grabs: HashMap::new(),
            enscape_stamps: HashMap::new(),
        }
    }
}

impl ModelSpace {
    /// Kick off (or re-poll) the off-thread parse of a model file.
    fn request_model(&mut self, cache_key: &str, path: &Path) {
        if self.external.contains(cache_key) || self.models.contains_key(cache_key) {
            return;
        }
        if let Some(msg) = model_preview::gap_message(path) {
            self.models.insert(
                cache_key.to_string(),
                CpuEntry {
                    state: ModelState::Failed(msg.to_string()),
                    last_used: Instant::now(),
                    progress: Arc::new(ParseProgress::new()),
                },
            );
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

    fn drain_sniffs(&mut self) -> bool {
        let mut any = false;
        while let Ok((key, enscape)) = self.sniff_rx.try_recv() {
            any = true;
            self.sniffed.insert(key.clone());
            if enscape {
                self.external.insert(key.clone());
                self.models.insert(
                    key,
                    CpuEntry {
                        state: ModelState::External(
                            super::enscape_host::resting_line().to_string(),
                        ),
                        last_used: Instant::now(),
                        progress: Arc::new(ParseProgress::new()),
                    },
                );
            }
        }
        any
    }

    /// Parsed CPU mesh when ready (for picking / measurement).
    pub fn mesh_for_key(&mut self, cache_key: &str) -> Option<Arc<PreviewScene>> {
        self.ready_model(cache_key)
    }

    fn ready_model(&mut self, cache_key: &str) -> Option<Arc<PreviewScene>> {
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

    fn render_capture_image(
        &mut self,
        gl: &Arc<glow::Context>,
        cache_key: &str,
        cam: &ModelCamera,
        w: u32,
        h: u32,
        depth: bool,
    ) -> Option<egui::ColorImage> {
        if !self.ensure_gpu(gl, cache_key) {
            return None;
        }
        let EngineSlot::Ready(engine) = &self.engine else {
            return None;
        };
        let gpu = self.gpu.get(cache_key)?;
        engine.render_capture(&gpu.model, cam, w, h, depth)
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
        let recognized = slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Model
            || self.model3d.external.contains(&item.cache_key);
        if !recognized || item.cache_key.is_empty() {
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
        if self.model3d.external.contains(&info.cache_key) {
            self.open_enscape_node(id);
            return;
        }
        if model_preview::gap_message(&info.path).is_some() {
            self.model3d.request_model(&info.cache_key, &info.path);
            return;
        }
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
        // Explicit activation is also retry: a file saved with meshes after
        // an earlier failure must not remain stuck until Slate restarts.
        if matches!(
            self.model3d.models.get(&info.cache_key).map(|e| &e.state),
            Some(ModelState::Failed(_))
        ) {
            self.model3d.models.remove(&info.cache_key);
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
            let aq = aspect_q(info.rect.w, info.rect.h);
            let name = poster_file_name(&info.cache_key, &cam, aq);
            // Keep the displayed frame alive throughout the transition,
            // including a failed high-resolution render or disk write.
            if let Some(tex) = vp.tex {
                self.model3d.posters.insert(name.clone(), tex);
            }
            if let Some(gl) = self.gl.clone() {
                let (pw, ph) = poster_size(aq);
                if let Some(img) = self
                    .model3d
                    .render_image(&gl, &info.cache_key, &cam, pw, ph)
                {
                    save_poster(&poster_path(&info.cache_key, &cam, aq), &img);
                    if let Some(tex) = self.model3d.posters.get_mut(&name) {
                        tex.set(img, egui::TextureOptions::LINEAR);
                    }
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

    /// Look at `.exe` items once. A hit becomes the same model card as any
    /// other 3D file, with copy that says to double-click. The sniff never
    /// starts the program. A dehydrated cloud file is left as a normal file
    /// card for this session so we do not download it to classify it.
    fn queue_executable_sniffs(&mut self) {
        let mut pending = Vec::new();
        for item in &self.doc().items {
            if item.cache_key.is_empty() || self.model3d.sniffed.contains(&item.cache_key) {
                continue;
            }
            let exe = item
                .path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
            if !exe {
                continue;
            }
            pending.push((item.cache_key.clone(), item.path.clone()));
        }
        for (key, path) in pending {
            self.model3d.sniffed.insert(key.clone());
            let tx = self.model3d.sniff_tx.clone();
            std::thread::spawn(move || {
                let yes = if atlas_core::cloud::is_dehydrated(&path) {
                    false
                } else {
                    model_preview::read_enscape_sample(&path)
                        .is_ok_and(|bytes| model_preview::sample_is_enscape(&bytes))
                };
                let _ = tx.send((key, yes));
            });
        }
    }

    /// Per-frame upkeep: parse results, auto-lock, eviction, repaint ticks.
    pub fn model3d_frame(&mut self, ctx: &egui::Context) {
        if self.model3d.drain_parses() | self.model3d.drain_sniffs() {
            ctx.request_repaint();
        }
        self.queue_executable_sniffs();
        self.maintain_enscape();

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
            .filter(|(_, v)| v.idle())
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
            Some(ModelState::Failed(_) | ModelState::External(_)) => return true,
            _ => return false, // still parsing
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
            if self.model3d.external.contains(&info.cache_key) {
                return;
            }
            self.model3d.request_model(&info.cache_key, &info.path);
            let blocked = matches!(
                self.model3d.models.get(&info.cache_key).map(|e| &e.state),
                Some(ModelState::Failed(_) | ModelState::External(_))
            );
            if !blocked {
                self.model3d.want_poster.insert(id);
            }
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
                    vp.last_interact = Instant::now();
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

    /// Show this Enscape card. The first time starts the program off the UI
    /// thread. Clicking away only hides it; the same session shows the window
    /// again. A different Enscape file replaces the one kept on standby.
    pub(crate) fn open_enscape_node(&mut self, id: NodeId) {
        let Some(info) = self.model_node_info(id) else {
            return;
        };
        if self
            .model3d
            .enscape
            .as_ref()
            .is_some_and(|session| session.key == info.cache_key && session.node == id)
        {
            if self.model3d.enscape.as_ref().is_some_and(|s| s.shown) {
                self.park_enscape();
            } else {
                self.show_enscape_session();
            }
            return;
        }
        let rotated = self
            .doc()
            .scene
            .node(id)
            .is_some_and(|n| n.rotation_deg.abs() > 0.5);
        let blocked = super::enscape_host::refusal(super::enscape_host::OpenFacts {
            windows: cfg!(windows),
            exists: info.path.is_file(),
            cloud_only: atlas_core::cloud::is_dehydrated(&info.path),
            rotated,
            already_open: false,
        });
        if let Some(msg) = blocked {
            self.show_enscape_block(&info.cache_key, msg);
            return;
        }
        if self
            .model3d
            .enscape
            .as_ref()
            .is_some_and(|session| session.key == info.cache_key)
        {
            if let Some(session) = self.model3d.enscape.as_mut() {
                session.node = id;
                session.shown = true;
            }
            self.set_enscape_line(&info.cache_key, super::enscape_host::resting_line());
            return;
        }
        if cfg!(test) {
            return;
        }
        #[cfg(windows)]
        {
            if self.frame_hwnd == 0 {
                self.show_enscape_block(&info.cache_key, super::enscape_host::NO_HOST);
                return;
            }
            self.discard_enscape();
            let (tx, rx) = unbounded();
            let path = info.path.clone();
            let node = id;
            let key = info.cache_key.clone();
            std::thread::spawn(move || {
                let _ = tx.send(super::enscape_host::launch(&path, node, key));
            });
            self.set_enscape_line(&info.cache_key, super::enscape_host::OPENING);
            self.model3d.enscape = Some(EnscapeSession {
                key: info.cache_key,
                node: id,
                shown: true,
                phase: EnscapePhase::Starting {
                    since: Instant::now(),
                    rx,
                },
            });
        }
    }

    /// Hide the window and keep the process. The card shows the last frame.
    pub(crate) fn park_enscape(&mut self) {
        let Some(session) = self.model3d.enscape.as_mut() else {
            return;
        };
        if !session.shown {
            return;
        }
        session.shown = false;
        let key = session.key.clone();
        #[cfg(windows)]
        let grabbed = match &session.phase {
            EnscapePhase::Ready(live) => {
                let frame = super::enscape_host::capture(live);
                super::enscape_host::place(live, None);
                frame
            }
            EnscapePhase::Starting { .. } => None,
        };
        #[cfg(not(windows))]
        let grabbed: Option<(u32, u32, Vec<u8>)> = None;
        if let Some((w, h, rgba)) = grabbed {
            self.remember_enscape_frame(key.clone(), w, h, rgba, true);
        }
        self.set_enscape_line(&key, super::enscape_host::resting_line());
    }

    fn show_enscape_session(&mut self) {
        if let Some(session) = self.model3d.enscape.as_mut() {
            session.shown = true;
        }
    }

    pub(crate) fn enscape_shown_node(&self) -> Option<NodeId> {
        let session = self.model3d.enscape.as_ref()?;
        session.shown.then_some(session.node)
    }

    pub(crate) fn enscape_shown_for(&self) -> std::time::Duration {
        let Some(session) = self.model3d.enscape.as_ref().filter(|s| s.shown) else {
            return std::time::Duration::ZERO;
        };
        #[cfg(windows)]
        {
            return match &session.phase {
                EnscapePhase::Starting { since, .. } => since.elapsed(),
                EnscapePhase::Ready(live) => live.since.elapsed(),
            };
        }
        #[cfg(not(windows))]
        {
            let _ = session;
            std::time::Duration::ZERO
        }
    }

    pub(crate) fn place_active_enscape(&self, rect: Option<(i32, i32, i32, i32)>) {
        #[cfg(windows)]
        if let Some(session) = self.model3d.enscape.as_ref().filter(|s| s.shown) {
            if let EnscapePhase::Ready(live) = &session.phase {
                super::enscape_host::place(live, rect);
            }
        }
        #[cfg(not(windows))]
        let _ = rect;
    }

    /// End the standby process. Used when another file replaces it, the card
    /// is deleted, or Slate exits.
    pub(crate) fn close_enscape(&mut self) {
        self.discard_enscape();
    }

    fn discard_enscape(&mut self) {
        let Some(session) = self.model3d.enscape.take() else {
            return;
        };
        #[cfg(windows)]
        match session.phase {
            EnscapePhase::Ready(live) => drop(live),
            EnscapePhase::Starting { rx, .. } => {
                std::thread::spawn(move || {
                    if let Ok(Ok(live)) = rx.recv() {
                        drop(live);
                    }
                });
            }
        }
        #[cfg(not(windows))]
        let _ = session;
    }

    fn remember_enscape_frame(&mut self, key: String, w: u32, h: u32, rgba: Vec<u8>, bump: bool) {
        if bump {
            let stamp = self.model3d.enscape_stamps.get(&key).copied().unwrap_or(0) + 1;
            self.model3d.enscape_stamps.insert(key.clone(), stamp);
        }
        let bytes = std::sync::Arc::new(rgba);
        self.model3d.enscape_grabs.insert(
            key.clone(),
            EnscapeGrab {
                w,
                h,
                rgba: bytes.clone(),
            },
        );
        self.model3d.posters.remove(&enscape_poster_name(&key));
        let path = enscape_poster_path(&key);
        std::thread::spawn(move || {
            let _ = std::fs::create_dir_all(poster_dir());
            let _ = image::save_buffer_with_format(
                path,
                bytes.as_slice(),
                w,
                h,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            );
        });
    }

    /// Attach a window once the background launch finishes, and hide it if
    /// the card was already parked.
    pub(crate) fn maintain_enscape(&mut self) {
        let Some(session) = self.model3d.enscape.as_ref() else {
            return;
        };
        let node = session.node;
        let key = session.key.clone();
        let left = self.doc().view.active_view != slate_doc::ViewKind::Board
            || self.model_node_info(node).is_none();
        if self.model_node_info(node).is_none() {
            self.discard_enscape();
            return;
        }
        if left {
            self.park_enscape();
        }
        #[cfg(windows)]
        {
            let failed = self.poll_enscape_launch();
            if let Some(msg) = failed {
                self.discard_enscape();
                self.show_enscape_block(&key, &msg);
                return;
            }
            let shown = self.model3d.enscape.as_ref().is_some_and(|s| s.shown);
            if let Some(session) = self.model3d.enscape.as_mut() {
                if let EnscapePhase::Ready(live) = &mut session.phase {
                    let had = live.has_window();
                    super::enscape_host::poll(live, self.frame_hwnd);
                    if !shown && live.has_window() {
                        super::enscape_host::place(live, None);
                    }
                    if !had && live.has_window() && shown {
                        // First pixels are grabbed when the user parks.
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    fn poll_enscape_launch(&mut self) -> Option<String> {
        let incoming = {
            let session = self.model3d.enscape.as_ref()?;
            match &session.phase {
                EnscapePhase::Ready(live) => {
                    if !live.has_window() && live.timed_out() {
                        return Some(super::enscape_host::NO_WINDOW.to_string());
                    }
                    return None;
                }
                EnscapePhase::Starting { since, rx } => {
                    let timed_out = since.elapsed() > super::enscape_host::OPEN_TIMEOUT;
                    match rx.try_recv() {
                        Ok(result) => Some(result),
                        Err(_) if timed_out => {
                            return Some(super::enscape_host::NO_WINDOW.to_string())
                        }
                        Err(_) => return None,
                    }
                }
            }
        };
        let Some(session) = self.model3d.enscape.as_mut() else {
            return None;
        };
        match incoming {
            Some(Ok(live)) => {
                session.phase = EnscapePhase::Ready(live);
                None
            }
            Some(Err(super::enscape_host::LaunchError::WrongMachine)) => {
                Some(super::enscape_host::WRONG_MACHINE.to_string())
            }
            Some(Err(super::enscape_host::LaunchError::Other(err))) => {
                Some(format!("Enscape didn't start on this computer. {err}"))
            }
            None => None,
        }
    }

    pub(crate) fn enscape_poster_texture(
        &mut self,
        ctx: &egui::Context,
        cache_key: &str,
    ) -> Option<TextureHandle> {
        let name = enscape_poster_name(cache_key);
        if let Some(tex) = self.model3d.posters.get(&name) {
            return Some(tex.clone());
        }
        if let Some(grab) = self.model3d.enscape_grabs.get(cache_key) {
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [grab.w as usize, grab.h as usize],
                grab.rgba.as_slice(),
            );
            let tex = ctx.load_texture(
                format!("slate-enscape-{name}"),
                color,
                egui::TextureOptions::LINEAR,
            );
            self.model3d.posters.insert(name, tex.clone());
            return Some(tex);
        }
        let img = image::open(enscape_poster_path(cache_key)).ok()?.to_rgba8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let color = egui::ColorImage::from_rgba_unmultiplied([w, h], img.as_raw());
        let tex = ctx.load_texture(
            format!("slate-enscape-{name}"),
            color,
            egui::TextureOptions::LINEAR,
        );
        self.model3d.posters.insert(name, tex.clone());
        Some(tex)
    }

    fn show_enscape_block(&mut self, cache_key: &str, msg: &str) {
        self.toast(msg);
        self.set_enscape_line(cache_key, msg);
    }

    fn set_enscape_line(&mut self, cache_key: &str, msg: &str) {
        if let Some(entry) = self.model3d.models.get_mut(cache_key) {
            entry.state = ModelState::External(msg.to_string());
        }
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
            Some(ModelState::Failed(msg) | ModelState::External(msg)) => Some(msg.as_str()),
            _ => None,
        }
    }

    /// Shaded view and exact depth from the node's current camera, written for an
    /// image generator. `Ok(None)` means the mesh is still loading; call again.
    pub fn capture_model_inputs(
        &mut self,
        id: NodeId,
        dir: &std::path::Path,
    ) -> Result<Option<ModelCapture>, String> {
        let info = self
            .model_node_info(id)
            .ok_or("That node is not a 3D model.")?;
        if let Some(message) = model_preview::gap_message(&info.path) {
            return Err(message.to_string());
        }
        if self.model3d.external.contains(&info.cache_key) {
            return self.capture_enscape_view(id, &info.cache_key, dir);
        }
        if let Some(message) = self.model_failure(&info.cache_key) {
            return Err(message.to_string());
        }
        let gl = self
            .gl
            .clone()
            .ok_or("3D viewports need GPU rendering (unavailable here).")?;
        let cam = self
            .model3d
            .live
            .get(&id)
            .map(|vp| vp.cam)
            .unwrap_or(info.cam);
        let (w, h) = capture_size(info.rect.w, info.rect.h);
        let Some(view) = self
            .model3d
            .render_capture_image(&gl, &info.cache_key, &cam, w, h, false)
        else {
            self.model3d.request_model(&info.cache_key, &info.path);
            return Ok(None);
        };
        let depth = self
            .model3d
            .render_capture_image(&gl, &info.cache_key, &cam, w, h, true)
            .ok_or("The depth pass failed.")?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let view_path = dir.join(format!("node-{}-view.png", id.0));
        let depth_path = dir.join(format!("node-{}-depth.png", id.0));
        write_fast_png(&view_path, &view, false)?;
        write_fast_png(&depth_path, &depth, true)?;
        Ok(Some(ModelCapture {
            view: view_path,
            depth: Some(depth_path),
        }))
    }

    /// The Enscape card's last frame is the picture the generator sees.
    /// There is no depth buffer in the standalone, so this is a source image
    /// rather than a mesh render. `Ok(None)` while the first launch is still
    /// starting and no frame exists yet.
    fn capture_enscape_view(
        &mut self,
        id: NodeId,
        key: &str,
        dir: &std::path::Path,
    ) -> Result<Option<ModelCapture>, String> {
        #[cfg(windows)]
        {
            let fresh = self.model3d.enscape.as_ref().and_then(|session| {
                if session.key != key || !session.shown {
                    return None;
                }
                match &session.phase {
                    EnscapePhase::Ready(live) => super::enscape_host::capture(live),
                    EnscapePhase::Starting { .. } => None,
                }
            });
            if let Some((w, h, rgba)) = fresh {
                self.remember_enscape_frame(key.to_string(), w, h, rgba, false);
            }
        }
        #[cfg(windows)]
        let starting = self.model3d.enscape.as_ref().is_some_and(|session| {
            session.key == key && matches!(session.phase, EnscapePhase::Starting { .. })
        });
        #[cfg(not(windows))]
        let starting = false;
        if starting && !self.model3d.enscape_grabs.contains_key(key) {
            return Ok(None);
        }
        let stored = if let Some(grab) = self.model3d.enscape_grabs.get(key) {
            Some((grab.w, grab.h, grab.rgba.clone()))
        } else if let Ok(img) = image::open(enscape_poster_path(key)) {
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            Some((w, h, std::sync::Arc::new(rgba.into_raw())))
        } else {
            None
        };
        let Some((w, h, rgba)) = stored else {
            return Err("Open this Enscape card once so the generator has a view of it.".into());
        };
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let view = dir.join(format!("node-{id}-view.png", id = id.0));
        image::save_buffer_with_format(
            &view,
            rgba.as_slice(),
            w,
            h,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|e| e.to_string())?;
        Ok(Some(ModelCapture { view, depth: None }))
    }

    /// One-time toast when shader setup failed (very old GPUs).
    pub fn note_engine_failure(&mut self) {
        if matches!(self.model3d.engine, EngineSlot::Failed) && !self.model3d.engine_toast_shown {
            self.model3d.engine_toast_shown = true;
            self.toast("3D viewport unavailable — GPU shader setup failed");
        }
    }
}

/// Fast compression: these captures are rewritten every live frame.
pub(crate) fn write_fast_png(
    path: &Path,
    img: &egui::ColorImage,
    luma: bool,
) -> Result<(), String> {
    use image::ImageEncoder;
    let (w, h) = (img.size[0] as u32, img.size[1] as u32);
    let (bytes, color): (Vec<u8>, image::ExtendedColorType) = if luma {
        (
            img.pixels.iter().map(|p| p.r()).collect(),
            image::ExtendedColorType::L8,
        )
    } else {
        (
            img.pixels
                .iter()
                .flat_map(|p| [p.r(), p.g(), p.b()])
                .collect(),
            image::ExtendedColorType::Rgb8,
        )
    };
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    image::codecs::png::PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(&bytes, w, h, color)
    .map_err(|e| e.to_string())
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
uniform vec3 u_mask;
// 0 shaded, 1 arctic (white clay), 2 material mask, 3 z-buffer.
uniform float u_mode;
// y, z are 1/near and 1/far of the model. Used by the z-buffer pass.
uniform vec3 u_depth;
out vec4 frag;
void main() {
    vec3 n = normalize(v_nrm);
    vec3 v = normalize(-v_pos);
    if (dot(n, v) < 0.0) n = -n;
    if (u_mode > 2.5) {
        float z = max(-v_pos.z, 1e-4);
        float d = clamp((1.0 / z - u_depth.z) / max(u_depth.y - u_depth.z, 1e-9), 0.0, 1.0);
        frag = vec4(vec3(d), 1.0);
        return;
    }
    if (u_mode > 1.5) {
        float edge = fwidth(n.x) + fwidth(n.y) + fwidth(n.z);
        vec3 mask = mix(u_mask, vec3(0.0), clamp(edge * 1.6, 0.0, 1.0));
        frag = vec4(mask, 1.0);
        return;
    }
    vec3 l = normalize(vec3(0.25, 0.4, 1.0));
    vec3 base = u_mode > 0.5 ? vec3(0.96) : pow(u_color, vec3(2.2));
    float ndl = max(dot(n, l), 0.0);
    float hemi = 0.5 + 0.5 * n.y;
    float amb = u_mode > 0.5 ? 0.42 : 0.22;
    float wrap = u_mode > 0.5 ? 0.28 : 0.16;
    float key = u_mode > 0.5 ? 0.38 : 0.72;
    vec3 col = base * (amb + wrap * hemi) + base * ndl * key;
    if (u_mode < 0.5) {
        vec3 hv = normalize(l + v);
        col += vec3(0.18) * pow(max(dot(n, hv), 0.0), 48.0);
    }
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

/// Stable saturated color for a mesh part that has no authored color.
/// Adjacent parts land far apart on the hue wheel.
pub fn mask_palette(index: usize) -> [f32; 3] {
    let h = (index as f32 * 0.618_034).fract();
    let s = 0.72;
    let v = 0.92;
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// Generator captures stand the model on a ground this many radii wide.
const GROUND_EXTENT: f32 = 60.0;
/// Slightly darker than parts, so edges separate a model from its ground.
const GROUND_COLOR: [u8; 3] = [150, 153, 150];

/// One `glDrawElements` range with its uniform color.
struct DrawRange {
    /// Byte offset into the index buffer.
    offset: i32,
    count: i32,
    color: [f32; 3],
    /// Flat segmentation color. Authored part colors stay shared; parts
    /// without a color get a stable palette entry.
    mask: [f32; 3],
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
    u_mask: glow::UniformLocation,
    u_mode: glow::UniformLocation,
    u_depth: glow::UniformLocation,
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
            let u_mask = gl.get_uniform_location(program, "u_mask")?;
            let u_mode = gl.get_uniform_location(program, "u_mode")?;
            let u_depth = gl.get_uniform_location(program, "u_depth")?;
            // Core profiles need a bound VAO even for bufferless draws.
            let bg_vao = gl.create_vertex_array().ok()?;
            Some(ModelEngine {
                gl,
                program,
                u_mvp,
                u_view,
                u_color,
                u_mask,
                u_mode,
                u_depth,
                bg_program,
                bg_vao,
            })
        }
    }

    /// Upload a parsed model: one interleaved (pos, normal) vertex buffer,
    /// one index buffer, per-color draw ranges (brep faces usually share a
    /// color, so most files collapse to a single draw call).
    pub fn upload(&self, model: &PreviewScene) -> Option<GpuModel> {
        let gl = &self.gl;

        // Group parts by color to minimize draw calls.
        let mut order: Vec<usize> = (0..model.meshes.len()).collect();
        let color_of = |p: &PreviewMesh| -> [u8; 3] { p.color.unwrap_or([255, 255, 255]) };
        order.sort_by_key(|i| {
            (
                model.meshes[*i].color.is_some(),
                color_of(&model.meshes[*i]),
            )
        });

        let total_verts: usize = model.meshes.iter().map(|p| p.positions.len()).sum();
        let total_idx: usize = model.meshes.iter().map(|p| p.indices.len()).sum();
        let mut verts: Vec<f32> = Vec::with_capacity(total_verts * 6);
        let mut indices: Vec<u32> = Vec::with_capacity(total_idx);
        let mut draws: Vec<DrawRange> = Vec::new();
        let mut base_vertex: u32 = 0;

        for i in order {
            let part = &model.meshes[i];
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
            let mask = part
                .color
                .map(|c| {
                    [
                        c[0] as f32 / 255.0,
                        c[1] as f32 / 255.0,
                        c[2] as f32 / 255.0,
                    ]
                })
                .unwrap_or_else(|| mask_palette(i));
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
                Some(last) if last.color == color && last.mask == mask => last.count += count,
                _ => draws.push(DrawRange {
                    offset: (start_index * 4) as i32,
                    count,
                    color,
                    mask,
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
        self.render_pass(model, None, cam, w, h, cam.display)
    }

    /// A generator capture: the model standing on a ground plane at its base,
    /// shaded, or as inverse depth (nearest geometry white, sky black).
    pub fn render_capture(
        &self,
        model: &GpuModel,
        cam: &ModelCamera,
        w: u32,
        h: u32,
        depth: bool,
    ) -> Option<egui::ColorImage> {
        let ground = self.ground_for(model);
        let mode = if depth {
            slate_doc::scene::ModelDisplay::Depth
        } else {
            slate_doc::scene::ModelDisplay::Shaded
        };
        let image = self.render_pass(model, ground.as_ref(), cam, w, h, mode);
        if let Some(ground) = ground {
            self.free(ground);
        }
        image
    }

    /// Massing models rarely include a site; a render needs somewhere to stand.
    fn ground_for(&self, model: &GpuModel) -> Option<GpuModel> {
        let (center, radius) = bounds_sphere(model.bounds_min, model.bounds_max);
        let z = model.bounds_min[2] - radius * 1e-3;
        let s = radius * GROUND_EXTENT;
        let (x, y) = (center[0], center[1]);
        self.upload(&PreviewScene {
            format: "ground",
            meshes: vec![PreviewMesh {
                positions: vec![
                    [x - s, y - s, z],
                    [x + s, y - s, z],
                    [x + s, y + s, z],
                    [x - s, y + s, z],
                ],
                normals: vec![[0.0, 0.0, 1.0]; 4],
                indices: vec![0, 1, 2, 0, 2, 3],
                color: Some(GROUND_COLOR),
            }],
            bounds_min: model.bounds_min,
            bounds_max: model.bounds_max,
            notes: Vec::new(),
        })
    }

    fn render_pass(
        &self,
        model: &GpuModel,
        ground: Option<&GpuModel>,
        cam: &ModelCamera,
        w: u32,
        h: u32,
        mode: slate_doc::scene::ModelDisplay,
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
        let far = cam.distance
            + radius
                * if ground.is_some() {
                    GROUND_EXTENT * 1.5
                } else {
                    4.0
                };
        let proj = perspective(w as f32 / h as f32, near, far);
        let mvp = mat_mul(&proj, &view);
        let (depth_near, depth_far) = view_depth_range(&view, model.bounds_min, model.bounds_max);
        // With a ground, the fade runs from the nearest visible ground at the
        // frame's bottom edge past the model toward the horizon.
        let (depth_near, depth_far) = match ground {
            Some(ground) => {
                let height = eye[2] - ground.bounds_min[2];
                let down = cam.pitch + FOV_Y * 0.5;
                let nearest = if height > 0.0 && down > 0.01 {
                    height / down.sin() * (FOV_Y * 0.5).cos()
                } else {
                    depth_near
                };
                (
                    nearest.clamp(depth_near * 0.05, depth_near),
                    depth_far * 4.0,
                )
            }
            None => (depth_near, depth_far),
        };

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
                let arctic = mode == slate_doc::scene::ModelDisplay::Arctic;
                if arctic {
                    gl.clear_color(0.93, 0.93, 0.91, 1.0);
                } else {
                    gl.clear_color(0.0, 0.0, 0.0, 1.0);
                }
                gl.clear_depth_f64(1.0);
                gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

                // Background gradient for the shaded view. Arctic is a light
                // field; material and z-buffer stay on a black field.
                gl.disable(glow::DEPTH_TEST);
                if mode == slate_doc::scene::ModelDisplay::Shaded {
                    gl.use_program(Some(self.bg_program));
                    gl.bind_vertex_array(Some(self.bg_vao));
                    gl.draw_arrays(glow::TRIANGLES, 0, 3);
                }

                // Model.
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LESS);
                gl.use_program(Some(self.program));
                gl.uniform_matrix_4_f32_slice(Some(&self.u_mvp), false, &mvp);
                gl.uniform_matrix_4_f32_slice(Some(&self.u_view), false, &view);
                let mode_id = match mode {
                    slate_doc::scene::ModelDisplay::Shaded => 0.0,
                    slate_doc::scene::ModelDisplay::Arctic => 1.0,
                    slate_doc::scene::ModelDisplay::Material => 2.0,
                    slate_doc::scene::ModelDisplay::Depth => 3.0,
                };
                gl.uniform_1_f32(Some(&self.u_mode), mode_id);
                gl.uniform_3_f32(Some(&self.u_depth), 0.0, 1.0 / depth_near, 1.0 / depth_far);
                for part in std::iter::once(model).chain(ground) {
                    gl.bind_vertex_array(Some(part.vao));
                    for draw in &part.draws {
                        gl.uniform_3_f32(
                            Some(&self.u_color),
                            draw.color[0],
                            draw.color[1],
                            draw.color[2],
                        );
                        gl.uniform_3_f32(
                            Some(&self.u_mask),
                            draw.mask[0],
                            draw.mask[1],
                            draw.mask[2],
                        );
                        gl.draw_elements(
                            glow::TRIANGLES,
                            draw.count,
                            glow::UNSIGNED_INT,
                            draw.offset,
                        );
                    }
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
    use super::super::tests::Harness;
    use super::*;

    fn live_model(tag: &str) -> (Harness, NodeId) {
        let mut h = Harness::new(tag);
        h.app.leave_home();
        h.app.ensure_work_tab();
        let source = h.base.join("model.3dm");
        std::fs::write(
            &source,
            include_bytes!(
                "../../../../crates/rhino-mesh/tests/fixtures/brep_with_render_mesh.3dm"
            ),
        )
        .unwrap();
        let model = model_preview::load_preview(&source, &std::fs::read(&source).unwrap()).unwrap();
        let items = h.app.add_paths(&[source]);
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_items_on_board(&items, egui::Pos2::ZERO);
        let id = h.app.doc().scene.nodes.last().unwrap().id;
        let info = h.app.model_node_info(id).unwrap();
        h.app.zoom_to_rect(info.rect);
        h.frame();
        let cam = resolve_camera(&info.cam, model.bounds_min, model.bounds_max);
        h.app.model3d.live.insert(
            id,
            LiveViewport {
                cache_key: info.cache_key,
                cam,
                before: info.cam,
                last_interact: Instant::now(),
                tex: Some(h.ctx.load_texture(
                    "test-model-frame",
                    egui::ColorImage::new([2, 2], egui::Color32::RED),
                    Default::default(),
                )),
                rendered: Some((cam.cache_hash(), 2, 2)),
                radius: bounds_sphere(model.bounds_min, model.bounds_max).1,
                toolbar_expanded: false,
                tool: ModelViewportTool::Navigate,
                measure_first: None,
                measure_preview: None,
                measures: Vec::new(),
            },
        );
        (h, id)
    }

    #[test]
    fn live_model_drops_the_selection_cast_until_it_locks() {
        let (mut h, id) = live_model("model_cast");
        h.app.board_sel.clear();
        h.app.board_sel.insert(id);
        assert!(h.app.frame_chrome_suppressed(id));
        assert!(h.app.selection_stringers_suppressed());
        h.app.lock_model(id);
        assert!(!h.app.frame_chrome_suppressed(id));
        assert!(!h.app.selection_stringers_suppressed());
    }

    #[test]
    fn model_input_routes_orbit_pan_and_zoom_without_moving_the_board() {
        let (mut h, id) = live_model("model_input");
        let rect = h.app.doc().scene.node(id).unwrap().rect;
        let board_cam = h.app.tab().cam;
        let start = h.app.canvas_rect.center();
        let world = h.app.board_xf().s2w(start);
        assert_eq!(h.app.live_model_at(world.x, world.y), Some(id));
        assert_eq!(h.app.board_tool, super::super::board::BoardTool::Select);
        // egui Areas settle their size across initial passes; the live
        // toolbar must have its final hit rect before the pointer presses.
        for _ in 0..3 {
            h.frame_with(|input| input.events.push(egui::Event::PointerMoved(start)));
        }
        for pan in [false, true] {
            let before = h.app.model3d.live[&id].cam;
            let modifiers = egui::Modifiers {
                shift: pan,
                ..Default::default()
            };
            h.frame_with(|input| {
                input.modifiers = modifiers;
                input.events.push(egui::Event::PointerMoved(start));
                input.events.push(egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                });
            });
            for dx in [20.0, 60.0] {
                h.frame_with(|input| {
                    input.modifiers = modifiers;
                    input
                        .events
                        .push(egui::Event::PointerMoved(start + egui::vec2(dx, 20.0)));
                });
                assert!(
                    matches!(
                        h.app.board_drag,
                        Some(super::super::board::BoardDrag::ModelOrbit { .. })
                    ),
                    "drag routing: {:?}, align={}, pointer={:?}, canvas={:?}",
                    h.app.board_drag.as_ref().map(std::mem::discriminant),
                    h.app.board_align_eat_press,
                    h.ctx.pointer_hover_pos(),
                    h.app.canvas_rect
                );
            }
            h.frame_with(|input| {
                input.modifiers = modifiers;
                input.events.push(egui::Event::PointerButton {
                    pos: start + egui::vec2(60.0, 20.0),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                });
            });
            let after = h.app.model3d.live[&id].cam;
            if pan {
                assert_ne!(after.target, before.target, "Shift+drag pans the model");
                assert_eq!((after.yaw, after.pitch), (before.yaw, before.pitch));
            } else {
                assert_ne!(after.yaw, before.yaw, "drag orbits the model");
                assert_eq!(after.target, before.target);
            }
        }
        let distance = h.app.model3d.live[&id].cam.distance;
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(start));
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 50.0),
                modifiers: Default::default(),
            });
        });
        assert_ne!(h.app.model3d.live[&id].cam.distance, distance);
        assert_eq!(h.app.doc().scene.node(id).unwrap().rect, rect);
        assert_eq!(h.app.tab().cam.offset, board_cam.offset);
        assert_eq!(h.app.tab().cam.z, board_cam.z);
    }

    #[test]
    fn freezing_keeps_the_displayed_frame_and_journals_the_camera_without_gl() {
        let (mut h, id) = live_model("model_freeze");
        h.app.model_drag(id, 40.0, 20.0, false, 600.0);
        let cam = h.app.model3d.live[&id].cam;
        let texture = h.app.model3d.live[&id].tex.as_ref().unwrap().id();
        h.app.lock_model(id);
        let info = h.app.model_node_info(id).unwrap();
        assert_eq!(info.cam, cam);
        assert!(!h.app.model3d.live.contains_key(&id));
        assert_eq!(
            h.app.model_poster_texture(&h.ctx, &info).unwrap().id(),
            texture
        );
        h.app.board_undo();
        assert_eq!(
            h.app.model_node_info(id).unwrap().cam,
            ModelCamera::default()
        );
    }

    #[test]
    fn loading_does_not_consume_the_model_idle_interval() {
        let (mut h, id) = live_model("model_idle");
        let vp = h.app.model3d.live.get_mut(&id).unwrap();
        vp.last_interact = Instant::now() - AUTO_LOCK - Duration::from_secs(1);
        let tex = vp.tex.take();
        assert!(!vp.idle());
        vp.tex = tex;
        assert!(vp.idle());
        vp.last_interact = Instant::now();
        assert!(!vp.idle());
    }

    #[test]
    fn saving_and_changing_tabs_preserve_live_model_cameras() {
        let (mut h, id) = live_model("model_save");
        h.app.model_drag(id, 40.0, 20.0, true, 600.0);
        let cam = h.app.model3d.live[&id].cam;
        let tab_id = h.app.tab().id;
        let path = h.base.join("model-view.slate");
        h.app.save_doc_to(tab_id, path.clone());
        assert!(h.app.model3d.live.is_empty());
        assert_eq!(h.app.model_node_info(id).unwrap().cam, cam);
        assert!(!h.app.tab().dirty);
        let restored = slate_doc::SlateDoc::load_from(&path).unwrap();
        let NodeKind::Image(restored_image) = &restored.scene.node(id).unwrap().kind else {
            panic!("saved model node");
        };
        assert_eq!(restored_image.model, cam);

        let (mut h, id) = live_model("model_new_tab");
        h.app.model_drag(id, 40.0, 20.0, false, 600.0);
        let cam = h.app.model3d.live[&id].cam;
        h.app.tab_mut().dirty = false;
        h.app.close_tab(h.app.active_tab);
        assert!(
            !h.app.tabs.is_empty(),
            "closing must notice an unsaved live pose"
        );
        assert_eq!(h.app.model_node_info(id).unwrap().cam, cam);
        let (mut h, id) = live_model("model_new_tab_live");
        h.app.model_drag(id, 20.0, 10.0, false, 600.0);
        let cam = h.app.model3d.live[&id].cam;
        h.app.new_tab();
        assert!(h.app.model3d.live.is_empty());
        h.app.switch_tab(0);
        assert_eq!(h.app.model_node_info(id).unwrap().cam, cam);
    }

    #[test]
    fn display_mode_changes_the_poster_key() {
        let shaded = cam(0.2, 0.3, 8.0);
        let mut arctic = shaded;
        arctic.display = slate_doc::scene::ModelDisplay::Arctic;
        assert_ne!(shaded.cache_hash(), arctic.cache_hash());
        let a = mask_palette(0);
        let b = mask_palette(1);
        assert_ne!(a, b);
    }

    fn cam(yaw: f32, pitch: f32, distance: f32) -> ModelCamera {
        ModelCamera {
            target: [0.0, 0.0, 0.0],
            yaw,
            pitch,
            distance,
            display: slate_doc::scene::ModelDisplay::Shaded,
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
    fn generator_captures_keep_the_card_aspect_near_512_square() {
        assert_eq!(capture_size(400.0, 400.0), (512, 512));
        let (w, h) = capture_size(1600.0, 900.0);
        assert_eq!((w % 8, h % 8), (0, 0));
        assert!((w as f32 / h as f32 - 16.0 / 9.0).abs() < 0.05);
        assert!(((w * h) as f32 - 262_144.0).abs() / 262_144.0 < 0.05);
        let (w, h) = capture_size(100.0, 1000.0);
        assert!(w >= 256 && h <= 1024, "extreme aspects are clamped");
    }

    #[test]
    fn depth_range_spans_the_model_bounds_in_front_of_the_camera() {
        let cam = ModelCamera {
            target: [0.0, 0.0, 0.0],
            yaw: 0.3,
            pitch: 0.2,
            distance: 10.0,
            display: slate_doc::scene::ModelDisplay::Shaded,
        };
        let view = look_at(eye_of(&cam), cam.target);
        let (near, far) = view_depth_range(&view, [-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]);
        assert!(near > 7.0 && near < 10.0, "near {near}");
        assert!(far > 10.0 && far < 13.0, "far {far}");
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
    fn every_model_extension_has_one_preview_owner() {
        let mut preview: Vec<_> = model_preview::extensions().collect();
        preview.sort_unstable();
        let mut media = slate_doc::media::MediaGroup::Model.extensions();
        media.sort_unstable();
        assert_eq!(
            preview, media,
            "slate-doc model extensions and model-preview readers drifted"
        );
    }

    #[test]
    fn raycast_hits_a_simple_triangle() {
        let model = PreviewScene {
            format: "test",
            meshes: vec![PreviewMesh {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: vec![[0.0, 0.0, 1.0]; 3],
                indices: vec![0, 1, 2],
                color: None,
            }],
            bounds_min: [0.0, 0.0, 0.0],
            bounds_max: [1.0, 1.0, 0.0],
            notes: Vec::new(),
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
