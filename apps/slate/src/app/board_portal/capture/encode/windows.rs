//! Windows' built-in H.264 MP4 writer. All COM objects stay on this worker.

use super::super::output::Draft;
use super::{CapturedFrame, SampleTiming, Timeline, Worker};
use ::windows::core::PCWSTR;
use ::windows::Win32::Media::MediaFoundation::*;
use ::windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

#[cfg(test)]
mod tests;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

pub(super) fn encode(
    worker: Worker,
    destination: PathBuf,
    width: u32,
    height: u32,
    fps: u32,
) -> Result<PathBuf, String> {
    worker.check_cancelled()?;
    let _runtime = Runtime::new()?;
    let draft = Draft::reserve(destination)?;
    let writer = Writer::new(draft.path(), width, height, fps)
        .map_err(|e| format!("Windows H.264 video encoding is unavailable: {e}"))?;
    let mut timeline = Timeline::new(worker.started);
    let mut pending: Option<CapturedFrame> = None;
    while let Ok(frame) = worker.frames.recv() {
        worker.check_cancelled()?;
        if pending.is_none() {
            pending = Some(frame);
            continue;
        }
        if let Some(timing) = timeline.advance(frame.captured_at) {
            writer.write(pending.take().expect("pending frame"), timing)?;
            pending = Some(frame);
        }
    }
    worker.check_cancelled()?;
    let Some(frame) = pending else {
        return Err(
            "Portal recording received no frames. Try again when the portal is visible.".into(),
        );
    };
    let stopped = worker
        .stop
        .try_recv()
        .unwrap_or_else(|_| std::time::Instant::now());
    writer.write(frame, timeline.finish(stopped))?;
    unsafe { writer.sink.Finalize() }
        .map_err(|e| format!("Could not finalize video capture: {e}"))?;
    drop(writer);
    worker.check_cancelled()?;
    draft.commit()
}

struct Runtime;

impl Runtime {
    fn new() -> Result<Self, String> {
        // Match atlas-core::video's worker apartment/startup model, pairing
        // both calls here so encoder failures cannot leak runtime references.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|e| format!("Could not initialize video capture: {e}"))?;
            if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) {
                CoUninitialize();
                return Err(format!("Windows video capture is unavailable: {e}"));
            }
        }
        Ok(Self)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}

struct Writer {
    sink: IMFSinkWriter,
    stream: u32,
}

impl Writer {
    fn new(path: &Path, width: u32, height: u32, fps: u32) -> ::windows::core::Result<Self> {
        let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 2)?;
            let attributes = attributes.expect("MFCreateAttributes succeeded");
            attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
            let sink = MFCreateSinkWriterFromURL(PCWSTR(path.as_ptr()), None, &attributes)?;
            let output = media_type(&MFVideoFormat_H264, width, height, fps)?;
            // A moderate quality budget scales with resolution/cadence, capped
            // at 12 Mbps for dashboard text and moving 3D content at 1080p30.
            let bitrate = (u64::from(width) * u64::from(height) * u64::from(fps) / 5)
                .clamp(500_000, 12_000_000) as u32;
            output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
            let stream = sink.AddStream(&output)?;
            let input = media_type(&MFVideoFormat_RGB32, width, height, fps)?;
            input.SetUINT32(&MF_MT_DEFAULT_STRIDE, width * 4)?;
            input.SetUINT32(&MF_MT_FIXED_SIZE_SAMPLES, 1)?;
            input.SetUINT32(&MF_MT_SAMPLE_SIZE, width * height * 4)?;
            sink.SetInputMediaType(stream, &input, None)?;
            sink.BeginWriting()?;
            Ok(Self { sink, stream })
        }
    }

    fn write(&self, frame: CapturedFrame, timing: SampleTiming) -> Result<(), String> {
        unsafe { self.write_sample(frame, timing) }
            .map_err(|e| format!("Could not encode portal video frame: {e}"))
    }

    unsafe fn write_sample(
        &self,
        frame: CapturedFrame,
        timing: SampleTiming,
    ) -> ::windows::core::Result<()> {
        let len = frame.rgba.len() as u32;
        let buffer = unsafe { MFCreateMemoryBuffer(len)? };
        let mut data = std::ptr::null_mut();
        unsafe { buffer.Lock(&mut data, None, None)? };
        // The buffer is exact-sized and Lock succeeded. Its only writer is
        // this thread; no borrowed slice outlives the balanced Unlock below.
        let destination = unsafe { std::slice::from_raw_parts_mut(data, len as usize) };
        for (source, target) in frame
            .rgba
            .chunks_exact(4)
            .zip(destination.chunks_exact_mut(4))
        {
            target.copy_from_slice(&[source[2], source[1], source[0], 255]);
        }
        unsafe {
            buffer.Unlock()?;
            buffer.SetCurrentLength(len)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(timing.start)?;
            sample.SetSampleDuration(timing.duration)?;
            self.sink.WriteSample(self.stream, &sample)
        }
    }
}

unsafe fn media_type(
    subtype: &::windows::core::GUID,
    width: u32,
    height: u32,
    fps: u32,
) -> ::windows::core::Result<IMFMediaType> {
    unsafe {
        let media = MFCreateMediaType()?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        media.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        media.SetUINT64(
            &MF_MT_FRAME_SIZE,
            (u64::from(width) << 32) | u64::from(height),
        )?;
        media.SetUINT64(&MF_MT_FRAME_RATE, (u64::from(fps) << 32) | 1)?;
        media.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1_u64 << 32) | 1)?;
        Ok(media)
    }
}
