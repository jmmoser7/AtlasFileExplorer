//! Chooser rows for image and text agents, from the pack catalog. This is the
//! one owner of which models a Generate or text chooser lists.

use atlas_ai::agent::{model_label, AgentModel};
use atlas_ai::packs::{ADD_KEY_LABEL, OPENAI};

use super::super::SlateApp;

/// (provider, model, label). An empty model is that provider's default.
pub(crate) type ModelRow = (String, String, String);

/// Which packs answered Ok this session. Read from the last worker probe.
#[derive(Clone, Copy, Default)]
pub(crate) struct Ready {
    pub(crate) comfy: bool,
    pub(crate) ollama: bool,
    pub(crate) codex: bool,
    pub(crate) openai: bool,
}

fn row(provider: &str, model: &str, label: String) -> ModelRow {
    (provider.to_string(), model.to_string(), label)
}

/// Install packs are listed only when Ok. OpenAI is the PK2 exception: with no
/// key it stays, dimmed, as one "add API key" row.
pub(crate) fn image_rows(ready: Ready, comfy: &[AgentModel], gpt: &[AgentModel]) -> Vec<ModelRow> {
    let mut rows = Vec::new();
    if ready.comfy {
        rows.push(row("comfy", "", "ComfyUI · Auto".into()));
        rows.extend(
            comfy
                .iter()
                .map(|m| row("comfy", &m.id, format!("ComfyUI · {}", m.name))),
        );
    }
    if ready.codex {
        rows.push(row("codex-image", "", "ChatGPT · your sign-in".into()));
    }
    if !ready.openai {
        rows.push(row(OPENAI, "", ADD_KEY_LABEL.into()));
    } else if gpt.is_empty() {
        rows.extend(
            atlas_ai::runtime::OPENAI_IMAGE_MODELS
                .iter()
                .map(|(id, name)| row(OPENAI, id, format!("OpenAI · {name}"))),
        );
    } else {
        rows.extend(
            gpt.iter()
                .map(|m| row(OPENAI, &m.id, format!("OpenAI · {}", m.name))),
        );
    }
    rows
}

/// Local models first when Ollama answers (no account needed, Art. I.4), then
/// ChatGPT through the Codex sign-in, then the OpenAI API.
pub(crate) fn text_rows(
    ready: Ready,
    local: &[AgentModel],
    codex: &[AgentModel],
    api: &[AgentModel],
) -> Vec<ModelRow> {
    let mut rows = Vec::new();
    if ready.ollama {
        rows.push(row("ollama", "", "Local · Auto".into()));
        rows.extend(
            local
                .iter()
                .map(|m| row("ollama", &m.id, format!("Local · {}", model_label(&m.name)))),
        );
    }
    if ready.codex {
        rows.push(row("codex-text", "", "ChatGPT · your sign-in".into()));
        rows.extend(codex.iter().map(|m| {
            row(
                "codex-text",
                &m.id,
                format!("ChatGPT · {}", model_label(&m.name)),
            )
        }));
    }
    if !ready.openai {
        rows.push(row("openai-text", "", ADD_KEY_LABEL.into()));
    } else if api.is_empty() {
        let model = atlas_ai::runtime::OPENAI_TEXT_MODEL;
        rows.push(row("openai-text", model, format!("OpenAI · {model}")));
    } else {
        rows.extend(
            api.iter()
                .map(|m| row("openai-text", &m.id, format!("OpenAI · {}", m.name))),
        );
    }
    rows
}

impl SlateApp {
    pub(crate) fn pack_ok(&self, id: &str) -> bool {
        self.ai.packs.ok(id)
    }

    pub(crate) fn packs_ready(&self) -> Ready {
        Ready {
            comfy: self.pack_ok("comfy"),
            ollama: self.pack_ok("ollama"),
            codex: self.pack_ok("codex"),
            openai: self.pack_ok(OPENAI),
        }
    }

    /// Chooser rows for an image or text agent. Catalogs of Ok packs are
    /// fetched off-thread as they are needed.
    pub(crate) fn agent_models(&mut self, image: bool) -> Vec<ModelRow> {
        let ready = self.packs_ready();
        let wanted: &[(bool, &str)] = if image {
            &[(ready.comfy, "comfy"), (ready.openai, "openai-image")]
        } else {
            &[
                (ready.ollama, "ollama"),
                (ready.codex, "codex"),
                (ready.openai, "openai-text"),
            ]
        };
        for (ok, catalog) in wanted {
            if *ok {
                self.agents.want_catalog(catalog);
            }
        }
        let catalog = |key: &str| self.agents.catalog(key);
        if image {
            image_rows(ready, catalog("comfy"), catalog("openai-image"))
        } else {
            text_rows(
                ready,
                catalog("ollama"),
                catalog("codex"),
                catalog("openai-text"),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_key_offers_one_dimmed_openai_row_and_hides_missing_installs() {
        let rows = image_rows(Ready::default(), &[], &[]);
        assert_eq!(rows, vec![row(OPENAI, "", ADD_KEY_LABEL.into())]);
        assert!(atlas_shell::selection_tools::needs_key_label(&rows[0].2));
        let text = text_rows(Ready::default(), &[], &[], &[]);
        assert_eq!(text, vec![row("openai-text", "", ADD_KEY_LABEL.into())]);
    }

    #[test]
    fn a_stored_key_shows_openai_as_a_normal_option() {
        let ready = Ready {
            openai: true,
            ..Ready::default()
        };
        let rows = image_rows(ready, &[], &[]);
        assert!(!rows.is_empty());
        assert!(rows
            .iter()
            .all(|r| r.0 == OPENAI && r.2.starts_with("OpenAI · ")));
        assert!(rows
            .iter()
            .all(|r| !atlas_shell::selection_tools::needs_key_label(&r.2)));
    }

    #[test]
    fn ready_installs_lead_the_list() {
        let ready = Ready {
            comfy: true,
            ollama: true,
            codex: true,
            openai: false,
        };
        let image = image_rows(ready, &[], &[]);
        assert_eq!(image[0].2, "ComfyUI · Auto");
        assert!(image.iter().any(|r| r.0 == "codex-image"));
        let text = text_rows(ready, &[], &[], &[]);
        assert_eq!(text[0].2, "Local · Auto");
        assert_eq!(text.last().map(|r| r.2.as_str()), Some(ADD_KEY_LABEL));
    }
}
