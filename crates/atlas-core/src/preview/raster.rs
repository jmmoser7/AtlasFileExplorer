//! One bounded raster decoder for file previews and ephemeral generator frames.
use image::{ImageDecoder, ImageReader};
use std::io::{BufRead, Cursor, Seek};
use std::path::Path;

pub const MAX_LIVE_ENCODED_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_LIVE_TARGET_PX: u32 = 1024;
const LIVE_SOURCE_PIXELS: u64 = 16_000_000;
const LIVE_DECODE_BYTES: u64 = 128 * 1024 * 1024;
// Preserve the established file source ceiling and image 0.25 allocation limit.
const FILE_SOURCE_PIXELS: u64 = 180_000_000;
const FILE_DECODE_BYTES: u64 = 512 * 1024 * 1024;

pub(super) fn decode_file(path: &Path, target: u32) -> Option<(u32, u32, Vec<u8>)> {
    let reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    decode(
        reader,
        target.min(super::MAX_PX_MAX),
        FILE_SOURCE_PIXELS,
        FILE_DECODE_BYTES,
    )
}

pub(super) fn decode_bytes(bytes: &[u8], target: u32) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.is_empty() || bytes.len() > MAX_LIVE_ENCODED_BYTES {
        return None;
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    decode(
        reader,
        target.min(MAX_LIVE_TARGET_PX),
        LIVE_SOURCE_PIXELS,
        LIVE_DECODE_BYTES,
    )
}

fn decode<R: BufRead + Seek>(
    mut reader: ImageReader<R>,
    target: u32,
    source_pixels: u64,
    allocation: u64,
) -> Option<(u32, u32, Vec<u8>)> {
    if target == 0 {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(source_pixels.min(u32::MAX as u64) as u32);
    limits.max_image_height = limits.max_image_width;
    limits.max_alloc = Some(allocation);
    reader.limits(limits);
    let decoder = reader.into_decoder().ok()?;
    let (w, h) = decoder.dimensions();
    if w == 0
        || h == 0
        || u64::from(w) * u64::from(h) > source_pixels
        || decoder.total_bytes() > allocation
    {
        return None;
    }
    let image = image::DynamicImage::from_decoder(decoder).ok()?;
    let image = if w > target || h > target {
        image.thumbnail(target, target)
    } else {
        image
    };
    let rgba = image.into_rgba8();
    Some((rgba.width(), rgba.height(), rgba.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                &vec![0; (w * h * 4) as usize],
                w,
                h,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn bytes_decode_caps_target_and_never_upscales() {
        let (w, h, pixels) = decode_bytes(&png(1200, 600), 8192).unwrap();
        assert_eq!((w, h), (1024, 512));
        assert_eq!(pixels.len(), (w * h * 4) as usize);
        assert_eq!(decode_bytes(&png(20, 10), 1024).unwrap().0, 20);
    }

    #[test]
    fn bytes_refuse_oversized_encoded_or_header_sources() {
        assert!(decode_bytes(&vec![0; MAX_LIVE_ENCODED_BYTES + 1], 1024).is_none());
        // A PNG with a real, small encoded stream but more than 16 MP.
        assert!(decode_bytes(&png(4001, 4000), 1024).is_none());
        assert!(decode_bytes(b"not an image", 1024).is_none());
        assert!(decode_bytes(&png(2, 2), 0).is_none());
    }
}
