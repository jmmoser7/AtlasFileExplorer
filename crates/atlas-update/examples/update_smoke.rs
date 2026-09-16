//! Headless CI fixture for the real installer/upgrade test. Never distributed.
fn main() {
    let _guard = atlas_update::startup().expect("installation guard");
    let exe = std::env::current_exe().unwrap();
    let current = exe.parent().unwrap();
    let root = current.parent().unwrap();
    let args: Vec<_> = std::env::args().skip(1).collect();
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
