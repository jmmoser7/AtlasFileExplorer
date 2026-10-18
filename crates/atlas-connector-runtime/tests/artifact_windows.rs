#![cfg(windows)]

use atlas_connector::{ArtifactTransfer, FrameError, MAX_FRAME_BYTES};
use atlas_connector_runtime::{
    artifact::{receive_artifact, transfer_address, ArtifactError, ArtifactSender, ArtifactStore},
    transport::{Cancellation, PipeAddress, PipeListener},
};
use serde_json::{json, Value};
use std::{
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(8);

fn bytes(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 251) as u8).collect()
}

fn stored(size: usize) -> (ArtifactStore, ArtifactTransfer) {
    let mut store = ArtifactStore::default();
    let descriptor = store
        .insert("binary-1", "application/octet-stream", bytes(size))
        .unwrap();
    (store, descriptor)
}

struct ChildPeer(Child);
impl ChildPeer {
    fn finish(&mut self) {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success(), "artifact peer failed: {status}");
                return;
            }
            assert!(Instant::now() < deadline, "artifact peer did not exit");
            thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for ChildPeer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn artifact_peer_process() {
    let Ok(token) = std::env::var("ATLAS_ARTIFACT_CHILD_TOKEN") else {
        return;
    };
    let parent: u32 = std::env::var("ATLAS_ARTIFACT_PARENT_PID")
        .unwrap()
        .parse()
        .unwrap();
    let control = PipeAddress::from_token(token).unwrap();
    let (store, descriptor) = stored(MAX_FRAME_BYTES + 37);
    let sender = ArtifactSender::bind(
        &control,
        descriptor.clone(),
        store.get(&descriptor.transfer_id).unwrap(),
        parent,
    )
    .unwrap();
    sender.send(WAIT, &Cancellation::default()).unwrap();
}

#[test]
fn separate_process_binary_transfer_crosses_chunk_boundary_and_verifies_hash() {
    let control = PipeAddress::fresh().unwrap();
    let (_, descriptor) = stored(MAX_FRAME_BYTES + 37);
    let mut child = ChildPeer(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "artifact_peer_process", "--nocapture"])
            .env("ATLAS_ARTIFACT_CHILD_TOKEN", control.token())
            .env("ATLAS_ARTIFACT_PARENT_PID", std::process::id().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let received = receive_artifact(
        &control,
        &descriptor,
        child.0.id(),
        descriptor.byte_length as usize,
        WAIT,
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(received, bytes(MAX_FRAME_BYTES + 37));
    child.finish();
}

fn malicious_transfer(case: &str) -> ArtifactError {
    let control = PipeAddress::fresh().unwrap();
    let (_, descriptor) = stored(3);
    let listener = PipeListener::bind(
        transfer_address(&control, &descriptor).unwrap(),
        std::process::id(),
    )
    .unwrap();
    let sent = descriptor.clone();
    let case = case.to_owned();
    let server = thread::spawn(move || {
        let cancel = Cancellation::default();
        let mut pipe = listener.accept(WAIT, &cancel).unwrap();
        let mut announced = sent.clone();
        if case == "descriptor" {
            announced.media_type = "image/png".into();
        }
        pipe.send(&announced, WAIT, &cancel).unwrap();
        match case.as_str() {
            "descriptor" => {}
            "hash" => {
                pipe.send_bytes(&[9, 9, 9], WAIT, &cancel).unwrap();
                pipe.send(
                    &json!({"transfer_id":sent.transfer_id,"complete":true}),
                    WAIT,
                    &cancel,
                )
                .unwrap();
            }
            "oversized" => pipe.send_bytes(&[0, 1, 2, 3], WAIT, &cancel).unwrap(),
            "truncated" => {
                pipe.send_bytes(&[0, 1], WAIT, &cancel).unwrap();
            }
            "completion" => {
                pipe.send_bytes(&[0, 1, 2], WAIT, &cancel).unwrap();
                pipe.send(
                    &json!({"transfer_id":"wrong-transfer","complete":true}),
                    WAIT,
                    &cancel,
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        if case == "truncated" {
            // The receiver needs EOF; let it finish reading the complete first
            // chunk before closing so the test specifically exercises truncation.
            thread::sleep(Duration::from_millis(50));
            pipe.close();
        } else {
            let _ = pipe.recv::<Value>(WAIT, &cancel);
        }
    });
    let result = receive_artifact(
        &control,
        &descriptor,
        std::process::id(),
        3,
        WAIT,
        &Cancellation::default(),
    );
    server.join().unwrap();
    result.unwrap_err()
}

#[test]
fn hash_mismatch_never_returns_pixels_or_verified_receipt() {
    assert!(matches!(
        malicious_transfer("hash"),
        ArtifactError::HashMismatch
    ));
}

#[test]
fn oversized_payload_chunk_is_refused_before_allocation() {
    assert!(matches!(
        malicious_transfer("oversized"),
        ArtifactError::Frame(FrameError::TooLarge {
            length: 4,
            limit: 3
        })
    ));
}

#[test]
fn descriptor_and_completion_mismatches_are_refused() {
    assert!(matches!(
        malicious_transfer("descriptor"),
        ArtifactError::DescriptorMismatch
    ));
    assert!(matches!(
        malicious_transfer("completion"),
        ArtifactError::InvalidCompletion
    ));
}

#[test]
fn truncated_transfer_is_not_a_successful_short_artifact() {
    assert!(matches!(
        malicious_transfer("truncated"),
        ArtifactError::UnexpectedEnd
    ));
}

#[test]
fn accept_timeout_and_cancellation_cover_the_whole_transfer() {
    let control = PipeAddress::fresh().unwrap();
    let (store, descriptor) = stored(3);
    let sender = ArtifactSender::bind(
        &control,
        descriptor.clone(),
        store.get(&descriptor.transfer_id).unwrap(),
        std::process::id(),
    )
    .unwrap();
    let result = sender.send(Duration::from_millis(25), &Cancellation::default());
    assert!(
        matches!(result, Err(ArtifactError::Io(e)) if e.kind() == std::io::ErrorKind::TimedOut)
    );
    let control = PipeAddress::fresh().unwrap();
    let sender = ArtifactSender::bind(
        &control,
        descriptor.clone(),
        store.get(&descriptor.transfer_id).unwrap(),
        std::process::id(),
    )
    .unwrap();
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(
        matches!(sender.send(WAIT, &cancel), Err(ArtifactError::Io(e)) if e.kind() == std::io::ErrorKind::ConnectionAborted)
    );
}
