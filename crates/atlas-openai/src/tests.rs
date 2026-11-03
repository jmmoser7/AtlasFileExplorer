
#[test]
fn custom_ratio_is_rejected_before_building_an_api_request() {
    let client = super::Client::new("unused-test-key".into(), std::path::PathBuf::new());
    let request = atlas_agent::AgentRequest {
        image: Some(atlas_agent::ImageParams {
            aspect: atlas_agent::Aspect::Custom {
                width: 16,
                height: 9,
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(client
        .request(&request, 1)
        .err()
        .is_some_and(|error| error.contains("preset sizes")));
}
use super::*;
#[test]
fn text_payload_includes_explicit_source_and_only_requested_active_context() {
    let client = Client::new("sk-test".into(), PathBuf::from("out"));
    let mut request = AgentRequest {
        prompt: "Explain".into(),
        oneshot: true,
        text_source: Some("A café & tea.".into()),
        ..Default::default()
    };
    let off = client.text_request(&request).unwrap().config().unwrap();
    assert!(off.contains("quoted document data"));
    assert!(off.contains("café"));
    assert_eq!(off.matches("café").count(), 1);
    assert!(!off.contains("Earlier active answer"));
    request.inputs.wired.push(atlas_agent::ContextItem {
        node: 8,
        text: "WIRED_PROMPT_ONCE".into(),
        slot: Some(InputSlot::Prompt),
        images: Vec::new(),
        outputs: Default::default(),
        active: None,
        depth: None,
    });
    request.history.push(atlas_agent::AgentTurn {
        role: "assistant".into(),
        text: "Earlier active answer".into(),
        at: 0,
    });
    let on = client.text_request(&request).unwrap().config().unwrap();
    assert!(on.contains("Earlier active answer"));
    assert_eq!(on.matches("Earlier active answer").count(), 1);
    assert_eq!(on.matches("café").count(), 1);
    assert_eq!(on.matches("WIRED_PROMPT_ONCE").count(), 1);
    assert_eq!(request.prompt, "Explain");
}

#[test]
fn image_payload_replays_only_explicitly_admitted_history_once() {
    let mut request = request(json!([]), None);
    assert!(!prompt(&request).contains("ADMITTED_IMAGE_CONTEXT"));
    request.history.push(atlas_agent::AgentTurn {
        role: "user".into(),
        text: "ADMITTED_IMAGE_CONTEXT".into(),
        at: 0,
    });
    assert_eq!(
        prompt(&request).matches("ADMITTED_IMAGE_CONTEXT").count(),
        1
    );
    request.history.clear();
    assert!(!prompt(&request).contains("ADMITTED_IMAGE_CONTEXT"));
}

fn request(wired: Value, model: Option<&str>) -> AgentRequest {
    serde_json::from_value(json!({
        "id": "r", "prompt": "a watercolor pavilion", "at": 0,
        "model": model,
        "image": {"count": 3, "aspect": "landscape"},
        "inputs": {"revision": "1", "context": [], "wired": wired}
    }))
    .unwrap()
}

#[test]
fn a_prompt_alone_generates_and_pictures_edit_with_their_roles_named() {
    let client = Client::new("sk-test".into(), PathBuf::from("out"));
    let plain = client.request(&request(json!([]), None), 3).unwrap();
    assert!(plain.url.ends_with("/images/generations"));
    let config = plain.config().unwrap();
    assert!(config.contains("\\\"n\\\":3"));
    assert!(config.contains("\\\"size\\\":\\\"1536x1024\\\""));
    assert!(config.contains("\\\"model\\\":\\\"gpt-image-2\\\""));
    assert!(config.contains("Authorization: Bearer sk-test"));

    let wired = json!([
        {"node": 1, "text": "", "images": ["hall.png"], "slot": "media"},
        {"node": 2, "text": "", "images": ["monet.jpg"], "slot": "style"}
    ]);
    let edit = client
        .request(&request(wired.clone(), Some("gpt-image-1-mini")), 2)
        .unwrap();
    assert!(edit.url.ends_with("/images/edits"));
    let config = edit.config().unwrap();
    let hall = config.find("image[]=@hall.png").unwrap();
    let monet = config.find("image[]=@monet.jpg").unwrap();
    assert!(hall < monet, "the source goes first");
    assert!(config.contains("size=1536x1024"));
    assert!(prompt(&request(wired, None)).contains("style reference only"));
}

#[test]
fn sizes_follow_the_aspect_and_older_models_keep_standard_sizes() {
    assert_eq!(size("gpt-image-2", Aspect::Wide, None), "2048x1152");
    assert_eq!(size("gpt-image-1-mini", Aspect::Wide, None), "1536x1024");
    assert_eq!(size("gpt-image-2", Aspect::Portrait, None), "1024x1536");
    assert_eq!(size("gpt-image-2", Aspect::Source, Some(0.5)), "1024x1536");
    assert_eq!(size("gpt-image-2", Aspect::Source, None), "1024x1024");
}

#[test]
fn the_model_listing_offers_image_models_newest_first() {
    let listing = json!({"data": [
        {"id": "gpt-5", "created": 30},
        {"id": "gpt-image-1", "created": 10},
        {"id": "gpt-image-2", "created": 20},
        {"id": "gpt-image-2-2026-04-21", "created": 20},
        {"id": "gpt-image-2.5-sunburst", "created": 40},
        {"id": "gpt-image-2.5-sunburst-2026-09-08", "created": 40},
        {"id": "chatgpt-image-latest", "created": 15},
        {"id": "dall-e-3", "created": 5}
    ]});
    let models = image_models(&listing);
    assert_eq!(
        models.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(),
        [
            "gpt-image-2.5-sunburst",
            "gpt-image-2",
            "chatgpt-image-latest",
            "gpt-image-1"
        ]
    );
    assert_eq!(models[0].1, "GPT Image 2.5 Sunburst");
    assert_eq!(models[1].1, "GPT Image 2");
    assert_eq!(models[2].1, "ChatGPT Image Latest");
    assert_eq!(size("gpt-image-2.5-flare", Aspect::Wide, None), "2048x1152");
}

#[test]
fn a_text_block_sends_its_prompt_and_pictures_and_reads_the_reply() {
    let dir = std::env::temp_dir().join(format!("atlas-openai-text-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let picture = dir.join("hall.png");
    std::fs::write(&picture, b"\x89PNG....").unwrap();
    let client = Client::new("sk-test".into(), dir.clone());
    let request: AgentRequest = serde_json::from_value(json!({
        "id": "t", "prompt": "Describe the style", "at": 0, "oneshot": true, "model": "gpt-5.5",
        "inputs": {"revision": "1", "context": [], "wired": [
            {"node": 1, "text": "", "images": [picture.to_string_lossy()], "slot": "media"}
        ]}
    }))
    .unwrap();
    let http = client.text_request(&request).unwrap();
    assert!(http.url.ends_with("/responses"));
    let config = http.config().unwrap();
    assert!(config.contains("\\\"model\\\":\\\"gpt-5.5\\\""));
    assert!(config.contains("data:image/png;base64,"));
    assert!(config.contains("input_text"));
    let reply = json!({"output": [
        {"type": "reasoning", "content": []},
        {"type": "message", "content": [{"type": "output_text", "text": " cool, geometric "}]}
    ]});
    assert_eq!(reply_text(&reply).as_deref(), Some("cool, geometric"));
    let listing = json!({"data": [
        {"id": "gpt-5.5", "created": 50},
        {"id": "gpt-5.5-2026-08-01", "created": 50},
        {"id": "gpt-image-2", "created": 60},
        {"id": "gpt-realtime", "created": 55},
        {"id": "o4-mini", "created": 30},
        {"id": "omni-moderation-latest", "created": 40},
        {"id": "text-embedding-3-large", "created": 20}
    ]});
    assert_eq!(
        chat_models(&listing)
            .iter()
            .map(|m| m.0.as_str())
            .collect::<Vec<_>>(),
        ["gpt-5.5", "o4-mini"]
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn responses_decode_and_errors_name_the_key() {
    let ok = json!({"data": [{"b64_json": "iVBORw0KGgo="}]});
    assert_eq!(decode(&ok).unwrap()[0][..4], [0x89, b'P', b'N', b'G']);
    assert!(decode(&json!({"data": []})).is_err());
    let refused = api_error(Failure::Status(
        r#"{"error":{"message":"Incorrect API key provided."}}"#.into(),
    ));
    assert!(refused.starts_with("The OpenAI API key was refused"));
}
