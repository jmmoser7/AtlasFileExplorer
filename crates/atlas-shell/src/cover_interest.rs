//! Cheap "is this worth a cover tile?" ranker for the home album.
//!
//! Workbook covers used to take the first linked files in document order, so a
//! shelf filled with blank pages and one-stroke exports. This looks at a small
//! spread of images and keeps the ones with something actually going on.
//!
//! The score is a 16×16 sample, not a model: a block counts as detail only when
//! its tones are more mixed than a hard edge, and a picture made of one or two
//! flat fields (a blank, a single line, a bar) stays under the bar. Color is a
//! tie-break, so a black-and-white photograph still wins.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

const GRID: u32 = 16;
const BLOCK: u32 = 4;
/// Below this, the mosaic would rather show the workbook tile.
const STRIKING_MIN: f32 = 0.9;
/// Share of samples that must differ from the dominant color. A hairline is under this.
const INK_MIN: f32 = 0.08;
/// Two-tone split tops out at 1 bit; detail has to be busier than an edge.
const RICH_ENTROPY: f32 = 1.5;
const PALETTE_WEIGHT: f32 = 0.35;
const PROBE_PX: u32 = 48;
/// How many files a cover may open. Evenly spread, so the last pages still count.
const PROBE_CAP: usize = 24;
/// PNG and WebP have no scaled decode. Larger than this, skip rather than slurp.
const MAX_FULL_DECODE: u64 = 8 * 1024 * 1024;

/// Up to `limit` images that read as pictures, highest score first.
///
/// Fewer than `limit` — including none — means the rest were blank, a stroke,
/// or not cheap to look at. Callers fall back to the workbook tile.
pub fn rank_striking(paths: &[PathBuf], limit: usize) -> Vec<PathBuf> {
    if limit == 0 {
        return Vec::new();
    }
    let mut seen = HashSet::with_capacity(paths.len().min(64));
    let unique: Vec<&Path> = paths
        .iter()
        .map(PathBuf::as_path)
        .filter(|path| seen.insert(*path))
        .collect();
    let mut ranked = Vec::new();
    for (i, path) in choose(&unique, PROBE_CAP).into_iter().enumerate() {
        let Some((w, h, rgba)) = probe(path) else {
            continue;
        };
        let score = striking_score(w, h, &rgba);
        if score >= STRIKING_MIN {
            ranked.push((score, i, path));
        }
    }
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, _, path)| path.to_path_buf())
        .collect()
}

fn choose<'a>(paths: &[&'a Path], cap: usize) -> Vec<&'a Path> {
    if paths.is_empty() || cap == 0 {
        return Vec::new();
    }
    if paths.len() <= cap {
        return paths.to_vec();
    }
    (0..cap)
        .map(|i| paths[i * (paths.len() - 1) / (cap - 1)])
        .collect()
}

fn probe(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    // Fail closed: a placeholder must not be opened, or the sync client downloads it.
    if atlas_core::cloud::is_dehydrated(path) {
        return None;
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let jpeg = matches!(ext.as_str(), "jpg" | "jpeg" | "jpe" | "jfif");
    if !jpeg {
        if !matches!(ext.as_str(), "png" | "webp") {
            return None;
        }
        let len = std::fs::metadata(path).ok()?.len();
        if len > MAX_FULL_DECODE {
            return None;
        }
    }
    if atlas_core::rasterthumb::handles(&ext) {
        return atlas_core::rasterthumb::thumbnail(path, PROBE_PX);
    }
    let img = image::open(path).ok()?.thumbnail(PROBE_PX, PROBE_PX);
    let rgba = img.to_rgba8();
    Some((rgba.width(), rgba.height(), rgba.into_raw()))
}

#[derive(Clone, Copy)]
struct Sample {
    r: u8,
    g: u8,
    b: u8,
    luma: u8,
    on: bool,
}

fn striking_score(w: u32, h: u32, rgba: &[u8]) -> f32 {
    if w == 0 || h == 0 || rgba.len() < (w as usize) * (h as usize) * 4 {
        return 0.0;
    }
    let mut samples = [Sample {
        r: 0,
        g: 0,
        b: 0,
        luma: 0,
        on: false,
    }; (GRID * GRID) as usize];
    let mut counts = [0u32; 512];
    let mut opaque = 0u32;
    for gy in 0..GRID {
        for gx in 0..GRID {
            let x = (gx * w / GRID).min(w - 1);
            let y = (gy * h / GRID).min(h - 1);
            let i = ((y * w + x) as usize) * 4;
            let (r, g, b, a) = (rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]);
            let on = a >= 128;
            if on {
                opaque += 1;
                counts[quant_key(r, g, b)] += 1;
            }
            let luma = ((r as u16 * 54 + g as u16 * 183 + b as u16 * 19) >> 8) as u8;
            samples[(gy * GRID + gx) as usize] = Sample { r, g, b, luma, on };
        }
    }
    let mode = counts.iter().copied().max().unwrap_or(0) as f32;
    let ink = if opaque == 0 {
        0.0
    } else {
        1.0 - mode / (opaque as f32)
    };
    if ink < INK_MIN {
        return 0.0;
    }

    let mut rich = 0u32;
    let mut palette_seen = [false; 512];
    let mut palette = 0u32;
    let mut sat_sum = 0u32;
    for by in 0..BLOCK {
        for bx in 0..BLOCK {
            let mut hist = [0u32; 16];
            let mut n = 0u32;
            let (mut rs, mut gs, mut bs) = (0u32, 0u32, 0u32);
            for ly in 0..BLOCK {
                for lx in 0..BLOCK {
                    let s = samples[((by * BLOCK + ly) * GRID + bx * BLOCK + lx) as usize];
                    if !s.on {
                        continue;
                    }
                    n += 1;
                    hist[(s.luma >> 4) as usize] += 1;
                    rs += s.r as u32;
                    gs += s.g as u32;
                    bs += s.b as u32;
                    sat_sum += (s.r.max(s.g).max(s.b) - s.r.min(s.g).min(s.b)) as u32;
                }
            }
            if n >= 8 && entropy(&hist, n) > RICH_ENTROPY {
                rich += 1;
            }
            if n >= 4 {
                let key = quant_key((rs / n) as u8, (gs / n) as u8, (bs / n) as u8);
                if !palette_seen[key] {
                    palette_seen[key] = true;
                    palette += 1;
                }
            }
        }
    }
    let structure = rich as f32 + PALETTE_WEIGHT * palette.saturating_sub(1) as f32;
    let mean_sat = sat_sum as f32 / (opaque as f32 * 255.0);
    structure * (1.0 + mean_sat)
}

fn quant_key(r: u8, g: u8, b: u8) -> usize {
    ((r as usize) >> 5 << 6) | ((g as usize) >> 5 << 3) | ((b as usize) >> 5)
}

fn entropy(hist: &[u32; 16], n: u32) -> f32 {
    let nf = n as f32;
    let mut h = 0.0f32;
    for &count in hist {
        if count == 0 {
            continue;
        }
        let p = count as f32 / nf;
        h -= p * p.log2();
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> image::RgbaImage {
        image::RgbaImage::from_fn(w, h, |x, y| image::Rgba(f(x, y)))
    }

    fn score_of(img: &image::RgbaImage) -> f32 {
        striking_score(img.width(), img.height(), img.as_raw())
    }

    fn blank(w: u32, h: u32) -> image::RgbaImage {
        image::RgbaImage::from_pixel(w, h, image::Rgba([248, 248, 248, 255]))
    }

    fn line(w: u32, h: u32) -> image::RgbaImage {
        let y = h / 2;
        paint(w, h, |_x, py| {
            if py == y || py + 1 == y {
                [0, 0, 0, 255]
            } else {
                [255, 255, 255, 255]
            }
        })
    }

    fn bar(w: u32, h: u32) -> image::RgbaImage {
        let y0 = h * 2 / 5;
        let y1 = h * 3 / 5;
        paint(w, h, |_x, y| {
            if (y0..y1).contains(&y) {
                [0, 0, 0, 255]
            } else {
                [255, 255, 255, 255]
            }
        })
    }

    fn photo(w: u32, h: u32) -> image::RgbaImage {
        paint(w, h, |x, y| {
            [
                (x * 37 + y * 17) as u8,
                (x * 13 + y * 53 + 80) as u8,
                (x * 91 + y * 29 + 40) as u8,
                255,
            ]
        })
    }

    fn poster(w: u32, h: u32) -> image::RgbaImage {
        paint(w, h, |x, y| {
            let right = x >= w / 2;
            let bottom = y >= h / 2;
            match (right, bottom) {
                (false, false) => [255, 0, 0, 255],
                (true, false) => [0, 170, 0, 255],
                (false, true) => [0, 0, 255, 255],
                (true, true) => [255, 210, 0, 255],
            }
        })
    }

    fn subject(w: u32, h: u32) -> image::RgbaImage {
        let x0 = w / 4;
        let y0 = h / 4;
        let x1 = w * 3 / 4;
        let y1 = h * 3 / 4;
        paint(w, h, |x, y| {
            if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
                [
                    (x * 37 + y * 17) as u8,
                    (x * 13 + y * 53 + 80) as u8,
                    (x * 91 + y * 29 + 40) as u8,
                    255,
                ]
            } else {
                [255, 255, 255, 255]
            }
        })
    }

    #[test]
    fn blanks_lines_and_bars_sink_and_pictures_float() {
        let blank = score_of(&blank(128, 128));
        let line = score_of(&line(128, 128));
        let bar = score_of(&bar(128, 128));
        let poster = score_of(&poster(128, 128));
        let subject = score_of(&subject(128, 128));
        let photo = score_of(&photo(128, 128));
        assert!(blank < STRIKING_MIN, "blank {blank}");
        assert!(line < STRIKING_MIN, "line {line}");
        assert!(bar < STRIKING_MIN, "bar {bar}");
        assert!(poster >= STRIKING_MIN, "poster {poster}");
        assert!(subject >= STRIKING_MIN, "subject {subject}");
        assert!(photo >= STRIKING_MIN, "photo {photo}");
        assert!(photo > poster, "photo {photo} poster {poster}");
    }

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("slate-cover-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn save(&self, name: &str, img: &image::RgbaImage) -> PathBuf {
            let path = self.0.join(name);
            img.save(&path).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rank_keeps_pictures_and_drops_blanks_and_lines() {
        let dir = Scratch::new("rank");
        let line = dir.save("line.png", &line(128, 128));
        let blank = dir.save("blank.png", &blank(128, 128));
        let poster = dir.save("poster.png", &poster(128, 128));
        let photo = dir.save("photo.png", &photo(128, 128));
        let ranked = rank_striking(&[line.clone(), blank, poster.clone(), photo.clone()], 4);
        assert_eq!(ranked.first(), Some(&photo), "{ranked:?}");
        assert!(ranked.contains(&poster), "{ranked:?}");
        assert!(!ranked.contains(&line), "{ranked:?}");
        assert_eq!(ranked.len(), 2, "{ranked:?}");
    }

    #[test]
    fn the_same_file_is_not_picked_twice() {
        let dir = Scratch::new("dup");
        let photo = dir.save("photo.png", &photo(64, 64));
        let ranked = rank_striking(&[photo.clone(), photo], 4);
        assert_eq!(ranked.len(), 1);
    }

    #[test]
    fn a_picture_at_the_end_of_a_long_list_is_still_seen() {
        let dir = Scratch::new("spread");
        let mut paths = Vec::new();
        let empty = blank(32, 32);
        for i in 0..40 {
            paths.push(dir.save(&format!("blank-{i}.png"), &empty));
        }
        let photo = dir.save("photo.png", &photo(64, 64));
        paths.push(photo.clone());
        let ranked = rank_striking(&paths, 4);
        assert_eq!(ranked, vec![photo]);
    }

    #[cfg(windows)]
    #[test]
    fn a_cloud_placeholder_is_not_opened() {
        let dir = Scratch::new("cloud");
        let photo = dir.save("photo.png", &photo(64, 64));
        assert!(
            atlas_core::cloud::mark_offline(&photo),
            "offline bit is the stand-in for a dehydrated file"
        );
        assert!(
            rank_striking(&[photo], 4).is_empty(),
            "scoring a placeholder would download it"
        );
    }
}
