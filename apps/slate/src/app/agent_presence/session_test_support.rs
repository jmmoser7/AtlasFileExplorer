//! One bounded session publication/poll mechanism, with explicit fixture admission.
use crate::app::tests::Harness;
use atlas_agent::AgentSession;
use std::{path::Path, time::Instant};

pub(in crate::app::agent_presence) fn publish_and_observe(
    h: &mut Harness,
    who: &str,
    session: &AgentSession,
) {
    publish_and_observe_until(h, who, session, |h| session_observed(h, who, session));
}

pub(in crate::app::agent_presence) fn publish_and_observe_until(
    h: &mut Harness,
    who: &str,
    session: &AgentSession,
    observed: impl Fn(&Harness) -> bool,
) {
    let dir = session_directory(h, who);
    let deadline = Instant::now() + std::time::Duration::from_secs(15);
    // A backend answering a real send must first receive that exact request.
    super::delivery::wait_request_until(&dir.join("request.json"), &session.request, deadline);
    publish_session_until(h, &dir, session, deadline, observed);
}

/// Declared visual fixtures fabricate a session identity without sending a task.
/// Their public observations must never manufacture an outgoing provider request.
pub(in crate::app::agent_presence) fn publish_synthetic_and_observe(
    h: &mut Harness,
    who: &str,
    session: &AgentSession,
) {
    publish_synthetic_and_observe_until(h, who, session, |h| session_observed(h, who, session));
}

pub(in crate::app::agent_presence) fn publish_synthetic_and_observe_until(
    h: &mut Harness,
    who: &str,
    session: &AgentSession,
    observed: impl Fn(&Harness) -> bool,
) {
    let dir = session_directory(h, who);
    assert!(
        !dir.join("request.json").exists(),
        "synthetic fixture sent a task"
    );
    // Only a test-owned synthetic backend creates this directory; no request
    // worker has been asked to deliver anything for a declared observation.
    std::fs::create_dir_all(&dir).unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(15);
    publish_session_until(h, &dir, session, deadline, observed);
    assert!(
        !dir.join("request.json").exists(),
        "synthetic observation sent a task"
    );
}

fn session_directory(h: &Harness, who: &str) -> std::path::PathBuf {
    h.app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == who)
        .unwrap()
        .dir
        .clone()
}

fn session_observed(h: &Harness, who: &str, session: &AgentSession) -> bool {
    h.app
        .presence
        .agents
        .iter()
        .any(|a| a.profile.id == who && a.snapshot.as_deref().is_some_and(|s| s == session))
}

fn publish_session_until(
    h: &mut Harness,
    dir: &Path,
    session: &AgentSession,
    deadline: Instant,
    observed: impl Fn(&Harness) -> bool,
) {
    let path = dir.join("session.json");
    let previous_mtime = std::fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .ok();
    atlas_ai::agent::atomic_write_json(&path, &session).unwrap();
    if let Some(previous_mtime) = previous_mtime {
        // The real file poller keys observations by mtime. Give rapid fixture
        // revisions distinct timestamps even on coarse-resolution filesystems.
        let modified =
            std::time::SystemTime::now().max(previous_mtime + std::time::Duration::from_secs(1));
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }
    h.app.presence.sources.expect(dir, &session.request);
    loop {
        h.app.presence_pump(&h.ctx);
        if observed(h) {
            break;
        }
        if Instant::now() >= deadline {
            let expected = serde_json::to_value(session).unwrap();
            let actual = h
                .app
                .presence
                .agents
                .iter()
                .find(|a| a.dir == dir)
                .and_then(|a| a.snapshot.as_deref())
                .map(|s| serde_json::to_value(s).unwrap())
                .unwrap_or_default();
            let differing = |value: &serde_json::Value| {
                expected
                    .as_object()
                    .unwrap()
                    .keys()
                    .filter(|field| expected[field.as_str()] != value[field.as_str()])
                    .cloned()
                    .collect::<Vec<_>>()
            };
            let saved = std::fs::read(dir.join("session.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            panic!("helper session was not observed; expected update {}, activity {:?}; snapshot fields {:?}; disk fields {:?}",
                session.updated_at, session.activity.last().map(|e| (e.at, e.text.chars().count())), differing(&actual), differing(&saved));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
