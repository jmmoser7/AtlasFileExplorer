//! Point the real thumbnail pipeline at a real folder and report, per file,
//! which tier produced pixels. Diagnostic only.
//!
//!   $env:ATLAS_PROBE_DIR = "C:\path\to\folder"
//!   cargo test -p atlas-core --release --test folder_probe -- --ignored --nocapture

use std::time::Instant;

#[test]
#[ignore = "diagnostic; needs ATLAS_PROBE_DIR"]
fn probe_folder() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    let dir = std::env::var("ATLAS_PROBE_DIR").expect("set ATLAS_PROBE_DIR");
    let dir = std::path::PathBuf::from(dir);
    println!("probing {}\n", dir.display());

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("read_dir")
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for e in entries {
        let ft = match e.file_type() {
            Ok(ft) => ft,
            Err(err) => {
                println!(
                    "{:<34} file_type FAILED: {err}",
                    e.file_name().to_string_lossy()
                );
                continue;
            }
        };
        let name = e.file_name().to_string_lossy().into_owned();
        if !ft.is_file() {
            println!(
                "{name:<34} SKIPPED by scanner (is_file={}, is_dir={}, is_symlink={})",
                ft.is_file(),
                ft.is_dir(),
                ft.is_symlink()
            );
            continue;
        }
        let path = e.path();
        let fam = atlas_core::types::Family::from_ext(
            &path
                .extension()
                .map(|x| x.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default(),
        );

        // Never probe the byte-reading tiers on a cloud placeholder: this test
        // would download the folder it was asked to diagnose.
        if atlas_core::cloud::is_dehydrated(&path) {
            let full = atlas_core::thumbs::probe_extract(&path, None);
            println!(
                "{name:<34} family={fam:?}  CLOUD-ONLY (not downloaded)\n    PIPELINE={}",
                match &full {
                    Some((w, h, _, true)) => format!("{w}x{h}"),
                    Some((w, h, _, false)) => format!("{w}x{h} ICON(not cached)"),
                    None => "none".to_string(),
                }
            );
            continue;
        }

        // Tier by tier, the way a worker sees it.
        let t = Instant::now();
        let raster = atlas_core::rasterthumb::thumbnail(&path, 192);
        let t_raster = t.elapsed();

        let t = Instant::now();
        #[cfg(windows)]
        let shell = atlas_core::thumbs::probe_shell_thumbnail(&path);
        #[cfg(not(windows))]
        let shell: Option<(u32, u32, Vec<u8>)> = None;
        let t_shell = t.elapsed();

        let t = Instant::now();
        let full = atlas_core::thumbs::probe_extract(&path, None);
        let t_full = t.elapsed();

        let show = |r: &Option<(u32, u32, Vec<u8>)>| match r {
            Some((w, h, _)) => format!("{w}x{h}"),
            None => "none".to_string(),
        };
        let full_show = match &full {
            Some((w, h, _, true)) => format!("{w}x{h}"),
            Some((w, h, _, false)) => format!("{w}x{h} ICON(not cached)"),
            None => "none".to_string(),
        };
        println!(
            "{name:<34} family={fam:?}\n    raster={:<10} ({:>7.1}ms)   shell={:<10} ({:>7.1}ms)   PIPELINE={:<22} ({:>7.1}ms)",
            show(&raster),
            t_raster.as_secs_f64() * 1000.0,
            show(&shell),
            t_shell.as_secs_f64() * 1000.0,
            full_show,
            t_full.as_secs_f64() * 1000.0,
        );
    }
}
