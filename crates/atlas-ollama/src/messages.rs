use super::{AgentRequest, AgentTurn, Value, ONESHOT_SYSTEM};
use serde_json::json;

/// History (the runtime empties it for a text block), then this turn. A text
/// block sends its instruction under [`ONESHOT_SYSTEM`], without transport
/// guides.
pub(super) fn chat_messages(
    turns: &[AgentTurn],
    request: &AgentRequest,
    images: Vec<String>,
) -> Vec<Value> {
    let mut messages: Vec<Value> = Vec::new();
    if request.oneshot {
        messages.push(json!({"role": "system", "content": ONESHOT_SYSTEM}));
    }
    if !(request.oneshot && (request.text_source.is_some() || !request.history.is_empty())) {
        messages.extend(
            turns
                .iter()
                .filter(|t| matches!(t.role.as_str(), "user" | "assistant"))
                .map(|t| json!({"role": t.role, "content": t.text})),
        );
    }
    // A local chat without tools has no file or board output to publish.
    let conversation = request.artifact.is_none()
        && request.collaborator.is_none()
        && request.goal.is_none()
        && request.image.is_none()
        && request.inputs.wired.is_empty();
    let content = if request.oneshot || conversation {
        request.prompt_with_history()
    } else {
        request.input_text()
    };
    let mut user = json!({"role": "user", "content": content});
    if !images.is_empty() {
        user["images"] = json!(images);
    }
    messages.push(user);
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_chat_and_its_followup_send_the_actual_words_without_file_guides() {
        let mut request = AgentRequest {
            prompt: "The tile is green. What color is it?".into(),
            ..Default::default()
        };
        assert_eq!(
            chat_messages(&[], &request, vec![]),
            [json!({"role":"user","content":request.prompt})]
        );
        let turn = AgentTurn {
            role: "assistant".into(),
            text: "Green.".into(),
            at: 0,
        };
        request.prompt = "Which tile did you mean?".into();
        request.policy = atlas_agent::TurnPolicy::ReadOnly;
        assert_eq!(
            chat_messages(std::slice::from_ref(&turn), &request, vec![]),
            [
                json!({"role":"assistant","content":"Green."}),
                json!({"role":"user","content":request.prompt}),
            ]
        );
        request.history.push(turn);
        let replay = chat_messages(&[], &request, vec![]);
        assert_eq!(replay[0]["content"], request.prompt_with_history());
        assert!(replay[0]["content"].as_str().unwrap().contains("Green."));
        let wired: AgentRequest = serde_json::from_value(json!({
            "id":"wired","at":0,"prompt":"Which tile?",
            "inputs":{"revision":"1","context":[],"wired":[
                {"node":1,"text":"Tile B is blue.","images":[],"slot":"prompt"}
            ]}
        }))
        .unwrap();
        let message = chat_messages(&[], &wired, vec![]);
        assert_eq!(message[0]["content"], wired.input_text());
        assert!(message[0]["content"]
            .as_str()
            .unwrap()
            .contains("Tile B is blue."));
    }
}
