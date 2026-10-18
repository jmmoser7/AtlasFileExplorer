use super::*;
use crate::app::board_portal::capture::encode::VideoEncoder;
use std::time::Duration;

#[test]
#[ignore = "Exercises the installed Windows H.264 encoder and decoder"]
fn portal_capture_mp4_native_smoke() {
    let destination = std::env::temp_dir().join(format!(
        "slate-capture-native-test-{}-{}.mp4",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // The test owns every byte it reads. No existing source/cloud file opens.
    let (mut encoder, worker) = VideoEncoder::channels(64, 64);
    let origin = encoder.started;
    for at in [0, 100, 2100] {
        let mut rgba = Vec::with_capacity(64 * 64 * 4);
        for y in 0..64 {
            for _ in 0..64 {
                rgba.extend_from_slice(if y < 32 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 0, 255, 255]
                });
            }
        }
        assert!(encoder.try_frame(CapturedFrame {
            generation: 1,
            w: 64,
            h: 64,
            rgba,
            captured_at: origin + Duration::from_millis(at),
            recycle: None,
        }));
    }
    encoder
        .stop
        .try_send(origin + Duration::from_millis(3100))
        .unwrap();
    encoder.frames.take();
    assert_eq!(
        encode(worker, destination.clone(), 64, 64, 30).unwrap(),
        destination
    );
    let bytes = std::fs::read(&destination).unwrap();
    assert!(
        bytes.windows(4).any(|b| b == b"avc1"),
        "H.264 sample entry missing"
    );
    let duration = movie_duration(&bytes).expect("valid MP4 movie duration");
    assert!(
        (duration - 3.1).abs() < 0.05,
        "recording duration was {duration}s instead of 3.1s"
    );
    let first = decode_first_frame(&destination);
    let top = (8 * first.w as usize + 32) * 4;
    let bottom = (56 * first.w as usize + 32) * 4;
    assert!(
        first.rgba[top] > 220 && first.rgba[top + 2] < 35,
        "top red pixels changed or flipped"
    );
    assert!(
        first.rgba[bottom + 2] > 220 && first.rgba[bottom] < 35,
        "bottom blue pixels changed or flipped"
    );
    std::fs::remove_file(destination).unwrap();
}

fn decode_first_frame(path: &Path) -> atlas_core::video::VideoFrame {
    let _runtime = Runtime::new().unwrap();
    let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let reader = MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), None).unwrap();
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let media = MFCreateMediaType().unwrap();
        media
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .unwrap();
        media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).unwrap();
        reader.SetCurrentMediaType(stream, None, &media).unwrap();
        for _ in 0..16 {
            let mut sample = None;
            reader
                .ReadSample(stream, 0, None, None, None, Some(&mut sample))
                .unwrap();
            let Some(sample) = sample else { continue };
            let media = reader.GetCurrentMediaType(stream).unwrap();
            let size = media.GetUINT64(&MF_MT_FRAME_SIZE).unwrap();
            let (w, h) = ((size >> 32) as u32, size as u32);
            let stride = media.GetUINT32(&MF_MT_DEFAULT_STRIDE).unwrap_or(w * 4) as i32;
            let buffer = sample.ConvertToContiguousBuffer().unwrap();
            let mut data = std::ptr::null_mut();
            let mut len = 0;
            buffer.Lock(&mut data, None, Some(&mut len)).unwrap();
            let decoded = atlas_core::video::bgra_scaled(
                std::slice::from_raw_parts(data, len as usize),
                w,
                h,
                stride,
                64,
            );
            buffer.Unlock().unwrap();
            return decoded.expect("valid decoded pixel buffer");
        }
    }
    panic!("MP4 decoder returned no video frames");
}

fn movie_duration(bytes: &[u8]) -> Option<f64> {
    let moov = atom(bytes, b"moov")?;
    let header = atom(moov, b"mvhd")?;
    let (timescale, duration) = match *header.first()? {
        0 => (
            u32::from_be_bytes(header.get(12..16)?.try_into().ok()?),
            u32::from_be_bytes(header.get(16..20)?.try_into().ok()?) as u64,
        ),
        1 => (
            u32::from_be_bytes(header.get(20..24)?.try_into().ok()?),
            u64::from_be_bytes(header.get(24..32)?.try_into().ok()?),
        ),
        _ => return None,
    };
    (timescale > 0).then(|| duration as f64 / timescale as f64)
}

fn atom<'a>(mut bytes: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    while bytes.len() >= 8 {
        let size = u32::from_be_bytes(bytes[..4].try_into().ok()?) as usize;
        let (size, start) = if size == 1 {
            (
                u64::from_be_bytes(bytes.get(8..16)?.try_into().ok()?) as usize,
                16,
            )
        } else if size == 0 {
            (bytes.len(), 8)
        } else {
            (size, 8)
        };
        if size < start || size > bytes.len() {
            return None;
        }
        if &bytes[4..8] == kind {
            return bytes.get(start..size);
        }
        bytes = &bytes[size..];
    }
    None
}
