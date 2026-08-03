//! Proof that thumbnailing a cloud placeholder does not download it.
//!
//! Needs a real dehydrated file, so it is opt-in:
//!
//!   $env:ATLAS_PROBE_FILE = "C:\path\to\placeholder.jpg"
//!   cargo test -p atlas-core --release --test cloud_guard -- --ignored --nocapture

#[test]
#[ignore = "needs ATLAS_PROBE_FILE pointing at a real cloud placeholder"]
fn thumbnailing_a_placeholder_does_not_hydrate_it() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    let path = std::env::var("ATLAS_PROBE_FILE").expect("set ATLAS_PROBE_FILE");
    let path = std::path::PathBuf::from(path);

    assert!(
        atlas_core::cloud::is_dehydrated(&path),
        "{} is already local, so this proves nothing — pick a placeholder",
        path.display()
    );

    let got = atlas_core::thumbs::probe_extract(&path, None);
    match &got {
        Some((w, h, _, cacheable)) => {
            println!("extracted {w}x{h}, cacheable={cacheable} (false = type icon, as expected)")
        }
        None => println!("no pixels available without downloading"),
    }

    assert!(
        atlas_core::cloud::is_dehydrated(&path),
        "thumbnailing hydrated {} — Atlas must never download a file to draw a \
         thumbnail (see crates/atlas-core/src/cloud.rs)",
        path.display()
    );
    println!("still dehydrated: no download was triggered");
}

/// The same guarantee across a whole batch, checked per file so a leak names the
/// file that leaked.
///
///   $env:ATLAS_PROBE_DIR = "C:\folder"; $env:ATLAS_PROBE_LIMIT = "60"
///   cargo test -p atlas-core --release --test cloud_guard batch -- --ignored --nocapture
#[test]
#[ignore = "needs ATLAS_PROBE_DIR containing cloud placeholders"]
fn batch_thumbnailing_hydrates_nothing() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let dir = std::path::PathBuf::from(std::env::var("ATLAS_PROBE_DIR").expect("ATLAS_PROBE_DIR"));
    let limit: usize = std::env::var("ATLAS_PROBE_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let mut checked = 0usize;
    let mut icons = 0usize;
    let mut leaked = Vec::new();
    let started = std::time::Instant::now();
    for e in std::fs::read_dir(&dir).expect("read_dir").flatten() {
        if checked >= limit {
            break;
        }
        let path = e.path();
        if !path.is_file() || !atlas_core::cloud::is_dehydrated(&path) {
            continue;
        }
        checked += 1;
        if let Some((_, _, _, false)) = atlas_core::thumbs::probe_extract(&path, None) {
            icons += 1;
        }
        if !atlas_core::cloud::is_dehydrated(&path) {
            leaked.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    println!(
        "checked {checked} placeholders in {:.0}ms ({:.0}/sec), {icons} resolved to type icons",
        started.elapsed().as_secs_f64() * 1000.0,
        checked as f64 / started.elapsed().as_secs_f64()
    );
    assert!(
        leaked.is_empty(),
        "these placeholders were downloaded: {leaked:?}"
    );
    println!("no file was downloaded");
}

/// Can the sync client's own thumbnail provider give us a real preview without
/// downloading the file? That is the difference between "cloud folders show type
/// icons" and "cloud folders look like Explorer".
#[cfg(windows)]
#[test]
#[ignore = "needs ATLAS_PROBE_FILE pointing at a real cloud placeholder"]
fn measure_what_a_placeholder_gives_up_without_downloading() {
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let path =
        std::path::PathBuf::from(std::env::var("ATLAS_PROBE_FILE").expect("ATLAS_PROBE_FILE"));
    assert!(
        atlas_core::cloud::is_dehydrated(&path),
        "pick a placeholder"
    );

    for memory_only in [true, false] {
        let got = atlas_core::thumbs::probe_shell_cloud(&path, memory_only);
        let hydrated_after = !atlas_core::cloud::is_dehydrated(&path);
        println!(
            "MEMORYONLY={memory_only:<5} -> {:<26} hydrated_after={hydrated_after}",
            match got {
                Some((w, h, true)) => format!("{w}x{h} type icon"),
                Some((w, h, false)) => format!("{w}x{h} REAL THUMBNAIL"),
                None => "nothing".to_string(),
            }
        );
        if hydrated_after {
            println!("  !! that flag combination downloaded the file");
            break;
        }
    }
}
