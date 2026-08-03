//! Comparative measurement of the thumbnail hot path: full decode, the Windows
//! shell, our scaled decode, and our embedded-preview path.
//!
//! Not a correctness test — a stopwatch, and ignored by default because it
//! encodes ~38 MB of JPEG to get a fair sample. Run it deliberately:
//!
//!   cargo test -p atlas-core --release --test thumb_bench -- --ignored --nocapture

use std::time::Instant;

/// A photo-shaped JPEG with an EXIF thumbnail, written the way a camera would:
/// APP1/EXIF segment carrying a 160x120 preview ahead of the full-size image.
fn write_photo(path: &std::path::Path, w: u32, h: u32) {
    let mut img = image::RgbImage::new(w, h);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]);
    }
    let mut full = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut full, 88)
        .encode_image(&img)
        .unwrap();
    std::fs::write(path, &full).unwrap();
}

#[test]
#[ignore = "encodes a multi-megabyte corpus; run explicitly"]
fn baseline_thumbnail_paths() {
    let dir = std::env::temp_dir().join("atlas_thumb_bench");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // 24 MP-ish: the case that matters. Keep the count low; this is a probe.
    let n = 12;
    let mut paths = Vec::new();
    let t = Instant::now();
    for i in 0..n {
        let p = dir.join(format!("photo{i}.jpg"));
        write_photo(&p, 6000, 4000);
        paths.push(p);
    }
    println!("wrote {n} 6000x4000 JPEGs in {:?}", t.elapsed());
    let bytes: u64 = paths
        .iter()
        .filter_map(|p| p.metadata().ok())
        .map(|m| m.len())
        .sum();
    println!("corpus = {:.1} MB", bytes as f64 / 1e6);

    // Path A: full decode + resize, which is what `image::open().thumbnail()` costs.
    let t = Instant::now();
    for p in &paths {
        let img = image::open(p).unwrap();
        let _ = img.thumbnail(192, 192);
    }
    let full = t.elapsed();
    println!(
        "full decode + resize:  {:?} total, {:.1} ms/file, {:.1} thumbs/sec",
        full,
        full.as_secs_f64() * 1000.0 / n as f64,
        n as f64 / full.as_secs_f64()
    );

    // Path B: what the shell currently does per file (Windows only). COM must
    // be initialised on this thread or every call fails fast and the timing is
    // meaningless.
    #[cfg(windows)]
    {
        unsafe {
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let t = Instant::now();
        let mut ok = 0;
        for p in &paths {
            if atlas_core::thumbs::probe_shell_thumbnail(p).is_some() {
                ok += 1;
            }
        }
        let shell = t.elapsed();
        println!(
            "windows shell:         {:?} total, {:.1} ms/file, {:.1} thumbs/sec ({ok}/{n} ok)",
            shell,
            shell.as_secs_f64() * 1000.0 / n as f64,
            n as f64 / shell.as_secs_f64()
        );
    }

    // Path C: the new pipeline — scaled DCT decode, no embedded preview here
    // because `image` does not write one.
    let t = Instant::now();
    let mut ok = 0;
    for p in &paths {
        if atlas_core::rasterthumb::thumbnail(p, 192).is_some() {
            ok += 1;
        }
    }
    let scaled = t.elapsed();
    println!(
        "scaled decode (no exif): {:?} total, {:.1} ms/file, {:.1} thumbs/sec ({ok}/{n} ok)",
        scaled,
        scaled.as_secs_f64() * 1000.0 / n as f64,
        n as f64 / scaled.as_secs_f64()
    );

    // Path D: with an embedded preview, which is what real camera files have.
    // Only the head of each file is touched.
    let preview = {
        let mut small = image::RgbImage::new(160, 120);
        for (x, y, px) in small.enumerate_pixels_mut() {
            *px = image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]);
        }
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
            .encode_image(&small)
            .unwrap();
        out
    };
    let mut exif_paths = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        let full = std::fs::read(p).unwrap();
        let q = dir.join(format!("camera{i}.jpg"));
        std::fs::write(
            &q,
            atlas_core::rasterthumb::test_support::wrap_exif(&full, &preview),
        )
        .unwrap();
        exif_paths.push(q);
    }
    let t = Instant::now();
    let mut ok = 0;
    for p in &exif_paths {
        if atlas_core::rasterthumb::thumbnail(p, 192).is_some() {
            ok += 1;
        }
    }
    let embedded = t.elapsed();
    println!(
        "embedded preview:      {:?} total, {:.2} ms/file, {:.0} thumbs/sec ({ok}/{n} ok)",
        embedded,
        embedded.as_secs_f64() * 1000.0 / n as f64,
        n as f64 / embedded.as_secs_f64()
    );
    println!(
        "\nspeedup vs full decode: scaled {:.1}x, embedded {:.0}x",
        full.as_secs_f64() / scaled.as_secs_f64(),
        full.as_secs_f64() / embedded.as_secs_f64()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
