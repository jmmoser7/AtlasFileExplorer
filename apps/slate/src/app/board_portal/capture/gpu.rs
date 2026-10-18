//! Bounded, asynchronous snapshots of the already-painted portal contents.
//!
//! The cached callback runs between the content and its board chrome. It never
//! waits for a fence or mutex; a busy GPU/encoder drops samples instead.

use std::sync::{Arc, Mutex};
use std::thread::ThreadId;
use std::time::Instant;

use crossbeam_channel::{bounded, Receiver, Sender};
use eframe::{egui, egui_glow, glow};
use glow::HasContext as _;

use super::CapturedFrame;

mod geometry;
mod gl;
#[cfg(all(test, windows))]
mod native_tests;

const PENDING_LIMIT: usize = 2;
const CPU_LIMIT: usize = 3;

#[derive(Clone, Copy)]
struct Request {
    generation: u64,
    max_size: [u32; 2],
    output_size: Option<[u32; 2]>,
}

struct Pending {
    fence: glow::Fence,
    generation: u64,
    epoch: u64,
    captured_at: Instant,
}

struct Slot {
    buffer: glow::Buffer,
    pending: Option<Pending>,
}

struct State {
    request: Option<Request>,
    generation: Option<u64>,
    epoch: u64,
    planned_size: Option<geometry::Size>,
    targets: Option<gl::Targets>,
    slots: Vec<Slot>,
    pool: Vec<Vec<u8>>,
    returned: Receiver<Vec<u8>>,
    recycle: Sender<Vec<u8>>,
    ready: Option<CapturedFrame>,
    last_delivered: Option<Instant>,
    error: Option<&'static str>,
}

// Native GL fences are opaque pointers, not dereferenced by Rust. All GL use,
// including fence destruction, is asserted to run on the creating thread.
// The mutex only transfers requests and owned byte buffers to that callback.
unsafe impl Send for State {}

struct Shared {
    gl: Arc<glow::Context>,
    owner: ThreadId,
    state: Mutex<State>,
}

/// One capture stream, retaining at most two GPU reads and three CPU buffers.
pub(super) struct GpuCapture {
    shared: Arc<Shared>,
    callback: Arc<egui_glow::CallbackFn>,
}

impl GpuCapture {
    pub(super) fn new(gl: Arc<glow::Context>) -> Self {
        let (recycle, returned) = bounded(CPU_LIMIT);
        let shared = Arc::new(Shared {
            gl,
            owner: std::thread::current().id(),
            state: Mutex::new(State {
                request: None,
                generation: None,
                epoch: 0,
                planned_size: None,
                targets: None,
                slots: Vec::with_capacity(PENDING_LIMIT),
                pool: Vec::with_capacity(CPU_LIMIT),
                returned,
                recycle,
                ready: None,
                last_delivered: None,
                error: None,
            }),
        });
        let worker = Arc::clone(&shared);
        let callback = Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
            assert_eq!(worker.owner, std::thread::current().id());
            let Ok(mut state) = worker.state.try_lock() else {
                return;
            };
            // egui's framebuffer is current here. Save each binding separately:
            // FRAMEBUFFER would overwrite a caller's distinct read binding.
            unsafe {
                let restore = gl::Restore::save(&worker.gl);
                state.render(&worker.gl, &info, painter.intermediate_fbo());
                restore.restore(&worker.gl);
            }
        }));
        Self { shared, callback }
    }

    pub(super) fn request(
        &self,
        generation: u64,
        max_size: [u32; 2],
        output_size: Option<[u32; 2]>,
    ) {
        if let Ok(mut state) = self.shared.state.try_lock() {
            if state.generation != Some(generation) {
                state.ready = None;
                state.last_delivered = None;
                state.error = None;
            }
            state.generation = Some(generation);
            state.request = Some(Request {
                generation,
                max_size,
                output_size,
            });
        }
    }

    /// Also polls pending reads on frames without a newly requested sample.
    pub(super) fn paint(&self, painter: &egui::Painter, rect: egui::Rect) {
        painter.add(egui::PaintCallback {
            rect,
            callback: self.callback.clone(),
        });
    }

    pub(super) fn take_frame(&mut self) -> Option<CapturedFrame> {
        let mut state = self.shared.state.try_lock().ok()?;
        let frame = state.ready.take()?;
        state.last_delivered = Some(frame.captured_at);
        Some(frame)
    }

    pub(super) fn take_error(&self) -> Option<&'static str> {
        self.shared.state.try_lock().ok()?.error.take()
    }

    pub(super) fn cancel(&self) {
        if let Ok(mut state) = self.shared.state.try_lock() {
            state.generation = None;
            state.request = None;
            state.ready = None;
        }
    }

    /// Call from `on_exit`, while the creating GL context is still current.
    pub(super) fn destroy(&self) {
        assert_eq!(self.shared.owner, std::thread::current().id());
        if let Ok(mut state) = self.shared.state.try_lock() {
            state.generation = None;
            state.request = None;
            state.ready = None;
            unsafe { state.release(&self.shared.gl, false) };
        }
    }
}

impl State {
    unsafe fn render(
        &mut self,
        gl: &glow::Context,
        info: &egui::PaintCallbackInfo,
        source: Option<glow::Framebuffer>,
    ) {
        self.reclaim();
        let planned = self.request.and_then(|request| {
            match geometry::plan(info, request.max_size, request.output_size) {
                Ok(plan) => Some((request, plan)),
                Err(error) => {
                    self.error = Some(error);
                    self.request = None;
                    None
                }
            }
        });
        if let Some((_, plan)) = planned.as_ref() {
            if self.planned_size != Some(plan.size) {
                self.epoch = self.epoch.wrapping_add(1);
                self.planned_size = Some(plan.size);
                self.ready = None;
            }
        }
        unsafe { self.poll(gl) };
        let Some((request, plan)) = planned else {
            return;
        };
        if self
            .targets
            .as_ref()
            .is_none_or(|target| target.size != plan.size)
        {
            if self.slots.iter().any(|slot| slot.pending.is_some()) {
                // Keep the new request while old reads retire. Deleting and
                // reallocating every resize would hide unbounded driver work.
                return;
            }
            // A recording's output size is fixed. Resizing its visible source
            // replaces GPU targets but retains all three leased CPU buffers.
            let retain_cpu = self
                .targets
                .as_ref()
                .is_some_and(|target| target.size.output == plan.size.output);
            unsafe { self.release(gl, retain_cpu) };
            match unsafe { gl::Targets::new(gl, plan.size) } {
                Ok(targets) => {
                    self.targets = Some(targets);
                    if !unsafe { self.allocate_slots(gl, plan.size.output, !retain_cpu) } {
                        self.error = Some("The graphics driver could not allocate capture buffers");
                        self.request = None;
                        unsafe { self.release(gl, false) };
                        return;
                    }
                }
                Err(error) => {
                    self.error = Some(error);
                    self.request = None;
                    return;
                }
            }
        }
        if self.pool.is_empty() && self.ready.is_none() {
            return; // All three buffers are leased to the writer/encoder.
        }
        let Some(slot) = self.slots.iter_mut().find(|slot| slot.pending.is_none()) else {
            return; // Two copies are pending: retain interaction latency.
        };
        let targets = self.targets.as_ref().expect("capture target allocated");
        self.request = None;
        unsafe {
            targets.capture(gl, source, &plan, slot.buffer);
            match gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0) {
                Ok(fence) => {
                    slot.pending = Some(Pending {
                        fence,
                        generation: request.generation,
                        epoch: self.epoch,
                        captured_at: Instant::now(),
                    });
                    // Submit, never wait: the next frame probes with timeout 0.
                    gl.flush();
                }
                Err(_) => self.error = Some("The graphics driver could not schedule capture"),
            }
        }
    }

    fn reclaim(&mut self) {
        let len = self.targets.as_ref().map(|target| target.byte_len());
        while let Ok(buffer) = self.returned.try_recv() {
            if Some(buffer.len()) == len && self.pool.len() < CPU_LIMIT {
                self.pool.push(buffer);
            }
        }
    }

    unsafe fn allocate_slots(
        &mut self,
        gl: &glow::Context,
        output: [u32; 2],
        allocate_cpu: bool,
    ) -> bool {
        let len = output[0] as usize * output[1] as usize * 4;
        for _ in 0..PENDING_LIMIT {
            let Ok(buffer) = (unsafe { gl.create_buffer() }) else {
                return false;
            };
            unsafe {
                gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(buffer));
                gl.buffer_data_size(glow::PIXEL_PACK_BUFFER, len as i32, glow::STREAM_READ);
                if gl.get_buffer_parameter_i32(glow::PIXEL_PACK_BUFFER, glow::BUFFER_SIZE)
                    != len as i32
                {
                    gl.delete_buffer(buffer);
                    return false;
                }
            }
            self.slots.push(Slot {
                buffer,
                pending: None,
            });
        }
        if allocate_cpu {
            for _ in 0..CPU_LIMIT {
                self.pool.push(vec![0; len]);
            }
        }
        true
    }

    unsafe fn poll(&mut self, gl: &glow::Context) {
        for slot in &mut self.slots {
            let Some(pending) = slot.pending.as_ref() else {
                continue;
            };
            let status = unsafe { gl.client_wait_sync(pending.fence, 0, 0) };
            if status == glow::TIMEOUT_EXPIRED {
                continue;
            }
            let pending = slot.pending.take().expect("pending fence");
            unsafe { gl.delete_sync(pending.fence) };
            if status == glow::WAIT_FAILED {
                self.error = Some("The graphics driver could not complete capture");
            }
            if !matches!(status, glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED)
                || !accepts(
                    self.generation,
                    self.epoch,
                    self.ready
                        .as_ref()
                        .map(|frame| frame.captured_at)
                        .or(self.last_delivered),
                    pending.generation,
                    pending.epoch,
                    pending.captured_at,
                )
            {
                continue;
            }
            // Reuse the queued frame's buffer if the UI has not consumed it.
            if let Some(previous) = self.ready.take() {
                drop(previous);
                while let Ok(buffer) = self.returned.try_recv() {
                    if self.pool.len() < CPU_LIMIT
                        && self
                            .targets
                            .as_ref()
                            .is_some_and(|target| target.byte_len() == buffer.len())
                    {
                        self.pool.push(buffer);
                    }
                }
            }
            let Some(mut rgba) = self.pool.pop() else {
                continue; // Encoder pressure: no allocation and no waiting.
            };
            unsafe {
                gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(slot.buffer));
                // The fence signaled, so this is a CPU copy of completed data.
                gl.get_buffer_sub_data(glow::PIXEL_PACK_BUFFER, 0, &mut rgba);
            }
            let [w, h] = self.targets.as_ref().expect("pending target").size.output;
            self.ready = Some(CapturedFrame {
                generation: pending.generation,
                w,
                h,
                rgba,
                captured_at: pending.captured_at,
                recycle: Some(self.recycle.clone()),
            });
        }
    }

    unsafe fn release(&mut self, gl: &glow::Context, retain_cpu: bool) {
        self.ready = None;
        if !retain_cpu {
            self.pool.clear();
            while self.returned.try_recv().is_ok() {}
        }
        for slot in self.slots.drain(..) {
            if let Some(pending) = slot.pending {
                unsafe { gl.delete_sync(pending.fence) };
            }
            unsafe { gl.delete_buffer(slot.buffer) };
        }
        if let Some(targets) = self.targets.take() {
            unsafe { targets.destroy(gl) };
        }
    }
}

fn accepts(
    generation: Option<u64>,
    epoch: u64,
    newest: Option<Instant>,
    pending_generation: u64,
    pending_epoch: u64,
    captured_at: Instant,
) -> bool {
    generation == Some(pending_generation)
        && epoch == pending_epoch
        && newest.is_none_or(|newest| captured_at > newest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_gpu_results_cannot_cross_cancel_new_session_or_resize() {
        let now = Instant::now();
        assert!(accepts(Some(7), 3, None, 7, 3, now));
        assert!(!accepts(None, 3, None, 7, 3, now));
        assert!(!accepts(Some(8), 3, None, 7, 3, now));
        assert!(!accepts(Some(7), 4, None, 7, 3, now));
    }

    #[test]
    fn reused_gpu_slots_never_publish_an_older_completed_frame() {
        let old = Instant::now();
        let new = old + std::time::Duration::from_millis(33);
        assert!(accepts(Some(7), 3, Some(old), 7, 3, new));
        assert!(!accepts(Some(7), 3, Some(new), 7, 3, old));
        assert!(!accepts(Some(7), 3, Some(new), 7, 3, new));
    }
}
