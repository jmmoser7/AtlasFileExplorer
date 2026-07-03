//! PDF first-page thumbnails via pdfium (Chrome's PDF engine).
//!
//! pdfium.dll is loaded dynamically from next to the exe (or a `vendor/`
//! folder during development). If the DLL is missing the binding simply
//! stays `None` and PDF previews fall back to whatever shell handler the
//! machine has — the app never fails because of it.
//!
//! `Pdfium` handles aren't `Sync`, so each worker thread holds its own
//! binding; the crate's `thread_safe` feature serializes the underlying
//! FFI calls into the (single-threaded) pdfium library.

use pdfium_render::prelude::*;
use std::path::Path;

thread_local! {
    static PDFIUM: Option<Pdfium> = init();
}

fn init() -> Option<Pdfium> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.to_path_buf());
        }
    }
    candidates.push(std::path::PathBuf::from("vendor"));
    candidates.push(std::path::PathBuf::from("."));
    for dir in candidates {
        let lib = Pdfium::pdfium_platform_library_name_at_path(&dir);
        if let Ok(bindings) = Pdfium::bind_to_library(&lib) {
            return Some(Pdfium::new(bindings));
        }
    }
    match Pdfium::bind_to_system_library() {
        Ok(bindings) => Some(Pdfium::new(bindings)),
        Err(_) => {
            eprintln!("[atlas] pdfium.dll not found — PDF previews limited to shell handlers");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn renders_minimal_pdf_first_page() {
        // pdfium tolerates the sloppy xref: it rebuilds the table by scanning.
        let pdf = b"%PDF-1.4\n\
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] >> endobj\n\
trailer << /Root 1 0 R >>\n\
%%EOF";
        let dir = std::env::temp_dir().join(format!("nfa_pdf_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("mini.pdf");
        std::fs::write(&path, pdf).unwrap();

        let result = super::thumbnail(&path, 192);
        // Only assert when the pdfium DLL is actually present (vendor/).
        if std::path::Path::new("vendor/pdfium.dll").exists() {
            let (w, h, rgba) = result.expect("pdfium failed to render minimal pdf");
            assert!(w > 0 && h > 0);
            assert_eq!(rgba.len(), (w * h * 4) as usize);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Render page 1 of a PDF at thumbnail size. Returns RGBA pixels.
pub fn thumbnail(path: &Path, target_px: i32) -> Option<(u32, u32, Vec<u8>)> {
    PDFIUM.with(|pdfium| {
        let pdfium = pdfium.as_ref()?;
        let doc = pdfium.load_pdf_from_file(path, None).ok()?;
        let page = doc.pages().first().ok()?;
        let config = PdfRenderConfig::new()
            .set_target_width(target_px)
            .set_maximum_height(target_px * 2)
            .render_form_data(false);
        let bitmap = page.render_with_config(&config).ok()?;
        let img = bitmap.as_image();
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        if w == 0 || h == 0 {
            return None;
        }
        Some((w, h, rgba.into_raw()))
    })
}
