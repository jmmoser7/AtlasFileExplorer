use super::VideoFrame;

/// Box-filter a BGRA buffer (Media Foundation `RGB32`) into straight RGBA.
/// Negative `stride` is a bottom-up bitmap. Alpha is forced opaque.
pub fn bgra_scaled(src: &[u8], w: u32, h: u32, stride: i32, max_edge: u32) -> Option<VideoFrame> {
    let origin = if stride < 0 {
        (h.checked_sub(1)? as usize).checked_mul(stride.unsigned_abs() as usize)?
    } else {
        0
    };
    bgra_scaled_rows(src, w, h, stride, origin, max_edge)
}

/// Scale directly from a locked buffer's displayed scanline and actual pitch.
/// No full source-frame copy is retained in either process.
pub(super) fn bgra_scaled_rows(
    src: &[u8],
    w: u32,
    h: u32,
    stride: i32,
    origin: usize,
    max_edge: u32,
) -> Option<VideoFrame> {
    if w == 0 || h == 0 || max_edge == 0 {
        return None;
    }
    validate_rows(src.len(), origin, w, h, stride, true)?;
    let (dw, dh) = fit_edge(w, h, max_edge);
    let size = (dw as usize).checked_mul(dh as usize)?.checked_mul(4)?;
    if size > 128 * 1024 * 1024 {
        return None;
    }
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(size).ok()?;
    rgba.resize(size, 0);
    for y in 0..dh {
        let y0 = (u64::from(y) * u64::from(h) / u64::from(dh)) as u32;
        let y1 = ((u64::from(y + 1) * u64::from(h) / u64::from(dh)) as u32)
            .max(y0 + 1)
            .min(h);
        for x in 0..dw {
            let x0 = (u64::from(x) * u64::from(w) / u64::from(dw)) as u32;
            let x1 = ((u64::from(x + 1) * u64::from(w) / u64::from(dw)) as u32)
                .max(x0 + 1)
                .min(w);
            let mut r = 0u64;
            let mut g = 0u64;
            let mut b = 0u64;
            let mut n = 0u64;
            for sy in y0..y1 {
                let delta = sy as usize * stride.unsigned_abs() as usize;
                let row = if stride < 0 {
                    origin - delta
                } else {
                    origin + delta
                };
                for sx in x0..x1 {
                    let i = row + sx as usize * 4;
                    b += src[i] as u64;
                    g += src[i + 1] as u64;
                    r += src[i + 2] as u64;
                    n += 1;
                }
            }
            if n == 0 {
                continue;
            }
            let o = (y as usize * dw as usize + x as usize) * 4;
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

/// Prove every row lies inside one locked allocation before touching a pixel.
/// `scanline` is the first displayed row when `signed_origin` is true; otherwise
/// the slice begins at the allocation's lowest address (the public BGRA API).
pub(super) fn validate_rows(
    len: usize,
    scanline: usize,
    w: u32,
    h: u32,
    stride: i32,
    signed_origin: bool,
) -> Option<()> {
    if w == 0 || h == 0 || stride == 0 {
        return None;
    }
    let row = (w as usize).checked_mul(4)?;
    let pitch = stride.unsigned_abs() as usize;
    if pitch < row {
        return None;
    }
    let span = (h as usize - 1).checked_mul(pitch)?;
    let (first, last) = if signed_origin && stride < 0 {
        (scanline.checked_sub(span)?, scanline)
    } else {
        (scanline, scanline.checked_add(span)?)
    };
    if first > len || last.checked_add(row)? > len {
        return None;
    }
    Some(())
}

/// A declared linear pitch may have extra coded rows after the visible image.
/// Without a declared pitch, only an exactly packed buffer is unambiguous.
#[cfg(any(windows, test))]
pub(super) fn linear_stride(len: usize, w: u32, h: u32, declared: Option<i32>) -> Option<i32> {
    let row = (w as usize).checked_mul(4)?;
    let declared = declared.filter(|pitch| *pitch != 0);
    let stride = match declared {
        Some(pitch) => pitch,
        None if row.checked_mul(h as usize)? == len => i32::try_from(row).ok()?,
        None => return None,
    };
    validate_rows(len, 0, w, h, stride, false)?;
    Some(stride)
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FrameArea {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}
#[cfg(any(windows, test))]
impl FrameArea {
    pub fn checked(self, coded_w: u32, coded_h: u32) -> Option<Self> {
        if self.w == 0
            || self.h == 0
            || self.x.checked_add(self.w)? > coded_w
            || self.y.checked_add(self.h)? > coded_h
        {
            return None;
        }
        Some(self)
    }
}
/// Crop the valid display aperture while preserving the coded buffer's pitch.
#[cfg(any(windows, test))]
pub(super) fn scaled_region(
    src: &[u8],
    coded: (u32, u32),
    stride: i32,
    origin: usize,
    area: FrameArea,
    edge: u32,
) -> Option<VideoFrame> {
    area.checked(coded.0, coded.1)?;
    validate_rows(src.len(), origin, coded.0, coded.1, stride, true)?;
    let row_delta = (area.y as usize).checked_mul(stride.unsigned_abs() as usize)?;
    let row = if stride < 0 {
        origin.checked_sub(row_delta)?
    } else {
        origin.checked_add(row_delta)?
    };
    let crop_origin = row.checked_add((area.x as usize).checked_mul(4)?)?;
    bgra_scaled_rows(src, area.w, area.h, stride, crop_origin, edge)
}
