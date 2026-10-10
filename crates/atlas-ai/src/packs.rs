//! Session pack catalog. Probes run on a worker; the frame only reads the
//! last snapshot.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

pub use atlas_packs::{PackHealth, PackId, PackRow};
use atlas_packs::{Catalog, ContractId, Offer};
use crossbeam_channel::{Receiver, unbounded};

use crate::agent::{provider_by_id, AgentProvider};

pub const ADD_KEY_LABEL: &str = "OpenAI · add API key";
pub const KEY_URL: &str = "https://platform.openai.com/api-keys";

static OVERRIDES: Mutex<Vec<(String, PackHealth)>> = Mutex::new(Vec::new());
static PROBES: AtomicU32 = AtomicU32::new(0);

/// Replace a probe result. The next worker probe records it again, so a test
/// can flip Cursor from missing to installed without restarting the process.
pub fn set_health_override(id: &str, health: PackHealth) {
    let mut rows = OVERRIDES.lock().expect("pack overrides");
    if let Some(slot) = rows.iter_mut().find(|(name, _)| name == id) {
        slot.1 = health;
    } else {
        rows.push((id.to_string(), health));
    }
}

pub fn clear_health_overrides() {
    OVERRIDES.lock().expect("pack overrides").clear();
}

pub fn probes_started() -> u32 {
    PROBES.load(Ordering::Relaxed)
}

pub struct PackSession {
    catalog: Catalog,
    rx: Option<Receiver<Catalog>>,
    started: bool,
    chooser_painted: bool,
    chooser_latched: bool,
    /// Tests pin the program grid so a background probe cannot replace it.
    pub hold: bool,
    pub key_entry: bool,
    pending_programs: Option<Vec<AgentProvider>>,
}

impl PackSession {
    pub fn new() -> Self {
        Self {
            catalog: Catalog::from_manifests(Vec::new()),
            rx: None,
            started: false,
            chooser_painted: false,
            chooser_latched: false,
            hold: false,
            key_entry: false,
            pending_programs: None,
        }
    }

    pub fn ensure_started(&mut self, workspace: Option<PathBuf>) {
        if self.started {
            return;
        }
        self.started = true;
        self.spawn(workspace);
    }

    pub fn refresh(&mut self, workspace: Option<PathBuf>) {
        self.spawn(workspace);
    }

    /// One probe when the chooser becomes visible, then again the next time
    /// it opens. `programs_started` must not freeze the list for the session.
    pub fn begin_frame(&mut self) {
        if !self.chooser_painted {
            self.chooser_latched = false;
        }
        self.chooser_painted = false;
    }

    pub fn note_chooser_open(&mut self, workspace: Option<PathBuf>) {
        self.chooser_painted = true;
        if self.chooser_latched {
            return;
        }
        self.chooser_latched = true;
        self.spawn(workspace);
    }

    pub fn probe_pending(&mut self) -> bool {
        self.drain();
        self.rx.is_some()
    }

    pub fn poll_programs(&mut self) -> Option<Vec<AgentProvider>> {
        self.drain();
        self.pending_programs.take()
    }

    pub fn record(&mut self, id: &str, health: PackHealth) {
        self.catalog.set_health(&PackId::new(id), health);
    }

    pub fn cursor_ok(&self) -> bool {
        self.health("cursor") == PackHealth::Ok
    }

    pub fn health(&self, id: &str) -> PackHealth {
        self.catalog.health(&PackId::new(id))
    }

    pub fn title(&self, id: &str) -> Option<String> {
        self.catalog.title(&PackId::new(id)).map(str::to_string)
    }

    pub fn detail(&self, id: &str) -> String {
        self.catalog.detail(&PackId::new(id)).to_string()
    }

    pub fn needs_key(&self, id: &str) -> bool {
        self.offers_for("image")
            .into_iter()
            .chain(self.offers_for("chat"))
            .any(|offer| offer.id.as_str() == id && offer.needs_key)
    }

    pub fn rows(&self) -> Vec<PackRow> {
        self.catalog.rows()
    }

    pub fn forget(&mut self, id: &str, workspace: Option<PathBuf>) -> Result<(), String> {
        self.catalog.forget(&PackId::new(id))?;
        self.spawn(workspace);
        Ok(())
    }

    fn offers_for(&self, contract: &str) -> Vec<Offer> {
        let Some(contract) = contract_id(contract) else {
            return Vec::new();
        };
        self.catalog.offers(contract)
    }

    fn drain(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        match rx.try_recv() {
            Ok(catalog) => {
                self.catalog = catalog;
                self.pending_programs = Some(programs_of(&self.catalog));
                self.rx = None;
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => self.rx = None,
        }
    }

    fn spawn(&mut self, workspace: Option<PathBuf>) {
        if self.rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            PROBES.fetch_add(1, Ordering::Relaxed);
            let _ = tx.send(probe_catalog(workspace.as_deref()));
        });
    }
}

impl Default for PackSession {
    fn default() -> Self {
        Self::new()
    }
}

fn probe_catalog(workspace: Option<&Path>) -> Catalog {
    let mut catalog = Catalog::load(workspace);
    register_builtin(&mut catalog);
    catalog.probe_all();
    let overrides = OVERRIDES.lock().expect("pack overrides").clone();
    for (id, health) in overrides {
        catalog.set_health(&PackId::new(id), health);
    }
    catalog
}

fn register_builtin(catalog: &mut Catalog) {
    catalog.register_custom("cursor", || {
        if crate::launch::cursor_available() {
            PackHealth::Ok
        } else {
            PackHealth::Missing
        }
    });
    catalog.register_custom("codex", || {
        if crate::runtime::codex_executable().is_some() {
            PackHealth::Ok
        } else {
            PackHealth::Missing
        }
    });
    catalog.register_custom("openai-key", || {
        if atlas_core::secrets::health(crate::runtime::OPENAI_KEY_SLOT)
            == atlas_core::secrets::SecretHealth::Ok
        {
            PackHealth::Ok
        } else {
            PackHealth::Missing
        }
    });
    catalog.register_custom("pdfium", || {
        if atlas_core::pdf::library_present() {
            PackHealth::Ok
        } else {
            PackHealth::Missing
        }
    });
    catalog.register_custom("sam", || {
        if atlas_segment::installed(&atlas_core::index::data_dir()) {
            PackHealth::Ok
        } else {
            PackHealth::Missing
        }
    });
}

fn programs_of(catalog: &Catalog) -> Vec<AgentProvider> {
    let mut out = Vec::new();
    for id in catalog.ready(ContractId::Chat) {
        out.push(named(catalog, &id, false));
    }
    for offer in catalog.offers(ContractId::Image) {
        out.push(named(catalog, &offer.id, offer.needs_key));
    }
    out
}

fn named(catalog: &Catalog, id: &PackId, needs_key: bool) -> AgentProvider {
    let mut program = provider_by_id(id.as_str());
    let title = catalog.title(id).unwrap_or(id.as_str());
    program.display_name = if needs_key && id.as_str() == "openai-image" {
        ADD_KEY_LABEL.to_string()
    } else if needs_key {
        format!("{title} · add API key")
    } else {
        title.to_string()
    };
    program
}

fn contract_id(name: &str) -> Option<ContractId> {
    Some(match name {
        "chat" => ContractId::Chat,
        "image" => ContractId::Image,
        "segment" => ContractId::Segment,
        "page" => ContractId::Page,
        "program" => ContractId::Program,
        _ => return None,
    })
}

/// Generate-chooser rows. Install packs stay out unless their probe is Ok.
/// OpenAI stays in, dimmed, when the only miss is the API key.
pub fn image_chooser(
    comfy_ok: bool,
    codex_ok: bool,
    openai_ok: bool,
    comfy: &[atlas_agent::AgentModel],
    gpt: &[atlas_agent::AgentModel],
) -> Vec<(String, String, String)> {
    let mut models = crate::runtime::image_models(
        if comfy_ok { comfy } else { &[] },
        codex_ok,
        openai_ok,
        gpt,
    );
    if !comfy_ok {
        models.retain(|row| row.0 != "comfy");
    }
    if !openai_ok {
        models.retain(|row| row.0 != "openai-image");
        models.push((
            "openai-image".into(),
            String::new(),
            ADD_KEY_LABEL.to_string(),
        ));
    } else {
        for row in &mut models {
            if row.0 == "openai-image" {
                let name = row.2.split(" · ").next().unwrap_or(row.2.as_str());
                row.2 = format!("OpenAI · {name}");
            }
        }
    }
    models
}

pub fn text_chooser(
    ollama_ok: bool,
    codex_ok: bool,
    openai_ok: bool,
    local: &[atlas_agent::AgentModel],
    codex: Option<&[atlas_agent::AgentModel]>,
    api: &[atlas_agent::AgentModel],
) -> Vec<(String, String, String)> {
    let mut models =
        crate::runtime::text_models(if ollama_ok { local } else { &[] }, codex.filter(|_| codex_ok), openai_ok, api);
    if !ollama_ok {
        models.retain(|row| row.0 != "ollama");
    }
    if !codex_ok {
        models.retain(|row| row.0 != "codex-text");
    }
    if !openai_ok {
        models.retain(|row| row.0 != "openai-text");
        models.push(("openai-text".into(), String::new(), ADD_KEY_LABEL.into()));
    }
    models
}

/// Contract slot shared by every chat pack. A new vendor does not add a slot.
pub fn contract_slot(pack_id: &str) -> Option<&'static str> {
    match pack_id {
        "cursor" | "codex" | "codex-text" | "ollama" | "openai-text" => Some("chat"),
        "comfy" | "openai-image" | "codex-image" => Some("image"),
        "sam" => Some("segment"),
        "pdfium" => Some("page"),
        _ => None,
    }
}

/// Existing leaf crate. Cursor and Codex both sit on the chat slot above.
pub fn leaf(pack_id: &str) -> Option<&'static str> {
    match pack_id {
        "cursor" => Some("cursor"),
        "codex" | "codex-text" | "codex-image" => Some("atlas-codex"),
        "ollama" => Some("atlas-ollama"),
        "comfy" => Some("atlas-comfy"),
        "openai-image" | "openai-text" => Some("atlas-openai"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_key_offers_one_dimmed_openai_row_and_hides_comfy() {
        let rows = image_chooser(false, false, false, &[], &[]);
        assert!(rows.iter().all(|row| row.0 != "comfy"));
        assert_eq!(
            rows.iter().filter(|row| row.0 == "openai-image").count(),
            1
        );
        assert_eq!(rows.last().map(|row| row.2.as_str()), Some(ADD_KEY_LABEL));
    }

    #[test]
    fn a_stored_key_shows_openai_as_a_normal_option() {
        let rows = image_chooser(false, false, true, &[], &[]);
        assert!(rows.iter().all(|row| row.2 != ADD_KEY_LABEL));
        assert!(rows.iter().any(|row| row.0 == "openai-image" && row.2.starts_with("OpenAI · ")));
    }

    #[test]
    fn cursor_and_codex_share_the_chat_slot() {
        assert_eq!(contract_slot("cursor"), Some("chat"));
        assert_eq!(contract_slot("codex"), contract_slot("cursor"));
        assert_eq!(leaf("cursor"), Some("cursor"));
        assert_eq!(leaf("codex"), Some("atlas-codex"));
        assert!(leaf("ollama").is_some());
    }

    #[test]
    fn the_cursor_probe_is_registered_on_the_shipped_manifest() {
        let mut catalog = Catalog::from_manifests(atlas_packs::shipped_manifests());
        register_builtin(&mut catalog);
        catalog.probe_all();
        let health = catalog.health(&PackId::new("cursor"));
        let expected = if crate::launch::cursor_available() {
            PackHealth::Ok
        } else {
            PackHealth::Missing
        };
        assert_eq!(health, expected);
        assert!(catalog.detail(&PackId::new("cursor")).contains("cursor"));
    }
}
