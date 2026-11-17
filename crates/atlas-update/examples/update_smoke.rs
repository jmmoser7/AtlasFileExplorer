//! Headless CI fixture for the real installer/upgrade test. Never distributed.
fn main() {
    let _guard = atlas_update::startup().expect("installation guard");
    let exe = std::env::current_exe().unwrap();
    let current = exe.parent().unwrap();
    let root = current.parent().unwrap();
    let args: Vec<_> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if let [command, releases, previous, version] = args.as_slice() {
        if command == "--stage-stable-upgrade" {
            stage_stable_upgrade(releases, previous, version);
            return;
        }
    }
    if args.len() == 2 {
        std::fs::write(&args[1], "running").unwrap();
        let stop = std::path::Path::new(&args[0]);
        let start = std::time::Instant::now();
        while !stop.exists() && start.elapsed().as_secs() < 120 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    } else if let Ok(version) = std::fs::read(current.join("smoke-version.txt")) {
        std::fs::write(root.join("restarted-version.txt"), version).unwrap();
    }
}

#[cfg(windows)]
fn stage_stable_upgrade(releases: &str, previous: &str, version: &str) {
    use velopack::{sources::FileSource, UpdateCheck, UpdateManager, UpdateOptions};

    // The candidate is private until the smoke passes. Exercise Velopack's
    // normal discovery and download from its unmodified local release feed.
    let manager = UpdateManager::new(
        FileSource::new(releases),
        Some(UpdateOptions {
            ExplicitChannel: Some("stable".into()),
            MaximumDeltasBeforeFallback: -1,
            ..Default::default()
        }),
        None,
    )
    .expect("locate installed stable suite");
    assert_eq!(manager.get_app_id(), "AtlasSuite");
    assert_eq!(manager.get_current_version_as_string(), previous);
    let UpdateCheck::UpdateAvailable(info) = manager.check_for_updates().expect("read stable feed")
    else {
        panic!("expected a stable update from {previous} to {version}");
    };
    assert_eq!(info.TargetFullRelease.PackageId, manager.get_app_id());
    assert_eq!(info.TargetFullRelease.Version, version);
    assert_eq!(
        info.TargetFullRelease.FileName,
        format!("AtlasSuite-{version}-stable-full.nupkg"),
    );
    manager
        .download_updates(&info, None)
        .expect("stage the stable full update");
}
