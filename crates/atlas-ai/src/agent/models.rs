/// Friendly display names never change the provider model ID sent on the wire.
/// The model catalog a provider's portals choose from.
pub fn model_catalog(provider: &str) -> &str {
    if provider == "ollama" || provider.starts_with("ollama/") {
        "ollama"
    } else if provider == "codex-text" {
        // ChatGPT text through the sign-in offers the Codex model catalog.
        "codex"
    } else {
        provider
    }
}

/// An image engine Slate supervises in process: runs wait their turn on the
/// portal, 3D and video inputs are captured at run start, and Live reruns on
/// changes. ChatGPT (through Codex) and the OpenAI API run remotely but are
/// supervised the same way.
pub fn local_image_engine(provider: &str) -> bool {
    matches!(provider, "comfy" | "codex-image" | "openai-image")
}

pub fn model_label(name: &str) -> &str {
    // A checkpoint is named by its file; the extension carries no meaning.
    for ext in [".safetensors", ".ckpt"] {
        if name.len() > ext.len() && name.to_ascii_lowercase().ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    for (suffix, label) in [
        ("astra", "Astra"),
        ("sol", "Sol"),
        ("terra", "Terra"),
        ("luna", "Luna"),
    ] {
        if name.to_ascii_lowercase().split(['-', ' ']).next_back() == Some(suffix) {
            return label;
        }
    }
    name
}

#[cfg(test)]
mod model_label_tests {
    #[test]
    fn friendly_names_preserve_unknown_model_identity() {
        for (id, expected) in [
            ("gpt-6-astra", "Astra"),
            ("GPT-5.6 Sol", "Sol"),
            ("gpt-5.6-terra", "Terra"),
            ("gpt-5.6-luna", "Luna"),
            ("llama3.1:8b", "llama3.1:8b"),
            ("DreamShaper8_LCM.safetensors", "DreamShaper8_LCM"),
            ("old.CKPT", "old"),
        ] {
            assert_eq!(super::model_label(id), expected);
        }
        assert_eq!(super::model_catalog("ollama/qwen3"), "ollama");
        assert_eq!(super::model_catalog("comfy"), "comfy");
        assert!(super::local_image_engine("comfy"));
        assert!(!super::local_image_engine("image-link"));
    }
}
