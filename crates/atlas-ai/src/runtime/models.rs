//! Shared provider catalog presentation; discovery remains in runtime.

/// Every text model a text agent offers, as (provider, model, label): local
/// models first (the default needs no account, Art. I.4), then ChatGPT
/// through the Codex sign-in, then OpenAI API models.
pub fn text_models(
    local: &[atlas_agent::AgentModel],
    codex: Option<&[atlas_agent::AgentModel]>,
    openai: bool,
    api: &[atlas_agent::AgentModel],
) -> Vec<(String, String, String)> {
    let mut models = vec![(
        "ollama".to_string(),
        String::new(),
        "Local · Auto".to_string(),
    )];
    models.extend(local.iter().map(|m| {
        (
            "ollama".to_string(),
            m.id.clone(),
            format!("Local · {}", crate::agent::model_label(&m.name)),
        )
    }));
    if let Some(codex) = codex {
        models.push((
            "codex-text".into(),
            String::new(),
            "ChatGPT · your sign-in".into(),
        ));
        models.extend(codex.iter().map(|m| {
            (
                "codex-text".to_string(),
                m.id.clone(),
                format!("ChatGPT · {}", crate::agent::model_label(&m.name)),
            )
        }));
    }
    if openai && api.is_empty() {
        // The key's catalog has not arrived yet.
        models.push((
            "openai-text".into(),
            atlas_openai::DEFAULT_TEXT_MODEL.into(),
            format!("OpenAI · {}", atlas_openai::DEFAULT_TEXT_MODEL),
        ));
    } else if openai {
        models.extend(api.iter().map(|m| {
            (
                "openai-text".to_string(),
                m.id.clone(),
                format!("OpenAI · {}", m.name),
            )
        }));
    } else {
        models.push((
            "openai-text".into(),
            atlas_openai::DEFAULT_TEXT_MODEL.into(),
            "OpenAI · add API key".into(),
        ));
    }
    models
}

/// Every image model the image agent offers, as (provider, model, label). An
/// empty model is that provider's default. ComfyUI checkpoints and the GPT
/// Image models a key may call come from their catalogs; before the key's
/// catalog arrives the known GPT Image models stand in.
pub fn image_models(
    comfy: &[atlas_agent::AgentModel],
    codex: bool,
    openai: bool,
    gpt: &[atlas_agent::AgentModel],
) -> Vec<(String, String, String)> {
    let mut models = vec![(
        "comfy".to_string(),
        String::new(),
        "ComfyUI · Auto".to_string(),
    )];
    models.extend(comfy.iter().map(|m| {
        (
            "comfy".to_string(),
            m.id.clone(),
            format!("ComfyUI · {}", m.name),
        )
    }));
    if codex {
        models.push((
            "codex-image".into(),
            String::new(),
            "ChatGPT · your sign-in".into(),
        ));
    }
    let known: Vec<(String, String)> = if gpt.is_empty() {
        atlas_openai::MODELS
            .iter()
            .map(|(id, name)| (id.to_string(), name.to_string()))
            .collect()
    } else {
        gpt.iter().map(|m| (m.id.clone(), m.name.clone())).collect()
    };
    models.extend(known.into_iter().map(|(id, name)| {
        let label = if openai {
            format!("{name} · API key")
        } else {
            format!("{name} · add API key")
        };
        ("openai-image".to_string(), id, label)
    }));
    models
}
