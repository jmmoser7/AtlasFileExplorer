//! Manifest types. Serde JSON is the trial format and the shipped format.

use serde::{Deserialize, Serialize};

/// Stable pack identity. Documents store this string; they do not store health.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PackId(pub String);

impl PackId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for PackId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// What a pack is allowed to fill. Versioned so a mismatch can paint Unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContractId {
    Chat,
    Image,
    Segment,
    Page,
    Program,
}

impl ContractId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Image => "image",
            Self::Segment => "segment",
            Self::Page => "page",
            Self::Program => "program",
        }
    }
}

/// Closed probe set. A new vendor is a new manifest, not a new variant here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ProbeSpec {
    KnownPath {
        names: Vec<String>,
        roots: Vec<String>,
    },
    CommandOnPath {
        command: String,
    },
    HttpGet {
        url: String,
        timeout_ms: u64,
    },
    Custom {
        id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackManifest {
    pub id: PackId,
    pub title: String,
    pub contract: ContractId,
    pub contract_version: u32,
    #[serde(default)]
    pub install_note: Option<String>,
    pub probe: ProbeSpec,
    /// Retired packs stay in the catalog and never appear in `ready`.
    #[serde(default)]
    pub retired: bool,
    /// Secret slot. A miss is "add a key", not "install a program".
    #[serde(default)]
    pub credential: Option<String>,
}
