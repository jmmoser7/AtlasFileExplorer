use super::*;

#[test]
fn video_worker_discovery_prefers_its_neighbor_and_handles_cargo_test_output() {
    let temp = std::env::temp_dir().join(format!("atlas-video-worker-path-{}", std::process::id()));
    let target = temp.join("target");
    let profile = target.join("debug");
    let generated = profile.join("build/atlas-core/test-hash/out");
    std::fs::create_dir_all(&generated).unwrap();
    let direct = adjacent_worker(&profile);
    std::fs::write(&direct, []).unwrap();
    assert_eq!(adjacent_worker(&profile.join("deps")), direct);
    assert_eq!(adjacent_worker(&profile.join("examples")), direct);
    let expected_neighbor = generated.join(direct.file_name().unwrap());
    assert_eq!(adjacent_worker(&generated), direct);
    let cross_profile = target.join("x86_64-pc-windows-msvc/release");
    let cross_test = cross_profile.join("build/atlas-core/test-hash/out");
    assert_eq!(
        adjacent_worker(&cross_test),
        cross_profile.join(direct.file_name().unwrap())
    );
    let custom = temp.join("build-cache");
    let custom_profile = custom.join("release");
    let custom_test = custom_profile.join("build/atlas-core/test-hash/out");
    std::fs::create_dir_all(&custom_test).unwrap();
    assert_eq!(
        adjacent_worker(&custom_test),
        custom_test.join(direct.file_name().unwrap())
    );
    std::fs::write(custom.join(".rustc_info.json"), "{}").unwrap();
    assert_eq!(
        adjacent_worker(&custom_test),
        custom_profile.join(direct.file_name().unwrap())
    );
    std::fs::write(&expected_neighbor, []).unwrap();
    assert_eq!(adjacent_worker(&generated), expected_neighbor);
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn absent_worker_launch_returns_error_instead_of_poisoning_process_owner() {
    let mut command = Command::new("C:/atlas-deliberately-missing/video-worker.exe");
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let job = Job::Strip {
        id: 1,
        token: 1,
        path: "clip.mp4".into(),
        start: 0.0,
        end: None,
    };
    assert!(Decoder::with_command(command, job).is_err());
}
