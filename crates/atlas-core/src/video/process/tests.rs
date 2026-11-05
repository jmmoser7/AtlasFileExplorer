use super::*;

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
