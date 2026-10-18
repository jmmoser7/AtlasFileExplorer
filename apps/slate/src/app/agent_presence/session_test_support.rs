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
    atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &session).unwrap();
    h.app.presence.sources.expect(dir, &session.request);
    loop {
        h.app.presence_pump(&h.ctx);
        if observed(h) {
            break;
        }
        assert!(Instant::now() < deadline, "helper session was not observed");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
