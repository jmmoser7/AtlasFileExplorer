//! Canvas video frames. The board scrubs and plays; the HTML artifact still
//! uses `<video>` for web-safe files.
//!
//! Decode goes through Windows Media Foundation on a worker thread. Formats
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
    if !(span > 1.0e-4) || !time.is_finite() {
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

/// Box-filter a BGRA buffer (Media Foundation `RGB32`) into straight RGBA.
/// Negative `stride` is a bottom-up bitmap. Alpha is forced opaque.
pub fn bgra_scaled(src: &[u8], w: u32, h: u32, stride: i32, max_edge: u32) -> Option<VideoFrame> {
    if w == 0 || h == 0 || max_edge == 0 {
        return None;
    }
    let abs = stride.unsigned_abs() as usize;
    if abs < w as usize * 4 {
        return None;
    }
    let need = abs.saturating_mul(h as usize);
    if src.len() < need {
        return None;
    }
    let (dw, dh) = fit_edge(w, h, max_edge);
    let mut rgba = vec![0u8; (dw * dh * 4) as usize];
    for y in 0..dh {
        let y0 = (y * h) / dh;
        let y1 = (((y + 1) * h) / dh).max(y0 + 1).min(h);
        for x in 0..dw {
            let x0 = (x * w) / dw;
            let x1 = (((x + 1) * w) / dw).max(x0 + 1).min(w);
            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            let mut n = 0u32;
            for sy in y0..y1 {
                let row = row_offset(stride, sy, h)?;
                for sx in x0..x1 {
                    let i = row + sx as usize * 4;
                    b += src[i] as u32;
                    g += src[i + 1] as u32;
                    r += src[i + 2] as u32;
                    n += 1;
                }
            }
            if n == 0 {
                continue;
            }
            let o = ((y * dw + x) * 4) as usize;
            rgba[o] = (r / n) as u8;
            rgba[o + 1] = (g / n) as u8;
            rgba[o + 2] = (b / n) as u8;
            rgba[o + 3] = 255;
        }
    }
    Some(VideoFrame { w: dw, h: dh, rgba })
}

fn fit_edge(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let long = w.max(h).max(1);
    if long <= max_edge {
        return (w.max(1), h.max(1));
    }
    let s = max_edge as f32 / long as f32;
    (
        ((w as f32) * s).round().max(1.0) as u32,
        ((h as f32) * s).round().max(1.0) as u32,
    )
}

fn row_offset(stride: i32, y: u32, h: u32) -> Option<usize> {
    let abs = stride.unsigned_abs() as usize;
    if stride >= 0 {
        Some(y as usize * abs)
    } else if y < h {
        Some((h as usize - 1 - y as usize) * abs)
    } else {
        None
    }
}

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

/// Background decoder. One clip at a time: a filmstrip waits while another
/// node is playing, and a play request interrupts a filmstrip.
pub struct VideoPool {
    tx: crossbeam_channel::Sender<Job>,
    rx: crossbeam_channel::Receiver<VideoEvent>,
    /// `0` means nothing is playing. The worker watches this so pause does
    /// not wait behind a queued filmstrip.
    play_token: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl VideoPool {
    pub fn spawn() -> Self {
        let (job_tx, job_rx) = crossbeam_channel::unbounded();
        let (ev_tx, ev_rx) = crossbeam_channel::unbounded();
        let play_token = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let token = play_token.clone();
        let _ = std::thread::Builder::new()
            .name("slate-video".into())
            .spawn(move || {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    worker_main(job_rx, ev_tx, token);
                }));
            });
        Self {
            tx: job_tx,
            rx: ev_rx,
            play_token,
        }
    }

    pub fn request_strip(&self, id: u64, token: u64, path: PathBuf, start: f32, end: Option<f32>) {
        let _ = self.tx.send(Job::Strip {
            id,
            token,
            path,
            start,
            end,
        });
    }

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
        self.play_token
            .store(token, std::sync::atomic::Ordering::SeqCst);
        let _ = self.tx.send(Job::Play {
            id,
            token,
            path,
            start,
            end,
            from,
            looped,
        });
    }

    pub fn stop(&self) {
        self.play_token
            .store(0, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn try_recv(&self) -> Option<VideoEvent> {
        self.rx.try_recv().ok()
    }
}

#[cfg(not(windows))]
fn worker_main(
    job_rx: crossbeam_channel::Receiver<Job>,
    ev_tx: crossbeam_channel::Sender<VideoEvent>,
    play_token: std::sync::Arc<std::sync::atomic::AtomicU64>,
) {
    while let Ok(job) = job_rx.recv() {
        match job {
            Job::Strip { id, token, .. } | Job::Play { id, token, .. } => {
                play_token.store(0, std::sync::atomic::Ordering::SeqCst);
                let _ = ev_tx.send(VideoEvent::Failed { id, token });
            }
        }
    }
}

#[cfg(windows)]
fn worker_main(
    job_rx: crossbeam_channel::Receiver<Job>,
    ev_tx: crossbeam_channel::Sender<VideoEvent>,
    play_token: std::sync::Arc<std::sync::atomic::AtomicU64>,
) {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_MULTITHREADED,
        );
        if windows::Win32::Media::MediaFoundation::MFStartup(
            windows::Win32::Media::MediaFoundation::MF_VERSION,
            windows::Win32::Media::MediaFoundation::MFSTARTUP_NOSOCKET,
        )
        .is_err()
        {
            return;
        }
    }
    let mut pending_strip: Option<Job> = None;
    let mut reader: Option<mf::Open> = None;
    loop {
        let token_now = play_token.load(std::sync::atomic::Ordering::SeqCst);
        if token_now != 0 {
            match job_rx.recv_timeout(std::time::Duration::from_millis(30)) {
                Ok(job @ Job::Play { token, .. }) if token == token_now => {
                    mf::run_play(&job_rx, &ev_tx, &play_token, &mut reader, job);
                }
                Ok(Job::Play { .. }) => {}
                Ok(job @ Job::Strip { .. }) => pending_strip = Some(job),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
            continue;
        }
        if let Some(job) = pending_strip.take() {
            if mf::run_strip(&job_rx, &ev_tx, &play_token, &mut reader, &job) {
                pending_strip = Some(job);
            }
            continue;
        }
        match job_rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(job @ Job::Strip { .. }) => {
                if mf::run_strip(&job_rx, &ev_tx, &play_token, &mut reader, &job) {
                    pending_strip = Some(job);
                }
            }
            Ok(job @ Job::Play { token, .. })
                if token == play_token.load(std::sync::atomic::Ordering::SeqCst) =>
            {
                mf::run_play(&job_rx, &ev_tx, &play_token, &mut reader, job);
            }
            Ok(Job::Play { .. }) => {}
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(reader);
    unsafe {
        let _ = windows::Win32::Media::MediaFoundation::MFShutdown();
    }
}

#[cfg(windows)]
mod mf {
    use super::{
        bgra_scaled, playback_window, sample_times, Job, VideoEvent, VideoFrame, PLAY_EDGE,
        STRIP_EDGE, STRIP_FRAMES,
    };
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Media::MediaFoundation::{
        IMFAttributes, IMFMediaType, IMFSample, IMFSourceReader, MFCreateMediaType,
        MFCreateSourceReaderFromURL, MFMediaType_Video, MFVideoFormat_RGB32, MF_MT_DEFAULT_STRIDE,
        MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_PD_DURATION,
        MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READER_MEDIASOURCE,
    };
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Variant::{VT_I8, VT_UI8};

    pub struct Open {
        path: std::path::PathBuf,
        reader: IMFSourceReader,
        duration: f32,
        stream: u32,
    }

    pub fn run_strip(
        _job_rx: &crossbeam_channel::Receiver<Job>,
        ev_tx: &crossbeam_channel::Sender<VideoEvent>,
        play_token: &Arc<AtomicU64>,
        slot: &mut Option<Open>,
        job: &Job,
    ) -> bool {
        let Job::Strip {
            id,
            token,
            path,
            start,
            end,
        } = job
        else {
            return false;
        };
        let Some(open) = open_into(slot, path) else {
            let _ = ev_tx.send(VideoEvent::Failed {
                id: *id,
                token: *token,
            });
            return false;
        };
        let _ = ev_tx.send(VideoEvent::Probed {
            id: *id,
            token: *token,
            duration: open.duration,
        });
        let (win_start, win_end) = playback_window(*start, *end, open.duration);
        let times = sample_times(win_start, win_end, STRIP_FRAMES);
        for (index, time) in times.iter().copied().enumerate() {
            // A play request sets the token before its job is read. Stop this
            // filmstrip so playback does not wait behind the remaining seeks.
            if play_token.load(Ordering::SeqCst) != 0 {
                return true;
            }
            if let Some(frame) = frame_at(open, time, STRIP_EDGE) {
                let _ = ev_tx.send(VideoEvent::Strip {
                    id: *id,
                    token: *token,
                    index: index as u16,
                    time,
                    frame,
                });
            }
        }
        let _ = ev_tx.send(VideoEvent::StripDone {
            id: *id,
            token: *token,
        });
        false
    }

    pub fn run_play(
        _job_rx: &crossbeam_channel::Receiver<Job>,
        ev_tx: &crossbeam_channel::Sender<VideoEvent>,
        play_token: &Arc<AtomicU64>,
        slot: &mut Option<Open>,
        job: Job,
    ) {
        let Job::Play {
            id,
            token,
            path,
            start,
            end,
            from,
            looped,
        } = job
        else {
            return;
        };
        let Some(open) = open_into(slot, &path) else {
            if play_token.load(Ordering::SeqCst) == token {
                play_token.store(0, Ordering::SeqCst);
            }
            let _ = ev_tx.send(VideoEvent::Failed { id, token });
            return;
        };
        let _ = ev_tx.send(VideoEvent::Probed {
            id,
            token,
            duration: open.duration,
        });
        let (win_start, win_end) = playback_window(start, end, open.duration);
        let mut origin_media = from.clamp(win_start, win_end);
        if seek(open, origin_media).is_err() {
            let _ = ev_tx.send(VideoEvent::Failed { id, token });
            return;
        }
        let mut origin = Instant::now();
        loop {
            if play_token.load(Ordering::SeqCst) != token {
                return;
            }
            match next_sample(open) {
                Ok(Sample::Frame { time, frame }) => {
                    if time > win_end + 0.05 {
                        if looped && win_end > win_start {
                            origin_media = win_start;
                            origin = Instant::now();
                            if seek(open, win_start).is_err() {
                                let _ = ev_tx.send(VideoEvent::Ended { id, token });
                                return;
                            }
                            continue;
                        }
                        let _ = ev_tx.send(VideoEvent::Ended { id, token });
                        return;
                    }
                    let due = origin + Duration::from_secs_f32((time - origin_media).max(0.0));
                    let now = Instant::now();
                    if due > now {
                        sleep_until(due, play_token, token);
                        if play_token.load(Ordering::SeqCst) != token {
                            return;
                        }
                    } else if now.saturating_duration_since(due) > Duration::from_millis(120) {
                        continue;
                    }
                    let scaled =
                        bgra_scaled(&frame.bgra, frame.w, frame.h, frame.stride, PLAY_EDGE);
                    if let Some(frame) = scaled {
                        let _ = ev_tx.send(VideoEvent::Play {
                            id,
                            token,
                            time,
                            frame,
                        });
                    }
                }
                Ok(Sample::Eos) => {
                    if looped && win_end > win_start {
                        origin_media = win_start;
                        origin = Instant::now();
                        if seek(open, win_start).is_err() {
                            let _ = ev_tx.send(VideoEvent::Ended { id, token });
                            return;
                        }
                        continue;
                    }
                    let _ = ev_tx.send(VideoEvent::Ended { id, token });
                    return;
                }
                Ok(Sample::Gap) => {}
                Err(()) => {
                    let _ = ev_tx.send(VideoEvent::Failed { id, token });
                    return;
                }
            }
        }
    }

    fn sleep_until(due: Instant, play_token: &Arc<AtomicU64>, token: u64) {
        while Instant::now() < due {
            if play_token.load(Ordering::SeqCst) != token {
                return;
            }
            let left = due.saturating_duration_since(Instant::now());
            std::thread::sleep(left.min(Duration::from_millis(8)));
        }
    }

    fn open_into<'a>(slot: &'a mut Option<Open>, path: &'a Path) -> Option<&'a mut Open> {
        if slot.as_ref().is_some_and(|open| open.path == path) {
            return slot.as_mut();
        }
        *slot = Open::open(path).ok();
        slot.as_mut()
    }

    struct RawFrame {
        w: u32,
        h: u32,
        stride: i32,
        bgra: Vec<u8>,
    }

    enum Sample {
        Frame { time: f32, frame: RawFrame },
        Gap,
        Eos,
    }

    impl Open {
        fn open(path: &Path) -> Result<Self, ()> {
            let wide = wide_path(path);
            let reader = unsafe {
                MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), None::<&IMFAttributes>)
            }
            .map_err(|_| ())?;
            let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            configure_rgb32(&reader, stream)?;
            let duration = duration_secs(&reader)?;
            Ok(Self {
                path: path.to_path_buf(),
                reader,
                duration,
                stream,
            })
        }
    }

    fn configure_rgb32(reader: &IMFSourceReader, stream: u32) -> Result<(), ()> {
        unsafe {
            let media_type: IMFMediaType = MFCreateMediaType().map_err(|_| ())?;
            let attrs: IMFAttributes = media_type.cast().map_err(|_| ())?;
            attrs
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(|_| ())?;
            attrs
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .map_err(|_| ())?;
            reader
                .SetCurrentMediaType(stream, None, &media_type)
                .map_err(|_| ())?;
        }
        Ok(())
    }

    fn duration_secs(reader: &IMFSourceReader) -> Result<f32, ()> {
        let index = MF_SOURCE_READER_MEDIASOURCE.0 as u32;
        let var =
            unsafe { reader.GetPresentationAttribute(index, &MF_PD_DURATION) }.map_err(|_| ())?;
        let ticks = prop_u64(&var).ok_or(())?;
        Ok((ticks as f64 / 10_000_000.0) as f32)
    }

    fn prop_u64(var: &PROPVARIANT) -> Option<u64> {
        unsafe {
            let inner = &var.Anonymous.Anonymous;
            if inner.vt == VT_UI8 {
                Some(inner.Anonymous.uhVal)
            } else if inner.vt == VT_I8 {
                Some(inner.Anonymous.hVal as u64)
            } else {
                None
            }
        }
    }

    fn seek(open: &Open, secs: f32) -> Result<(), ()> {
        let hns = (secs.max(0.0) as f64 * 10_000_000.0) as i64;
        let mut var = PROPVARIANT::default();
        unsafe {
            (*var.Anonymous.Anonymous).vt = VT_I8;
            (*var.Anonymous.Anonymous).Anonymous.hVal = hns;
            open.reader
                .SetCurrentPosition(&windows::core::GUID::zeroed(), &var)
                .map_err(|_| ())?;
        }
        Ok(())
    }

    fn frame_at(open: &mut Open, secs: f32, max_edge: u32) -> Option<VideoFrame> {
        seek(open, secs).ok()?;
        // A seek lands on a keyframe. Read forward until the sample time
        // reaches the requested instant so the filmstrip is the real frame,
        // not the previous keyframe. Cap the walk so a long GOP cannot stall.
        let mut last = None;
        for _ in 0..120 {
            match next_sample(open).ok()? {
                Sample::Frame { time, frame } => {
                    if time + 0.03 >= secs {
                        return bgra_scaled(&frame.bgra, frame.w, frame.h, frame.stride, max_edge);
                    }
                    last = Some(frame);
                }
                Sample::Gap => continue,
                Sample::Eos => break,
            }
        }
        let frame = last?;
        bgra_scaled(&frame.bgra, frame.w, frame.h, frame.stride, max_edge)
    }

    fn next_sample(open: &Open) -> Result<Sample, ()> {
        let mut flags = 0u32;
        let mut stamp = 0i64;
        let mut sample: Option<IMFSample> = None;
        unsafe {
            open.reader
                .ReadSample(
                    open.stream,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut stamp),
                    Some(&mut sample),
                )
                .map_err(|_| ())?;
        }
        if flags & 1 != 0 {
            return Err(());
        }
        if flags & 32 != 0 {
            configure_rgb32(&open.reader, open.stream)?;
        }
        if flags & 2 != 0 && sample.is_none() {
            return Ok(Sample::Eos);
        }
        let Some(sample) = sample else {
            return Ok(if flags & 2 != 0 {
                Sample::Eos
            } else {
                Sample::Gap
            });
        };
        let time = if stamp > 0 {
            (stamp as f64 / 10_000_000.0) as f32
        } else {
            unsafe { sample.GetSampleTime() }
                .map(|hns| (hns.max(0) as f64 / 10_000_000.0) as f32)
                .unwrap_or(0.0)
        };
        let frame = copy_sample(&sample, &open.reader, open.stream)?;
        Ok(Sample::Frame { time, frame })
    }

    fn copy_sample(
        sample: &IMFSample,
        reader: &IMFSourceReader,
        stream: u32,
    ) -> Result<RawFrame, ()> {
        let (w, h, stride) = frame_layout(reader, stream)?;
        let buffer = unsafe { sample.ConvertToContiguousBuffer() }.map_err(|_| ())?;
        let mut ptr = std::ptr::null_mut();
        let mut max_len = 0u32;
        let mut cur_len = 0u32;
        unsafe {
            buffer
                .Lock(&mut ptr, Some(&mut max_len), Some(&mut cur_len))
                .map_err(|_| ())?;
        }
        let len = cur_len as usize;
        let bgra = if ptr.is_null() || len == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(ptr, len).to_vec() }
        };
        unsafe {
            let _ = buffer.Unlock();
        }
        if bgra.is_empty() {
            return Err(());
        }
        Ok(RawFrame { w, h, stride, bgra })
    }

    fn frame_layout(reader: &IMFSourceReader, stream: u32) -> Result<(u32, u32, i32), ()> {
        let media = unsafe { reader.GetCurrentMediaType(stream) }.map_err(|_| ())?;
        let packed = unsafe { media.GetUINT64(&MF_MT_FRAME_SIZE) }.map_err(|_| ())?;
        let w = (packed >> 32) as u32;
        let h = packed as u32;
        if w == 0 || h == 0 {
            return Err(());
        }
        let stride = unsafe { media.GetUINT32(&MF_MT_DEFAULT_STRIDE) }
            .map(|s| s as i32)
            .unwrap_or((w.saturating_mul(4)) as i32);
        let stride = if stride == 0 {
            w.saturating_mul(4) as i32
        } else {
            stride
        };
        Ok((w, h, stride))
    }

    fn wide_path(path: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pan_covers_the_trim_window() {
        let (start, end) = playback_window(3.0, Some(11.0), 20.0);
        assert_eq!(time_at(0.0, start, end), 3.0);
        assert_eq!(time_at(1.0, start, end), 11.0);
        assert_eq!(time_at(0.5, start, end), 7.0);
        assert_eq!(fraction_at(7.0, start, end), 0.5);
        let (start, end) = playback_window(0.0, None, 8.0);
        assert_eq!((start, end), (0.0, 8.0));
        let (start, end) = playback_window(30.0, Some(10.0), 20.0);
        assert_eq!(start, end);
    }

    #[test]
    fn scrub_u_follows_the_node_width_and_rotation() {
        assert_eq!(scrub_u(0.0, 20.0, 0.0, 0.0, 100.0, 40.0, 0.0), Some(0.0));
        assert_eq!(scrub_u(100.0, 20.0, 0.0, 0.0, 100.0, 40.0, 0.0), Some(1.0));
        assert_eq!(scrub_u(50.0, 20.0, 0.0, 0.0, 100.0, 40.0, 0.0), Some(0.5));
        assert!(scrub_u(-1.0, 20.0, 0.0, 0.0, 100.0, 40.0, 0.0).is_none());
        // 180° swaps the left and right edges.
        assert_eq!(scrub_u(0.0, 20.0, 0.0, 0.0, 100.0, 40.0, 180.0), Some(1.0));
        assert_eq!(
            scrub_u(100.0, 20.0, 0.0, 0.0, 100.0, 40.0, 180.0),
            Some(0.0)
        );
    }

    #[test]
    fn timecode_and_sample_grid() {
        assert_eq!(format_timecode(0.0), "0:00.0");
        assert_eq!(format_timecode(65.24), "1:05.2");
        assert_eq!(format_timecode(3600.0), "1:00:00.0");
        let times = sample_times(0.0, 10.0, 96);
        assert_eq!(times.len(), 96);
        assert_eq!(times[0], 0.0);
        assert!(times[95] < 10.0);
        assert!(times[95] > 9.0);
    }

    #[test]
    fn bgra_scale_swaps_channels_and_honors_bottom_up() {
        let px = |b, g, r| [b, g, r, 0];
        let mut top_down = Vec::new();
        top_down.extend_from_slice(&px(10, 20, 30));
        top_down.extend_from_slice(&px(10, 20, 30));
        top_down.extend_from_slice(&px(10, 20, 30));
        top_down.extend_from_slice(&px(10, 20, 30));
        let frame = bgra_scaled(&top_down, 2, 2, 8, 2).unwrap();
        assert_eq!(&frame.rgba[0..4], &[30, 20, 10, 255]);

        let mut bottom_up = Vec::new();
        bottom_up.extend_from_slice(&px(1, 2, 3));
        bottom_up.extend_from_slice(&px(1, 2, 3));
        bottom_up.extend_from_slice(&px(4, 5, 6));
        bottom_up.extend_from_slice(&px(4, 5, 6));
        let frame = bgra_scaled(&bottom_up, 2, 2, -8, 2).unwrap();
        assert_eq!(&frame.rgba[0..4], &[6, 5, 4, 255]);
    }
}
