//! Session pack catalog. Probes run on a worker; the frame only reads the
//! last snapshot.

use std::path::{Path, PathBuf};

use atlas_packs::{Catalog, ContractId};
pub use atlas_packs::{PackHealth, PackId, PackRow};
use crossbeam_channel::{unbounded, Receiver};

use crate::agent::{provider_by_id, AgentProvider};

pub const ADD_KEY_LABEL: &str = "OpenAI · add API key";
pub const KEY_URL: &str = "https://platform.openai.com/api-keys";
/// The pack whose credential is the OpenAI API key. Text and image share it.
pub const OPENAI: &str = "openai-image";

struct Probed {
    catalog: Catalog,
    programs: Vec<AgentProvider>,
}

pub struct PackSession {
    catalog: Catalog,
    rx: Option<Receiver<Probed>>,
    /// A refresh asked for while a probe was in flight. The older answer is
    /// dropped and this one runs, so a key saved mid-probe is not undone.
    again: Option<Option<PathBuf>>,
    workspace: Option<PathBuf>,
    started: bool,
    chooser_painted: bool,
    chooser_latched: bool,
    overrides: Vec<(String, PackHealth)>,
    probes: u32,
    /// Tests pin the program grid so a background probe cannot replace it.
    pub hold: bool,
    /// The OpenAI key window is open.
    pub key_entry: bool,
    pub key_draft: String,
    pending_programs: Option<Vec<AgentProvider>>,
}

impl PackSession {
    pub fn new() -> Self {
        Self {
            catalog: Catalog::from_manifests(Vec::new()),
            rx: None,
            again: None,
            workspace: None,
            started: false,
            chooser_painted: false,
            chooser_latched: false,
            overrides: Vec::new(),
            probes: 0,
            hold: false,
            key_entry: false,
            key_draft: String::new(),
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
        self.started = true;
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
        self.refresh(workspace);
    }

    pub fn probe_pending(&mut self) -> bool {
        self.drain();
        self.rx.is_some()
    }

    /// A probe is running. Does not drain; the frame may still repaint.
    pub fn in_flight(&self) -> bool {
        self.rx.is_some()
    }

    /// Worker probes started by this session.
    pub fn probes(&self) -> u32 {
        self.probes
    }

    pub fn poll_programs(&mut self) -> Option<Vec<AgentProvider>> {
        self.drain();
        self.pending_programs.take()
    }

    /// Pin a probe answer for this session. Every later probe reports it, so
    /// a test can flip Cursor from missing to installed without a restart.
    pub fn set_health_override(&mut self, id: &str, health: PackHealth) {
        self.overrides.retain(|(name, _)| name != id);
        self.overrides.push((id.to_string(), health));
        if self.rx.is_some() && self.again.is_none() {
            self.again = Some(self.workspace.clone());
        }
    }

    pub fn clear_health_overrides(&mut self) {
        self.overrides.clear();
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

    pub fn ok(&self, id: &str) -> bool {
        self.health(id) == PackHealth::Ok
    }

    pub fn title(&self, id: &str) -> Option<String> {
        self.catalog.title(&PackId::new(id)).map(str::to_string)
    }

    pub fn detail(&self, id: &str) -> String {
        self.catalog.detail(&PackId::new(id)).to_string()
    }

    /// Offered in a chooser although only its credential is missing (PK2).
    pub fn needs_key(&self, id: &str) -> bool {
        [ContractId::Chat, ContractId::Image]
            .into_iter()
            .flat_map(|contract| self.catalog.offers(contract))
            .any(|offer| offer.id.as_str() == id && offer.needs_key)
    }

    pub fn rows(&self) -> Vec<PackRow> {
        self.catalog.rows()
    }

    pub fn forget(&mut self, id: &str, workspace: Option<PathBuf>) -> Result<(), String> {
        self.catalog.forget(&PackId::new(id))?;
        self.refresh(workspace);
        Ok(())
    }

    /// Store the OpenAI API key for this operating-system user and probe
    /// again. The key never reaches the workbook, a log, or this struct after
    /// the call.
    pub fn save_openai_key(&mut self, workspace: Option<PathBuf>) -> Result<(), String> {
        let key = std::mem::take(&mut self.key_draft);
        let key = key.trim();
        if key.is_empty() {
            return Err("Paste an OpenAI API key first.".into());
        }
        atlas_core::secrets::store(crate::runtime::OPENAI_KEY_SLOT, key)?;
        self.record(OPENAI, PackHealth::Ok);
        self.key_entry = false;
        self.refresh(workspace);
        Ok(())
    }

    fn drain(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        match rx.try_recv() {
            Ok(probed) => {
                if self.again.is_none() {
                    self.catalog = probed.catalog;
                    self.pending_programs = Some(probed.programs);
                }
                self.rx = None;
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => self.rx = None,
        }
        if self.rx.is_none() {
            if let Some(workspace) = self.again.take() {
                self.spawn(workspace);
            }
        }
    }

    fn spawn(&mut self, workspace: Option<PathBuf>) {
        if self.rx.is_some() {
            self.again = Some(workspace);
            return;
        }
        let (tx, rx) = unbounded();
        self.rx = Some(rx);
        self.probes += 1;
        self.workspace.clone_from(&workspace);
        let overrides = self.overrides.clone();
        std::thread::spawn(move || {
            let catalog = probe_catalog(workspace.as_deref(), &overrides);
            let programs = programs_of(&catalog, workspace.as_deref());
            let _ = tx.send(Probed { catalog, programs });
        });
    }
}

impl Default for PackSession {
    fn default() -> Self {
        Self::new()
    }
}

fn probe_catalog(workspace: Option<&Path>, overrides: &[(String, PackHealth)]) -> Catalog {
    let mut catalog = Catalog::load(workspace);
    register_builtin(&mut catalog);
    catalog.probe_all();
    for (id, health) in overrides {
        catalog.set_health(&PackId::new(id.as_str()), *health);
    }
    catalog
}

fn found(present: bool) -> PackHealth {
    if present {
        PackHealth::Ok
    } else {
        PackHealth::Missing
    }
}

fn register_builtin(catalog: &mut Catalog) {
    catalog.register_custom("cursor", || found(crate::launch::cursor_available()));
    catalog.register_custom("codex", || {
        found(crate::runtime::codex_executable().is_some())
    });
    catalog.register_custom("openai-key", || {
        found(
            atlas_core::secrets::health(crate::runtime::OPENAI_KEY_SLOT)
                == atlas_core::secrets::SecretHealth::Ok,
        )
    });
    catalog.register_custom("pdfium", || found(atlas_core::pdf::library_present()));
    catalog.register_custom("sam", || {
        found(atlas_segment::installed(&atlas_core::index::data_dir()))
    });
}

/// The agent grid: Ok chat packs, image packs on offer, then the file-link
/// adapters a workspace declares in `.atlas-ai/programs.json`.
fn programs_of(catalog: &Catalog, workspace: Option<&Path>) -> Vec<AgentProvider> {
    let mut out: Vec<AgentProvider> = catalog
        .ready(ContractId::Chat)
        .iter()
        .map(|id| named(catalog, id, false))
        .collect();
    for offer in catalog.offers(ContractId::Image) {
        out.push(named(catalog, &offer.id, offer.needs_key));
    }
    for program in workspace_programs(workspace) {
        if !out.iter().any(|p| p.id == program.id) {
            out.push(program);
        }
    }
    out
}

/// File-link adapters from `.atlas-ai/programs.json`. They never launch an
/// application, whatever the file says.
fn workspace_programs(workspace: Option<&Path>) -> Vec<AgentProvider> {
    let Some(bytes) =
        workspace.and_then(|ws| std::fs::read(ws.join(".atlas-ai/programs.json")).ok())
    else {
        return Vec::new();
    };
    serde_json::from_slice::<Vec<AgentProvider>>(&bytes)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !p.id.is_empty() && !p.id.starts_with("ollama/"))
        .map(|mut p| {
            p.launch = crate::agent::LaunchKind::None;
            p
        })
        .collect()
}

fn named(catalog: &Catalog, id: &PackId, needs_key: bool) -> AgentProvider {
    let mut program = provider_by_id(id.as_str());
    let title = catalog.title(id).unwrap_or(id.as_str());
    program.display_name = if needs_key {
        format!("{title} · add API key")
    } else {
        title.to_string()
    };
    program
}

/// Contract slot shared by every pack of a kind. A new vendor does not add a slot.
pub fn contract_slot(pack_id: &str) -> Option<&'static str> {
    match pack_id {
        "cursor" | "codex" | "codex-text" | "ollama" | "openai-text" => Some("chat"),
        id if id.starts_with("ollama/") => Some("chat"),
        "comfy" | "openai-image" | "codex-image" => Some("image"),
        "sam" => Some("segment"),
        "pdfium" => Some("page"),
        _ => None,
    }
}

/// The existing leaf crate that executes a pack. `runtime` starts engines
/// through this table; a new vendor is a row here, never a new enum variant.
pub fn leaf(pack_id: &str) -> Option<&'static str> {
    match pack_id {
        "cursor" => Some("cursor"),
        "codex" | "codex-text" | "codex-image" => Some("atlas-codex"),
        "ollama" => Some("atlas-ollama"),
        id if id.starts_with("ollama/") => Some("atlas-ollama"),
        "comfy" => Some("atlas-comfy"),
        "openai-image" | "openai-text" => Some("atlas-openai"),
        "sam" => Some("atlas-segment"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "packs_tests.rs"]
mod tests;
