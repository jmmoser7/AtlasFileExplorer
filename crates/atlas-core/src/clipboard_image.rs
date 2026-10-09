//! Bitmap pictures on the system clipboard, read as PNG bytes. Slate's board
//! paste and the shared Suggestion box both read through here.

/// Clipboard bitmaps larger than this are refused. A paste is one user
/// action, not a frame, but it still must not allocate a runaway buffer.
pub const MAX_PASTE_PIXELS: u64 = 64 * 1024 * 1024;
pub const MAX_PASTE_PNG_BYTES: usize = 80 * 1024 * 1024;

/// A picture as the clipboard holds it, read while the clipboard is open.
/// Decode with [`ClipImage::into_png`] after closing it.
pub enum ClipImage {
    Png(Vec<u8>),
    Bitmap(Vec<u8>),
}

impl ClipImage {
    pub fn into_png(self) -> Option<Vec<u8>> {
        match self {
            Self::Png(bytes) => Some(bytes),
            Self::Bitmap(bytes) => decode_clipboard_bitmap(&bytes),
        }
    }
}

/// The clipboard's picture as PNG bytes: a PNG format first, then a DIB.
/// `None` when it holds no picture or another program keeps it open.
#[cfg(windows)]
pub fn read_png() -> Option<Vec<u8>> {
    let image = {
        let _clip = clipboard_win::Clipboard::new_attempts(5).ok()?;
        read_open()
    };
    image?.into_png()
}

#[cfg(not(windows))]
pub fn read_png() -> Option<Vec<u8>> {
    None
}

/// The picture on a clipboard the caller has already opened.
#[cfg(windows)]
pub fn read_open() -> Option<ClipImage> {
    use clipboard_win::formats::{Bitmap, RawData, CF_DIB, CF_DIBV5};
    use clipboard_win::{Format, Getter};
    for name in ["PNG", "image/png"] {
        let Some(fmt) = clipboard_win::register_format(name) else {
            continue;
        };
        let format = RawData(fmt.get());
        if !format.is_format_avail() {
            continue;
        }
        let mut bytes = Vec::new();
        if format.read_clipboard(&mut bytes).is_ok() && png_within_limits(&bytes) {
            return Some(ClipImage::Png(bytes));
        }
    }
    for format in [CF_DIBV5, CF_DIB] {
        let format = RawData(format);
        if !format.is_format_avail() {
            continue;
        }
        let mut bytes = Vec::new();
        if format.read_clipboard(&mut bytes).is_ok() && !bytes.is_empty() {
            return Some(ClipImage::Bitmap(bytes));
        }
    }
    if Bitmap.is_format_avail() {
        let mut bytes = Vec::new();
        if Bitmap.read_clipboard(&mut bytes).is_ok() && !bytes.is_empty() {
            return Some(ClipImage::Bitmap(bytes));
        }
    }
    None
}
/// A PNG small enough to paste: a real signature, readable dimensions, and
/// no more than [`MAX_PASTE_PIXELS`].
pub fn png_within_limits(bytes: &[u8]) -> bool {
    if bytes.len() < 24 || bytes.len() > MAX_PASTE_PNG_BYTES || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
    {
        return false;
    }
    let Ok(reader) = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()
    else {
        return false;
    };
    let Ok((w, h)) = reader.into_dimensions() else {
        return false;
    };
    w > 0 && h > 0 && u64::from(w) * u64::from(h) <= MAX_PASTE_PIXELS
}

/// Turn a clipboard DIB, DIBV5, or BMP file into a PNG. 24- and 32-bit
/// uncompressed bitmaps only — that is what screenshots and "Copy image"
/// actually put on the clipboard.
pub fn decode_clipboard_bitmap(data: &[u8]) -> Option<Vec<u8>> {
    const BI_RGB: u32 = 0;
    const BI_BITFIELDS: u32 = 3;
    let (dib, file_pixel_off) = if data.starts_with(b"BM") && data.len() >= 14 {
        let off = u32::from_le_bytes(data[10..14].try_into().ok()?) as usize;
        if off < 14 || off > data.len() {
            return None;
        }
        (&data[14..], Some(off - 14))
    } else {
        (data, None)
    };
    if dib.len() < 40 {
        return None;
    }
    let bi_size = u32::from_le_bytes(dib[0..4].try_into().ok()?) as usize;
    if bi_size < 40 || bi_size > dib.len() {
        return None;
    }
    let width = i32::from_le_bytes(dib[4..8].try_into().ok()?);
    let height_raw = i32::from_le_bytes(dib[8..12].try_into().ok()?);
    if width <= 0 || height_raw == 0 || height_raw == i32::MIN {
        return None;
    }
    let width = width as u32;
    let top_down = height_raw < 0;
    let height = height_raw.unsigned_abs();
    let count = u64::from(width).checked_mul(u64::from(height))?;
    if count == 0 || count > MAX_PASTE_PIXELS {
        return None;
    }
    let bit_count = u16::from_le_bytes(dib[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);
    if bit_count != 24 && bit_count != 32 {
        return None;
    }
    if compression != BI_RGB && compression != BI_BITFIELDS {
        return None;
    }
    let masks = if compression == BI_BITFIELDS {
        if dib.len() < 52 {
            return None;
        }
        let r = u32::from_le_bytes(dib[40..44].try_into().ok()?);
        let g = u32::from_le_bytes(dib[44..48].try_into().ok()?);
        let b = u32::from_le_bytes(dib[48..52].try_into().ok()?);
        let a = if bi_size >= 56 && dib.len() >= 56 {
            u32::from_le_bytes(dib[52..56].try_into().ok()?)
        } else {
            0
        };
        if r == 0 || g == 0 || b == 0 {
            return None;
        }
        Some([r, g, b, a])
    } else {
        None
    };
    let bpp = (bit_count / 8) as usize;
    let stride = (width as usize * bpp + 3) & !3;
    let pixels_off = if let Some(off) = file_pixel_off {
        off
    } else if compression == BI_BITFIELDS && bi_size == 40 {
        let rows = height as usize;
        if dib_fits(dib, 56, rows, stride) && !dib_fits(dib, 52, rows, stride) {
            56
        } else {
            52
        }
    } else {
        bi_size
    };
    if !dib_fits(dib, pixels_off, height as usize, stride) {
        return None;
    }
    let pixels = &dib[pixels_off..];
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    let mut any_alpha = false;
    for y in 0..height as usize {
        let src_y = if top_down { y } else { height as usize - 1 - y };
        let row = &pixels[src_y * stride..src_y * stride + width as usize * bpp];
        for x in 0..width as usize {
            let px = &row[x * bpp..x * bpp + bpp];
            let (r, g, b, a) = if let Some(masks) = masks {
                let mut dword = [0u8; 4];
                dword[..bpp].copy_from_slice(px);
                let v = u32::from_le_bytes(dword);
                let a = if masks[3] == 0 {
                    255
                } else {
                    mask_channel(v, masks[3])
                };
                (
                    mask_channel(v, masks[0]),
                    mask_channel(v, masks[1]),
                    mask_channel(v, masks[2]),
                    a,
                )
            } else if bpp == 4 {
                (px[2], px[1], px[0], px[3])
            } else {
                (px[2], px[1], px[0], 255)
            };
            any_alpha |= a != 0;
            let i = (y * width as usize + x) * 4;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = a;
        }
    }
    // 32-bit screenshots store unused alpha as zero. A PNG with real
    // transparency arrives as PNG bytes and never takes this path.
    if bpp == 4 && masks.is_none() && !any_alpha {
        for px in rgba.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
    }
    encode_png(width, height, &rgba)
}

fn dib_fits(data: &[u8], off: usize, rows: usize, stride: usize) -> bool {
    rows.checked_mul(stride)
        .and_then(|n| off.checked_add(n))
        .is_some_and(|end| end <= data.len())
}

fn mask_channel(pixel: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let bits = mask.count_ones();
    let value = (pixel & mask) >> shift;
    if bits >= 8 {
        (value >> (bits - 8)) as u8
    } else {
        let max = (1u32 << bits) - 1;
        ((value * 255) / max) as u8
    }
}

/// Straight RGBA8 to PNG bytes.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    use image::ImageEncoder;
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(bytes)
}
