//! One readiness reader shared by both subscription clients.
use super::*;
use atlas_agent::helper::HelperLaunch;

pub fn helper_tool_aliases(launch: &HelperLaunch) -> Vec<String> {
    match launch {
        HelperLaunch::Model(model) => tool_aliases(&model.session, &model.request_id).to_vec(),
        HelperLaunch::Artifact(artifact) => super::artifact::aliases(&artifact.identity),
    }
}
pub fn helper_initialized(launch: &HelperLaunch) -> bool {
    state(launch, false)
}
pub fn helper_ready(launch: &HelperLaunch) -> bool {
    state(launch, true)
}
fn state(launch: &HelperLaunch, listed: bool) -> bool {
    if launch.remaining_time().is_err() {
        return false;
    }
    let (dir, expected) = match launch {
        HelperLaunch::Model(model) => {
            let Some(root) = model.arguments.get(1) else {
                return false;
            };
            let root = Path::new(root);
            (
                model.session.transport_dir(root),
                json!({"session":model.session,"request_id":model.request_id}),
            )
        }
        HelperLaunch::Artifact(artifact) => (
            artifact.transport_dir(),
            json!({"identity":artifact.identity,"request_id":artifact.identity.request_id}),
        ),
    };
    // These files are host-owned mailbox metadata, never model-provided paths.
    let guard = Some((|_: &Path| false) as CloudGuard);
    let bound = match launch {
        HelperLaunch::Model(model) => {
            transport::binding_matches(&dir, &model.session, &model.request_id, guard)
        }
        HelperLaunch::Artifact(artifact) => {
            atlas_agent::artifact_tools::transport::binding_matches(&dir, &artifact.identity, guard)
        }
    };
    let deadline = match launch {
        HelperLaunch::Model(model) => model.deadline_unix_ms,
        HelperLaunch::Artifact(artifact) => artifact.deadline_unix_ms,
    };
    bound
        && transport::read_json(&dir.join("helper-ready.json"), guard).is_some_and(|value| {
            expected
                .as_object()
                .unwrap()
                .iter()
                .all(|(key, expected)| value.get(key) == Some(expected))
                && value["initialized"] == true
                && (!listed || value["tools_listed"] == true)
                && value["tool_names"] == json!(helper_tool_aliases(launch))
                && value["deadline_unix_ms"] == json!(deadline)
        })
}
