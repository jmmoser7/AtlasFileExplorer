#![cfg(windows)]

use atlas_connector::{FrameError, MAX_FRAME_BYTES};
use atlas_connector_runtime::transport::{Cancellation, PipeAddress, PipeConnection, PipeListener};
use serde_json::{json, Value};
use std::{
    fs::File,
    io::{self, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE},
        Storage::FileSystem::{
            CreateFileW, FILE_SHARE_MODE, OPEN_EXISTING, SECURITY_IDENTIFICATION,
            SECURITY_SQOS_PRESENT,
        },
    },
};

const WAIT: Duration = Duration::from_secs(3);

fn pair() -> (PipeConnection, PipeConnection) {
    let address = PipeAddress::fresh().unwrap();
    let listener = PipeListener::bind(address.clone(), std::process::id()).unwrap();
    let client =
        PipeConnection::connect(address, std::process::id(), WAIT, &Cancellation::default())
            .unwrap();
    let server = listener.accept(WAIT, &Cancellation::default()).unwrap();
    (server, client)
}

fn raw_client(address: &PipeAddress) -> File {
    let name: Vec<u16> = address
        .pipe_name()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            PCWSTR(name.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            SECURITY_IDENTIFICATION | SECURITY_SQOS_PRESENT,
            None,
        )
    }
    .unwrap();
    // SAFETY: CreateFileW transferred one owned handle; File closes it exactly once.
    unsafe { File::from_raw_handle(handle.0) }
}

struct PeerProcess(Child);

impl PeerProcess {
    fn spawn(address: &PipeAddress, scenario: &str) -> Self {
        Self(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "pipe_peer_process", "--nocapture"])
                .env("ATLAS_CONNECTOR_PIPE_CHILD", scenario)
                .env("ATLAS_CONNECTOR_PIPE_TOKEN", address.token())
                .env(
                    "ATLAS_CONNECTOR_PIPE_PARENT",
                    std::process::id().to_string(),
                )
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }

    fn finish(&mut self) {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success(), "pipe peer process failed: {status}");
                return;
            }
            assert!(
                Instant::now() < deadline,
                "pipe peer process failed to stop"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for PeerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The test executable itself is the cooperating external process. Ordinary
/// workspace invocation leaves this inert; spawned cases select it by name.
#[test]
fn pipe_peer_process() {
    let Ok(scenario) = std::env::var("ATLAS_CONNECTOR_PIPE_CHILD") else {
        return;
    };
    if scenario == "idle" {
        thread::sleep(WAIT);
        return;
    }
    let address =
        PipeAddress::from_token(std::env::var("ATLAS_CONNECTOR_PIPE_TOKEN").unwrap()).unwrap();
    let parent: u32 = std::env::var("ATLAS_CONNECTOR_PIPE_PARENT")
        .unwrap()
        .parse()
        .unwrap();
    let mut client =
        PipeConnection::connect(address, parent, WAIT, &Cancellation::default()).unwrap();
    let _ = client.send(
        &json!({"child_pid":std::process::id()}),
        WAIT,
        &Cancellation::default(),
    );
    let result = client.recv::<Value>(WAIT, &Cancellation::default());
    if scenario == "roundtrip" {
        assert_eq!(result.unwrap(), Some(json!({"accepted":true})));
    }
}

#[test]
fn separate_process_roundtrip_verifies_os_identity() {
    let address = PipeAddress::fresh().unwrap();
    // Child waits for this independently named endpoint; its actual spawned PID
    // is the expected peer, rather than a PID/pack identity claimed in JSON.
    let mut child = PeerProcess::spawn(&address, "roundtrip");
    let listener = PipeListener::bind(address, child.0.id()).unwrap();
    let mut server = listener.accept(WAIT, &Cancellation::default()).unwrap();
    assert_eq!(server.peer().process_id, child.0.id());
    assert!(server.peer().user_sid.starts_with("S-1-"));
    assert!(server.peer().created_at_100ns > 0);
    let message: Value = server
        .recv(WAIT, &Cancellation::default())
        .unwrap()
        .unwrap();
    assert_eq!(message["child_pid"], child.0.id());
    server
        .send(&json!({"accepted":true}), WAIT, &Cancellation::default())
        .unwrap();
    child.finish();
}

#[test]
fn unexpected_client_process_is_rejected() {
    let address = PipeAddress::fresh().unwrap();
    let listener = PipeListener::bind(address.clone(), std::process::id()).unwrap();
    let mut child = PeerProcess::spawn(&address, "rejected");
    let result = listener.accept(WAIT, &Cancellation::default());
    assert!(matches!(result, Err(e) if e.kind() == io::ErrorKind::PermissionDenied));
    child.finish();
}

#[test]
fn unexpected_server_process_is_rejected() {
    let address = PipeAddress::fresh().unwrap();
    let _listener = PipeListener::bind(address.clone(), std::process::id()).unwrap();
    let child = PeerProcess::spawn(&address, "idle");
    let result = PipeConnection::connect(address, child.0.id(), WAIT, &Cancellation::default());
    assert!(matches!(result, Err(e) if e.kind() == io::ErrorKind::PermissionDenied));
}

#[test]
fn first_instance_guard_and_stale_address() {
    let old = PipeAddress::fresh().unwrap();
    let listener = PipeListener::bind(old.clone(), std::process::id()).unwrap();
    assert!(PipeListener::bind(old.clone(), std::process::id()).is_err());
    drop(listener);
    let fresh = PipeAddress::fresh().unwrap();
    assert_ne!(old, fresh);
    let _fresh_listener = PipeListener::bind(fresh, std::process::id()).unwrap();
    let result = PipeConnection::connect(
        old,
        std::process::id(),
        Duration::from_millis(30),
        &Cancellation::default(),
    );
    assert!(matches!(result, Err(e) if e.kind() == io::ErrorKind::TimedOut));
}

#[test]
fn oversized_wire_header_is_rejected_before_body_allocation() {
    let address = PipeAddress::fresh().unwrap();
    let listener = PipeListener::bind(address.clone(), std::process::id()).unwrap();
    let mut client = raw_client(&address);
    let mut server = listener.accept(WAIT, &Cancellation::default()).unwrap();
    client
        .write_all(&((MAX_FRAME_BYTES + 1) as u32).to_le_bytes())
        .unwrap();
    assert!(matches!(
        server.recv::<Value>(WAIT, &Cancellation::default()),
        Err(FrameError::TooLarge { .. })
    ));
    assert!(!server.is_open());
}

#[test]
fn truncated_frame_and_clean_disconnect_are_distinct() {
    let address = PipeAddress::fresh().unwrap();
    let listener = PipeListener::bind(address.clone(), std::process::id()).unwrap();
    let mut client = raw_client(&address);
    let mut server = listener.accept(WAIT, &Cancellation::default()).unwrap();
    client.write_all(&100u32.to_le_bytes()).unwrap();
    client.write_all(b"{\"a\":1}").unwrap();
    drop(client);
    assert!(matches!(
        server.recv::<Value>(WAIT, &Cancellation::default()),
        Err(FrameError::Truncated { .. })
    ));
    assert!(!server.is_open());
    let (mut server, client) = pair();
    drop(client);
    assert_eq!(
        server
            .recv::<Value>(WAIT, &Cancellation::default())
            .unwrap(),
        None
    );
    assert!(!server.is_open());
}

#[test]
fn accept_read_and_backpressured_write_have_deadlines() {
    let address = PipeAddress::fresh().unwrap();
    let listener = PipeListener::bind(address, std::process::id()).unwrap();
    assert!(
        matches!(listener.accept(Duration::from_millis(25), &Cancellation::default()),
        Err(e) if e.kind() == io::ErrorKind::TimedOut)
    );
    let (mut server, _client) = pair();
    let started = Instant::now();
    assert!(
        matches!(server.recv::<Value>(Duration::from_millis(25), &Cancellation::default()),
        Err(FrameError::Io(e)) if e.kind() == io::ErrorKind::TimedOut)
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(!server.is_open());
    let (mut server, _client) = pair();
    assert!(
        matches!(server.send(&"x".repeat(1024 * 1024), Duration::from_millis(25), &Cancellation::default()),
        Err(FrameError::Io(e)) if e.kind() == io::ErrorKind::TimedOut)
    );
    assert!(!server.is_open());
}

#[test]
fn cancellation_aborts_pending_read_and_connection_teardown() {
    let (mut server, _client) = pair();
    let cancel = Cancellation::default();
    let signal = cancel.clone();
    let task = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        signal.cancel();
    });
    let started = Instant::now();
    assert!(matches!(server.recv::<Value>(WAIT, &cancel),
        Err(FrameError::Io(e)) if e.kind() == io::ErrorKind::ConnectionAborted));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(!server.is_open());
    task.join().unwrap();
}

#[test]
fn cancellation_also_bounds_accept_connect_and_full_write() {
    let address = PipeAddress::fresh().unwrap();
    let listener = PipeListener::bind(address, std::process::id()).unwrap();
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(matches!(listener.accept(WAIT, &cancel),
        Err(e) if e.kind() == io::ErrorKind::ConnectionAborted));
    let address = PipeAddress::fresh().unwrap();
    assert!(
        matches!(PipeConnection::connect(address, std::process::id(), WAIT, &cancel),
        Err(e) if e.kind() == io::ErrorKind::ConnectionAborted)
    );
    let (mut server, _client) = pair();
    let cancel = Cancellation::default();
    let signal = cancel.clone();
    let task = thread::spawn(move || {
        thread::sleep(Duration::from_millis(25));
        signal.cancel();
    });
    assert!(
        matches!(server.send(&"x".repeat(1024 * 1024), WAIT, &cancel),
        Err(FrameError::Io(e)) if e.kind() == io::ErrorKind::ConnectionAborted)
    );
    assert!(!server.is_open());
    task.join().unwrap();
}

#[test]
fn raw_chunks_roundtrip_and_reject_receiver_limit() {
    let (mut server, mut client) = pair();
    let bytes = vec![0, 255, 128, 10];
    client
        .send_bytes(&bytes, WAIT, &Cancellation::default())
        .unwrap();
    assert_eq!(
        server
            .recv_bytes(4, WAIT, &Cancellation::default())
            .unwrap(),
        Some(bytes)
    );
    client
        .send_bytes(&[1, 2, 3], WAIT, &Cancellation::default())
        .unwrap();
    assert!(matches!(
        server.recv_bytes(2, WAIT, &Cancellation::default()),
        Err(FrameError::TooLarge {
            length: 3,
            limit: 2
        })
    ));
    assert!(!server.is_open());
}

#[test]
fn kernel_pipe_has_protected_current_user_dacl() {
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::{LocalFree, HANDLE, HLOCAL},
            Security::{
                AclSizeInformation,
                Authorization::{ConvertSidToStringSidW, GetSecurityInfo, SE_KERNEL_OBJECT},
                GetAce, GetAclInformation, GetSecurityDescriptorControl, ACCESS_ALLOWED_ACE, ACL,
                ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
                SE_DACL_PROTECTED,
            },
        },
    };
    let address = PipeAddress::fresh().unwrap();
    let listener = PipeListener::bind(address.clone(), std::process::id()).unwrap();
    let client = raw_client(&address);
    let server = listener.accept(WAIT, &Cancellation::default()).unwrap();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let result = unsafe {
        GetSecurityInfo(
            HANDLE(client.as_raw_handle()),
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut dacl),
            None,
            Some(&mut descriptor),
        )
    };
    assert_eq!(result.0, 0);
    assert!(!dacl.is_null(), "a null DACL would grant everyone access");
    let (mut control, mut revision) = (0u16, 0);
    unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) }.unwrap();
    assert_ne!(control & SE_DACL_PROTECTED.0, 0);
    let mut information = ACL_SIZE_INFORMATION::default();
    unsafe {
        GetAclInformation(
            dacl,
            (&mut information as *mut ACL_SIZE_INFORMATION).cast(),
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    }
    .unwrap();
    assert_eq!(
        information.AceCount, 1,
        "only the current user may appear in the ACL"
    );
    let mut ace = std::ptr::null_mut();
    unsafe { GetAce(dacl, 0, &mut ace) }.unwrap();
    let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
    assert_eq!(allowed.Header.AceType, 0); // ACCESS_ALLOWED_ACE_TYPE
    let sid = PSID((&allowed.SidStart as *const u32).cast_mut().cast());
    let mut text = PWSTR::null();
    unsafe { ConvertSidToStringSidW(sid, &mut text) }.unwrap();
    assert_eq!(unsafe { text.to_string() }.unwrap(), server.peer().user_sid);
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
    }
}
