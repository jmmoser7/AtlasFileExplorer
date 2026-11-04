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

use super::*;
pub fn codex_executable() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_BIN")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(path);
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for name in ["codex.exe", "codex"] {
                let p = dir.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let root = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("OpenAI/Codex/bin");
    let mut entries: Vec<_> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path().join("codex.exe"))
        .filter(|p| p.is_file())
        .collect();
    entries.sort();
    entries.pop()
}

pub fn comfy_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    let _ = atlas_comfy::Client::start(&AtomicBool::new(false))?;
    Ok(atlas_comfy::checkpoints()?
        .into_iter()
        .map(|name| atlas_agent::AgentModel {
            id: name.clone(),
            name,
            ..Default::default()
        })
        .collect())
}
pub fn local_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    let _ = atlas_ollama::Client::start(None, &AtomicBool::new(false))?;
    Ok(atlas_ollama::models()?
        .into_iter()
        .filter(|m| !m.contains("cloud"))
        .map(|name| atlas_agent::AgentModel {
            id: name.clone(),
            name,
            ..Default::default()
        })
        .collect())
}
pub fn codex_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    atlas_codex::Client::models(
        &codex_executable().ok_or("Codex is not installed")?,
        &std::env::current_dir().map_err(|e| e.to_string())?,
    )
}
pub fn cursor_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    serde_json::from_value(crate::sidecar::query_cursor_models()?)
        .map_err(|e| format!("Cursor model catalog: {e}"))
}
pub fn codex_projects() -> Vec<(String, PathBuf)> {
    codex_executable()
        .and_then(|exe| atlas_codex::Client::projects(&exe, &std::env::current_dir().ok()?).ok())
        .unwrap_or_default()
}

/// GPT Image models the stored key may call, newest first. Worker-only: it
/// reads Credential Manager and asks the API.
pub fn openai_image_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    let key = atlas_core::secrets::load(OPENAI_KEY_SLOT).ok_or("Add an OpenAI API key.")?;
    Ok(atlas_openai::models(&key)?
        .into_iter()
        .map(|(id, name)| atlas_agent::AgentModel {
            id,
            name,
            ..Default::default()
        })
        .collect())
}

/// OpenAI text models the stored key may call, newest first. Worker-only.
pub fn openai_text_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    let key = atlas_core::secrets::load(OPENAI_KEY_SLOT).ok_or("Add an OpenAI API key.")?;
    Ok(atlas_openai::text_models(&key)?
        .into_iter()
        .map(|(id, name)| atlas_agent::AgentModel {
            id,
            name,
            ..Default::default()
        })
        .collect())
}
