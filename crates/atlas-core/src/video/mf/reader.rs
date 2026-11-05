use super::diagnostics::{self, at};
use crate::video::{timebase::Timeline, STRIP_EDGE};
use std::path::Path;
use windows::core::{Interface, PCWSTR};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFByteStream, IMFMediaType, IMFSourceReader, MFCreateAttributes, MFCreateFile,
    MFCreateMediaType, MFCreateSourceReaderFromByteStream, MFMediaType_Video, MFVideoFormat_RGB32,
    MF_ACCESSMODE_READ, MF_FILEFLAGS_NONE, MF_MT_FRAME_RATE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
    MF_OPENMODE_FAIL_IF_NOT_EXIST, MF_PD_DURATION, MF_PD_PLAYBACK_BOUNDARY_TIME,
    MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_SOURCE_READER_ALL_STREAMS,
    MF_SOURCE_READER_CURRENT_TYPE_INDEX, MF_SOURCE_READER_DISABLE_DXVA,
    MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    MF_SOURCE_READER_MEDIASOURCE,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Variant::{VT_I8, VT_UI8};

pub struct Open {
    pub(super) path: std::path::PathBuf,
    pub(super) reader: IMFSourceReader,
    _stream: IMFByteStream,
    pub(super) duration: f32,
    pub(super) stream: u32,
    timeline: std::cell::Cell<Option<Timeline>>,
}

impl Open {
    pub(super) fn frame_rate(&self) -> Option<f32> {
        let current = unsafe { self.reader.GetCurrentMediaType(self.stream) }
            .ok()
            .and_then(|media| unsafe { media.GetUINT64(&MF_MT_FRAME_RATE) }.ok());
        let packed = current.or_else(|| {
            unsafe {
                self.reader
                    .GetNativeMediaType(self.stream, MF_SOURCE_READER_CURRENT_TYPE_INDEX.0 as u32)
            }
            .ok()
            .and_then(|media| unsafe { media.GetUINT64(&MF_MT_FRAME_RATE) }.ok())
        })?;
        crate::video::sampling::frame_rate(packed)
    }
    pub(super) fn clip_time(&self, native_hns: i64) -> f32 {
        let timeline = self.timeline.get().unwrap_or_else(|| {
            let timeline = Timeline::from_first_frame(native_hns.max(0));
            if native_hns >= 0 {
                self.timeline.set(Some(timeline));
            }
            timeline
        });
        timeline.clip_time(native_hns)
    }
    pub(super) fn from_path(path: &Path) -> Result<Self, ()> {
        let wide = super::path::reader_path(path)?;
        let (byte_stream, reader) = unsafe {
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 3).map_err(at("attributes.create"))?;
            let attributes = attributes.ok_or(())?;
            // H.264 and other YUV decoders need the source reader's color
            // converter before they can supply our RGB32 frame format.
            attributes
                .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
                .map_err(at("attributes.video-processing"))?;
            attributes
                .SetUINT32(&MF_SOURCE_READER_DISABLE_DXVA, 1)
                .map_err(at("attributes.disable-dxva"))?;
            attributes
                .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 0)
                .map_err(at("attributes.hardware-transforms"))?;
            // Open the exact filesystem namespace instead of asking a URL
            // scheme handler to interpret verbatim prefixes or percent signs.
            let stream = MFCreateFile(
                MF_ACCESSMODE_READ,
                MF_OPENMODE_FAIL_IF_NOT_EXIST,
                MF_FILEFLAGS_NONE,
                PCWSTR(wide.as_ptr()),
            )
            .map_err(at("reader.file-open"))?;
            let reader = MFCreateSourceReaderFromByteStream(&stream, &attributes)
                .map_err(at("reader.byte-stream"))?;
            (stream, reader)
        };
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        unsafe {
            reader
                .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
                .map_err(at("reader.deselect-streams"))?;
            reader
                .SetStreamSelection(stream, true)
                .map_err(at("reader.select-video"))?;
        }
        configure_rgb32(&reader, stream)?;
        let duration = duration_secs(&reader)?;
        let open = Self {
            path: path.to_path_buf(),
            reader,
            _stream: byte_stream,
            duration,
            stream,
            timeline: std::cell::Cell::new(None),
        };
        // Establish one video-only origin before any user trim/resume seek.
        // A bounded small-frame calibration never retains a full source copy.
        seek(&open, 0.0)?;
        for _ in 0..120 {
            match super::next_sample(&open, STRIP_EDGE)? {
                super::Sample::Frame { .. } if open.timeline.get().is_some() => break,
                super::Sample::Eos => return Err(()),
                _ => {}
            }
        }
        let timeline = open.timeline.get().ok_or(())?;
        let boundary = unsafe {
            open.reader.GetPresentationAttribute(
                MF_SOURCE_READER_MEDIASOURCE.0 as u32,
                &MF_PD_PLAYBACK_BOUNDARY_TIME,
            )
        }
        .ok()
        .and_then(|value| prop_u64(&value));
        // PLAYBACK_BOUNDARY_TIME describes presentation/playlist boundaries;
        // the chosen clip policy instead starts at the first actual video frame.
        diagnostics::timeline(duration, timeline.origin_hns(), boundary);
        Ok(open)
    }
}

pub(super) fn configure_rgb32(reader: &IMFSourceReader, stream: u32) -> Result<(), ()> {
    unsafe {
        let media_type: IMFMediaType = MFCreateMediaType().map_err(at("media.create"))?;
        let attrs: IMFAttributes = media_type.cast().map_err(at("media.cast"))?;
        attrs
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(at("media.major-type"))?;
        attrs
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
            .map_err(at("media.rgb32-subtype"))?;
        reader
            .SetCurrentMediaType(stream, None, &media_type)
            .map_err(at("reader.set-rgb32"))?;
    }
    Ok(())
}

fn duration_secs(reader: &IMFSourceReader) -> Result<f32, ()> {
    let index = MF_SOURCE_READER_MEDIASOURCE.0 as u32;
    let var = unsafe { reader.GetPresentationAttribute(index, &MF_PD_DURATION) }
        .map_err(at("reader.duration"))?;
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

pub(super) fn seek(open: &Open, secs: f32) -> Result<(), ()> {
    let hns = open
        .timeline
        .get()
        .unwrap_or(Timeline::from_first_frame(0))
        .seek_position(secs);
    let mut var = PROPVARIANT::default();
    unsafe {
        (*var.Anonymous.Anonymous).vt = VT_I8;
        (*var.Anonymous.Anonymous).Anonymous.hVal = hns;
        open.reader
            .SetCurrentPosition(&windows::core::GUID::zeroed(), &var)
            .map_err(at("reader.seek"))?;
    }
    Ok(())
}
