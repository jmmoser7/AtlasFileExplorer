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
//! - **Unlocked (double-click the node).** The mesh is parsed off-thread
//!   (`model-preview` reads the file), uploaded to the GPU, and rendered
//!   live with orbit / pan / zoom:
//!   drag = orbit, Shift+drag = pan, scroll = zoom. At most [`MAX_LIVE`]
//!   viewports stay live; unlocking more locks the least-recently-used one.
//!   Navigate is the only resting tool. Measure is armed from the node's
//!   selection strip and falls back to Navigate after one measurement.
//! - **Locking.** Esc (after peeling any measure state) or a primary press
//!   outside the node locks it, as does [`AUTO_LOCK`] of idle time: the
//!   current framebuffer pose is written out as the new poster, the camera
//!   is committed to the document as one undoable patch, and the GPU
//!   resources are released.
//!
//! Rendering happens **offscreen inside `update`** (the glow GL context is
//! current there): the scene is drawn into an MSAA framebuffer, resolved,
//! sampled directly by egui as a native texture — so model nodes go
//! through the exact same `textured_polygon` path as every other board
//! image (stroke, corners, opacity all apply), and the headless test
//! harness (no GL) simply never sees a live viewport.
//!
//! Files without cached render meshes ("Save Small", wireframe-only saves)
//! degrade to the embedded-preview thumbnail the thumb pool already
//! extracts — same look as before this feature existed.

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, TextureHandle};
use eframe::glow::{self, HasContext};
#[cfg(test)]
use model_preview::PreviewMesh;
use model_preview::PreviewScene;
use slate_doc::scene::{ImageAdjust, ModelCamera, ModelDisplay, NodeKind};
use slate_doc::NodeId;

use super::SlateApp;

mod gpu;
mod materials;
mod posters;
#[cfg(test)]
use gpu::mask_palette;
pub use gpu::ModelEngine;
use gpu::{GpuModel, LiveTarget};

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
/// Low-res chip faces for the viewport-display stringer (photo-filter size).
const DISPLAY_SWATCH_EDGE: u32 = 32;
const DISPLAY_SWATCH_MODES: [ModelDisplay; 5] = ModelDisplay::ALL;
/// Vertical field of view, radians (Rhino default perspective lens).
pub use model_preview::view_meta::FOV_Y;
/// Orbit sensitivity, radians per screen px.
const ORBIT_PER_PX: f32 = 0.008;

// ---------- viewport tools ----------

/// Active tool inside a live 3D viewport. Navigation matches Rhino's default
/// perspective viewport and is the default; measure tools own primary clicks
/// and are armed from the selection strip (`board.model_measure`).
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

pub fn lerp_model_camera(from: ModelCamera, to: ModelCamera, t: f32) -> ModelCamera {
    let t = t.clamp(0.0, 1.0);
    ModelCamera {
        target: [
            from.target[0] + (to.target[0] - from.target[0]) * t,
            from.target[1] + (to.target[1] - from.target[1]) * t,
            from.target[2] + (to.target[2] - from.target[2]) * t,
        ],
        yaw: from.yaw + (to.yaw - from.yaw) * t,
        pitch: from.pitch + (to.pitch - from.pitch) * t,
        distance: from.distance + (to.distance - from.distance) * t,
        display: if t >= 1.0 { to.display } else { from.display },
        materials: if t >= 1.0 {
            to.materials
        } else {
            from.materials
        },
    }
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
    // Mesh extraction and render changes invalidate posters, not file thumbnails.
    const CACHE_KEY_VERSION: u32 = 4;
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

/// Live frame size for a node on screen, and the aspect its projection uses.
/// The frame is stretched onto the node, so it projects at the node's aspect.
pub(crate) fn live_frame_size(screen_w: f32, screen_h: f32, ppp: f32) -> (u32, u32, f32) {
    let w = quantize_px(screen_w * ppp);
    let h = quantize_px(screen_h * ppp);
    (w, h, screen_w / screen_h.max(1.0))
}

/// Pixel size of a viewport screenshot for a node of `w` x `h`: about 512²
/// at the node's own aspect.
pub(crate) fn screenshot_size(w: f32, h: f32) -> (u32, u32) {
    let aspect = (w / h.max(1.0)).clamp(1.0 / 16.0, 16.0);
    let sw = (512.0 * 512.0 * aspect).sqrt();
    let px = |v: f32| (v.round() as u32).clamp(16, 4096);
    (px(sw), px(sw / aspect))
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
fn parse_with_progress(
    path: &Path,
    progress: &ParseProgress,
) -> Result<(PreviewScene, String), String> {
    if let Some(msg) = model_preview::gap_message(path) {
        return Err(msg.to_string());
    }
    let bytes = read_counted(path, progress).map_err(|e| e.to_string())?;
    let hash = model_preview::view_meta::hash_bytes(&bytes);
    progress.set_stage(STAGE_PARSING);
    let mut scene =
        model_preview::load_preview_with_reader(path, &bytes, &mut materials::read_dependency)
            .map_err(|e| e.to_string())?;
    materials::decode_textures(&mut scene);
    progress.set_stage(STAGE_DONE);
    Ok((scene, hash))
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

/// Derived camera blend when picking a wired saved view (not journaled).
pub struct ViewTween {
    pub from: ModelCamera,
    pub to: ModelCamera,
    pub started: Instant,
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
    /// An egui-managed copy of the displayed frame, only where no GL exists
    /// (the headless harness). GPU viewports paint their live slot instead.
    tex: Option<TextureHandle>,
    /// The live slot holds a drawn frame.
    shown: bool,
    /// Camera hash, frame size, filter hash, and projection aspect bits of
    /// the drawn frame.
    pub(crate) rendered: Option<(u64, u32, u32, u64, u32)>,
    /// Bounds radius once known (zoom clamps, pan scale).
    pub radius: f32,
    pub tool: ModelViewportTool,
    /// In-progress first point for point-to-point measure.
    pub measure_first: Option<[f32; 3]>,
    /// Hover/drag preview endpoint.
    pub measure_preview: Option<[f32; 3]>,
    /// Completed measurements this live session (cleared on lock).
    pub measures: Vec<DistanceMeasurement>,
    pub view_tween: Option<ViewTween>,
}

impl LiveViewport {
    fn idle(&self) -> bool {
        // Loading is not inactivity. Give the user a full idle interval
        // after the first successful frame, even on a slow source.
        (self.tex.is_some() || self.shown) && self.last_interact.elapsed() >= AUTO_LOCK
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

type ModelNodeKey = (u64, NodeId);

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
    /// Shared recency of each CPU/GPU poster pair.
    poster_residency: posters::Residency,
    /// Nodes whose poster needs (re)generation once mesh + GL are ready.
    want_poster: std::collections::HashSet<ModelNodeKey>,
    /// Bounds by cache key (kept even after CPU mesh eviction — needed to
    /// resolve auto-fit cameras cheaply, e.g. for artifact export).
    pub bounds: HashMap<String, ([f32; 3], [f32; 3])>,
    /// SHA-256 hex of model file bytes, keyed by item cache key.
    pub model_hashes: HashMap<String, String>,
    parse_tx: Sender<(String, Option<String>, Result<PreviewScene, String>)>,
    parse_rx: Receiver<(String, Option<String>, Result<PreviewScene, String>)>,
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
    /// Generation-tagged chip renders for the display stringer (never paint).
    display_swatch_gen: HashMap<ModelNodeKey, u64>,
    display_swatch_stamp: HashMap<ModelNodeKey, u64>,
    display_swatch_queue: VecDeque<(u64, NodeId, u64, ModelDisplay)>,
    display_swatch_pixels: HashMap<(u64, NodeId, u64, ModelDisplay), egui::ColorImage>,
    /// Parsed `slateview` cameras from wired screenshot items (item cache key).
    pub view_wire_meta: HashMap<String, Option<ModelCamera>>,
    /// Decoded poster pixels (key = on-disk poster file name).
    poster_pixels: HashMap<String, std::sync::Arc<egui::ColorImage>>,
    /// Live viewport GPU frames, claimed by node; never more than
    /// [`MAX_LIVE`].
    live_slots: Vec<LiveSlot>,
    /// Screenshot frame, created on the first screenshot and parked between.
    capture_target: Option<LiveTarget>,
    /// Headless tests enter viewports without GL (nothing renders).
    #[cfg(test)]
    pub(crate) headless_live: bool,
}

struct LiveSlot {
    target: LiveTarget,
    node: Option<NodeId>,
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
        rx: Receiver<Result<super::enscape_host::Started, super::enscape_host::LaunchError>>,
    },
    #[cfg(windows)]
    Ready(super::enscape_host::Live),
    #[cfg(not(windows))]
    #[allow(dead_code)] // No Enscape session starts off Windows.
    Absent,
}

pub(crate) struct EnscapeSession {
    key: String,
    node: NodeId,
    /// Window is on the card. False keeps the process and shows the grab.
    shown: bool,
    #[cfg_attr(not(windows), allow(dead_code))] // No Enscape session starts off Windows.
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
            poster_residency: posters::Residency::default(),
            want_poster: std::collections::HashSet::new(),
            bounds: HashMap::new(),
            model_hashes: HashMap::new(),
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
            display_swatch_gen: HashMap::new(),
            display_swatch_stamp: HashMap::new(),
            display_swatch_queue: VecDeque::new(),
            display_swatch_pixels: HashMap::new(),
            view_wire_meta: HashMap::new(),
            poster_pixels: HashMap::new(),
            live_slots: Vec::new(),
            capture_target: None,
            #[cfg(test)]
            headless_live: false,
        }
    }
}

impl ModelSpace {
    /// Draw a live pose into the node's slot (claiming a free slot, or
    /// creating one up to [`MAX_LIVE`], on first use). `true` once drawn.
    #[allow(clippy::too_many_arguments)]
    fn render_live(
        &mut self,
        gl: &Arc<glow::Context>,
        id: NodeId,
        cache_key: &str,
        cam: &ModelCamera,
        frame: (u32, u32, f32),
        adjust: Option<&ImageAdjust>,
    ) -> bool {
        if !self.ensure_gpu(gl, cache_key) {
            return false;
        }
        let slot = match self.live_slots.iter().position(|s| s.node == Some(id)) {
            Some(i) => i,
            None => match self.live_slots.iter().position(|s| s.node.is_none()) {
                Some(i) => i,
                None if self.live_slots.len() < MAX_LIVE => {
                    let EngineSlot::Ready(engine) = &self.engine else {
                        return false;
                    };
                    let Some(target) = engine.new_live_target() else {
                        return false;
                    };
                    self.live_slots.push(LiveSlot { target, node: None });
                    self.live_slots.len() - 1
                }
                None => return false,
            },
        };
        let (EngineSlot::Ready(engine), Some(gpu)) = (&self.engine, self.gpu.get(cache_key)) else {
            return false;
        };
        let slot = &mut self.live_slots[slot];
        slot.node = Some(id);
        engine.render_live(&mut slot.target, &gpu.model, cam, frame, adjust)
    }

    /// A viewport screenshot: the live pass (display mode, filter, MSAA,
    /// background, projection at `aspect`) drawn into the pooled capture
    /// target and read back, top row first.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_view_screenshot(
        &mut self,
        gl: &Arc<glow::Context>,
        cache_key: &str,
        cam: &ModelCamera,
        frame: (u32, u32, f32),
        adjust: Option<&ImageAdjust>,
    ) -> Option<egui::ColorImage> {
        if !self.ensure_gpu(gl, cache_key) {
            return None;
        }
        let (EngineSlot::Ready(engine), Some(gpu)) = (&self.engine, self.gpu.get(cache_key)) else {
            return None;
        };
        if self.capture_target.is_none() {
            self.capture_target = engine.new_live_target();
        }
        let target = self.capture_target.as_mut()?;
        let image = engine
            .render_live(target, &gpu.model, cam, frame, adjust)
            .then(|| engine.read_target(target))
            .flatten();
        engine.park_live_target(target);
        image
    }

    /// The egui texture a live viewport paints, once its slot is drawn and
    /// registered.
    pub(crate) fn live_texture_id(&self, id: NodeId) -> Option<egui::TextureId> {
        self.live_slots
            .iter()
            .find(|s| s.node == Some(id))
            .and_then(|s| s.target.egui_id)
    }

    /// Give each new live slot's display texture to egui's painter. Needs the
    /// frame (only `eframe::App::update` has one); a slot drawn this frame
    /// shows from the next.
    pub(crate) fn register_live_textures(&mut self, frame: &mut eframe::Frame) {
        if frame.gl().is_none() {
            return;
        }
        for slot in &mut self.live_slots {
            if slot.target.egui_id.is_none() {
                slot.target.egui_id = Some(frame.register_native_glow_texture(slot.target.display));
            }
        }
    }

    /// Release slots whose viewport stopped being live.
    fn park_idle_live_slots(&mut self) {
        let EngineSlot::Ready(engine) = &self.engine else {
            return;
        };
        for slot in &mut self.live_slots {
            if slot.node.is_some_and(|n| !self.live.contains_key(&n)) {
                slot.node = None;
                engine.park_live_target(&mut slot.target);
            }
        }
    }

    /// Queue one low-res render per display mode for the stringer. Returns the
    /// generation tag textures must match.
    fn queue_display_swatches(&mut self, doc: u64, id: NodeId, cam_hash: u64) -> u64 {
        let key = (doc, id);
        if self.display_swatch_stamp.get(&key) == Some(&cam_hash) {
            return *self.display_swatch_gen.get(&key).unwrap_or(&0);
        }
        let gen = self.display_swatch_gen.entry(key).or_insert(0);
        *gen += 1;
        let tag = *gen;
        self.display_swatch_stamp.insert(key, cam_hash);
        self.display_swatch_queue
            .retain(|(queued_doc, node, _, _)| (*queued_doc, *node) != key);
        self.display_swatch_pixels
            .retain(|(queued_doc, node, _, _), _| (*queued_doc, *node) != key);
        for mode in DISPLAY_SWATCH_MODES {
            self.display_swatch_queue.push_back((doc, id, tag, mode));
        }
        tag
    }

    fn display_swatch_generation(&self, doc: u64, id: NodeId) -> u64 {
        self.display_swatch_gen
            .get(&(doc, id))
            .copied()
            .unwrap_or(0)
    }

    fn take_display_swatch_job(&mut self, doc: u64) -> Option<(u64, NodeId, u64, ModelDisplay)> {
        for _ in 0..self.display_swatch_queue.len() {
            let job = self.display_swatch_queue.pop_front()?;
            if self.display_swatch_gen.get(&(job.0, job.1)) != Some(&job.2) {
                continue;
            }
            if job.0 == doc {
                return Some(job);
            }
            self.display_swatch_queue.push_back(job);
        }
        None
    }

    /// Kick off (or re-poll) the off-thread parse of a model file.
    pub(crate) fn request_model(&mut self, cache_key: &str, path: &Path) {
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
            let (hash, result) = match parse_with_progress(&path, &progress) {
                Ok((scene, hash)) => (Some(hash), Ok(scene)),
                Err(e) => (None, Err(e)),
            };
            let _ = tx.send((key, hash, result));
        });
    }

    fn drain_parses(&mut self) -> bool {
        let mut any = false;
        while let Ok((key, hash, result)) = self.parse_rx.try_recv() {
            any = true;
            if let Some(h) = hash {
                self.model_hashes.insert(key.clone(), h);
            }
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

    /// `None` while the file is still parsing, then whether it parsed.
    pub(crate) fn parse_outcome(&self, cache_key: &str) -> Option<Result<(), String>> {
        match self.models.get(cache_key).map(|e| &e.state) {
            Some(ModelState::Loading) => None,
            Some(ModelState::Failed(e) | ModelState::External(e)) => Some(Err(e.clone())),
            Some(ModelState::Ready(_)) | None => Some(Ok(())),
        }
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
        adjust: Option<&ImageAdjust>,
    ) -> Option<egui::ColorImage> {
        if !self.ensure_gpu(gl, cache_key) {
            return None;
        }
        let EngineSlot::Ready(engine) = &self.engine else {
            return None;
        };
        let gpu = self.gpu.get(cache_key)?;
        engine.render(&gpu.model, cam, w, h, adjust)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_capture_image(
        &mut self,
        gl: &Arc<glow::Context>,
        cache_key: &str,
        cam: &ModelCamera,
        w: u32,
        h: u32,
        depth: bool,
        adjust: Option<&ImageAdjust>,
    ) -> Option<egui::ColorImage> {
        if !self.ensure_gpu(gl, cache_key) {
            return None;
        }
        let EngineSlot::Ready(engine) = &self.engine else {
            return None;
        };
        let gpu = self.gpu.get(cache_key)?;
        engine.render_capture(&gpu.model, cam, w, h, depth, adjust)
    }

    /// The frame a live viewport's slot currently shows, top row first
    /// (`bench_model3d`).
    #[cfg(test)]
    pub(crate) fn live_frame_pixels(&self, id: NodeId) -> Option<egui::ColorImage> {
        let EngineSlot::Ready(engine) = &self.engine else {
            return None;
        };
        let slot = self.live_slots.iter().find(|s| s.node == Some(id))?;
        engine.read_target(&slot.target)
    }

    /// Render passes, pixel readbacks, and GL objects created so far.
    #[cfg(test)]
    pub(crate) fn gl_counts(&self) -> (u64, u64, u64) {
        match &self.engine {
            EngineSlot::Ready(e) => (
                e.render_passes.load(Ordering::Relaxed),
                e.readbacks.load(Ordering::Relaxed),
                e.gl_creates.load(Ordering::Relaxed),
            ),
            _ => (0, 0, 0),
        }
    }

    /// Free GPU/CPU entries nothing is using (called once per frame).
    fn evict(&mut self) {
        self.park_idle_live_slots();
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

        self.evict_posters();
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
            path: self.item_path(img.item)?,
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
        #[cfg(test)]
        let headless = self.model3d.headless_live;
        #[cfg(not(test))]
        let headless = false;
        if self.gl.is_none() && !headless {
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
                shown: false,
                rendered: None,
                radius,
                tool: ModelViewportTool::Navigate,
                measure_first: None,
                measure_preview: None,
                measures: Vec::new(),
                view_tween: None,
            },
        );
    }

    /// Lock a live viewport: freeze the current pose as the poster, commit
    /// the camera to the document (one undo step), release GPU work.
    pub fn lock_model(&mut self, id: NodeId) {
        self.release_view_drop_preview_for(id);
        let doc = self.tab().id;
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
                self.model3d.cache_poster_texture(name.clone(), tex);
            }
            if let Some(gl) = self.gl.clone() {
                let (pw, ph) = poster_size(aq);
                if let Some(img) =
                    self.model3d
                        .render_image(&gl, &info.cache_key, &cam, pw, ph, None)
                {
                    self.model3d
                        .store_poster(name, poster_path(&info.cache_key, &cam, aq), img);
                    self.model3d.want_poster.remove(&(doc, id));
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

    pub(super) fn tick_model_view_tweens(&mut self) -> bool {
        const VIEW_TWEEN: Duration = Duration::from_millis(220);
        let mut changed = false;
        for vp in self.model3d.live.values_mut() {
            let Some(ref tween) = vp.view_tween else {
                continue;
            };
            let t =
                (tween.started.elapsed().as_secs_f32() / VIEW_TWEEN.as_secs_f32()).clamp(0.0, 1.0);
            if t >= 1.0 {
                vp.cam = tween.to;
                vp.view_tween = None;
            } else {
                vp.cam = lerp_model_camera(tween.from, tween.to, t);
            }
            changed = true;
        }
        changed
    }

    /// Per-frame upkeep: parse results, auto-lock, eviction, repaint ticks.
    pub fn model3d_frame(&mut self, ctx: &egui::Context) {
        if self.model3d.drain_parses() | self.model3d.drain_sniffs() {
            ctx.request_repaint();
        }
        self.queue_executable_sniffs();
        self.maintain_enscape();
        self.maintain_view_drop();
        self.maintain_view_drop_preview(ctx);
        self.maintain_model_shot_pending();
        self.maintain_portal_capture(ctx);
        self.maintain_view_wire_cache();
        if self.tick_model_view_tweens() {
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

        // The display pass is journaled on its own, so undo/redo can change
        // it under a live viewport.
        let display: Vec<(NodeId, ModelDisplay, bool)> = self
            .model3d
            .live
            .keys()
            .filter_map(|id| {
                Some((
                    *id,
                    self.model_node_info(*id)?.cam.display,
                    self.model_node_info(*id)?.cam.materials,
                ))
            })
            .collect();
        for (id, doc, materials) in display {
            if let Some(vp) = self.model3d.live.get_mut(&id) {
                if vp.before.materials != materials {
                    vp.before.materials = materials;
                    vp.cam.materials = materials;
                }
                if vp.before.display != doc {
                    vp.before.display = doc;
                    vp.cam.display = doc;
                }
            }
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
            let doc = self.tab().id;
            let wanted: Vec<ModelNodeKey> = self
                .model3d
                .want_poster
                .iter()
                .filter(|(queued_doc, _)| *queued_doc == doc)
                .copied()
                .collect();
            for key @ (_, id) in wanted {
                if self.model3d.live.contains_key(&id) {
                    self.model3d.want_poster.remove(&key);
                    continue;
                }
                if self.generate_poster(id) {
                    self.model3d.want_poster.remove(&key);
                    ctx.request_repaint();
                }
            }
        }

        if self.gl.is_some() && self.tick_display_swatch() {
            ctx.request_repaint();
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
            .render_image(&gl, &info.cache_key, &cam, pw, ph, None)
        {
            Some(img) => {
                self.model3d.store_poster(
                    poster_file_name(&info.cache_key, &cam, aq),
                    poster_path(&info.cache_key, &cam, aq),
                    img,
                );
                true
            }
            None => matches!(self.model3d.engine, EngineSlot::Failed),
        }
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
            if !blocked && self.model3d.admit_model_poster(&info) {
                self.model3d.want_poster.insert((self.tab().id, id));
            }
        }
    }

    /// Texture for a live viewport, re-drawn on the GPU (no readback) when
    /// the camera or the on-screen size changed. `None` while the mesh parses
    /// or before the live slot is registered with egui.
    pub fn model_live_texture(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        screen_w: f32,
        screen_h: f32,
        adjust: &slate_doc::scene::ImageAdjust,
    ) -> Option<egui::TextureId> {
        if let Some(tex) = self.model3d.live.get(&id).and_then(|vp| vp.tex.as_ref()) {
            return Some(tex.id());
        }
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

        let (w, h, aspect) = live_frame_size(screen_w, screen_h, ctx.pixels_per_point());
        let stamp = (
            cam.cache_hash(),
            w,
            h,
            adjust.cache_hash(),
            aspect.to_bits(),
        );
        let up_to_date = self
            .model3d
            .live
            .get(&id)
            .is_some_and(|vp| vp.rendered == Some(stamp) && vp.shown);
        if !up_to_date {
            let adjust = (!adjust.is_identity()).then_some(adjust);
            if !self
                .model3d
                .render_live(&gl, id, &cache_key, &cam, (w, h, aspect), adjust)
            {
                return None;
            }
            let vp = self.model3d.live.get_mut(&id)?;
            if !vp.shown {
                vp.last_interact = Instant::now();
                vp.shown = true;
            }
            vp.rendered = Some(stamp);
        }
        self.model3d.live_texture_id(id)
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

    /// Commit a measure pick (first or second point). The second point
    /// completes the measurement and returns the viewport to Navigate.
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
                vp.tool = ModelViewportTool::Navigate;
            }
        }
        vp.measure_preview = None;
    }

    /// The measuring viewport a primary press at `screen` picks in. Plain
    /// press only (Shift orbits, Alt grabs the node), and never on a grip
    /// or handle, which win the press.
    pub(crate) fn model_measure_press_at(
        &self,
        screen: egui::Pos2,
        world: egui::Pos2,
    ) -> Option<NodeId> {
        if self.shift_down || self.alt_down {
            return None;
        }
        let id = self.live_model_at(world.x, world.y)?;
        let measuring = self
            .model3d
            .live
            .get(&id)
            .is_some_and(|vp| vp.tool == ModelViewportTool::MeasureDistance);
        let xf = self.board_xf();
        let on_handle = self.fillet_grip_hit_at(screen).is_some()
            || self.wire_grip_at(screen, &xf).is_some()
            || self
                .transform_hit_at(screen)
                .is_some_and(|(_, hit)| !matches!(hit, super::board_handles::BoardHitTarget::Body));
        (measuring && !on_handle).then_some(id)
    }

    /// After the first pick, the rubber band follows the hovering pointer.
    pub(crate) fn model_measure_hover(&mut self, screen: egui::Pos2) {
        let xf = self.board_xf();
        let target = self.model3d.live.iter().find_map(|(id, vp)| {
            (vp.tool == ModelViewportTool::MeasureDistance && vp.measure_first.is_some())
                .then_some(*id)
        });
        let Some(id) = target else {
            return;
        };
        let Some(srect) = self.doc().scene.node(id).map(|n| xf.rect_w2s(n.rect)) else {
            return;
        };
        if srect.contains(screen) {
            self.model_measure_preview(id, screen, srect);
        }
    }

    /// A mesh viewport the selection strip can drive: a placed model with a
    /// preview reader. Enscape standalones and recognized gaps are not.
    pub fn model_has_viewport(&self, id: NodeId) -> bool {
        self.model_node_info(id).is_some_and(|info| {
            !self.model3d.external.contains(&info.cache_key)
                && model_preview::gap_message(&info.path).is_none()
                && self.model_failure(&info.cache_key).is_none()
        })
    }

    /// The single selected node, when it is a mesh viewport (palette runs of
    /// `board.model_display` / `board.model_measure`).
    pub fn selected_model_viewport(&self) -> Option<NodeId> {
        let mut sel = self.board_sel.iter();
        let id = *sel.next()?;
        (sel.next().is_none() && self.model_has_viewport(id)).then_some(id)
    }

    /// Display pass shown on the node: the live camera's while unlocked.
    pub fn model_display_of(&self, id: NodeId) -> Option<ModelDisplay> {
        if let Some(vp) = self.model3d.live.get(&id) {
            return Some(vp.cam.display);
        }
        self.model_node_info(id).map(|info| info.cam.display)
    }

    /// `board.model_display`: one journaled patch of the display pass, live
    /// or frozen. A live viewport shows it at once and does not re-commit
    /// it on lock.
    /// Low-res chip faces for the viewport-display stringer. Queues offscreen
    /// renders when the camera stamp changes; never runs in the paint path.
    pub fn ensure_model_display_swatches(&mut self, id: NodeId) -> u64 {
        let Some(info) = self.model_node_info(id) else {
            return 0;
        };
        let mut cam = info.cam;
        if cam.distance <= 0.0 {
            if let Some((min, max)) = self.model3d.bounds.get(&info.cache_key).copied() {
                cam = resolve_camera(&cam, min, max);
            }
        }
        // Each chip renders every mode itself; switching modes is not a new pose.
        cam.display = ModelDisplay::default();
        self.model3d
            .queue_display_swatches(self.tab().id, id, cam.cache_hash())
    }

    fn tick_display_swatch(&mut self) -> bool {
        let doc = self.tab().id;
        let Some((_, id, gen, mode)) = self.model3d.take_display_swatch_job(doc) else {
            return false;
        };
        let Some(info) = self.model_node_info(id) else {
            return false;
        };
        let Some(gl) = self.gl.clone() else {
            return false;
        };
        self.model3d.request_model(&info.cache_key, &info.path);
        match self.model3d.models.get(&info.cache_key).map(|e| &e.state) {
            Some(ModelState::Ready(_)) => {}
            Some(ModelState::Failed(_) | ModelState::External(_)) => return false,
            _ => {
                self.model3d
                    .display_swatch_queue
                    .push_front((doc, id, gen, mode));
                return false;
            }
        }
        let mut cam = if info.cam.distance > 0.0 {
            info.cam
        } else {
            let Some((min, max)) = self.model3d.bounds.get(&info.cache_key).copied() else {
                self.model3d
                    .display_swatch_queue
                    .push_front((doc, id, gen, mode));
                return false;
            };
            resolve_camera(&info.cam, min, max)
        };
        cam.display = mode;
        let edge = DISPLAY_SWATCH_EDGE;
        let Some(img) = self
            .model3d
            .render_image(&gl, &info.cache_key, &cam, edge, edge, None)
        else {
            return false;
        };
        if self.model3d.display_swatch_gen.get(&(doc, id)) != Some(&gen) {
            return false;
        }
        let face = super::imagefx::square_swatch(&img, edge as usize);
        self.model3d
            .display_swatch_pixels
            .insert((doc, id, gen, mode), face);
        true
    }

    pub fn model_display_swatch_ids(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
    ) -> [Option<egui::TextureId>; 5] {
        let doc = self.tab().id;
        let gen = self.model3d.display_swatch_generation(doc, id);
        let mut out = [None; 5];
        for (i, mode) in DISPLAY_SWATCH_MODES.iter().enumerate() {
            let key = (doc, id, gen, *mode);
            if !self.model3d.display_swatch_pixels.contains_key(&key) {
                continue;
            }
            if !self.model_display_swatch_tex.contains_key(&key) {
                self.model_display_swatch_tex
                    .retain(|k, _| (k.0, k.1) != (doc, id) || k.2 == gen);
                let image = self.model3d.display_swatch_pixels[&key].clone();
                let tex = ctx.load_texture(
                    format!("slate-model-display-{doc}-{}-{}-{}", id.0, gen, mode.key()),
                    image,
                    egui::TextureOptions::NEAREST,
                );
                self.model_display_swatch_tex.insert(key, tex);
            }
            out[i] = Some(self.model_display_swatch_tex[&key].id());
        }
        out
    }

    pub fn set_model_display(&mut self, id: NodeId, mode: ModelDisplay) -> bool {
        if !self.model_has_viewport(id)
            || self.doc().scene.node(id).is_none_or(|n| n.locked)
            || self.model_display_of(id) == Some(mode)
            || self.refuse_read_only_edit()
        {
            return false;
        }
        self.last_board_edit = None;
        self.patch_nodes(&[id], |n| {
            if let NodeKind::Image(img) = &mut n.kind {
                img.model.display = mode;
            }
        });
        self.last_board_edit = None;
        if let Some(vp) = self.model3d.live.get_mut(&id) {
            vp.cam.display = mode;
            vp.before.display = mode;
            vp.last_interact = Instant::now();
        }
        true
    }

    /// `board.model_measure`: arm point-to-point measure, entering the
    /// viewport first when it is frozen. Armed, the same action returns to
    /// Navigate.
    pub fn toggle_model_measure(&mut self, id: NodeId) -> bool {
        if !self.model_has_viewport(id) {
            return false;
        }
        if !self.model3d.live.contains_key(&id) {
            self.unlock_model(id);
        }
        let Some(vp) = self.model3d.live.get_mut(&id) else {
            return false;
        };
        vp.tool = match vp.tool {
            ModelViewportTool::Navigate => ModelViewportTool::MeasureDistance,
            ModelViewportTool::MeasureDistance => ModelViewportTool::Navigate,
        };
        vp.measure_first = None;
        vp.measure_preview = None;
        vp.last_interact = Instant::now();
        true
    }

    pub fn model_measuring(&self, id: NodeId) -> bool {
        self.model3d
            .live
            .get(&id)
            .is_some_and(|vp| vp.tool == ModelViewportTool::MeasureDistance)
    }

    /// Live viewports the board owns. A focused generator steers its wired
    /// model and releases it itself.
    fn board_live_models(&self) -> Vec<NodeId> {
        self.model3d
            .live
            .keys()
            .filter(|id| !self.agents.steers_model(**id))
            .copied()
            .collect()
    }

    /// One Esc layer inside live viewports (P0.1): a pending measure point,
    /// then the armed Measure, then completed measurements, then the
    /// viewport itself locks.
    pub fn model_cancel_step(&mut self) -> bool {
        let ids = self.board_live_models();
        if ids.is_empty() {
            return false;
        }
        let any = |app: &Self, test: &dyn Fn(&LiveViewport) -> bool| {
            ids.iter()
                .any(|id| app.model3d.live.get(id).is_some_and(test))
        };
        if any(self, &|vp| vp.measure_first.is_some()) {
            for id in &ids {
                if let Some(vp) = self.model3d.live.get_mut(id) {
                    vp.measure_first = None;
                    vp.measure_preview = None;
                }
            }
        } else if any(self, &|vp| vp.tool != ModelViewportTool::Navigate) {
            for id in &ids {
                if let Some(vp) = self.model3d.live.get_mut(id) {
                    vp.tool = ModelViewportTool::Navigate;
                }
            }
        } else if any(self, &|vp| !vp.measures.is_empty()) {
            for id in &ids {
                if let Some(vp) = self.model3d.live.get_mut(id) {
                    vp.measures.clear();
                }
            }
        } else {
            for id in ids {
                self.lock_model(id);
            }
        }
        true
    }

    /// A primary press outside a live viewport ends it, unless selection
    /// chrome took the press. The press then belongs to the board.
    pub fn lock_models_pressed_outside(
        &mut self,
        ui: &egui::Ui,
        xf: &super::board::BoardXf,
        pointer: Option<egui::Pos2>,
        chrome_captured: bool,
    ) {
        if chrome_captured || !ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        {
            return;
        }
        let Some(p) = pointer.filter(|p| self.canvas_rect.contains(*p)) else {
            return;
        };
        // Screen-constant slop keeps the edge resize handles with the node.
        const HANDLE_SLOP_PX: f32 = 8.0;
        for id in self.board_live_models() {
            let inside = self
                .doc()
                .scene
                .node(id)
                .is_some_and(|n| xf.rect_w2s(n.rect).expand(HANDLE_SLOP_PX).contains(p));
            if !inside {
                self.lock_model(id);
            }
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
    #[cfg_attr(not(windows), allow(clippy::needless_return))] // The launch block follows.
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
            match &session.phase {
                EnscapePhase::Starting { since, .. } => since.elapsed(),
                EnscapePhase::Ready(live) => live.since.elapsed(),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = session;
            std::time::Duration::ZERO
        }
    }

    #[cfg(windows)]
    pub(crate) fn place_active_enscape(&self, rect: Option<(i32, i32, i32, i32)>) {
        if let Some(session) = self.model3d.enscape.as_ref().filter(|s| s.shown) {
            if let EnscapePhase::Ready(live) = &session.phase {
                super::enscape_host::place(live, rect);
            }
        }
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
        self.model3d
            .remove_poster_texture(&enscape_poster_name(&key));
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
        #[cfg(windows)]
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
                    return live
                        .surface_error()
                        .map(|error| format!("{} {error}", super::enscape_host::NO_HOST))
                        .or_else(|| {
                            (!live.has_window() && live.timed_out())
                                .then(|| super::enscape_host::NO_WINDOW.to_string())
                        });
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
        let session = self.model3d.enscape.as_mut()?;
        match incoming {
            Some(Ok(live)) => {
                session.phase = EnscapePhase::Ready(live.into_live());
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
        let Some(view) =
            self.model3d
                .render_capture_image(&gl, &info.cache_key, &cam, w, h, false, None)
        else {
            self.model3d.request_model(&info.cache_key, &info.path);
            return Ok(None);
        };
        let depth = self
            .model3d
            .render_capture_image(&gl, &info.cache_key, &cam, w, h, true, None)
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

fn color_image_from_readback(rgba: &[u8], w: i32, h: i32, flip_rows: bool) -> egui::ColorImage {
    let mut ordered = rgba.to_vec();
    if flip_rows {
        let row = (w * 4) as usize;
        for y in 0..h as usize / 2 {
            let opposite = h as usize - 1 - y;
            let (before, after) = ordered.split_at_mut(opposite * row);
            before[y * row..(y + 1) * row].swap_with_slice(&mut after[..row]);
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &ordered)
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

    #[test]
    fn cpu_texture_readback_keeps_top_first_rows() {
        let top = [255, 0, 0, 255];
        let bottom = [0, 0, 255, 255];
        let rgba = [top, bottom].concat();
        let uploaded = color_image_from_readback(&rgba, 1, 2, false);
        assert_eq!(uploaded.pixels[0], egui::Color32::RED);
        assert_eq!(uploaded.pixels[1], egui::Color32::BLUE);

        let rendered = color_image_from_readback(&rgba, 1, 2, true);
        assert_eq!(rendered.pixels[0], egui::Color32::BLUE);
        assert_eq!(rendered.pixels[1], egui::Color32::RED);
    }

    pub(super) fn live_model(tag: &str) -> (Harness, NodeId) {
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
                shown: false,
                rendered: Some((cam.cache_hash(), 2, 2, 0, 1.0f32.to_bits())),
                radius: bounds_sphere(model.bounds_min, model.bounds_max).1,
                tool: ModelViewportTool::Navigate,
                measure_first: None,
                measure_preview: None,
                measures: Vec::new(),
                view_tween: None,
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
        // egui Areas settle their size across initial passes; selection
        // chrome must have its final hit rect before the pointer presses.
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
            h.app
                .model_poster_texture(&h.ctx, &info, &slate_doc::scene::ImageAdjust::default())
                .unwrap()
                .id(),
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

    /// The parsed mesh, so picks can raycast without a GL context.
    fn with_mesh(h: &mut Harness, id: NodeId) {
        let info = h.app.model_node_info(id).unwrap();
        let bytes = std::fs::read(&info.path).unwrap();
        let model = model_preview::load_preview(&info.path, &bytes).unwrap();
        h.app
            .model3d
            .bounds
            .insert(info.cache_key.clone(), (model.bounds_min, model.bounds_max));
        h.app.model3d.models.insert(
            info.cache_key,
            CpuEntry {
                state: ModelState::Ready(Arc::new(model)),
                last_used: Instant::now(),
                progress: Arc::new(ParseProgress::new()),
            },
        );
    }

    fn cancel(h: &mut Harness) {
        assert!(h
            .app
            .dispatch(&h.ctx, atlas_commands::CommandId("app.cancel"), None));
    }

    #[test]
    fn one_measurement_returns_the_viewport_to_navigate() {
        let (mut h, id) = live_model("model_measure");
        with_mesh(&mut h, id);
        assert!(h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId("board.model_measure"),
            Some(id.0.to_string())
        ));
        assert!(h.app.model_measuring(id));
        let srect = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        // Aim at two triangle centroids so each ray is known to hit.
        let key = h.app.model3d.live[&id].cache_key.clone();
        let mesh = h.app.model3d.mesh_for_key(&key).unwrap();
        let bounds = h.app.model3d.bounds[&key];
        let cam = h.app.model3d.live[&id].cam;
        let aspect = srect.width() / srect.height();
        let part = &mesh.meshes[0];
        let aim = |tri: usize| {
            let c = part.indices[tri * 3..tri * 3 + 3]
                .iter()
                .map(|i| part.positions[*i as usize])
                .fold([0.0f32; 3], |s, p| {
                    [s[0] + p[0] / 3.0, s[1] + p[1] / 3.0, s[2] + p[2] / 3.0]
                });
            let (u, v) = project_model_point(c, aspect, &cam, bounds).unwrap();
            srect.min + egui::vec2(u * srect.width(), v * srect.height())
        };
        let (a, b) = (aim(0), aim(part.indices.len() / 3 - 1));
        h.app.model_measure_pick(id, a, srect);
        assert!(
            h.app.model3d.live[&id].measure_first.is_some(),
            "first pick hits"
        );
        assert!(h.app.model_measuring(id));
        h.app.model_measure_pick(id, b, srect);
        let vp = &h.app.model3d.live[&id];
        assert_eq!(vp.measures.len(), 1);
        assert_eq!(vp.tool, ModelViewportTool::Navigate);
    }

    /// Screen points on the first and last triangle centroids of the mesh.
    fn measure_targets(h: &mut Harness, id: NodeId) -> (egui::Pos2, egui::Pos2) {
        let srect = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let key = h.app.model3d.live[&id].cache_key.clone();
        let mesh = h.app.model3d.mesh_for_key(&key).unwrap();
        let bounds = h.app.model3d.bounds[&key];
        let cam = h.app.model3d.live[&id].cam;
        let aspect = srect.width() / srect.height();
        let part = &mesh.meshes[0];
        let aim = |tri: usize| {
            let c = part.indices[tri * 3..tri * 3 + 3]
                .iter()
                .map(|i| part.positions[*i as usize])
                .fold([0.0f32; 3], |s, p| {
                    [s[0] + p[0] / 3.0, s[1] + p[1] / 3.0, s[2] + p[2] / 3.0]
                });
            let (u, v) = project_model_point(c, aspect, &cam, bounds).unwrap();
            srect.min + egui::vec2(u * srect.width(), v * srect.height())
        };
        (aim(0), aim(part.indices.len() / 3 - 1))
    }

    /// Frames with no input, long enough that the next click is not a
    /// double-click with the last one.
    fn pause(h: &mut Harness) {
        for _ in 0..30 {
            h.frame();
        }
    }

    fn press_escape(h: &mut Harness) {
        for pressed in [true, false] {
            h.frame_with(|input| {
                input.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Default::default(),
                });
            });
        }
    }

    fn painted_texts(out: &egui::FullOutput) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                _ => {}
            }
        }
        let mut texts = Vec::new();
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut texts);
        }
        texts
    }

    /// GP3 through real input: the strip's Measure squircle, two plain
    /// clicks on the model with the rubber band between them, the length
    /// label, Navigate again, then the Esc ladder.
    #[test]
    fn measure_golden_path_runs_on_plain_clicks_from_the_strip() {
        let (mut h, id) = live_model("model_measure_golden");
        with_mesh(&mut h, id);
        h.app.board_sel = [id].into_iter().collect();
        // Leave the scaled property strip clear of the bottom toolbar.
        h.app.tab_mut().cam.z *= 0.7;
        pause(&mut h);
        let button = h
            .app
            .model_measure_button()
            .expect("a selected model offers Measure on the strip");
        click_at(&mut h, button.center());
        assert!(
            h.app.model3d.live.contains_key(&id),
            "the strip click keeps the viewport entered"
        );
        assert!(h.app.model_measuring(id), "the strip click arms Measure");
        pause(&mut h);

        let (a, b) = measure_targets(&mut h, id);
        click_at(&mut h, a);
        assert!(
            h.app.model3d.live[&id].measure_first.is_some(),
            "a plain click picks the first point"
        );
        pause(&mut h);
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(b)));
        h.frame();
        assert!(
            h.app.model3d.live[&id].measure_preview.is_some(),
            "hovering the model draws the rubber band to the second point"
        );
        click_at(&mut h, b);
        let vp = &h.app.model3d.live[&id];
        assert_eq!(vp.measures.len(), 1, "the second click completes it");
        assert_eq!(vp.tool, ModelViewportTool::Navigate, "back to Navigate");
        let label = format!("{:.3}", vp.measures[0].length());
        let out = h.frame_output(|_| {});
        assert!(
            painted_texts(&out).contains(&label),
            "the length label {label} is drawn"
        );
        assert!(h.app.model3d.live.contains_key(&id));

        press_escape(&mut h);
        assert!(h.app.model3d.live[&id].measures.is_empty(), "Esc clears it");
        press_escape(&mut h);
        assert!(!h.app.model3d.live.contains_key(&id), "Esc again freezes");
        assert!(h.app.board_sel.contains(&id));
        press_escape(&mut h);
        assert!(h.app.board_sel.is_empty(), "then Esc deselects");
    }

    /// A PNG carrying a `slateview` packet for `cam`, and the camera as it
    /// reads back.
    fn saved_view_png(
        h: &Harness,
        name: &str,
        model: NodeId,
        cam: ModelCamera,
    ) -> (PathBuf, ModelCamera) {
        use model_preview::view_meta;
        let png = h.base.join(name);
        let meta = view_meta::ViewMetaInput {
            camera: cam,
            eye: eye_of(&cam),
            up: [0.0, 0.0, 1.0],
            aspect: 1.6,
            width: 8,
            height: 5,
            model_name: "model.3dm".into(),
            model_path: "model.3dm".into(),
            model_hash: String::new(),
            model_size: 1,
            node_id: model.0,
            created_unix: 0,
            image_adjust_hash: None,
        };
        let xmp = view_meta::build_xmp_packet(&meta).unwrap();
        view_meta::write_png_with_xmp(&png, &[128u8; 8 * 5 * 4], 8, 5, &xmp).unwrap();
        let read = view_meta::read_view_meta(&png).unwrap().unwrap().camera;
        (png, read)
    }

    /// A frozen model (entering needs no GL here) with its mesh, and a
    /// saved camera that differs from the node's.
    fn frozen_model_and_saved_view(tag: &str) -> (Harness, NodeId, ModelCamera, ModelCamera) {
        let (mut h, id) = live_model(tag);
        with_mesh(&mut h, id);
        let resolved = h.app.model3d.live[&id].cam;
        h.app.lock_model(id);
        h.app.model3d.headless_live = true;
        let original = h.app.model_node_info(id).unwrap().cam;
        let saved = ModelCamera {
            yaw: resolved.yaw + 1.2,
            pitch: 0.45,
            distance: resolved.distance * 1.3,
            ..resolved
        };
        (h, id, original, saved)
    }

    /// Place the picture left of the model and frame both on screen.
    fn place_picture(h: &mut Harness, model: NodeId, png: PathBuf) -> NodeId {
        use slate_doc::scene::{ImageNode, WorldRect};
        let item = h
            .app
            .doc_mut()
            .add_item(png, "saved-view.png", 1, 0, "saved-view-cache");
        let rect = h.app.doc().scene.node(model).unwrap().rect;
        let node = h.app.doc_mut().scene.build_node(
            WorldRect::new(rect.x - rect.w * 0.9, rect.y, rect.w * 0.4, rect.h * 0.4),
            NodeKind::Image(ImageNode::new(item)),
        );
        let pic = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.clear();
        h.app.zoom_to_rect(WorldRect::new(
            rect.x - rect.w * 1.2,
            rect.y - rect.h * 0.4,
            rect.w * 2.6,
            rect.h * 1.8,
        ));
        for _ in 0..3 {
            h.frame();
        }
        pic
    }

    fn screen_center(h: &Harness, id: NodeId) -> egui::Pos2 {
        let rect = h.app.doc().scene.node(id).unwrap().rect;
        h.app.board_xf().rect_w2s(rect).center()
    }

    fn press(h: &mut Harness, p: egui::Pos2, pressed: bool) {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(p));
            input.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        });
    }

    fn move_to(h: &mut Harness, from: egui::Pos2, to: egui::Pos2) {
        for i in 1..=8 {
            let p = from.lerp(to, i as f32 / 8.0);
            h.frame_with(|input| input.events.push(egui::Event::PointerMoved(p)));
        }
    }

    /// Frames at real time (packet reads and tweens run on the clock) until
    /// `done`, or panic after three seconds.
    fn frames_until(h: &mut Harness, what: &str, done: impl Fn(&SlateApp) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !done(&h.app) {
            assert!(Instant::now() < deadline, "timed out waiting: {what}");
            std::thread::sleep(Duration::from_millis(8));
            h.frame();
        }
    }

    fn doc_camera(h: &Harness, id: NodeId) -> ModelCamera {
        h.app.model_node_info(id).unwrap().cam
    }

    fn live_camera(app: &SlateApp, id: NodeId) -> Option<ModelCamera> {
        app.model3d.live.get(&id).map(|vp| vp.cam)
    }

    #[test]
    fn a_saved_view_dragged_over_a_model_previews_then_release_commits_one_patch() {
        let (mut h, id, original, saved) = frozen_model_and_saved_view("view_drop_release");
        let (png, saved) = saved_view_png(&h, "release.png", id, saved);
        let pic = place_picture(&mut h, id, png);
        let pic_rect = h.app.doc().scene.node(pic).unwrap().rect;
        let depth = h.app.tab().journal.undo_depth();
        let (from, over) = (screen_center(&h, pic), screen_center(&h, id));

        press(&mut h, from, true);
        move_to(&mut h, from, over);
        frames_until(&mut h, "the viewport orients to the saved view", |app| {
            live_camera(app, id) == Some(saved)
        });
        assert_eq!(
            h.app.tab().journal.undo_depth(),
            depth,
            "the preview journals nothing"
        );
        assert_eq!(doc_camera(&h, id), original, "the document keeps its pose");

        press(&mut h, over, false);
        assert_eq!(
            h.app.tab().journal.undo_depth(),
            depth + 1,
            "release commits one patch"
        );
        assert_eq!(doc_camera(&h, id), saved);
        assert_eq!(
            h.app.doc().scene.node(pic).unwrap().rect,
            pic_rect,
            "the picture goes back where it was"
        );
        for _ in 0..3 {
            h.frame();
        }
        h.app.lock_all_models();
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "and only one");
        assert_eq!(doc_camera(&h, id), saved);
    }

    #[test]
    fn leaving_the_model_mid_drag_restores_the_original_camera_exactly() {
        let (mut h, id, original, saved) = frozen_model_and_saved_view("view_drop_leave");
        let (png, saved) = saved_view_png(&h, "leave.png", id, saved);
        let pic = place_picture(&mut h, id, png);
        let depth = h.app.tab().journal.undo_depth();
        let (from, over) = (screen_center(&h, pic), screen_center(&h, id));

        press(&mut h, from, true);
        move_to(&mut h, from, over);
        frames_until(&mut h, "the preview reaches the saved view", |app| {
            live_camera(app, id) == Some(saved)
        });
        move_to(&mut h, over, from);
        frames_until(&mut h, "the viewport returns to its pose", |app| {
            live_camera(app, id).is_none()
        });
        assert_eq!(doc_camera(&h, id), original, "exactly the original pose");
        assert_eq!(h.app.tab().journal.undo_depth(), depth, "nothing journaled");
        press(&mut h, from, false);
        assert_eq!(doc_camera(&h, id), original);
    }

    #[test]
    fn an_os_file_drag_previews_the_saved_view_and_the_drop_commits_one_patch() {
        let (mut h, id) = live_model("view_drop_os_file");
        with_mesh(&mut h, id);
        let original = h.app.model3d.live[&id].cam;
        let saved = ModelCamera {
            yaw: original.yaw - 0.9,
            pitch: 0.3,
            ..original
        };
        let (png, saved) = saved_view_png(&h, "os-drag.png", id, saved);
        let items = h.app.doc().items.len();
        let depth = h.app.tab().journal.undo_depth();
        let over = screen_center(&h, id);

        h.app
            .external_drop
            .set_hover_test(Some((vec![png.clone()], over)));
        frames_until(&mut h, "the OS drag previews the saved view", |app| {
            live_camera(app, id) == Some(saved)
        });
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        h.app.external_drop.set_hover_test(None);
        frames_until(&mut h, "leaving restores the live pose", |app| {
            live_camera(app, id) == Some(original)
        });
        assert!(h.app.model3d.live.contains_key(&id), "it was live before");
        assert_eq!(h.app.tab().journal.undo_depth(), depth);

        h.app
            .external_drop
            .set_hover_test(Some((vec![png.clone()], over)));
        frames_until(&mut h, "the preview returns", |app| {
            live_camera(app, id) == Some(saved)
        });
        h.app.external_drop.set_hover_test(None);
        h.app
            .external_drop
            .push_test(super::super::external_drop::DropEvent {
                payload: super::super::external_drop::Payload::Files(vec![png]),
                at: over,
                alt: false,
            });
        h.frame();
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one patch");
        assert_eq!(doc_camera(&h, id), saved);
        assert_eq!(live_camera(&h.app, id), Some(saved));
        assert_eq!(h.app.doc().items.len(), items, "the file is not placed");
        h.app.lock_all_models();
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "and only one");
    }

    /// The picture as the board paints it this frame.
    fn painted_picture(app: &SlateApp, pic: NodeId) -> slate_doc::scene::Node {
        let mut nodes = vec![app.doc().scene.node(pic).unwrap().clone()];
        app.sink_view_drop_picture(&mut nodes);
        nodes.remove(0)
    }

    fn world_center(r: slate_doc::scene::WorldRect) -> egui::Pos2 {
        egui::pos2(r.x + r.w * 0.5, r.y + r.h * 0.5)
    }

    /// Alt-drag (duplicate) a saved-view picture onto a model: the staged
    /// copy leaves, the original stays put, and the release journals only
    /// the camera patch.
    #[test]
    fn an_alt_dragged_saved_view_commits_only_the_camera_patch() {
        let (mut h, id, original, saved) = frozen_model_and_saved_view("view_drop_alt");
        let (png, saved) = saved_view_png(&h, "alt.png", id, saved);
        let pic = place_picture(&mut h, id, png);
        let before = h.app.doc().scene.nodes.clone();
        let depth = h.app.tab().journal.undo_depth();
        let (from, over) = (screen_center(&h, pic), screen_center(&h, id));
        let alt = egui::Modifiers {
            alt: true,
            ..Default::default()
        };
        let alt_frame = |h: &mut Harness, events: Vec<egui::Event>| {
            h.frame_with(|input| {
                input.modifiers = alt;
                input.events.extend(events);
            })
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: alt,
        };

        alt_frame(
            &mut h,
            vec![egui::Event::PointerMoved(from), button(from, true)],
        );
        for i in 1..=8 {
            let p = from.lerp(over, i as f32 / 8.0);
            alt_frame(&mut h, vec![egui::Event::PointerMoved(p)]);
        }
        assert!(
            matches!(
                h.app.board_drag,
                Some(super::super::board::BoardDrag::Move { dup: true, .. })
            ),
            "Alt stages a duplicate move"
        );
        assert_eq!(h.app.doc().scene.nodes.len(), before.len() + 1);
        frames_until(&mut h, "the viewport orients to the saved view", |app| {
            live_camera(app, id) == Some(saved)
        });

        alt_frame(&mut h, vec![button(over, false)]);
        assert!(h.app.board_drag.is_none());
        assert_eq!(doc_camera(&h, id), saved);
        assert_eq!(
            h.app.tab().journal.undo_depth(),
            depth + 1,
            "release commits one patch"
        );
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            before.len(),
            "no copy is left"
        );
        for node in before.iter().filter(|n| n.id != id) {
            assert_eq!(h.app.doc().scene.node(node.id), Some(node), "unmoved");
        }
        for _ in 0..3 {
            h.frame();
        }
        h.app.lock_all_models();
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "and only one");
        h.app.board_undo();
        assert_eq!(doc_camera(&h, id), original, "that patch was the camera");
        assert_eq!(h.app.doc().scene.nodes.len(), before.len());
    }

    #[test]
    fn a_held_saved_view_sinks_its_picture_into_the_viewport_and_leaving_restores_it() {
        let (mut h, id, _, saved) = frozen_model_and_saved_view("view_drop_sink");
        let (png, _) = saved_view_png(&h, "sink.png", id, saved);
        let pic = place_picture(&mut h, id, png);
        let model_rect = h.app.doc().scene.node(id).unwrap().rect;
        let screen = h.app.board_xf().rect_w2s(model_rect);
        let from = screen_center(&h, pic);
        let over = screen.min.lerp(screen.center(), 0.5);

        press(&mut h, from, true);
        move_to(&mut h, from, over);
        frames_until(&mut h, "the picture sinks in", |app| {
            painted_picture(app, pic).opacity < 0.02
        });
        let placed = h.app.doc().scene.node(pic).unwrap().clone();
        let sunk = painted_picture(&h.app, pic);
        assert!(
            sunk.rect.w < placed.rect.w * 0.2 && sunk.rect.h < placed.rect.h * 0.2,
            "it shrinks: {:?} from {:?}",
            sunk.rect,
            placed.rect
        );
        let target = world_center(model_rect);
        assert!(
            world_center(sunk.rect).distance(target)
                < world_center(placed.rect).distance(target) * 0.1,
            "into the viewport"
        );
        assert_eq!(placed.opacity, 1.0, "the scene is untouched");

        move_to(&mut h, over, from);
        frames_until(&mut h, "the picture comes back", |app| {
            painted_picture(app, pic) == *app.doc().scene.node(pic).unwrap()
        });
        press(&mut h, from, false);
    }

    #[test]
    fn escape_mid_drag_cancels_the_held_view() {
        let (mut h, id, original, saved) = frozen_model_and_saved_view("view_drop_escape");
        let (png, saved) = saved_view_png(&h, "escape.png", id, saved);
        let pic = place_picture(&mut h, id, png);
        let pic_rect = h.app.doc().scene.node(pic).unwrap().rect;
        let depth = h.app.tab().journal.undo_depth();
        let (from, over) = (screen_center(&h, pic), screen_center(&h, id));

        press(&mut h, from, true);
        move_to(&mut h, from, over);
        frames_until(&mut h, "the preview reaches the saved view", |app| {
            live_camera(app, id) == Some(saved)
        });
        press_escape(&mut h);
        assert_eq!(
            h.app.doc().scene.node(pic).unwrap().rect,
            pic_rect,
            "the picture goes back at once"
        );
        frames_until(&mut h, "the viewport goes back and freezes", |app| {
            live_camera(app, id).is_none()
        });
        assert!(painted_picture(&h.app, pic) == *h.app.doc().scene.node(pic).unwrap());
        press(&mut h, over, false);
        assert_eq!(h.app.tab().journal.undo_depth(), depth, "nothing journaled");
        assert_eq!(doc_camera(&h, id), original);
        assert_eq!(h.app.doc().scene.node(pic).unwrap().rect, pic_rect);
        for _ in 0..3 {
            h.frame();
        }
        assert!(
            !h.app.model3d.live.contains_key(&id),
            "the drop did not reopen it"
        );
        assert_eq!(doc_camera(&h, id), original);
    }

    #[test]
    fn escape_peels_measure_state_then_locks_the_viewport() {
        let (mut h, id) = live_model("model_escape");
        h.app.board_sel = [id].into_iter().collect();
        assert!(h.app.toggle_model_measure(id));
        let vp = h.app.model3d.live.get_mut(&id).unwrap();
        vp.measure_first = Some([0.0; 3]);
        vp.measures.push(DistanceMeasurement {
            a: [0.0; 3],
            b: [1.0, 0.0, 0.0],
        });
        cancel(&mut h);
        assert!(h.app.model3d.live[&id].measure_first.is_none());
        assert!(h.app.model_measuring(id), "the pending point peels first");
        cancel(&mut h);
        assert!(!h.app.model_measuring(id));
        assert_eq!(h.app.model3d.live[&id].measures.len(), 1);
        cancel(&mut h);
        assert!(h.app.model3d.live[&id].measures.is_empty());
        cancel(&mut h);
        assert!(
            !h.app.model3d.live.contains_key(&id),
            "then the viewport locks"
        );
        assert!(h.app.board_sel.contains(&id), "selection is the next layer");
        cancel(&mut h);
        assert!(h.app.board_sel.is_empty());
    }

    #[test]
    fn measure_enters_a_frozen_viewport_and_toggles_back() {
        let (mut h, id) = live_model("model_measure_toggle");
        h.app.lock_model(id);
        assert!(!h.app.model3d.live.contains_key(&id));
        // Headless has no GL, so entering is refused rather than faked.
        assert!(!h.app.toggle_model_measure(id));
        let (mut h, id) = live_model("model_measure_toggle_live");
        assert!(h.app.toggle_model_measure(id));
        assert!(h.app.toggle_model_measure(id));
        assert!(!h.app.model_measuring(id));
    }

    #[test]
    fn a_press_outside_a_live_viewport_locks_it() {
        let (mut h, id) = live_model("model_press_outside");
        h.app.tab_mut().cam.z *= 0.25;
        h.frame();
        let srect = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let outside = h.app.canvas_rect.min + egui::vec2(6.0, 6.0);
        assert!(!srect.expand(8.0).contains(outside));
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(outside));
            input.events.push(egui::Event::PointerButton {
                pos: outside,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            });
        });
        assert!(!h.app.model3d.live.contains_key(&id));
    }

    /// A point on empty canvas to the right of the node, with frames settled.
    fn empty_canvas_beside(h: &mut Harness, id: NodeId) -> egui::Pos2 {
        h.app.tab_mut().cam.z *= 0.25;
        for _ in 0..3 {
            h.frame();
        }
        let srect = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let p = egui::pos2(srect.right() + 60.0, srect.center().y);
        assert!(h.app.canvas_rect.contains(p));
        let world = h.app.board_xf().s2w(p);
        assert!(h.app.doc().scene.node_at(world.x, world.y).is_none());
        p
    }

    fn click_at(h: &mut Harness, p: egui::Pos2) {
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(p)));
        for pressed in [true, false] {
            h.frame_with(|input| {
                input.events.push(egui::Event::PointerMoved(p));
                input.events.push(egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
            });
        }
        h.frame();
    }

    #[test]
    fn one_click_on_empty_canvas_freezes_and_deselects_a_live_viewport() {
        let (mut h, id) = live_model("model_click_off_live");
        h.app.board_sel = [id].into_iter().collect();
        let p = empty_canvas_beside(&mut h, id);
        click_at(&mut h, p);
        assert!(
            !h.app.model3d.live.contains_key(&id),
            "the click freezes it"
        );
        assert!(h.app.board_sel.is_empty(), "the same click deselects it");
    }

    #[test]
    fn one_click_on_empty_canvas_deselects_a_frozen_viewport() {
        let (mut h, id) = live_model("model_click_off_frozen");
        h.app.lock_model(id);
        h.app.board_sel = [id].into_iter().collect();
        let p = empty_canvas_beside(&mut h, id);
        click_at(&mut h, p);
        assert!(h.app.board_sel.is_empty());
    }

    /// Closed stroked paths one frame painted, with their stroke colour.
    fn painted_rings(h: &mut Harness) -> Vec<(Vec<egui::Pos2>, egui::epaint::ColorMode)> {
        let out = h.frame_output(|_| {});
        super::super::tests::painted_closed_paths_with_color(&out)
    }

    #[test]
    fn viewport_rings_follow_the_fillet_live_and_frozen() {
        assert_viewport_rings_follow(
            "model_ring_fillet",
            slate_doc::scene::Corner::Rounded { radius: 24.0 },
        );
    }

    /// P1.node.corner-grip: the live ring (the viewport's contents-focus)
    /// and the frozen multi-selection outline follow a chamfer or fillet.
    #[test]
    fn viewport_rings_follow_the_chamfer_live_and_frozen() {
        assert_viewport_rings_follow(
            "model_ring_chamfer",
            slate_doc::scene::Corner::Chamfer { cut: 24.0 },
        );
    }

    fn assert_viewport_rings_follow(tag: &str, authored: slate_doc::scene::Corner) {
        let (mut h, id) = live_model(tag);
        h.app.patch_nodes(&[id], |n| {
            if let NodeKind::Image(img) = &mut n.kind {
                img.corner = authored;
            }
        });
        h.app.board_sel = std::iter::once(id).collect();
        let corner_outline = |h: &Harness| {
            let xf = h.app.board_xf();
            let node = h.app.doc().scene.node(id).unwrap();
            let corner = h.app.node_resolved_corner(node);
            let expected =
                super::super::board::corner_outline(xf.rect_w2s(node.rect), corner, xf.z);
            (expected, h.app.node_screen_outline(&h.ctx, &xf, node))
        };
        let same = |a: &[egui::Pos2], b: &[egui::Pos2]| {
            a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.distance(*q) < 0.01)
        };

        let rings = painted_rings(&mut h);
        let (expected, selection) = corner_outline(&h);
        assert!(expected.len() > 4, "a cornered outline, not a box");
        assert!(
            same(&selection, &expected),
            "the selection outline is the corner outline"
        );
        let accent = egui::epaint::ColorMode::Solid(h.app.palette().accent);
        assert!(
            rings
                .iter()
                .any(|(pts, c)| *c == accent && same(pts, &expected)),
            "the live ring is the corner outline"
        );

        h.app.lock_model(id);
        h.app.board_sel = std::iter::once(id).collect();
        let out = h.frame_output(|_| {});
        let host = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        assert!(super::super::tests::painted_shadow_encloses(&out, host));
        let rings = painted_rings(&mut h);
        let (expected, _) = corner_outline(&h);
        assert!(
            !rings.iter().any(|(pts, _)| same(pts, &expected)),
            "a single frozen viewport uses its shadow without a ring"
        );
        let other = super::super::tests::add_rect(&mut h.app, 360.0, 0.0);
        h.app.board_sel.insert(other);
        let rings = painted_rings(&mut h);
        let (expected, _) = corner_outline(&h);
        assert!(
            rings.iter().any(|(pts, _)| same(pts, &expected)),
            "the frozen multi-selection outline is the corner outline"
        );
    }

    #[test]
    fn display_swatches_render_once_per_pose_not_per_mode() {
        let (mut h, id) = live_model("model_swatch_once");
        h.app.lock_model(id);
        let first = h.app.ensure_model_display_swatches(id);
        assert!(h.app.set_model_display(id, ModelDisplay::Depth));
        assert_eq!(
            h.app.ensure_model_display_swatches(id),
            first,
            "picking a mode keeps the five chips"
        );
        h.app.patch_nodes(&[id], |n| {
            if let NodeKind::Image(img) = &mut n.kind {
                img.model.yaw += 0.5;
            }
        });
        assert_ne!(
            h.app.ensure_model_display_swatches(id),
            first,
            "a new pose re-renders them"
        );
    }

    #[test]
    fn live_display_is_one_undo_step_and_follows_undo() {
        let (mut h, id) = live_model("model_live_display");
        assert!(h.app.set_model_display(id, ModelDisplay::Depth));
        let vp = &h.app.model3d.live[&id];
        assert_eq!(vp.cam.display, ModelDisplay::Depth);
        assert_eq!(
            h.app.model_node_info(id).unwrap().cam.display,
            ModelDisplay::Depth
        );
        h.app.board_undo();
        h.app.model3d_frame(&h.ctx);
        assert_eq!(h.app.model3d.live[&id].cam.display, ModelDisplay::Shaded);
        h.app.board_redo();
        h.app.model3d_frame(&h.ctx);
        assert_eq!(h.app.model3d.live[&id].cam.display, ModelDisplay::Depth);
        // The camera commits on lock as its own step; the display stays put
        // until its own step is undone.
        h.app.lock_model(id);
        h.app.board_undo();
        assert_eq!(
            h.app.model_node_info(id).unwrap().cam.display,
            ModelDisplay::Depth
        );
        h.app.board_undo();
        assert_eq!(
            h.app.model_node_info(id).unwrap().cam.display,
            ModelDisplay::Shaded
        );
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
            materials: true,
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

    /// A live frame is rendered at a quantized size and stretched onto the
    /// node, so its projection must use the node's aspect, not the frame's.
    #[test]
    fn live_frames_project_at_the_node_aspect() {
        for (w, h, ppp) in [
            (320.0, 200.0, 1.0),
            (333.0, 217.0, 1.25),
            (900.0, 300.0, 1.0),
        ] {
            let (_, _, aspect) = live_frame_size(w, h, ppp);
            let node = w / h;
            assert!(
                (aspect - node).abs() / node < 0.002,
                "{w}x{h}@{ppp}: projection aspect {aspect} vs node {node}"
            );
        }
    }

    #[test]
    fn screenshots_keep_the_node_aspect() {
        for (w, h) in [
            (320.0, 200.0),
            (900.0, 300.0),
            (200.0, 640.0),
            (1000.0, 250.0),
        ] {
            let (sw, sh) = screenshot_size(w, h);
            let (shot, node) = (sw as f32 / sh as f32, w / h);
            assert!(
                (shot - node).abs() / node < 0.01,
                "{w}x{h}: screenshot {sw}x{sh} ({shot}) vs node {node}"
            );
        }
    }

    #[test]
    fn depth_range_spans_the_model_bounds_in_front_of_the_camera() {
        let cam = ModelCamera {
            target: [0.0, 0.0, 0.0],
            yaw: 0.3,
            pitch: 0.2,
            distance: 10.0,
            display: slate_doc::scene::ModelDisplay::Shaded,
            materials: true,
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
                ..Default::default()
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
