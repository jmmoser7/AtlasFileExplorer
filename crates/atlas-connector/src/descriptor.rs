use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CommandDescriptor, SurfaceMode, ViewSchema};

pub const PROTOCOL_FAMILY: &str = "slate.application";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

pub const V0_1: ProtocolVersion = ProtocolVersion { major: 0, minor: 1 };
/// Explicitly granted, isolated-draft agent commands; the wire shapes are unchanged.
pub const V0_2: ProtocolVersion = ProtocolVersion { major: 0, minor: 2 };

/// Capability groups are independent. Unknown optional offers are ignored;
/// unknown required offers cause negotiation to fail.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityGroup {
    Lifecycle,
    Sources,
    Surfaces,
    Commands,
    Views,
    Capture,
    Publication,
}

impl CapabilityGroup {
    pub const ALL: [Self; 7] = [
        Self::Lifecycle,
        Self::Sources,
        Self::Surfaces,
        Self::Commands,
        Self::Views,
        Self::Capture,
        Self::Publication,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lifecycle => "lifecycle",
            Self::Sources => "sources",
            Self::Surfaces => "surfaces",
            Self::Commands => "commands",
            Self::Views => "views",
            Self::Capture => "capture",
            Self::Publication => "publication",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|group| group.as_str() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityOffer {
    pub offered: Vec<String>,
    #[serde(default)]
    pub required: Vec<String>,
}

impl Default for CapabilityOffer {
    fn default() -> Self {
        Self {
            offered: CapabilityGroup::ALL
                .iter()
                .map(|g| g.as_str().into())
                .collect(),
            required: vec!["lifecycle".into()],
        }
    }
}

/// Installation claim only: this is not launch authorization or connection health.
/// `registration` names machine-local registration; never an executable command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConnectorDescriptor {
    pub protocol: String,
    pub versions: Vec<ProtocolVersion>,
    pub registration: String,
    #[serde(default)]
    pub facets: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<CapabilityGroup>,
}

impl ConnectorDescriptor {
    pub fn validate(&self) -> Result<(), crate::ProtocolError> {
        use crate::{ErrorCode, ProtocolError};
        if self.protocol != PROTOCOL_FAMILY || self.versions.is_empty() || self.versions.len() > 32
        {
            return Err(ProtocolError::new(
                ErrorCode::InvalidDescriptor,
                "invalid protocol family or version offer",
            ));
        }
        if !crate::validation::valid_id(&self.registration) {
            return Err(ProtocolError::new(
                ErrorCode::InvalidDescriptor,
                "registration must be a bounded machine-local name, not a path or command",
            ));
        }
        if self.facets.len() > 64
            || self.extensions.len() > 128
            || self
                .facets
                .iter()
                .chain(self.extensions.iter())
                .any(|s| !crate::validation::valid_id(s))
        {
            return Err(ProtocolError::new(
                ErrorCode::InvalidDescriptor,
                "invalid or unbounded format/facet identifiers",
            ));
        }
        Ok(())
    }

    /// v0 uses exact version intersection; it does not assume minor revisions
    /// are compatible. A future compatibility rule has one owner here.
    pub fn supports_version(&self, version: ProtocolVersion) -> bool {
        self.validate().is_ok() && self.versions.contains(&version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConnectorIdentity {
    pub pack_id: String,
    pub application_id: String,
    pub application_version: String,
    pub connector_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Hello {
    pub protocol: String,
    pub versions: Vec<ProtocolVersion>,
    pub identity: ConnectorIdentity,
    pub capabilities: CapabilityOffer,
    #[serde(default)]
    pub surface_modes: Vec<SurfaceMode>,
    #[serde(default)]
    pub commands: Vec<CommandDescriptor>,
    #[serde(default)]
    pub view_schemas: Vec<ViewSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Welcome {
    pub protocol: String,
    pub version: ProtocolVersion,
    pub identity: ConnectorIdentity,
    pub session_id: String,
    pub generation: u64,
    pub capabilities: Vec<CapabilityGroup>,
}

/// Host policy comes from trusted machine state, never from a workbook or peer.
#[derive(Debug, Clone)]
pub struct HostPolicy {
    pub versions: Vec<ProtocolVersion>,
    pub capabilities: Vec<CapabilityGroup>,
    pub required: Vec<CapabilityGroup>,
    /// Includes pending requests and terminal tombstones. Full sessions refuse
    /// new IDs instead of evicting a tombstone and accidentally replaying work.
    pub max_cached_requests: usize,
    /// Retained serialized request/receipt budget, including reserved terminal
    /// tombstones. Exhaustion refuses new work without evicting old IDs.
    pub max_cached_bytes: usize,
}

impl Default for HostPolicy {
    fn default() -> Self {
        Self {
            versions: vec![V0_2, V0_1],
            capabilities: CapabilityGroup::ALL.to_vec(),
            required: vec![CapabilityGroup::Lifecycle],
            max_cached_requests: 1024,
            max_cached_bytes: 8 * 1024 * 1024,
        }
    }
}
