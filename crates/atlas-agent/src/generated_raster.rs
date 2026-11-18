//! Commit an image provider's completed output only after the board can decode it.
use image::{ImageDecoder, ImageFormat};
use std::io::{BufReader, Seek};
use std::path::Path;

// Match the board's full-resolution file preview ceilings.
const MAX_PIXELS: u64 = 180_000_000;
const MAX_DECODE_BYTES: u64 = 512 * 1024 * 1024;

fn validate(path: &Path) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut input = BufReader::new(file);
    let mut reader = image::ImageReader::new(&mut input)
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    match reader.format() {
        Some(ImageFormat::Png) => {
            // Check the final chunk as well as every pixel row. The provider may
            // still be producing a PNG with a readable header and partial data.
            input.rewind().map_err(|e| e.to_string())?;
            let mut decoder = png::Decoder::new(input);
            decoder.set_limits(png::Limits {
                bytes: MAX_DECODE_BYTES as usize,
            });
            let mut png = decoder.read_info().map_err(|e| e.to_string())?;
            let (w, h) = png.info().size();
            if u64::from(w) * u64::from(h) > MAX_PIXELS
                || png
                    .output_buffer_size()
                    .is_none_or(|n| n as u64 > MAX_DECODE_BYTES)
            {
                return Err("Generated image exceeds the canvas preview size limit".into());
            }
            while png
                .next_interlaced_row()
                .map_err(|e| e.to_string())?
                .is_some()
            {}
            png.finish().map_err(|e| e.to_string())
        }
        Some(ImageFormat::Jpeg) => {
            // The canvas decoder tolerates incomplete scans and fills missing
            // pixels. A finished provider result must reach its outer EOI marker.
            input.rewind().map_err(|e| e.to_string())?;
            let mut jpeg = jpeg_decoder::Decoder::new(input);
            jpeg.set_max_decoding_buffer_size(MAX_DECODE_BYTES as usize);
            jpeg.read_info().map_err(|e| e.to_string())?;
            let info = jpeg.info().ok_or("Missing generated JPEG dimensions")?;
            if u64::from(info.width) * u64::from(info.height) > MAX_PIXELS {
                return Err("Generated image exceeds the canvas preview size limit".into());
            }
            jpeg.decode().map(|_| ()).map_err(|e| e.to_string())
        }
        Some(ImageFormat::WebP) => {
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(MAX_DECODE_BYTES);
            reader.limits(limits);
            let decoder = reader.into_decoder().map_err(|e| e.to_string())?;
            let (w, h) = decoder.dimensions();
            if u64::from(w) * u64::from(h) > MAX_PIXELS || decoder.total_bytes() > MAX_DECODE_BYTES
            {
                return Err("Generated image exceeds the canvas preview size limit".into());
            }
            let mut pixels = vec![0; decoder.total_bytes() as usize];
            decoder.read_image(&mut pixels).map_err(|e| e.to_string())
        }
        _ => Err("The provider returned an unsupported or incomplete image".into()),
    }
}

/// The staging path is private to this call, in the destination directory.
/// The final path and its previous contents remain untouched on failure.
pub fn publish_generated_image(
    dest: &Path,
    write: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let parent = dest
        .parent()
        .ok_or("Generated image has no output directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let staged = parent.join(format!(".{}.partial", crate::request_id()));
    let result = write(&staged)
        .and_then(|()| validate(&staged))
        .and_then(|()| std::fs::rename(&staged, dest).map_err(|e| e.to_string()));
    if result.is_err() {
        let _ = std::fs::remove_file(staged);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    #[test]
    fn incomplete_jpeg_and_webp_results_never_replace_the_last_good_picture() {
        let root = std::env::temp_dir().join(crate::request_id());
        let target = root.join("picture.png");
        let pixels = DynamicImage::ImageRgb8(RgbImage::from_fn(48, 32, |x, y| {
            let i = x + 48 * y;
            Rgb([(i * 37) as u8, (i * 71) as u8, (i * 19) as u8])
        }));
        for format in [ImageFormat::Jpeg, ImageFormat::WebP] {
            let mut encoded = std::io::Cursor::new(Vec::new());
            pixels.write_to(&mut encoded, format).unwrap();
            let good = encoded.into_inner();
            let publish = |bytes: &[u8]| {
                publish_generated_image(&target, |p| {
                    std::fs::write(p, bytes).map_err(|e| e.to_string())
                })
            };
            publish(&good).unwrap();
            for length in [good.len() * 85 / 100, good.len() - 2] {
                assert!(publish(&good[..length]).is_err(), "{format:?} at {length}");
                assert_eq!(std::fs::read(&target).unwrap(), good);
            }
            if format == ImageFormat::Jpeg {
                let mut with_trailer = good;
                with_trailer.extend_from_slice(b"trailing metadata");
                publish(&with_trailer).unwrap();
            }
        }
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generated_outputs_preserve_color_alpha_and_legitimate_black_pixels() {
        let root = std::env::temp_dir().join(crate::request_id());
        let picture = root.join("result.png");
        let rgba = include_bytes!("../tests/fixtures/generated-rgba.png");
        let write = |path: &Path| std::fs::write(path, rgba).map_err(|e| e.to_string());
        publish_generated_image(&picture, write).unwrap();
        let actual = image::open(&picture).unwrap().to_rgba8();
        assert_eq!(actual.dimensions(), (2, 2));
        assert_eq!(
            actual.into_raw(),
            [255, 0, 0, 255, 48, 123, 214, 127, 0, 0, 0, 255, 222, 20, 70, 0]
        );
        for bad in [
            &rgba[..8],
            &rgba[..rgba.len() - 12],
            &b"HTTP 200: queue is not ready"[..],
        ] {
            assert!(
                publish_generated_image(&picture, |path| std::fs::write(path, bad)
                    .map_err(|e| e.to_string()))
                .is_err()
            );
            assert_eq!(std::fs::read(&picture).unwrap(), rgba);
        }
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
            let black = DynamicImage::ImageRgb8(RgbImage::from_pixel(5, 4, Rgb([0, 0, 0])));
            publish_generated_image(&picture, |path| {
                black
                    .save_with_format(path, format)
                    .map_err(|e| e.to_string())
            })
            .unwrap();
            let decoded = image::load_from_memory(&std::fs::read(&picture).unwrap())
                .unwrap()
                .into_rgba8();
            assert_eq!(decoded.dimensions(), (5, 4));
            assert!(decoded.pixels().all(|px| px.0 == [0, 0, 0, 255]));
        }
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
