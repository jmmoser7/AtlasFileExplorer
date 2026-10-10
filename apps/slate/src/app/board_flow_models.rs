//! Chooser rows for image and text agents, from the pack catalog.

use super::super::SlateApp;

/// Which remote engines the pack catalog has already found. The probe itself
/// ran on a worker.
#[derive(Clone, Copy)]
pub(crate) struct Engines {
    pub(crate) codex: bool,
    pub(crate) openai: bool,
}

impl SlateApp {
    pub(crate) fn pack_ok(&self, id: &str) -> bool {
        self.ai.packs.health(id) == atlas_ai::packs::PackHealth::Ok
    }

    pub(crate) fn engines(&self) -> Engines {
        Engines {
            codex: self.pack_ok("codex"),
            openai: self.pack_ok("openai-image"),
        }
    }

    /// (provider, model, label) choices. Install packs stay out until Ok.
    /// OpenAI image stays in as "add API key" when the key is the only miss.
    pub(crate) fn agent_models(&mut self, image: bool) -> Vec<(String, String, String)> {
        if image {
            if self.pack_ok("comfy") {
                self.agents.want_catalog("comfy");
            }
            if self.pack_ok("openai-image") {
                self.agents.want_catalog("openai-image");
            }
            return atlas_ai::packs::image_chooser(
                self.pack_ok("comfy"),
                self.pack_ok("codex"),
                self.pack_ok("openai-image"),
                self.agents.catalog("comfy"),
                self.agents.catalog("openai-image"),
            );
        }
        if self.pack_ok("ollama") {
            self.agents.want_catalog("ollama");
        }
        if self.pack_ok("codex") {
            self.agents.want_catalog("codex");
        }
        if self.pack_ok("openai-image") {
            self.agents.want_catalog("openai-text");
        }
        let codex = self.pack_ok("codex").then(|| self.agents.catalog("codex"));
        atlas_ai::packs::text_chooser(
            self.pack_ok("ollama"),
            self.pack_ok("codex"),
            self.pack_ok("openai-image"),
            self.agents.catalog("ollama"),
            codex,
            self.agents.catalog("openai-text"),
        )
    }
}
