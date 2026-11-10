use super::*;
use atlas_agent::{
    artifact_tools::{transport::PendingArtifactCalls, ArtifactIdentity, ArtifactOperation},
    helper::ArtifactToolsLaunch,
    InputSnapshot,
};
use serde_json::json;
use std::{collections::HashMap, time::Duration};

struct Fixture {
    root: PathBuf,
    link: PathBuf,
    request: AgentRequest,
    launch: ArtifactToolsLaunch,
    stop: Arc<AtomicBool>,
    current: Arc<Mutex<String>>,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("artifact-broker-{}", atlas_agent::request_id()));
        std::fs::create_dir(&root).unwrap();
        let run = crate::versions::prepare_artifact_run(&root, "family", "run", None).unwrap();
        let link = root.join("link");
        std::fs::create_dir(&link).unwrap();
        let inputs = crate::versions::freeze_artifact_inputs(
            &run,
            &InputSnapshot::default(),
            &HashMap::new(),
        )
        .unwrap()
        .0;
        let request = AgentRequest {
            id: run.run_id.clone(),
            output_dir: Some(run.staging_output.to_string_lossy().into_owned()),
            artifact: Some(run),
            inputs,
            ..Default::default()
        };
        let launch =
            ArtifactToolsLaunch::artifact(root.join("slate.exe"), &link, &request).unwrap();
        Self {
            root,
            link,
            launch,
            stop: Arc::new(AtomicBool::new(false)),
            current: Arc::new(Mutex::new(request.id.clone())),
            request,
        }
    }
    fn broker(&self) -> ArtifactBroker {
        ArtifactBroker::start(
            &self.launch,
            &self.request,
            &self.link,
            self.stop.clone(),
            self.current.clone(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn response(pending: &mut PendingArtifactCalls) -> atlas_agent::artifact_tools::ArtifactReply {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(done) = pending
            .poll(&AtomicBool::new(false), Some(|_| false))
            .into_iter()
            .next()
        {
            return done.reply;
        }
        assert!(
            Instant::now() < deadline,
            "live parent did not answer bounded callback"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn production_parent_callback_writes_only_current_owned_stage_and_rejects_replay() {
    let f = Fixture::new();
    let broker = f.broker();
    let mut pending = PendingArtifactCalls::default();
    let dir = f.launch.transport_dir();
    let identity = ArtifactIdentity::from_request(&f.request).unwrap();
    pending
        .start(
            json!(1),
            dir.clone(),
            identity.clone(),
            ArtifactOperation::ReadContext,
        )
        .unwrap();
    assert!(response(&mut pending).success);
    pending
        .start(
            json!(2),
            dir.clone(),
            identity.clone(),
            ArtifactOperation::WriteFile {
                path: "index.html".into(),
                content: "<html><button>Toggle</button></html>".into(),
            },
        )
        .unwrap();
    assert!(response(&mut pending).success);
    let entry = f
        .request
        .artifact
        .as_ref()
        .unwrap()
        .staging_output
        .join("index.html");
    assert!(std::fs::read_to_string(&entry).unwrap().contains("Toggle"));
    let invalid = ArtifactCall {
        id: "call-replay".into(),
        identity,
        sequence: 1,
        operation: ArtifactOperation::WriteFile {
            path: "index.html".into(),
            content: "REPLAY".into(),
        },
    };
    write_packet(&dir, "artifact-calls", &invalid.id, &invalid).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    let reply = loop {
        if let Some(value) = read_json(
            &dir.join("artifact-replies/call-replay.json"),
            Some(|_| false),
        ) {
            break value;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(reply["success"], false);
    assert!(!std::fs::read_to_string(&entry).unwrap().contains("REPLAY"));
    drop(broker);
    assert!(!dir.join("artifact-session.json").exists());
}
#[test]
fn stopped_or_replaced_parent_revokes_scope_without_publishing_or_writing() {
    for replace in [false, true] {
        let f = Fixture::new();
        let broker = f.broker();
        if replace {
            *f.current.lock().unwrap() = "new-request".into();
        } else {
            f.stop.store(true, Ordering::Release);
        }
        let dir = f.launch.transport_dir();
        let until = Instant::now() + Duration::from_secs(5);
        while dir.join("artifact-session.json").exists() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!f
            .request
            .artifact
            .as_ref()
            .unwrap()
            .staging_output
            .join("index.html")
            .exists());
        drop(broker);
        assert!(ArtifactBroker::start(
            &f.launch,
            &f.request,
            &f.link,
            f.stop.clone(),
            f.current.clone()
        )
        .is_err());
    }
}

#[test]
fn max_artifact_parent_retains_captured_end_and_binds_budget_to_the_validated_request() {
    let mut f = Fixture::new();
    f.request
        .model_parameters
        .insert("effort".into(), "max".into());
    f.launch = ArtifactToolsLaunch::artifact_until(
        f.root.join("slate.exe"),
        &f.link,
        &f.request,
        atlas_agent::collaborator::host_tools::deadline_after(1).unwrap(),
    )
    .unwrap();
    assert_eq!(f.launch.identity.budget_seconds, 1800);
    let expires = f.launch.expires;
    std::thread::sleep(Duration::from_millis(30));
    let broker = f.broker();
    let dir = f.launch.transport_dir();
    assert_eq!(
        read_json(&dir.join("artifact-session.json"), Some(|_| false)).unwrap()["budget_seconds"],
        1800
    );
    assert_eq!(f.launch.expires, expires);
    let until = expires + Duration::from_secs(1);
    while dir.join("artifact-session.json").exists() {
        assert!(Instant::now() < until, "parent reset the original deadline");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!f
        .request
        .artifact
        .as_ref()
        .unwrap()
        .staging_output
        .join("index.html")
        .exists());
    drop(broker);
    assert!(ArtifactBroker::start(
        &f.launch,
        &f.request,
        &f.link,
        f.stop.clone(),
        f.current.clone()
    )
    .is_err());
    let default = Fixture::new();
    let mut forged = default.launch.clone();
    forged.identity.budget_seconds = 1800;
    forged.arguments[2] = serde_json::to_string(&forged.identity).unwrap();
    assert!(ArtifactBroker::start(
        &forged,
        &default.request,
        &default.link,
        default.stop.clone(),
        default.current.clone()
    )
    .is_err());
}
