use super::{
    bgra_scaled, playback_window, sample_times, Job, VideoEvent, VideoFrame, PLAY_EDGE, STRIP_EDGE,
    STRIP_FRAMES,
};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::core::{Interface, PCWSTR};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaType, IMFSample, IMFSourceReader, MFCreateAttributes, MFCreateMediaType,
    MFCreateSourceReaderFromURL, MFMediaType_Video, MFVideoFormat_RGB32, MF_MT_DEFAULT_STRIDE,
    MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_PD_DURATION,
    MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    MF_SOURCE_READER_MEDIASOURCE,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Variant::{VT_I8, VT_UI8};

#[cfg(test)]
mod tests;

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
                let scaled = bgra_scaled(&frame.bgra, frame.w, frame.h, frame.stride, PLAY_EDGE);
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
    *slot = Open::from_path(path).ok();
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
    fn from_path(path: &Path) -> Result<Self, ()> {
        let wide = wide_path(path);
        let reader = unsafe {
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 1).map_err(|_| ())?;
            let attributes = attributes.ok_or(())?;
            // H.264 and other YUV decoders need the source reader's color
            // converter before they can supply our RGB32 frame format.
            attributes
                .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
                .map_err(|_| ())?;
            MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), &attributes)
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
    let var = unsafe { reader.GetPresentationAttribute(index, &MF_PD_DURATION) }.map_err(|_| ())?;
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

fn copy_sample(sample: &IMFSample, reader: &IMFSourceReader, stream: u32) -> Result<RawFrame, ()> {
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
