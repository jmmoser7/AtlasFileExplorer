use super::diagnostics::{at, layout};
use crate::video::frame::{linear_stride, scaled_region, validate_rows, FrameArea};
use crate::video::VideoFrame;
use windows::core::Interface;
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer2, IMFMediaBuffer, IMFSample, IMFSourceReader, MF2DBuffer_LockFlags_Read,
    MFVideoFormat_RGB32, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_SUBTYPE,
};

const MAX_FRAME_BYTES: usize = 128 * 1024 * 1024;

pub(super) fn copy_sample(
    sample: &IMFSample,
    reader: &IMFSourceReader,
    stream: u32,
    max_edge: u32,
) -> Result<VideoFrame, ()> {
    let media = unsafe { reader.GetCurrentMediaType(stream) }.map_err(at("sample.media-type"))?;
    if unsafe { media.GetGUID(&MF_MT_SUBTYPE) }.map_err(at("sample.subtype"))?
        != MFVideoFormat_RGB32
    {
        return Err(());
    }
    let packed = unsafe { media.GetUINT64(&MF_MT_FRAME_SIZE) }.map_err(at("sample.dimensions"))?;
    let (w, h) = ((packed >> 32) as u32, packed as u32);
    let row = w
        .checked_mul(4)
        .filter(|v| *v <= i32::MAX as u32)
        .ok_or(())?;
    let _size = (row as usize)
        .checked_mul(h as usize)
        .filter(|s| *s > 0 && *s <= MAX_FRAME_BYTES)
        .ok_or(())?;
    let area = super::aperture::display_area(&media, reader, stream, (w, h))?;
    // Query the actual sample buffer BEFORE flattening it: flattening can change
    // its pitch and discard the only authoritative per-buffer layout metadata.
    if unsafe { sample.GetBufferCount() }.map_err(at("sample.buffer-count"))? == 1 {
        let buffer = unsafe { sample.GetBufferByIndex(0) }.map_err(at("sample.buffer"))?;
        if let Ok(two_d) = buffer.cast::<IMF2DBuffer2>() {
            return copy_2d(&two_d, w, h, area, max_edge);
        }
    }
    let buffer =
        unsafe { sample.ConvertToContiguousBuffer() }.map_err(at("sample.contiguous-buffer"))?;
    let stride = unsafe { media.GetUINT32(&MF_MT_DEFAULT_STRIDE) }
        .ok()
        .map(|v| v as i32);
    copy_linear(&buffer, w, h, stride, area, max_edge)
}

fn copy_2d(
    buffer: &IMF2DBuffer2,
    w: u32,
    h: u32,
    area: FrameArea,
    max_edge: u32,
) -> Result<VideoFrame, ()> {
    let (mut scanline, mut base) = (std::ptr::null_mut(), std::ptr::null_mut());
    let (mut pitch, mut len) = (0i32, 0u32);
    unsafe {
        buffer.Lock2DSize(
            MF2DBuffer_LockFlags_Read,
            &mut scanline,
            &mut pitch,
            &mut base,
            &mut len,
        )
    }
    .map_err(at("sample.lock-2d"))?;
    // Unlock even when validation/copy rejects the sample or allocation panics.
    struct Unlock<'a>(&'a IMF2DBuffer2);
    impl Drop for Unlock<'_> {
        fn drop(&mut self) {
            unsafe {
                let _ = self.0.Unlock2D();
            }
        }
    }
    let _unlock = Unlock(buffer);
    let offset = (scanline as usize).checked_sub(base as usize).ok_or(())?;
    if base.is_null() || scanline.is_null() || len as usize > MAX_FRAME_BYTES {
        return Err(());
    }
    validate_rows(len as usize, offset, w, h, pitch, true)
        .ok_or_else(|| layout("sample.2d-bounds", w, h, pitch, len as usize))?;
    let src = unsafe { std::slice::from_raw_parts(base, len as usize) };
    scaled_region(src, (w, h), pitch, offset, area, max_edge).ok_or(())
}

fn copy_linear(
    buffer: &IMFMediaBuffer,
    w: u32,
    h: u32,
    stride: Option<i32>,
    area: FrameArea,
    max_edge: u32,
) -> Result<VideoFrame, ()> {
    let mut ptr = std::ptr::null_mut();
    let (mut max_len, mut len) = (0u32, 0u32);
    unsafe { buffer.Lock(&mut ptr, Some(&mut max_len), Some(&mut len)) }
        .map_err(at("sample.lock-linear"))?;
    struct Unlock<'a>(&'a IMFMediaBuffer);
    impl Drop for Unlock<'_> {
        fn drop(&mut self) {
            unsafe {
                let _ = self.0.Unlock();
            }
        }
    }
    let _unlock = Unlock(buffer);
    if ptr.is_null() || len > max_len || len as usize > MAX_FRAME_BYTES {
        return Err(());
    }
    // The current type's pitch describes rows, not the full allocation.
    // Codecs may retain unused coded rows after the visible frame. Only the
    // checked visible row span must fit; never derive pitch from len / height.
    let stride = linear_stride(len as usize, w, h, stride).ok_or_else(|| {
        layout(
            "sample.linear-bounds",
            w,
            h,
            stride.unwrap_or(0),
            len as usize,
        )
    })?;
    let src = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let origin = if stride < 0 {
        (h as usize - 1) * stride.unsigned_abs() as usize
    } else {
        0
    };
    scaled_region(src, (w, h), stride, origin, area, max_edge).ok_or(())
}
