//! SVG thumbnails / zoom previews via `resvg`.
//!
//! Same role as `pdf.rs` for PDFs: a built-in extractor so Atlas does not
//! depend on whatever Windows Explorer thumbnail provider (if any) is
//! installed. Rasterizes to RGBA at a capped longest-edge size.
//!
//! External image hrefs are not resolved — loading a sibling PNG from a
//! OneDrive SVG would hydrate cloud placeholders. Text uses system fonts
//! loaded once into a shared `fontdb`.

use resvg::tiny_skia;
use resvg::usvg;
use std::path::Path;
use std::sync::OnceLock;

/// Skip absurd SVG sources (exports with inlined base64, etc.).
const MAX_SVG_BYTES: u64 = 16 * 1024 * 1024;

/// Hard ceiling on the pixmap edge even if a caller asks for more.
const MAX_PIXMAP_EDGE: u32 = 4096;

/// Rasterize `path` to RGBA with longest edge ≤ `target_px` (never upscales).
pub fn thumbnail(path: &Path, target_px: u32) -> Option<(u32, u32, Vec<u8>)> {
    let target_px = target_px.clamp(1, MAX_PIXMAP_EDGE);
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() == 0 || meta.len() > MAX_SVG_BYTES {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    if !looks_like_svg(&data) {
        return None;
    }
    render_bytes(&data, target_px)
}

fn looks_like_svg(data: &[u8]) -> bool {
    // Cheap reject for misnamed binaries. Real SVGs are XML (optionally with a
    // UTF-8 BOM) and contain an `<svg` tag somewhere in the head.
    let head = &data[..data.len().min(4096)];
    let lower: Vec<u8> = head.iter().map(|b| b.to_ascii_lowercase()).collect();
    lower.windows(4).any(|w| w == b"<svg")
}

fn render_bytes(data: &[u8], target_px: u32) -> Option<(u32, u32, Vec<u8>)> {
    let tree = usvg::Tree::from_data(data, options()).ok()?;
    let size = tree.size();
    let orig_w = size.width();
    let orig_h = size.height();
    if !orig_w.is_finite() || !orig_h.is_finite() || orig_w <= 0.0 || orig_h <= 0.0 {
        return None;
    }

    let scale = (target_px as f32 / orig_w.max(orig_h)).min(1.0);
    let w = (orig_w * scale).round().clamp(1.0, MAX_PIXMAP_EDGE as f32) as u32;
    let h = (orig_h * scale).round().clamp(1.0, MAX_PIXMAP_EDGE as f32) as u32;

    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    // JPEG thumb cache has no alpha — paint on white so transparent logos
    // don't collapse to black when cached.
    pixmap.fill(tiny_skia::Color::WHITE);
    let transform = tiny_skia::Transform::from_scale(w as f32 / orig_w, h as f32 / orig_h);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some((w, h, straight_rgba(pixmap)))
}

fn options() -> &'static usvg::Options<'static> {
    static OPTS: OnceLock<usvg::Options> = OnceLock::new();
    OPTS.get_or_init(|| {
        let mut opt = usvg::Options::default();
        // Text in icons / diagrams; shapes still render if fonts are absent.
        opt.fontdb_mut().load_system_fonts();
        opt
    })
}

/// Convert tiny-skia's premultiplied pixmap into straight RGBA the rest of
/// the thumbnail pipeline expects.
fn straight_rgba(pixmap: tiny_skia::Pixmap) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixmap.data().len());
    for px in pixmap.pixels() {
        let a = px.alpha();
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else if a == 255 {
            out.extend_from_slice(&[px.red(), px.green(), px.blue(), 255]);
        } else {
            let inv = 255.0 / a as f32;
            out.push((px.red() as f32 * inv).round().min(255.0) as u8);
            out.push((px.green() as f32 * inv).round().min(255.0) as u8);
            out.push((px.blue() as f32 * inv).round().min(255.0) as u8);
            out.push(a);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp_svg(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nfa_svg_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn rasterizes_simple_rect_and_fits_target() {
        let path = write_temp_svg(
            "red.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200">
                 <rect width="400" height="200" fill="#ff0000"/>
               </svg>"##,
        );
        let (w, h, rgba) = thumbnail(&path, 100).expect("svg renders");
        assert_eq!((w, h), (100, 50));
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        // Centre pixel should be solid red on the white-backed pixmap.
        let i = (((h / 2) * w + (w / 2)) * 4) as usize;
        assert!(rgba[i] > 240, "r={}", rgba[i]);
        assert!(rgba[i + 1] < 20, "g={}", rgba[i + 1]);
        assert!(rgba[i + 2] < 20, "b={}", rgba[i + 2]);
    }

    #[test]
    fn never_upscales_small_svg() {
        let path = write_temp_svg(
            "tiny.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40">
                 <circle cx="20" cy="20" r="18" fill="#00aa00"/>
               </svg>"##,
        );
        let (w, h, _) = thumbnail(&path, 192).expect("tiny svg renders");
        assert_eq!((w, h), (40, 40));
    }

    #[test]
    fn rejects_non_svg_bytes() {
        let path = write_temp_svg("fake.svg", "not an svg at all");
        assert!(thumbnail(&path, 64).is_none());
    }
}
