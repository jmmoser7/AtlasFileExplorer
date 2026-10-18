use super::*;

/// One short text reply on the person's OpenAI API key.
#[test]
#[ignore = "spends one short OpenAI text reply on the stored key"]
fn live_openai_text_through_the_stored_key() {
    let models = openai_text_models().unwrap();
    eprintln!(
        "text models: {:?}",
        models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()
    );
    let key = atlas_core::secrets::load(OPENAI_KEY_SLOT).unwrap();
    let mut client = atlas_openai::Client::new(key, std::env::temp_dir());
    let request: AgentRequest = serde_json::from_value(serde_json::json!({
        "id": "live-text", "prompt": "Name three colors, comma separated.", "at": 0,
        "oneshot": true, "model": models.first().map(|m| m.id.clone()),
        "inputs": {"revision": "1", "context": [], "wired": []}
    }))
    .unwrap();
    let mut session: AgentSession = serde_json::from_str(r#"{"provider":"openai-text"}"#).unwrap();
    let started = std::time::Instant::now();
    client
        .run(&request, &mut session, &AtomicBool::new(false), |_| {})
        .unwrap();
    eprintln!(
        "{:?} in {:?}: {}",
        request.model,
        started.elapsed(),
        session.turns.last().unwrap().text
    );
}

/// Spends one picture on the person's OpenAI API key. The key is read from
/// Credential Manager inside the adapter and never printed.
#[test]
#[ignore = "spends one GPT Image picture on the stored OpenAI API key"]
fn live_gpt_image_through_the_stored_key() {
    let models = openai_image_models().unwrap();
    eprintln!(
        "models: {:?}",
        models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()
    );
    let dir = std::env::temp_dir().join(format!("atlas-openai-live-{}", std::process::id()));
    let key = atlas_core::secrets::load(OPENAI_KEY_SLOT).unwrap();
    let mut client = atlas_openai::Client::new(key, dir.clone());
    let request: AgentRequest = serde_json::from_value(serde_json::json!({
        "id": "live-gpt", "prompt": "a small green cube on a plain white background", "at": 0,
        "image": {"count": 1, "aspect": "square"},
        "inputs": {"revision": "1", "context": [], "wired": []}
    }))
    .unwrap();
    let mut session: AgentSession = serde_json::from_str(r#"{"provider":"openai-image"}"#).unwrap();
    let started = std::time::Instant::now();
    let result = client.run(&request, &mut session, &AtomicBool::new(false), |_| {});
    eprintln!("{result:?} in {:?}", started.elapsed());
    result.unwrap();
    assert_eq!(session.bundle.images.len(), 1);
    eprintln!("{}", session.bundle.images[0].source);
}
