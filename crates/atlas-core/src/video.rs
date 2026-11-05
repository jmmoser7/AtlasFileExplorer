//! Canvas video frames. The board scrubs and plays; the HTML artifact still
//! uses `<video>` for web-safe files.
//!
//! Native clip time starts at the first non-preroll decoded video frame;
//! leading source gaps normalize away. This video-only mapping preserves frame
//! intervals and the reported presentation duration across trims, seeks and replay.
//! Document/export data stays in its existing clip-time representation.
//!
//! Decode goes through Windows Media Foundation in an isolated worker process. Formats
//! the OS can open (mp4, m4v, mov, wmv, avi, mpg, mpeg, and webm / mkv / ogv
//! when that codec is installed) yield frames. Anything else is a miss, and
//! the board keeps the poster. Linux builds expose the same time math and a
//! pool that reports failure without reading the file.
//!
//! A cloud placeholder is never opened here. Callers check
//! [`crate::cloud::is_dehydrated`] before [`VideoPool::request_strip`] or
//! [`VideoPool::play`].

use std::path::PathBuf;

/// Samples across a trim window. Enough for a pan to feel continuous on a
/// board node, few enough that a lookup stays cheaper than a seek.
pub const STRIP_FRAMES: usize = 96;
/// Long edge of a scrub frame, in pixels.
pub const STRIP_EDGE: u32 = 320;
/// Long edge of a playback frame, in pixels.
pub const PLAY_EDGE: u32 = 960;
/// Whole-strip resident pixel budget shared by native video consumers.
pub const STRIP_CACHE_BYTES: usize = 40 * 1024 * 1024;

/// One decoded image, RGBA, top-down, straight alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFrame {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// Worker → UI. `token` is the request the app issued; stale tokens are
/// dropped when a newer scrub or play replaced them.
#[derive(Clone, Debug)]
pub enum VideoEvent {
    /// Duration in seconds, before any filmstrip frame.
    Probed {
        id: u64,
        token: u64,
        duration: f32,
    },
    Strip {
        id: u64,
        token: u64,
        index: u16,
        time: f32,
        frame: VideoFrame,
    },
    StripDone {
        id: u64,
        token: u64,
    },
    Play {
        id: u64,
        token: u64,
        time: f32,
        frame: VideoFrame,
    },
    /// Playback reached the trim out-point and looping is off.
    Ended {
        id: u64,
        token: u64,
    },
    Failed {
        id: u64,
        token: u64,
    },
}

/// Inclusive trim window inside `duration`. An unset out-point plays to the
/// end. A reversed or empty window collapses to the in-point.
pub fn playback_window(start: f32, end: Option<f32>, duration: f32) -> (f32, f32) {
    let duration = if duration.is_finite() {
        duration.max(0.0)
    } else {
        0.0
    };
    let start = if start.is_finite() {
        start.clamp(0.0, duration)
    } else {
        0.0
    };
    let end = end
        .filter(|e| e.is_finite())
        .unwrap_or(duration)
        .clamp(start, duration.max(start));
    (start, end)
}

/// Resume inside the trim window; replay from its end starts at the in-point.
pub fn playback_origin(from: f32, start: f32, end: f32) -> f32 {
    if !from.is_finite() || from >= end - 0.001 {
        start
    } else {
        from.clamp(start, end)
    }
}

/// `u` is 0 at the left of the node and 1 at the right, across [`playback_window`].
pub fn time_at(u: f32, start: f32, end: f32) -> f32 {
    let u = if u.is_finite() {
        u.clamp(0.0, 1.0)
    } else {
        0.0
    };
    start + (end - start) * u
}

/// Where a time sits in the trim window, 0 at the in-point and 1 at the out-point.
pub fn fraction_at(time: f32, start: f32, end: f32) -> f32 {
    let span = end - start;
    if span.is_nan() || span <= 1.0e-4 || !time.is_finite() {
        return 0.0;
    }
    ((time - start) / span).clamp(0.0, 1.0)
}

/// Horizontal fraction across an axis-aligned or rotated rect.
///
/// `rotation_deg` matches the board: degrees, y-down, about the rect center.
/// `None` when the point is outside the rect.
pub fn scrub_u(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, rotation_deg: f32) -> Option<f32> {
    if !px.is_finite() || !py.is_finite() {
        return None;
    }
    let (x, w) = if w < 0.0 { (x + w, -w) } else { (x, w) };
    let (y, h) = if h < 0.0 { (y + h, -h) } else { (y, h) };
    if w < 1.0e-3 || h < 1.0e-3 {
        return None;
    }
    let (cx, cy) = (x + w * 0.5, y + h * 0.5);
    let (sin, cos) = (-rotation_deg).to_radians().sin_cos();
    let dx = px - cx;
    let dy = py - cy;
    let lx = cx + dx * cos - dy * sin;
    let ly = cy + dx * sin + dy * cos;
    if lx < x || lx > x + w || ly < y || ly > y + h {
        return None;
    }
    Some(((lx - x) / w).clamp(0.0, 1.0))
}

/// `m:ss.t`, or `h:mm:ss.t` once the clip reaches an hour.
pub fn format_timecode(secs: f32) -> String {
    let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
    let mut total = secs.floor() as u32;
    let mut tenths = ((secs - total as f32) * 10.0).round() as u32;
    if tenths >= 10 {
        tenths = 0;
        total = total.saturating_add(1);
    }
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{tenths}")
    } else {
        format!("{m}:{s:02}.{tenths}")
    }
}

/// Evenly spaced sample times. The last sample sits just inside `end` so a
/// seek does not land on end-of-stream.
pub fn sample_times(start: f32, end: f32, n: usize) -> Vec<f32> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 || end <= start + 1.0e-3 {
        return vec![start];
    }
    let last = (end - 0.04).max(start);
    (0..n)
        .map(|i| {
            if i + 1 == n {
                last
            } else {
                let u = i as f32 / (n as f32 - 1.0);
                start + (end - start) * u
            }
        })
        .collect()
}

mod frame;
#[cfg(any(windows, test))]
mod sampling;
#[cfg(any(windows, test))]
mod timebase;
pub use frame::bgra_scaled;

#[derive(Clone)]
enum Job {
    Strip {
        id: u64,
        token: u64,
        path: PathBuf,
        start: f32,
        end: Option<f32>,
    },
    Play {
        id: u64,
        token: u64,
        path: PathBuf,
        start: f32,
        end: Option<f32>,
        from: f32,
        looped: bool,
    },
}

/// Background decoder. A fresh isolated helper owns each clip request. Play
/// supersedes queued play immediately; filmstrips resume after playback stops.
pub struct VideoPool {
    tx: crossbeam_channel::Sender<runtime::StripRequest>,
    events: std::sync::Arc<events::Events>,
    control: std::sync::Arc<runtime::Control>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VideoStatus {
    Idle = 0,
    Decoding = 1,
    Unavailable = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoHealth {
    pub status: VideoStatus,
    pub restarts: u64,
    pub failures: u64,
    pub queued_frame_bytes: usize,
    pub dropped_frames: u64,
    pub worker_pid: Option<u32>,
}

#[cfg(debug_assertions)]
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum VideoWorkerFault {
    Hang,
    Crash,
    Malformed,
    Exit,
}

impl VideoPool {
    pub fn spawn() -> Self {
        Self::spawn_inner(None)
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn spawn_with_diagnostic_fault(fault: VideoWorkerFault) -> Self {
        Self::spawn_inner(Some(match fault {
            VideoWorkerFault::Hang => "hang",
            VideoWorkerFault::Crash => "crash",
            VideoWorkerFault::Malformed => "malformed",
            VideoWorkerFault::Exit => "exit",
        }))
    }

    fn spawn_inner(fault: Option<&'static str>) -> Self {
        let (tx, jobs) = crossbeam_channel::bounded(64);
        let events = std::sync::Arc::new(events::Events::new());
        let control = std::sync::Arc::new(runtime::Control::new());
        let (output, state) = (events.clone(), control.clone());
        if std::thread::Builder::new()
            .name("slate-video-supervisor".into())
            .spawn(move || runtime::supervise(jobs, output, state, fault))
            .is_err()
        {
            events.status(VideoStatus::Unavailable);
            control
                .closed
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Self {
            tx,
            events,
            control,
        }
    }

    pub fn request_strip(&self, id: u64, token: u64, path: PathBuf, start: f32, end: Option<f32>) {
        let epoch = self.control.epoch.load(std::sync::atomic::Ordering::SeqCst);
        let request = runtime::StripRequest {
            job: Job::Strip {
                id,
                token,
                path,
                start,
                end,
            },
            epoch,
        };
        if self.tx.try_send(request).is_err() {
            self.events.failed_in_context(id, token, epoch);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn play(
        &self,
        id: u64,
        token: u64,
        path: PathBuf,
        start: f32,
        end: Option<f32>,
        from: f32,
        looped: bool,
    ) {
        if self
            .control
            .closed
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            self.events.failed_in_context(
                id,
                token,
                self.control.epoch.load(std::sync::atomic::Ordering::SeqCst),
            );
            return;
        }
        self.control.play(Job::Play {
            id,
            token,
            path,
            start,
            end,
            from,
            looped,
        });
    }

    /// Pause playback; pending filmstrip preparation may continue.
    pub fn stop(&self) {
        self.control.stop();
    }

    /// Leave the source context: stop playback and invalidate all queued strips.
    pub fn cancel_all(&self) {
        let epoch = self.control.cancel_all();
        self.events.clear(epoch);
    }

    pub fn try_recv(&self) -> Option<VideoEvent> {
        self.events.pop()
    }

    pub fn health(&self) -> VideoHealth {
        self.events.health()
    }
}
impl Drop for VideoPool {
    fn drop(&mut self) {
        self.control.cancel_all();
        self.control
            .closed
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

mod events;
mod process;
mod protocol;
mod runtime;
mod worker;
/// Stdio entry point for the packaged decoder helper. Applications use VideoPool.
pub fn decoder_worker_stdio() -> std::io::Result<()> {
    worker::stdio()
}

#[cfg(windows)]
mod mf;

#[cfg(test)]
mod tests;
