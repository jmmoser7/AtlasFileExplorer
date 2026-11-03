//! Public image dialogue uses submitted words and real bundle outputs. Some
//! image APIs produce no assistant text; never fabricate a model reply for them.
use crate::agent::{AgentRequest, AgentSession};

pub(super) fn prepare_request(request: &mut AgentRequest) {
    if request.image.is_some() && request.image_history.is_some() {
        request.oneshot = true;
        if request.image_history == Some(false) {
            request.history.clear();
            request.image_history_prefix = None;
        }
    }
}

pub(super) fn begin(
    request: &AgentRequest,
    session: &mut AgentSession,
) -> Option<RetainedImageHistory> {
    if request.image.is_some() {
        match request.image_history {
            Some(false) => {
                return Some(RetainedImageHistory {
                    turns: std::mem::take(&mut session.turns),
                    conversation: std::mem::take(&mut session.conversation),
                })
            }
            Some(true) => {
                session.turns = request.history.clone();
                session.conversation.clear();
                return None;
            }
            None => {}
        }
    }
    if request.oneshot {
        session.turns.clear();
    }
    if session.turns.is_empty() {
        session.turns = request.history.clone();
    }
    None
}

pub(super) struct RetainedImageHistory {
    pub(super) turns: Vec<atlas_agent::AgentTurn>,
    pub(super) conversation: String,
}

/// Borrow the ordinary session manifest, projecting only its public dialogue
/// during an OFF image run. No bundle or transcript is cloned per progress tick.
pub(super) fn write_public(
    path: &std::path::Path,
    state: &AgentSession,
    retained: Option<&RetainedImageHistory>,
) -> std::io::Result<()> {
    let Some(retained) = retained else {
        return crate::agent::atomic_write_json(path, state);
    };
    #[derive(serde::Serialize)]
    struct Public<'a> {
        approval: &'a Option<atlas_agent::AgentApproval>,
        conversation: &'a str,
        artifacts: &'a [atlas_agent::AgentArtifact],
        status: &'a atlas_agent::AgentStatus,
        provider: &'a str,
        turns: &'a [atlas_agent::AgentTurn],
        #[serde(skip_serializing_if = "slice_empty")]
        activity: &'a [atlas_agent::AgentActivity],
        updated_at: u64,
        bundle: &'a atlas_agent::ImageBundle,
        request: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<u64>,
    }
    fn slice_empty<T>(value: &&[T]) -> bool {
        value.is_empty()
    }
    crate::agent::atomic_write_json(
        path,
        &Public {
            approval: &state.approval,
            conversation: &retained.conversation,
            artifacts: &state.artifacts,
            status: &state.status,
            provider: &state.provider,
            turns: &retained.turns,
            activity: &state.activity,
            updated_at: state.updated_at,
            bundle: &state.bundle,
            request: &state.request,
            usage: state.usage,
        },
    )
}

/// Called once after the existing submission ledger admits this request.
/// Codex owns its actual user/assistant stream; OpenAI Images and Comfy do not.
pub(super) fn record_submitted_user(
    request: &AgentRequest,
    session: &mut AgentSession,
    leaf_records_user: bool,
) -> bool {
    if request.image.is_none() || request.image_history != Some(true) || leaf_records_user {
        return false;
    }
    session.turns.push(atlas_agent::AgentTurn {
        role: "user".into(),
        text: request.prompt.clone(),
        at: request.at,
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn turn(text: &str) -> atlas_agent::AgentTurn {
        atlas_agent::AgentTurn {
            role: "user".into(),
            text: text.into(),
            at: 1,
        }
    }
    #[test]
    fn off_manifest_keeps_on_dialogue_and_real_outputs_across_success_failure_and_stop() {
        let folder = std::env::temp_dir().join(atlas_agent::request_id());
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("session.json");
        for provider in ["openai-image", "comfy", "codex-image"] {
            for outcome in [
                atlas_agent::AgentStatus::Idle,
                atlas_agent::AgentStatus::Error("Provider failed".into()),
                atlas_agent::AgentStatus::Error("Stopped".into()),
            ] {
                let mut request:AgentRequest=serde_json::from_value(serde_json::json!({"id":"off-run","prompt":"OFF question","at":2,"image":{},"image_history":false})).unwrap();
                let original = vec![turn("inherited"), turn("own ON question")];
                let mut state = AgentSession {
                    provider: provider.into(),
                    conversation: "ON-provider-thread".into(),
                    turns: original.clone(),
                    ..Default::default()
                };
                request.history = vec![turn("must never reach OFF provider")];
                prepare_request(&mut request);
                assert!(request.history.is_empty() && request.oneshot);
                assert!(!request.prompt_with_history().contains("must never reach"));
                let retained = begin(&request, &mut state).unwrap();
                assert!(state.turns.is_empty());
                // The first Thinking snapshot is persisted before leaf startup.
                // Recovery at that exact boundary must retain prior ON dialogue.
                state.request = request.id.clone();
                state.status = atlas_agent::AgentStatus::Thinking;
                write_public(&path, &state, Some(&retained)).unwrap();
                let initial: AgentSession =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                assert_eq!(initial.turns, original);
                assert_eq!(initial.conversation, "ON-provider-thread");
                assert_eq!(initial.request, "off-run");
                assert_eq!(initial.status, atlas_agent::AgentStatus::Thinking);
                assert!(!record_submitted_user(
                    &request,
                    &mut state,
                    provider == "codex-image"
                ));
                // A real leaf may emit working turns. None become saved ON dialogue.
                state.turns.push(turn("working OFF turn"));
                state.conversation = "OFF-provider-thread".into();
                state.request = request.id.clone();
                state.status = outcome.clone();
                state.bundle.images.push(atlas_agent::ImageOutput {
                    id: "actual-off-output".into(),
                    request: request.id.clone(),
                    source: "returned.png".into(),
                    prompt: request.prompt.clone(),
                    model: String::new(),
                    task: String::new(),
                    live: String::new(),
                });
                write_public(&path, &state, Some(&retained)).unwrap();
                let saved: AgentSession =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                assert_eq!(saved.turns, original);
                assert_eq!(saved.status, outcome);
                assert_eq!(saved.conversation, "ON-provider-thread");
                assert_eq!(saved.bundle.images[0].id, "actual-off-output");
                assert_eq!(saved.request, "off-run");
                let mut next = request.clone();
                next.image_history = Some(true);
                next.history = saved.turns.clone();
                prepare_request(&mut next);
                let mut resumed = saved;
                assert!(begin(&next, &mut resumed).is_none());
                assert_eq!(resumed.turns, original);
            }
        }
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }
    #[test]
    fn none_policy_keeps_legacy_request_and_manifest_serialization() {
        let mut request: AgentRequest = serde_json::from_value(
            serde_json::json!({"id":"legacy","prompt":"same","at":1,"image":{}}),
        )
        .unwrap();
        let bytes = serde_json::to_vec(&request).unwrap();
        prepare_request(&mut request);
        assert_eq!(serde_json::to_vec(&request).unwrap(), bytes);
        assert!(!String::from_utf8(bytes).unwrap().contains("image_history"));
        let mut state = AgentSession {
            turns: vec![turn("legacy visible")],
            ..Default::default()
        };
        let before = state.clone();
        assert!(begin(&request, &mut state).is_none());
        assert_eq!(state, before);
        assert!(!record_submitted_user(&request, &mut state, false));
    }
    #[test]
    fn actual_image_leaf_composition_keeps_inherited_and_own_words_without_fake_reply() {
        for provider in ["openai-image", "comfy", "codex-image"] {
            let mut request: AgentRequest = serde_json::from_value(serde_json::json!({
                "id":"image-b", "provider":provider, "prompt":"  current visible question  ",
                "at":42, "image":{}, "oneshot":true, "image_history":true
            }))
            .unwrap();
            let inherited = atlas_agent::AgentTurn {
                role: "user".into(),
                text: "inherited".into(),
                at: 1,
            };
            let own = atlas_agent::AgentTurn {
                role: "user".into(),
                text: "own prior".into(),
                at: 2,
            };
            request.history = vec![inherited.clone(), own.clone()];
            let mut session = AgentSession {
                turns: request.history.clone(),
                ..Default::default()
            };
            let codex = provider == "codex-image";
            assert_eq!(record_submitted_user(&request, &mut session, codex), !codex);
            if codex {
                // Actual Codex leaf owns these public messages.
                session.turns.push(atlas_agent::AgentTurn {
                    role: "user".into(),
                    text: request.prompt.clone(),
                    at: 42,
                });
                session.turns.push(atlas_agent::AgentTurn {
                    role: "assistant".into(),
                    text: "actual provider text".into(),
                    at: 43,
                });
            }
            assert_eq!(&session.turns[..2], &[inherited, own]);
            assert_eq!(session.turns[2].text, request.prompt);
            assert_eq!(
                session
                    .turns
                    .iter()
                    .filter(|t| t.role == "assistant")
                    .count(),
                usize::from(codex)
            );
            assert!(session.bundle.images.is_empty()); // no invented completion/output
        }
    }
}
