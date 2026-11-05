/// Opt-in, bounded diagnostic receipt. A helper emits at most one failure
/// receipt per request; paths and decoded bytes never enter diagnostics.
static EMITTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(super) fn at(stage: &'static str) -> impl FnOnce(windows::core::Error) {
    move |error| {
        if enabled() && !EMITTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!(
                "atlas-video stage={stage} hresult=0x{:08X}",
                error.code().0 as u32
            );
        }
    }
}
pub(super) fn layout(stage: &'static str, w: u32, h: u32, pitch: i32, bytes: usize) {
    if enabled() && !EMITTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("atlas-video stage={stage} width={w} height={h} pitch={pitch} bytes={bytes}");
    }
}
fn enabled() -> bool {
    std::env::var_os("ATLAS_VIDEO_DIAGNOSTICS").is_some_and(|value| value == "1")
}

static FIRST_FRAME: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(super) fn first_frame(time: f32) {
    if enabled() && !FIRST_FRAME.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("atlas-video stage=play.first-emitted time={time:.6}");
    }
}

static FIRST_DECODED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(super) fn first_decoded(time: f32) {
    if enabled() && !FIRST_DECODED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("atlas-video stage=play.first-decoded time={time:.6}");
    }
}

static FIRST_SAMPLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(super) fn sample_times(
    reader_hns: i64,
    sample: &windows::Win32::Media::MediaFoundation::IMFSample,
) {
    if enabled() && !FIRST_SAMPLE.swap(true, std::sync::atomic::Ordering::Relaxed) {
        let sample_hns = unsafe { sample.GetSampleTime() }.map_err(|error| error.code().0);
        eprintln!("atlas-video stage=sample.first-timestamps reader_hns={reader_hns} sample_hns={sample_hns:?}");
    }
}

static FORMAT_CHANGES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub(super) fn format_change(
    flags: u32,
    reader: &windows::Win32::Media::MediaFoundation::IMFSourceReader,
    stream: u32,
) {
    use windows::Win32::Media::MediaFoundation::{
        MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_GEOMETRIC_APERTURE,
        MF_MT_MINIMUM_DISPLAY_APERTURE, MF_MT_PAN_SCAN_APERTURE,
        MF_SOURCE_READER_CURRENT_TYPE_INDEX,
    };
    if !enabled() || FORMAT_CHANGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 8 {
        return;
    }
    let current = unsafe { reader.GetCurrentMediaType(stream) }.and_then(|media| unsafe {
        Ok((
            media.GetUINT64(&MF_MT_FRAME_SIZE)?,
            media
                .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                .ok()
                .map(|pitch| pitch as i32),
            [
                MF_MT_PAN_SCAN_APERTURE,
                MF_MT_MINIMUM_DISPLAY_APERTURE,
                MF_MT_GEOMETRIC_APERTURE,
            ]
            .map(|key| super::aperture::attribute(&media, &key)),
        ))
    });
    let native =
        unsafe { reader.GetNativeMediaType(stream, MF_SOURCE_READER_CURRENT_TYPE_INDEX.0 as u32) }
            .and_then(|media| unsafe { media.GetUINT64(&MF_MT_FRAME_SIZE) });
    match current {
        Ok((size, pitch, apertures)) => eprintln!("atlas-video stage=reader.format-change flags=0x{flags:X} width={} height={} pitch={pitch:?} native_size={native:?} apertures_pan_min_geom={apertures:?}", size >> 32, size as u32),
        Err(error) => eprintln!("atlas-video stage=reader.format-change flags=0x{flags:X} hresult=0x{:08X}", error.code().0 as u32),
    }
}

pub(super) fn timeline(duration: f32, origin_hns: i64, boundary_hns: Option<u64>) {
    if enabled() {
        eprintln!("atlas-video stage=reader.timeline duration={duration:.6} origin_hns={origin_hns} boundary_hns={boundary_hns:?}");
    }
}

pub(super) fn strip_samples(targets: usize, seeks: usize, reads: usize, frames: usize) {
    if enabled() {
        eprintln!("atlas-video stage=strip.samples targets={targets} seeks={seeks} reads={reads} frames={frames}");
    }
}
