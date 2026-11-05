#[cfg(test)]
use super::VideoFrame;
use super::{VideoEvent, VideoHealth, VideoStatus};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;

/// Retained decoded frames across the UI boundary. Codec/source memory lives in
/// the isolated worker and is never copied wholesale into the application.
pub(super) const FRAME_BUDGET: usize = 8 * 1024 * 1024;
const EVENT_LIMIT: usize = 128;
const CONTROL_RESERVE: usize = 32;
pub(super) enum Enqueue {
    Accepted,
    Stale,
    Full(VideoEvent),
}

pub(super) struct Events {
    queue: Mutex<Queue>,
    status: AtomicU8,
    pub failures: AtomicU64,
    pub restarts: AtomicU64,
    pub worker_pid: AtomicU32,
}
struct Queue {
    items: VecDeque<VideoEvent>,
    bytes: usize,
    dropped: u64,
    epoch: u64,
    opening: Option<Opening>,
}
#[derive(Clone, Copy)]
struct Opening {
    key: (u64, u64),
    consumed: bool,
}
impl Events {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(Queue {
                items: VecDeque::new(),
                bytes: 0,
                dropped: 0,
                epoch: 0,
                opening: None,
            }),
            status: AtomicU8::new(0),
            failures: AtomicU64::new(0),
            restarts: AtomicU64::new(0),
            worker_pid: AtomicU32::new(0),
        }
    }
    pub fn push_in_context(&self, event: VideoEvent, epoch: u64) -> bool {
        matches!(self.enqueue_in_context(event, epoch), Enqueue::Accepted)
    }
    pub fn enqueue_in_context(&self, event: VideoEvent, epoch: u64) -> Enqueue {
        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if epoch != queue.epoch {
            return Enqueue::Stale;
        }
        let mut first_queued = None;
        // Preserve the opening frame until the consumer sees it. While the
        // UI is stalled, retain just that opening plus the newest later frame.
        if let VideoEvent::Play {
            id, token, time, ..
        } = &event
        {
            let key = (*id, *token);
            if queue.opening.is_none_or(|opening| opening.key != key) {
                queue.opening = Some(Opening {
                    key,
                    consumed: false,
                });
            }
            let protected = queue.protected_opening();
            if protected.is_none() && queue.opening.is_some_and(|opening| !opening.consumed) {
                first_queued = Some((key, *time));
            }
            if let Some(index) = queue.items.iter().enumerate().find_map(|(index, old)| {
                (Some(index) != protected
                    && matches!(old, VideoEvent::Play { id: a, token: b, .. } if (*a, *b) == key))
                .then_some(index)
            }) {
                queue.remove(index);
            }
        }
        let bytes = event.frame_bytes();
        if bytes != 0 {
            while queue.bytes.saturating_add(bytes) > FRAME_BUDGET
                || queue.items.len() >= EVENT_LIMIT
            {
                let protected = queue.protected_opening();
                let disposable = queue.items.iter().enumerate().find_map(|(index, old)| {
                    (Some(index) != protected && matches!(old, VideoEvent::Play { .. }))
                        .then_some(index)
                });
                let Some(index) = disposable else {
                    return Enqueue::Full(event);
                };
                queue.remove(index);
            }
        } else {
            // Reserve a small bounded control lane. Strip pixels are never
            // evicted to deliver completion/failure, so Done stays behind all
            // accepted sample indexes even when the consumer stops draining.
            if queue
                .items
                .iter()
                .filter(|old| old.frame_bytes() == 0)
                .count()
                >= CONTROL_RESERVE
            {
                let index = queue
                    .items
                    .iter()
                    .position(|old| old.frame_bytes() == 0)
                    .expect("a counted control record exists while queue mutex is held");
                queue.remove(index);
            }
        }
        if matches!(event, VideoEvent::Failed { .. }) {
            self.failures.fetch_add(1, Ordering::Relaxed);
            self.status(VideoStatus::Unavailable);
        }
        if let Some((key, time)) = first_queued {
            trace("queue.first-queued", key, time);
        }
        queue.bytes += bytes;
        queue.items.push_back(event);
        Enqueue::Accepted
    }
    #[cfg(test)]
    pub fn push(&self, event: VideoEvent) {
        self.push_in_context(event, 0);
    }
    pub fn clear(&self, epoch: u64) {
        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        queue.epoch = epoch;
        queue.items.clear();
        queue.bytes = 0;
        queue.opening = None;
        self.status(VideoStatus::Idle);
    }
    pub fn pop(&self) -> Option<VideoEvent> {
        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let event = queue.items.pop_front()?;
        queue.bytes -= event.frame_bytes();
        if let VideoEvent::Play {
            id, token, time, ..
        } = &event
        {
            if let Some(opening) = queue
                .opening
                .as_mut()
                .filter(|opening| opening.key == (*id, *token) && !opening.consumed)
            {
                opening.consumed = true;
                trace("queue.first-delivered", opening.key, *time);
            }
        }
        Some(event)
    }
    pub fn status(&self, status: VideoStatus) {
        self.status.store(status as u8, Ordering::Relaxed);
    }
    pub fn failed_in_context(&self, id: u64, token: u64, epoch: u64) -> bool {
        self.push_in_context(VideoEvent::Failed { id, token }, epoch)
    }
    #[cfg(test)]
    pub fn failed(&self, id: u64, token: u64) {
        self.failed_in_context(id, token, 0);
    }
    pub fn health(&self) -> VideoHealth {
        let queue = self
            .queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        VideoHealth {
            status: match self.status.load(Ordering::Relaxed) {
                1 => VideoStatus::Decoding,
                2 => VideoStatus::Unavailable,
                _ => VideoStatus::Idle,
            },
            failures: self.failures.load(Ordering::Relaxed),
            restarts: self.restarts.load(Ordering::Relaxed),
            queued_frame_bytes: queue.bytes,
            dropped_frames: queue.dropped,
            worker_pid: std::num::NonZeroU32::new(self.worker_pid.load(Ordering::Relaxed))
                .map(|pid| pid.get()),
        }
    }
}
fn trace(stage: &str, key: (u64, u64), time: f32) {
    if std::env::var_os("ATLAS_VIDEO_DIAGNOSTICS").is_some_and(|value| value == "1") {
        eprintln!(
            "atlas-video stage={stage} id={} token={} time={time:.6}",
            key.0, key.1
        );
    }
}
impl Queue {
    fn protected_opening(&self) -> Option<usize> {
        let opening = self.opening.filter(|opening| !opening.consumed)?;
        self.items.iter().position(|event| {
            matches!(event, VideoEvent::Play { .. }) && event.key() == opening.key
        })
    }
    fn remove(&mut self, index: usize) {
        if let Some(event) = self.items.remove(index) {
            let bytes = event.frame_bytes();
            self.bytes -= bytes;
            if bytes != 0 {
                self.dropped += 1;
            }
        }
    }
}
impl VideoEvent {
    pub(super) fn key(&self) -> (u64, u64) {
        match self {
            Self::Probed { id, token, .. }
            | Self::Strip { id, token, .. }
            | Self::StripDone { id, token }
            | Self::Play { id, token, .. }
            | Self::Ended { id, token }
            | Self::Failed { id, token } => (*id, *token),
        }
    }
    fn frame_bytes(&self) -> usize {
        match self {
            Self::Strip { frame, .. } | Self::Play { frame, .. } => frame.rgba.len(),
            _ => 0,
        }
    }
    pub(super) fn terminal(&self) -> bool {
        matches!(
            self,
            Self::StripDone { .. } | Self::Ended { .. } | Self::Failed { .. }
        )
    }
}
#[cfg(test)]
mod tests;
