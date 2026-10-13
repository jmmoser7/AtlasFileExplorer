//! Load manifests, probe them, and answer `health` / `ready`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::manifest::{ContractId, PackId, PackManifest};
use crate::probe::{self, PackHealth};

type CustomProbe = Arc<dyn Fn() -> PackHealth + Send + Sync>;

/// One Connections row. The shell paints these; this crate does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackRow {
    pub id: String,
    pub name: String,
    pub contract: String,
    pub health: String,
    pub install_note: String,
    pub trial: bool,
    pub detail: String,
}

/// A chooser entry. `needs_key` is the credential exception: the pack is listed
/// dimmed even though it is not `Ok`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub id: PackId,
    pub title: String,
    pub needs_key: bool,
}

struct Entry {
    manifest: PackManifest,
    trial_path: Option<PathBuf>,
}

/// The session catalog. `probe_all` performs I/O; call it on a worker.
pub struct Catalog {
    entries: Vec<Entry>,
    health: HashMap<PackId, PackHealth>,
    detail: HashMap<PackId, String>,
    custom: HashMap<String, CustomProbe>,
}

impl Catalog {
    pub fn load(workspace: Option<&Path>) -> Self {
        Self::load_at(&machine_packs_dir(), workspace)
    }

    /// `machine_packs` is the trial directory (tests pass a temp folder).
    pub fn load_at(machine_packs: &Path, workspace: Option<&Path>) -> Self {
        let mut cat = Self::from_manifests(crate::shipped_manifests());
        cat.read_trials(machine_packs);
        if let Some(ws) = workspace {
            cat.read_trials(&ws.join(".atlas-ai").join("packs"));
        }
        cat
    }

    pub fn from_manifests(manifests: Vec<PackManifest>) -> Self {
        let entries = manifests
            .into_iter()
            .map(|manifest| Entry {
                manifest,
                trial_path: None,
            })
            .collect();
        Self {
            entries,
            health: HashMap::new(),
            detail: HashMap::new(),
            custom: HashMap::new(),
        }
    }

    pub fn register_custom<F>(&mut self, id: impl Into<String>, probe: F)
    where
        F: Fn() -> PackHealth + Send + Sync + 'static,
    {
        self.custom.insert(id.into(), Arc::new(probe));
    }

    pub fn probe_all(&mut self) {
        let custom = self.custom.clone();
        let call = |id: &str| custom.get(id).map(|probe| probe());
        for entry in &self.entries {
            let id = entry.manifest.id.clone();
            if entry.manifest.retired {
                self.health.insert(id.clone(), PackHealth::Retired);
                self.detail
                    .insert(id, format!("{} is retired", entry.manifest.title));
                continue;
            }
            let outcome = probe::run(&entry.manifest.probe, &call);
            self.health.insert(id.clone(), outcome.health);
            self.detail.insert(id, outcome.looked_for);
        }
    }

    pub fn set_health(&mut self, id: &PackId, health: PackHealth) {
        self.health.insert(id.clone(), health);
        self.detail
            .entry(id.clone())
            .or_insert_with(|| format!("recorded {health:?} for {}", id.as_str()));
    }

    pub fn health(&self, id: &PackId) -> PackHealth {
        self.health.get(id).copied().unwrap_or(PackHealth::Unknown)
    }

    pub fn detail(&self, id: &PackId) -> &str {
        self.detail.get(id).map(String::as_str).unwrap_or("")
    }

    pub fn title(&self, id: &PackId) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| &e.manifest.id == id)
            .map(|e| e.manifest.title.as_str())
    }

    /// Packs whose probe returned Ok. Retired never appears here.
    pub fn ready(&self, contract: ContractId) -> Vec<PackId> {
        self.entries
            .iter()
            .filter(|e| {
                e.manifest.contract == contract && self.health(&e.manifest.id) == PackHealth::Ok
            })
            .map(|e| e.manifest.id.clone())
            .collect()
    }

    /// Ok packs, plus a credential pack that is only missing its key.
    pub fn offers(&self, contract: ContractId) -> Vec<Offer> {
        self.entries
            .iter()
            .filter(|e| e.manifest.contract == contract && !e.manifest.retired)
            .filter_map(|e| {
                let health = self.health(&e.manifest.id);
                let needs_key = e.manifest.credential.is_some() && health != PackHealth::Ok;
                if health == PackHealth::Ok || needs_key {
                    Some(Offer {
                        id: e.manifest.id.clone(),
                        title: e.manifest.title.clone(),
                        needs_key,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn rows(&self) -> Vec<PackRow> {
        self.entries
            .iter()
            .map(|e| PackRow {
                id: e.manifest.id.0.clone(),
                name: e.manifest.title.clone(),
                contract: e.manifest.contract.as_str().to_string(),
                health: self.health(&e.manifest.id).as_str().to_string(),
                install_note: e.manifest.install_note.clone().unwrap_or_default(),
                trial: e.trial_path.is_some(),
                detail: self.detail(&e.manifest.id).to_string(),
            })
            .collect()
    }

    /// Drop a trial manifest from the probe list and delete its JSON file.
    /// Shipped packs and documents are left alone.
    pub fn forget(&mut self, id: &PackId) -> Result<(), String> {
        let Some(index) = self.entries.iter().position(|e| &e.manifest.id == id) else {
            return Err(format!("No pack named {}", id.as_str()));
        };
        let Some(path) = self.entries[index].trial_path.clone() else {
            return Err(format!(
                "{} is a shipped pack. Forget only removes a trial.",
                self.entries[index].manifest.title
            ));
        };
        if path.is_file() {
            std::fs::remove_file(&path).map_err(|e| format!("Could not forget the trial: {e}"))?;
        }
        self.entries.remove(index);
        self.health.remove(id);
        self.detail.remove(id);
        Ok(())
    }

    fn read_trials(&mut self, dir: &Path) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_slice::<PackManifest>(&bytes) else {
                continue;
            };
            if self
                .entries
                .iter()
                .any(|e| e.manifest.id == manifest.id && e.trial_path.is_none())
            {
                continue;
            }
            self.entries.retain(|e| e.manifest.id != manifest.id);
            self.entries.push(Entry {
                manifest,
                trial_path: Some(path),
            });
        }
    }
}

fn machine_packs_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("NativeFileAtlas")
        .join("packs")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{ContractId, ProbeSpec};
    use std::io::Read;
    use std::net::TcpListener;
    use std::time::Duration;

    fn manifest(id: &str, contract: ContractId, probe: ProbeSpec) -> PackManifest {
        PackManifest {
            id: PackId::new(id),
            title: id.to_string(),
            contract,
            contract_version: 1,
            install_note: None,
            probe,
            retired: false,
            credential: None,
        }
    }

    fn known(id: &str, file: &Path) -> PackManifest {
        let mut m = manifest(
            id,
            ContractId::Chat,
            ProbeSpec::KnownPath {
                names: vec![file.file_name().unwrap().to_string_lossy().into_owned()],
                roots: vec![file.parent().unwrap().display().to_string()],
            },
        );
        m.title = id.to_string();
        m
    }

    #[test]
    fn a_missing_file_is_missing() {
        let dir = std::env::temp_dir().join(format!("atlas-packs-miss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("gone.exe");
        let mut cat = Catalog::from_manifests(vec![known("tool", &file)]);
        cat.probe_all();
        let id = PackId::new("tool");
        assert_eq!(cat.health(&id), PackHealth::Missing);
        assert!(cat.detail(&id).contains("gone.exe"), "{}", cat.detail(&id));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_present_file_is_ok() {
        let dir = std::env::temp_dir().join(format!("atlas-packs-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("here.exe");
        std::fs::write(&file, b"x").unwrap();
        let mut cat = Catalog::from_manifests(vec![known("tool", &file)]);
        cat.probe_all();
        assert_eq!(cat.health(&PackId::new("tool")), PackHealth::Ok);
        assert_eq!(cat.ready(ContractId::Chat), vec![PackId::new("tool")]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_timeout_is_unknown() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let _ = sock.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0_u8; 64];
            let _ = sock.read(&mut buf);
            std::thread::sleep(Duration::from_millis(400));
        });
        let mut cat = Catalog::from_manifests(vec![manifest(
            "slow",
            ContractId::Image,
            ProbeSpec::HttpGet {
                url: format!("http://127.0.0.1:{port}/"),
                timeout_ms: 50,
            },
        )]);
        cat.probe_all();
        let id = PackId::new("slow");
        assert_eq!(cat.health(&id), PackHealth::Unknown);
        assert!(cat.detail(&id).contains("timed out"), "{}", cat.detail(&id));
        server.join().unwrap();
    }

    #[test]
    fn retired_never_appears_in_ready() {
        let dir = std::env::temp_dir().join(format!("atlas-packs-ret-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("old.exe");
        std::fs::write(&file, b"x").unwrap();
        let mut retired = known("old", &file);
        retired.retired = true;
        let mut live = known("live", &file);
        live.id = PackId::new("live");
        live.title = "live".into();
        let mut cat = Catalog::from_manifests(vec![retired, live]);
        cat.probe_all();
        assert_eq!(cat.health(&PackId::new("old")), PackHealth::Retired);
        assert_eq!(cat.ready(ContractId::Chat), vec![PackId::new("live")]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failing_trial_is_absent_from_ready() {
        let root = std::env::temp_dir().join(format!("atlas-packs-trial-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let packs = root.join("packs");
        std::fs::create_dir_all(&packs).unwrap();
        let note = root.join("workbook.slate");
        std::fs::write(&note, b"document").unwrap();
        let trial = serde_json::json!({
            "id": "trial-tool",
            "title": "Trial",
            "contract": "chat",
            "contract_version": 1,
            "probe": {
                "kind": "known-path",
                "names": ["missing.bin"],
                "roots": [root]
            }
        });
        std::fs::write(packs.join("trial-tool.json"), trial.to_string()).unwrap();
        let mut cat = Catalog::load_at(&packs, None);
        cat.probe_all();
        assert!(cat
            .ready(ContractId::Chat)
            .iter()
            .all(|id| id.as_str() != "trial-tool"));
        assert_eq!(cat.health(&PackId::new("trial-tool")), PackHealth::Missing);
        cat.forget(&PackId::new("trial-tool")).unwrap();
        assert!(!packs.join("trial-tool.json").exists());
        assert_eq!(std::fs::read(&note).unwrap(), b"document");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_credential_miss_is_offered_and_not_ready() {
        let mut pack = manifest(
            "openai-image",
            ContractId::Image,
            ProbeSpec::Custom {
                id: "openai-key".into(),
            },
        );
        pack.title = "OpenAI".into();
        pack.credential = Some("openai-api-key".into());
        let mut cat = Catalog::from_manifests(vec![pack]);
        cat.register_custom("openai-key", || PackHealth::Missing);
        cat.probe_all();
        assert!(cat.ready(ContractId::Image).is_empty());
        let offers = cat.offers(ContractId::Image);
        assert_eq!(offers.len(), 1);
        assert!(offers[0].needs_key);
        assert_eq!(offers[0].title, "OpenAI");
    }

    #[test]
    fn shipped_manifests_cover_the_shelves_and_not_photocraft() {
        let shipped = crate::shipped_manifests();
        assert!(shipped.iter().any(|m| m.id.as_str() == "cursor"));
        assert!(shipped
            .iter()
            .any(|m| m.id.as_str() == "openai-image" && m.credential.is_some()));
        assert!(shipped.iter().all(|m| m.id.as_str() != "photocraft"));
        assert!(shipped.iter().any(|m| m.contract == ContractId::Page));
        assert!(shipped.iter().any(|m| m.contract == ContractId::Segment));
    }
}
