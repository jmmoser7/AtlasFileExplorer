//! One bounded, asynchronous encoder for every moving portal.
//!
//! Capture callbacks never wait for a codec. Dropping frames under load keeps
//! their real timestamps: a slow encoder lowers motion quality, not duration.

#[cfg(any(windows, test))]
use crossbeam_channel::bounded;
use crossbeam_channel::{Receiver, Sender};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;

#[cfg(any(windows, test))]
const QUEUE_FRAMES: usize = 3;

#[derive(Debug)]
pub(crate) struct CapturedFrame {
    pub generation: u64,
    pub w: u32,
    pub h: u32,
    /// Top-down RGBA, tightly packed. Conversion belongs to the worker.
    pub rgba: Vec<u8>,
    pub captured_at: Instant,
    pub recycle: Option<Sender<Vec<u8>>>,
}

impl Drop for CapturedFrame {
    fn drop(&mut self) {
        if !self.rgba.is_empty() {
            if let Some(recycle) = self.recycle.as_ref() {
                let _ = recycle.try_send(std::mem::take(&mut self.rgba));
            }
        }
    }
}

pub(crate) struct VideoEncoder {
    frames: Option<Sender<CapturedFrame>>,
    stop: Sender<Instant>,
    result: Receiver<Result<PathBuf, String>>,
    cancelled: Arc<AtomicBool>,
    started: Instant,
    width: u32,
    height: u32,
    generation: Option<u64>,
    resolved: bool,
}

impl VideoEncoder {
    #[cfg(all(test, not(windows)))]
    pub(crate) fn start(path: PathBuf, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        Self::start_at(path, width, height, fps, Instant::now())
    }

    pub(crate) fn start_at(
        path: PathBuf,
        width: u32,
        height: u32,
        fps: u32,
        started: Instant,
    ) -> Result<Self, String> {
        validate_format(width, height, fps)?;
        #[cfg(not(windows))]
        {
            let _ = (path, started);
            Err("Portal video capture requires Windows Media Foundation.".into())
        }
        #[cfg(windows)]
        {
            let (mut encoder, mut worker) = Self::channels(width, height);
            encoder.started = started;
            worker.started = started;
            let (result_tx, result_rx) = bounded(1);
            encoder.result = result_rx;
            std::thread::Builder::new()
                .name("portal-video-encode".into())
                .spawn(move || {
                    let result = windows::encode(worker, path, width, height, fps);
                    let _ = result_tx.try_send(result);
                })
                .map_err(|e| format!("Could not start portal video encoding: {e}"))?;
            Ok(encoder)
        }
    }

    /// Full queue, stale generation, or wrong dimensions: discard immediately.
    pub(crate) fn try_frame(&mut self, frame: CapturedFrame) -> bool {
        let Some(sender) = self.frames.as_ref() else {
            return false;
        };
        if frame.w != self.width
            || frame.h != self.height
            || frame.rgba.len() != self.width as usize * self.height as usize * 4
            || frame.captured_at < self.started
            || self.generation.is_some_and(|g| g != frame.generation)
        {
            return false;
        }
        let generation = frame.generation;
        if sender.try_send(frame).is_err() {
            return false;
        }
        self.generation = Some(generation);
        true
    }

    /// Drain accepted frames, hold the last image until this instant, finalize.
    #[cfg(test)]
    pub(crate) fn finish(&mut self) {
        self.finish_at(Instant::now());
    }

    pub(super) fn finish_at(&mut self, stopped: Instant) {
        if self.frames.is_some() {
            let _ = self.stop.try_send(stopped);
            self.frames.take();
        }
    }

    pub(super) fn resume_generation(&mut self, generation: u64) {
        self.generation = Some(generation);
    }

    /// No joins or filesystem work on the UI/capture thread.
    pub(crate) fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.frames.take();
    }

    pub(crate) fn take_result(&mut self) -> Option<Result<PathBuf, String>> {
        if self.resolved {
            return None;
        }
        let result = match self.result.try_recv() {
            Ok(result) => result,
            Err(crossbeam_channel::TryRecvError::Empty) => return None,
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                Err("Portal video encoder stopped before completing the recording.".into())
            }
        };
        self.resolved = true;
        Some(result)
    }

    #[cfg(any(windows, test))]
    fn channels(width: u32, height: u32) -> (Self, Worker) {
        let (frames_tx, frames) = bounded(QUEUE_FRAMES);
        let (stop_tx, stop) = bounded(1);
        let (_, result) = bounded(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let started = Instant::now();
        (
            Self {
                frames: Some(frames_tx),
                stop: stop_tx,
                result,
                cancelled: Arc::clone(&cancelled),
                started,
                width,
                height,
                generation: None,
                resolved: false,
            },
            Worker {
                frames,
                stop,
                cancelled,
                started,
            },
        )
    }
}

impl Drop for VideoEncoder {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn validate_format(width: u32, height: u32, fps: u32) -> Result<(), String> {
    if width < 2 || height < 2 || width > 1920 || height > 1080 {
        return Err("Portal video capture must fit within 1920 × 1080 pixels.".into());
    }
    if width % 2 != 0 || height % 2 != 0 {
        return Err("Portal video capture requires even pixel dimensions.".into());
    }
    if !(1..=30).contains(&fps) {
        return Err("Portal video capture supports 1–30 frames per second.".into());
    }
    Ok(())
}

#[cfg(any(windows, test))]
struct Worker {
    frames: Receiver<CapturedFrame>,
    stop: Receiver<Instant>,
    cancelled: Arc<AtomicBool>,
    started: Instant,
}

#[cfg(any(windows, test))]
impl Worker {
    fn check_cancelled(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            Err("Portal video capture cancelled.".into())
        } else {
            Ok(())
        }
    }
}

#[cfg(any(windows, test))]
#[derive(Debug, PartialEq, Eq)]
struct SampleTiming {
    start: i64,
    duration: i64,
}

/// Media Foundation timestamps are 100-nanosecond ticks. Keep one pending
/// sample, so the next real capture determines how long it stays visible.
#[cfg(any(windows, test))]
struct Timeline {
    origin: Instant,
    pending: i64,
}

#[cfg(any(windows, test))]
impl Timeline {
    fn new(origin: Instant) -> Self {
        Self { origin, pending: 0 }
    }

    fn advance(&mut self, captured_at: Instant) -> Option<SampleTiming> {
        let next = self.ticks(captured_at);
        if next <= self.pending {
            return None;
        }
        let timing = SampleTiming {
            start: self.pending,
            duration: next - self.pending,
        };
        self.pending = next;
        Some(timing)
    }

    fn finish(&self, stopped: Instant) -> SampleTiming {
        SampleTiming {
            start: self.pending,
            duration: (self.ticks(stopped) - self.pending).max(1),
        }
    }

    fn ticks(&self, at: Instant) -> i64 {
        (at.saturating_duration_since(self.origin).as_nanos() / 100).min(i64::MAX as u128) as i64
    }
}
