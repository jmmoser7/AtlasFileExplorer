//! Embedded thumbnails from Office Open XML files (.pptx, .docx, .xlsx, ...).
//!
//! These files are zip archives; when the document was saved with "save
//! preview picture" (PowerPoint's default), a ready-made image sits at
//! `docProps/thumbnail.{jpeg,png,...}`. Reading it costs a few KB of I/O —
//! no Office installation required, network-friendly.

use std::io::Read;
use std::path::Path;

const CANDIDATES: [&str; 4] = [
    "docProps/thumbnail.jpeg",
    "docProps/thumbnail.jpg",
    "docProps/thumbnail.png",
    "docProps/thumbnail.gif",
];

pub fn is_ooxml(ext: &str) -> bool {
    matches!(
        ext,
        "pptx" | "ppsx" | "potx" | "pptm" | "docx" | "docm" | "dotx" | "xlsx" | "xlsm" | "xltx"
    )
}

pub fn embedded_thumbnail(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    for name in CANDIDATES {
        let Ok(mut entry) = archive.by_name(name) else {
            continue;
        };
        let mut buf = Vec::with_capacity(entry.size() as usize);
        if entry.read_to_end(&mut buf).is_err() {
            continue;
        }
        let Ok(img) = image::load_from_memory(&buf) else {
            continue;
        };
        let img = if img.width() > 512 || img.height() > 512 {
            img.thumbnail(384, 384)
        } else {
            img
        };
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        return Some((w, h, rgba.into_raw()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_thumbnail_from_synthetic_pptx() {
        let dir = std::env::temp_dir().join(format!("nfa_office_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let pptx = dir.join("deck.pptx");

        // Minimal zip with a green 32x32 PNG at the thumbnail path.
        let img = image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 200, 0, 255]));
        let mut png = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png),
            image::ImageFormat::Png,
        )
        .unwrap();

        let f = std::fs::File::create(&pptx).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        z.start_file("docProps/thumbnail.png", opts).unwrap();
        z.write_all(&png).unwrap();
        z.finish().unwrap();

        let (w, h, rgba) = embedded_thumbnail(&pptx).expect("no thumbnail extracted");
        assert_eq!((w, h), (32, 32));
        assert!(rgba[1] > 150, "expected green pixels");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
