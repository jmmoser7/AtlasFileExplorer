//! Fast raster thumbnails: embedded preview first, scaled decode second.
//!
//! The shell path (`IShellItemImageFactory`) and a plain `image::open` both pay
//! the same two costs for a photo: transfer every byte of the file, then decode
//! every pixel of it. At 6000x4000 that measured 189 ms per file — about five
//! thumbnails a second, which is fine for a folder of 40 and useless for a
//! folder of 20,000.
//!
//! Two observations make it cheap instead:
//!
//! 1. **Cameras already made the thumbnail.** A JPEG's EXIF block carries a
//!    ~160x120 preview within the first few tens of KB, so the whole job is a
//!    partial read plus a tiny decode. On a network share this is the dominant
//!    win: kilobytes instead of megabytes crossing the wire.
//! 2. **JPEG can be decoded at 1/2, 1/4 or 1/8 scale** straight from the DCT
//!    coefficients, so a thumbnail never needs the full pixel grid even when no
//!    preview exists.
//!
//! Neither path touches COM, so both are testable on Linux (Art. I).

use std::io::Read;
use std::path::Path;

/// How much of the file head to pull when hunting for an EXIF preview. The EXIF
/// APP1 segment is capped at 64 KB by the JPEG spec and sits near the start;
/// double that covers writers that emit a JFIF or ICC segment ahead of it.
pub const HEAD_BYTES: usize = 128 * 1024;

/// An embedded preview is only worth using if it is close to the size we want.
/// Below this fraction the upscale is visibly soft, so a scaled decode of the
/// real image earns its extra cost.
const MIN_PREVIEW_FRACTION: f32 = 0.6;

/// Extensions this module can thumbnail without the shell.
pub fn handles(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "jpe" | "jfif" | "png")
}

fn is_jpeg(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "jpe" | "jfif")
}

/// Thumbnail `path` at up to `want_px` on the long edge.
///
/// Returns RGBA. `None` means "not handled here" — the caller falls back to the
/// shell, which still owns the exotic formats.
pub fn thumbnail(path: &Path, want_px: u32) -> Option<(u32, u32, Vec<u8>)> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !handles(&ext) {
        return None;
    }

    if is_jpeg(&ext) {
        // One handle for both attempts: the preview read and the fallback share
        // it, so a miss costs one extra range request rather than a second open
        // plus a re-read of the head.
        let mut file = std::fs::File::open(path).ok()?;
        let mut head = read_upto(&mut file, HEAD_BYTES)?;
        // A camera records rotation as metadata and leaves the pixels alone, so
        // both paths below have to apply it or every portrait photo lands on its
        // side. The shell used to do this for us.
        let turn = exif_orientation(&head).unwrap_or(1);
        if let Some(preview) = exif_preview(&head) {
            if let Some(img) = decode_preview(preview, want_px, turn) {
                return Some(img);
            }
        }
        if head.len() == HEAD_BYTES {
            // Continue from where the head stopped rather than starting over.
            file.read_to_end(&mut head).ok()?;
        }
        return scaled_jpeg_oriented(&head, want_px, turn);
    }

    // PNG has neither an embedded preview nor a scaled decode, so the pixels
    // have to be decoded: 15 ms at 1080p, 347 ms at 48 MP. We still do it
    // ourselves, because the shell can only beat that when Explorer happens to
    // have the file cached already — on a miss it decodes the same pixels after
    // a COM round trip, and on a folder nobody has browsed a miss is the norm.
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?;
    Some(fit(img, want_px))
}

/// Read at most `limit` bytes from the current position.
fn read_upto(f: &mut std::fs::File, limit: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; limit];
    let mut read = 0;
    while read < limit {
        match f.read(&mut buf[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(_) => return None,
        }
    }
    buf.truncate(read);
    Some(buf)
}

/// Decode an embedded preview and accept it only if it is close enough to the
/// requested size to look right.
fn decode_preview(jpeg: &[u8], want_px: u32, turn: u16) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).ok()?;
    let long = img.width().max(img.height());
    if (long as f32) < want_px as f32 * MIN_PREVIEW_FRACTION {
        return None;
    }
    Some(fit(orient(img, turn), want_px))
}

/// Apply an EXIF orientation. Rotation happens before the downscale, on the
/// smallest image we will ever hold, so the cost is negligible.
fn orient(img: image::DynamicImage, turn: u16) -> image::DynamicImage {
    use image::DynamicImage as D;
    match turn {
        2 => D::ImageRgba8(image::imageops::flip_horizontal(&img)),
        3 => D::ImageRgba8(image::imageops::rotate180(&img)),
        4 => D::ImageRgba8(image::imageops::flip_vertical(&img)),
        5 => D::ImageRgba8(image::imageops::rotate90(
            &image::imageops::flip_horizontal(&img),
        )),
        6 => D::ImageRgba8(image::imageops::rotate90(&img)),
        7 => D::ImageRgba8(image::imageops::rotate270(
            &image::imageops::flip_horizontal(&img),
        )),
        8 => D::ImageRgba8(image::imageops::rotate270(&img)),
        // 1 is upright; anything else is a writer we do not trust to guess for.
        _ => img,
    }
}

/// The EXIF orientation tag (0x0112, IFD0). `None` when there is no EXIF.
pub fn exif_orientation(head: &[u8]) -> Option<u16> {
    let app1 = find_app1(head)?;
    let tiff = app1.strip_prefix(b"Exif\0\0")?;
    let le = match tiff.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    if read_u16(tiff, 2, le)? != 42 {
        return None;
    }
    let ifd0 = read_u32(tiff, 4, le)? as usize;
    ifd_entries(tiff, ifd0, le)?
        .into_iter()
        .find(|(tag, _)| *tag == 0x0112)
        .map(|(_, value)| value as u16)
        .filter(|v| (1..=8).contains(v))
}

/// Decode a JPEG at the coarsest DCT scale that still covers `want_px`, then
/// resize the remainder of the way.
///
/// Both dimensions have to be requested: `jpeg-decoder` reads them as a bounding
/// box and treats a zero as "as small as possible", which silently yields a 1/8
/// decode well under the target.
pub fn scaled_jpeg(bytes: &[u8], want_px: u32) -> Option<(u32, u32, Vec<u8>)> {
    let turn = exif_orientation(bytes).unwrap_or(1);
    scaled_jpeg_oriented(bytes, want_px, turn)
}

fn scaled_jpeg_oriented(bytes: &[u8], want_px: u32, turn: u16) -> Option<(u32, u32, Vec<u8>)> {
    let mut dec = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    dec.read_info().ok()?;
    let info = dec.info()?;
    let (fw, fh) = (info.width as u32, info.height as u32);
    let long = fw.max(fh).max(1);
    // Never upsample: a small image is already its own thumbnail.
    if long <= want_px {
        let pixels = dec.decode().ok()?;
        let img = to_dynamic(&pixels, fw, fh, dec.info()?.pixel_format)?;
        return Some(fit(orient(img, turn), want_px));
    }
    // Ask for the aspect-correct target and let the decoder round *up* to a
    // scale it supports; `fit` then finishes the last fractional step.
    let req_w = (fw * want_px).div_ceil(long).max(1) as u16;
    let req_h = (fh * want_px).div_ceil(long).max(1) as u16;
    let (sw, sh) = dec.scale(req_w, req_h).ok()?;
    let pixels = dec.decode().ok()?;
    let img = to_dynamic(&pixels, sw as u32, sh as u32, dec.info()?.pixel_format)?;
    Some(fit(orient(img, turn), want_px))
}

fn to_dynamic(
    pixels: &[u8],
    w: u32,
    h: u32,
    format: jpeg_decoder::PixelFormat,
) -> Option<image::DynamicImage> {
    use jpeg_decoder::PixelFormat;
    match format {
        PixelFormat::RGB24 => {
            image::RgbImage::from_raw(w, h, pixels.to_vec()).map(image::DynamicImage::ImageRgb8)
        }
        PixelFormat::L8 => {
            image::GrayImage::from_raw(w, h, pixels.to_vec()).map(image::DynamicImage::ImageLuma8)
        }
        PixelFormat::L16 => {
            // Native-endian pairs out of the decoder.
            let gray: Vec<u16> = pixels
                .chunks_exact(2)
                .map(|c| u16::from_ne_bytes([c[0], c[1]]))
                .collect();
            image::ImageBuffer::<image::Luma<u16>, _>::from_raw(w, h, gray)
                .map(image::DynamicImage::ImageLuma16)
        }
        PixelFormat::CMYK32 => {
            // Rare (Adobe-flavoured) and inverted; convert rather than fail so
            // these files still get a card instead of a placeholder.
            let rgb: Vec<u8> = pixels
                .chunks_exact(4)
                .flat_map(|c| {
                    let (c0, m, y, k) = (c[0] as u32, c[1] as u32, c[2] as u32, c[3] as u32);
                    [
                        (c0 * k / 255) as u8,
                        (m * k / 255) as u8,
                        (y * k / 255) as u8,
                    ]
                })
                .collect();
            image::RgbImage::from_raw(w, h, rgb).map(image::DynamicImage::ImageRgb8)
        }
    }
}

/// Downscale to fit `want_px` on the long edge and hand back RGBA.
fn fit(img: image::DynamicImage, want_px: u32) -> (u32, u32, Vec<u8>) {
    let long = img.width().max(img.height());
    let img = if long > want_px {
        img.thumbnail(want_px, want_px)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    (w, h, rgba.into_raw())
}

/// Locate the JPEG preview inside an EXIF APP1 segment.
///
/// Walks the real IFD chain (IFD0 → IFD1, tags 0x0201/0x0202) rather than
/// scanning for a nested `FFD8`, because a scan also matches compressed pixel
/// data and would hand back garbage that happens to start with a marker.
pub fn exif_preview(head: &[u8]) -> Option<&[u8]> {
    let app1 = find_app1(head)?;
    let tiff = app1.strip_prefix(b"Exif\0\0")?;
    let le = match tiff.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    if read_u16(tiff, 2, le)? != 42 {
        return None;
    }
    let ifd0 = read_u32(tiff, 4, le)? as usize;
    // The offset immediately after IFD0's entries points at IFD1, which is
    // where the thumbnail lives.
    let ifd1 = next_ifd_offset(tiff, ifd0, le)?;
    if ifd1 == 0 {
        return None;
    }
    let (mut offset, mut length) = (0usize, 0usize);
    for (tag, value) in ifd_entries(tiff, ifd1, le)? {
        match tag {
            0x0201 => offset = value as usize,
            0x0202 => length = value as usize,
            _ => {}
        }
    }
    if offset == 0 || length == 0 {
        return None;
    }
    let bytes = tiff.get(offset..offset.checked_add(length)?)?;
    // Only hand back something that actually is a JPEG.
    bytes.starts_with(&[0xFF, 0xD8]).then_some(bytes)
}

/// The payload of the first APP1 segment (EXIF), skipping any segments a writer
/// placed ahead of it.
fn find_app1(buf: &[u8]) -> Option<&[u8]> {
    if !buf.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    loop {
        // Segments may be preceded by fill bytes.
        while buf.get(i) == Some(&0xFF) && buf.get(i + 1) == Some(&0xFF) {
            i += 1;
        }
        if *buf.get(i)? != 0xFF {
            return None;
        }
        let marker = *buf.get(i + 1)?;
        // Start of scan: pixel data begins, no metadata past here.
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = read_u16(buf, i + 2, false)? as usize;
        if len < 2 {
            return None;
        }
        let payload = buf.get(i + 4..i + 2 + len)?;
        if marker == 0xE1 {
            return Some(payload);
        }
        i += 2 + len;
    }
}

/// `(tag, value)` for each entry of the IFD at `at`, values read as u32.
fn ifd_entries(tiff: &[u8], at: usize, le: bool) -> Option<Vec<(u16, u32)>> {
    let count = read_u16(tiff, at, le)? as usize;
    let mut out = Vec::with_capacity(count.min(64));
    for n in 0..count {
        let e = at + 2 + n * 12;
        let tag = read_u16(tiff, e, le)?;
        let kind = read_u16(tiff, e + 2, le)?;
        // Thumbnail offset/length are LONG or SHORT, always a single value.
        let value = match kind {
            3 => read_u16(tiff, e + 8, le)? as u32,
            4 => read_u32(tiff, e + 8, le)?,
            _ => continue,
        };
        out.push((tag, value));
    }
    Some(out)
}

fn next_ifd_offset(tiff: &[u8], at: usize, le: bool) -> Option<usize> {
    let count = read_u16(tiff, at, le)? as usize;
    read_u32(tiff, at + 2 + count * 12, le).map(|v| v as usize)
}

fn read_u16(buf: &[u8], at: usize, le: bool) -> Option<u16> {
    let b = buf.get(at..at + 2)?;
    Some(if le {
        u16::from_le_bytes([b[0], b[1]])
    } else {
        u16::from_be_bytes([b[0], b[1]])
    })
}

fn read_u32(buf: &[u8], at: usize, le: bool) -> Option<u32> {
    let b = buf.get(at..at + 4)?;
    Some(if le {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    } else {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    })
}

/// Builders for EXIF-bearing fixtures. `image` cannot write an embedded
/// preview, so the fast path would otherwise be untestable.
#[doc(hidden)]
pub mod test_support {
    /// Wrap `full` in an EXIF APP1 segment carrying `preview` in IFD1, the way
    /// a camera writes it.
    pub fn wrap_exif(full: &[u8], preview: &[u8]) -> Vec<u8> {
        wrap_exif_oriented(full, preview, 1)
    }

    /// As [`wrap_exif`], with an IFD0 orientation tag of `turn`.
    pub fn wrap_exif_oriented(full: &[u8], preview: &[u8], turn: u16) -> Vec<u8> {
        // TIFF: header(8) + IFD0(2 + 1*12 + 4) + IFD1(2 + 2*12 + 4) + preview.
        let ifd0_at = 8usize;
        let ifd1_at = ifd0_at + 2 + 12 + 4;
        let preview_at = ifd1_at + 2 + 24 + 4;
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&(ifd0_at as u32).to_le_bytes());
        // IFD0: the orientation tag, next = IFD1.
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x0112u16.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes()); // SHORT
        tiff.extend_from_slice(&1u32.to_le_bytes()); // count
        tiff.extend_from_slice(&turn.to_le_bytes());
        tiff.extend_from_slice(&0u16.to_le_bytes()); // pad to 4 bytes
        tiff.extend_from_slice(&(ifd1_at as u32).to_le_bytes());
        // IFD1: JPEGInterchangeFormat + length, next = 0.
        tiff.extend_from_slice(&2u16.to_le_bytes());
        for (tag, value) in [
            (0x0201u16, preview_at as u32),
            (0x0202, preview.len() as u32),
        ] {
            tiff.extend_from_slice(&tag.to_le_bytes());
            tiff.extend_from_slice(&4u16.to_le_bytes()); // LONG
            tiff.extend_from_slice(&1u32.to_le_bytes()); // count
            tiff.extend_from_slice(&value.to_le_bytes());
        }
        tiff.extend_from_slice(&0u32.to_le_bytes());
        debug_assert_eq!(tiff.len(), preview_at);
        tiff.extend_from_slice(preview);

        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend_from_slice(&tiff);

        let mut out = vec![0xFF, 0xD8];
        out.push(0xFF);
        out.push(0xE1);
        out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(&app1);
        // Everything after the SOI of the original file.
        out.extend_from_slice(&full[2..]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::wrap_exif as with_exif_preview;
    use super::*;

    fn photo(w: u32, h: u32) -> Vec<u8> {
        let mut img = image::RgbImage::new(w, h);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]);
        }
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
            .encode_image(&img)
            .unwrap();
        out
    }

    #[test]
    fn the_embedded_preview_is_found_and_used() {
        let preview = photo(160, 120);
        let file = with_exif_preview(&photo(1200, 900), &preview);
        let found = exif_preview(&file).expect("preview should be located");
        assert_eq!(found, preview.as_slice());
    }

    #[test]
    fn a_preview_is_read_from_the_head_alone() {
        // The point of the whole exercise: the preview must be reachable
        // without the rest of the file, or a network share still pays for the
        // full transfer.
        let preview = photo(160, 120);
        let file = with_exif_preview(&photo(2000, 1500), &preview);
        let head = &file[..8 * 1024.min(file.len())];
        assert!(
            exif_preview(head).is_some(),
            "preview must be inside the first few KB"
        );
    }

    #[test]
    fn a_file_with_no_exif_reports_none_rather_than_guessing() {
        let plain = photo(64, 64);
        assert!(exif_preview(&plain).is_none());
        // Not a JPEG at all.
        assert!(exif_preview(b"not an image").is_none());
        assert!(exif_preview(&[]).is_none());
    }

    #[test]
    fn a_truncated_exif_block_does_not_panic() {
        let preview = photo(160, 120);
        let file = with_exif_preview(&photo(600, 400), &preview);
        // Every prefix must be rejected cleanly — heads arrive truncated by
        // definition.
        for cut in 0..file.len().min(600) {
            let _ = exif_preview(&file[..cut]);
        }
    }

    #[test]
    fn a_tiny_preview_is_rejected_so_thumbnails_stay_sharp() {
        // 32px preview against a 192px target: upscaling that is worse than
        // paying for a scaled decode.
        assert!(decode_preview(&photo(32, 24), 192, 1).is_none());
        assert!(decode_preview(&photo(160, 120), 192, 1).is_some());
    }

    #[test]
    fn orientation_is_read_and_applied() {
        use super::test_support::wrap_exif_oriented;

        // No EXIF at all: nothing to read, and the image is left alone.
        assert_eq!(exif_orientation(&photo(64, 64)), None);

        // A landscape preview tagged "rotate 90" must come out portrait, which
        // is the whole point — the shell used to do this and photographers would
        // immediately notice every portrait shot lying on its side.
        let upright = wrap_exif_oriented(&photo(1200, 800), &photo(160, 120), 1);
        let turned = wrap_exif_oriented(&photo(1200, 800), &photo(160, 120), 6);
        assert_eq!(exif_orientation(&upright), Some(1));
        assert_eq!(exif_orientation(&turned), Some(6));

        let preview = exif_preview(&turned).unwrap().to_vec();
        let (w, h, _) = decode_preview(&preview, 192, 1).unwrap();
        assert!(w > h, "untransformed preview is landscape, got {w}x{h}");
        let (w, h, _) = decode_preview(&preview, 192, 6).unwrap();
        assert!(h > w, "orientation 6 should stand it up, got {w}x{h}");

        // A nonsense value is ignored rather than trusted.
        assert_eq!(
            exif_orientation(&wrap_exif_oriented(&photo(60, 60), &photo(160, 120), 42)),
            None
        );
    }

    #[test]
    fn every_orientation_maps_to_the_right_dimensions() {
        let landscape = image::DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(4, 2, vec![128; 4 * 2 * 3]).unwrap(),
        );
        for turn in [1, 2, 3, 4] {
            let out = orient(landscape.clone(), turn);
            assert_eq!(
                (out.width(), out.height()),
                (4, 2),
                "turn {turn} keeps axes"
            );
        }
        for turn in [5, 6, 7, 8] {
            let out = orient(landscape.clone(), turn);
            assert_eq!(
                (out.width(), out.height()),
                (2, 4),
                "turn {turn} swaps axes"
            );
        }
    }

    #[test]
    fn a_rotated_file_is_stood_up_end_to_end() {
        let dir = std::env::temp_dir().join(format!("atlas_turn_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("portrait.jpg");
        std::fs::write(
            &p,
            test_support::wrap_exif_oriented(&photo(2000, 1000), &photo(160, 80), 8),
        )
        .unwrap();
        let (w, h, _) = thumbnail(&p, 192).expect("should thumbnail");
        assert!(h > w, "a 90-degree turn should yield portrait, got {w}x{h}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scaled_decode_covers_the_target_without_upsampling() {
        let big = photo(1600, 1200);
        let (w, h, rgba) = scaled_jpeg(&big, 192).expect("should decode");
        assert_eq!(w.max(h), 192, "long edge should land on the target");
        assert_eq!(rgba.len(), (w * h * 4) as usize);

        // Smaller than the target: keep it as-is rather than blowing it up.
        let small = photo(80, 60);
        let (w, h, _) = scaled_jpeg(&small, 192).expect("should decode");
        assert_eq!((w, h), (80, 60));
    }

    #[test]
    fn a_portrait_image_keeps_its_aspect_ratio() {
        let tall = photo(600, 1800);
        let (w, h, _) = scaled_jpeg(&tall, 192).unwrap();
        assert_eq!(h, 192);
        assert!(w < h, "portrait must stay portrait, got {w}x{h}");
        assert_eq!(w, 64, "600:1800 is 1:3, so 192 tall is 64 wide");
    }

    #[test]
    fn thumbnail_reads_a_real_file_end_to_end() {
        let dir = std::env::temp_dir().join(format!("atlas_raster_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let with_preview = dir.join("camera.jpg");
        std::fs::write(
            &with_preview,
            with_exif_preview(&photo(2400, 1800), &photo(160, 120)),
        )
        .unwrap();
        let (w, h, _) = thumbnail(&with_preview, 192).expect("preview path");
        assert!(w.max(h) <= 192 && w > 0 && h > 0);

        let plain = dir.join("export.jpg");
        std::fs::write(&plain, photo(1000, 1000)).unwrap();
        let (w, h, _) = thumbnail(&plain, 192).expect("scaled decode path");
        assert_eq!((w, h), (192, 192));

        // PNG goes through the decode path, not the shell.
        let png = dir.join("shot.png");
        image::RgbaImage::from_pixel(300, 200, image::Rgba([10, 200, 30, 255]))
            .save(&png)
            .unwrap();
        let (w, h, rgba) = thumbnail(&png, 192).expect("png path");
        assert_eq!((w, h), (192, 128));
        assert!(rgba[1] > 150, "green should survive the resize");

        // Unhandled extensions defer to the caller's fallbacks.
        let odd = dir.join("model.3dm");
        std::fs::write(&odd, b"not an image").unwrap();
        assert!(thumbnail(&odd, 192).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_file_fails_instead_of_panicking() {
        let dir = std::env::temp_dir().join(format!("atlas_raster_bad_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("truncated.jpg");
        let good = photo(800, 600);
        std::fs::write(&bad, &good[..good.len() / 3]).unwrap();
        // Truncated JPEGs may decode partially or fail; either is fine so long
        // as it returns.
        let _ = thumbnail(&bad, 192);
        std::fs::write(&bad, b"\xFF\xD8garbage").unwrap();
        assert!(thumbnail(&bad, 192).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
