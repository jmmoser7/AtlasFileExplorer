//! Fallback preview extraction for Rhino .3dm files.
//!
//! Rhino embeds a preview image in the properties table of every saved .3dm.
//! Rather than implementing the full openNURBS chunk grammar, we scan the
//! head of the file for an embedded PNG or JPEG stream (Rhino 6+ writes PNG
//! previews) and decode it. On machines with Rhino installed the shell
//! handler takes priority and this code never runs.

use std::io::Read;
use std::path::Path;

const SCAN_BYTES: usize = 12 * 1024 * 1024;

pub fn embedded_preview(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; SCAN_BYTES];
    let n = read_up_to(&mut f, &mut head)?;
    head.truncate(n);

    // Confirm it's actually a 3dm file.
    if !head.starts_with(b"3D Geometry File Format") {
        return None;
    }

    find_png(&head)
        .or_else(|| find_jpeg(&head))
        .and_then(decode)
}

fn read_up_to(f: &mut std::fs::File, buf: &mut [u8]) -> Option<usize> {
    let mut total = 0;
    loop {
        match f.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => return None,
        }
        if total == buf.len() {
            break;
        }
    }
    Some(total)
}

fn find_png(data: &[u8]) -> Option<&[u8]> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    let start = find(data, SIG, 0)?;
    // Walk PNG chunks to find the true end (IEND + CRC).
    let mut pos = start + 8;
    loop {
        if pos + 8 > data.len() {
            return None;
        }
        let len = u32::from_be_bytes(data[pos..pos + 4].try_into().ok()?) as usize;
        let ctype = &data[pos + 4..pos + 8];
        let end = pos + 8 + len + 4;
        if end > data.len() {
            return None;
        }
        if ctype == b"IEND" {
            return Some(&data[start..end]);
        }
        pos = end;
    }
}

fn find_jpeg(data: &[u8]) -> Option<&[u8]> {
    let start = find(data, &[0xFF, 0xD8, 0xFF], 0)?;
    // Find the last EOI marker after start; good enough for an embedded blob.
    let mut end = None;
    let mut i = start + 2;
    while i + 1 < data.len() {
        if data[i] == 0xFF && data[i + 1] == 0xD9 {
            end = Some(i + 2);
        }
        i += 1;
    }
    end.map(|e| &data[start..e])
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = img.thumbnail(
        crate::thumbs::THUMB_PX as u32,
        crate::thumbs::THUMB_PX as u32,
    );
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some((w, h, rgba.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_png_preview_from_synthetic_3dm() {
        let dir = std::env::temp_dir().join(format!("nfa_3dm_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("model.3dm");

        // Build a fake .3dm: correct magic, junk chunk bytes, then an embedded
        // PNG preview like Rhino writes into the properties table.
        let mut png_bytes = Vec::new();
        let img = image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 200, 50, 255]));
        image::DynamicImage::ImageRgba8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut png_bytes),
                image::ImageFormat::Png,
            )
            .unwrap();

        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"3D Geometry File Format 00000060").unwrap();
        f.write_all(&[0u8; 512]).unwrap();
        f.write_all(&png_bytes).unwrap();
        f.write_all(&[0u8; 128]).unwrap();
        drop(f);

        let (w, h, rgba) = embedded_preview(&path).expect("no preview extracted");
        assert!(w > 0 && h > 0);
        // Green-dominant pixel.
        assert!(rgba[1] > 150 && rgba[0] < 60);

        // Non-3dm content must be rejected.
        let bogus = dir.join("not.3dm");
        std::fs::write(&bogus, b"definitely not rhino").unwrap();
        assert!(embedded_preview(&bogus).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
