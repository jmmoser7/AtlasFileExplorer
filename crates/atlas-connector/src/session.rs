use std::collections::{BTreeMap, BTreeSet};

use crate::{validation::*, *};

pub(crate) const RECEIPT_RESERVE: usize = 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Begin {
    Dispatch(RequestEnvelope),
    Replay(ReplyEnvelope),
}

#[derive(Debug)]
pub(crate) struct Entry {
    pub request: RequestEnvelope,
    pub reply: Option<ReplyEnvelope>,
    pub cancelled: bool,
    pub bytes: usize,
}

/// Pure admission/receipt state. The host must authenticate the channel and
/// derive `CallerAuthority` from its actual user/agent action independently of
/// the wire. Native edits remain owned by the foreign application's journal.
#[derive(Debug)]
pub struct Session {
    pub(crate) welcome: Welcome,
    pub(crate) commands: BTreeMap<String, CommandDescriptor>,
    pub(crate) schemas: Vec<ViewSchema>,
    pub(crate) modes: Vec<SurfaceMode>,
    pub(crate) binding: Option<SourceBinding>,
    pub(crate) surface: Option<SurfaceDescriptor>,
    pub(crate) visible: bool,
    pub(crate) focused: bool,
    pub(crate) detached: bool,
    pub(crate) entries: BTreeMap<String, Entry>,
    pub(crate) max_entries: usize,
    pub(crate) max_bytes: usize,
    pub(crate) retained_bytes: usize,
    pub(crate) draft_grants: BTreeMap<String, DraftAuthorityGrant>,
}

impl Session {
    pub fn negotiate(
        policy: &HostPolicy,
        hello: &Hello,
        session_id: String,
        generation: u64,
    ) -> Result<(Self, Welcome), ProtocolError> {
        if hello.protocol != PROTOCOL_FAMILY {
            return Err(error(
                ErrorCode::IncompatibleProtocol,
                "unknown protocol family",
            ));
        }
        if !valid_id(&session_id)
            || generation == 0
            || policy.max_cached_requests == 0
            || policy.max_cached_bytes < 2 * RECEIPT_RESERVE
            || hello.versions.is_empty()
            || hello.versions.len() > 32
            || hello.capabilities.offered.len() > 32
            || hello.capabilities.required.len() > 32
            || hello.commands.len() > 256
            || hello.view_schemas.len() > 32
            || hello.surface_modes.len() > 4
        {
            return Err(error(
                ErrorCode::InvalidDescriptor,
                "invalid or unbounded handshake",
            ));
        }
        validate_identity(&hello.identity)?;
        let version = policy
            .versions
            .iter()
            .copied()
            .filter(|v| matches!(*v, V0_1 | V0_2) && hello.versions.contains(v))
            .max()
            .ok_or_else(|| {
                error(
                    ErrorCode::IncompatibleProtocol,
                    "no implemented version in exact version intersection",
                )
            })?;
        let offered: BTreeSet<_> = hello
            .capabilities
            .offered
            .iter()
            .map(String::as_str)
            .collect();
        if hello
            .capabilities
            .offered
            .iter()
            .chain(hello.capabilities.required.iter())
            .any(|s| !valid_id(s))
        {
            return Err(error(
                ErrorCode::InvalidDescriptor,
                "invalid capability name",
            ));
        }
        let capabilities: Vec<_> = CapabilityGroup::ALL
            .into_iter()
            .filter(|g| offered.contains(g.as_str()) && policy.capabilities.contains(g))
            .collect();
        if !capabilities.contains(&CapabilityGroup::Lifecycle)
            || policy.required.iter().any(|g| !capabilities.contains(g))
            || hello
                .capabilities
                .required
                .iter()
                .any(|name| CapabilityGroup::parse(name).is_none_or(|g| !capabilities.contains(&g)))
        {
            return Err(error(
                ErrorCode::RequiredCapabilityMissing,
                "a required capability is unknown, unoffered or disallowed",
            ));
        }
        if capabilities
            .iter()
            .any(|group| !matches!(group, CapabilityGroup::Lifecycle | CapabilityGroup::Sources))
            && !capabilities.contains(&CapabilityGroup::Sources)
        {
            return Err(error(
                ErrorCode::RequiredCapabilityMissing,
                "source-scoped capability groups require sources plus lifecycle",
            ));
        }
        if capabilities.contains(&CapabilityGroup::Surfaces) && hello.surface_modes.is_empty() {
            return Err(error(
                ErrorCode::InvalidDescriptor,
                "surface capability requires an advertised mode",
            ));
        }
        let mut commands = BTreeMap::new();
        for command in &hello.commands {
            validate_command(command)?;
            if commands
                .insert(command.id.clone(), command.clone())
                .is_some()
            {
                return Err(error(
                    ErrorCode::InvalidDescriptor,
                    "duplicate command identifier",
                ));
            }
        }
        for schema in &hello.view_schemas {
            validate_schema(schema)?;
        }
        let welcome = Welcome {
            protocol: PROTOCOL_FAMILY.into(),
            version,
            identity: hello.identity.clone(),
            session_id,
            generation,
            capabilities,
        };
        let session = Self {
            welcome: welcome.clone(),
            commands,
            schemas: hello.view_schemas.clone(),
            modes: hello.surface_modes.clone(),
            binding: None,
            surface: None,
            visible: false,
            focused: false,
            detached: false,
            entries: BTreeMap::new(),
            max_entries: policy.max_cached_requests,
            max_bytes: policy.max_cached_bytes,
            retained_bytes: 0,
            draft_grants: BTreeMap::new(),
        };
        Ok((session, welcome))
    }

    pub fn welcome(&self) -> &Welcome {
        &self.welcome
    }
    pub fn binding(&self) -> Option<&SourceBinding> {
        self.binding.as_ref()
    }
    pub fn surface(&self) -> Option<&SurfaceDescriptor> {
        self.surface.as_ref()
    }
    pub fn focused(&self) -> bool {
        self.focused
    }
    pub fn visible(&self) -> bool {
        self.visible
    }
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    pub fn commands(&self) -> impl Iterator<Item = &CommandDescriptor> {
        self.commands.values()
    }
    pub fn view_schemas(&self) -> &[ViewSchema] {
        &self.schemas
    }

    pub fn state(&self, request_id: &str) -> Option<OperationState> {
        self.entries.get(request_id).map(|entry| {
            match entry.reply.as_ref().map(|reply| &reply.outcome) {
                Some(ReplyBody::OutcomeUnknown { .. }) => OperationState::OutcomeUnknown,
                Some(_) => OperationState::Completed,
                None if entry.cancelled => OperationState::CancellationRequested,
                None => OperationState::Pending,
            }
        })
    }

    /// A replacement connection gets a fresh generation and must bind its source
    /// again. No prior operation is automatically replayed, including uncertain ones.
    pub fn reconnect(&mut self, generation: u64) -> Result<Welcome, ProtocolError> {
        if generation <= self.welcome.generation {
            return Err(error(
                ErrorCode::StaleGeneration,
                "reconnect requires a newer generation",
            ));
        }
        self.welcome.generation = generation;
        self.entries.clear();
        self.draft_grants.clear();
        self.retained_bytes = 0;
        self.binding = None;
        self.surface = None;
        self.focused = false;
        self.visible = false;
        self.detached = false;
        Ok(self.welcome.clone())
    }

    pub(crate) fn check_envelope(
        &self,
        session_id: &str,
        generation: u64,
    ) -> Result<(), ProtocolError> {
        if session_id != self.welcome.session_id {
            return Err(error(
                ErrorCode::SessionMismatch,
                "session identity does not match",
            ));
        }
        if generation != self.welcome.generation {
            return Err(error(
                ErrorCode::StaleGeneration,
                "generation does not match",
            ));
        }
        Ok(())
    }

    pub(crate) fn check_scope(&self, scope: Option<&SourceScope>) -> Result<(), ProtocolError> {
        let binding = self
            .binding
            .as_ref()
            .ok_or_else(|| error(ErrorCode::SourceNotBound, "no explicitly bound source"))?;
        let scope = scope.ok_or_else(|| {
            error(
                ErrorCode::SourceMismatch,
                "operation requires an explicit source scope",
            )
        })?;
        if scope.source_id != binding.source.source_id {
            return Err(error(ErrorCode::SourceMismatch, "wrong source identity"));
        }
        if scope.revision != binding.source.revision {
            return Err(error(ErrorCode::StaleRevision, "source revision changed"));
        }
        Ok(())
    }

    /// Only the adapter's observed document state should call this. Updating it
    /// invalidates pending old-revision results without inventing revision order.
    pub fn observe_source_state(
        &mut self,
        scope: &SourceScope,
        dirty: bool,
    ) -> Result<(), ProtocolError> {
        let binding = self
            .binding
            .as_mut()
            .ok_or_else(|| error(ErrorCode::SourceNotBound, "no source"))?;
        if binding.source.source_id != scope.source_id {
            return Err(error(ErrorCode::SourceMismatch, "wrong observed source"));
        }
        if scope.revision.is_empty() || scope.revision.len() > 256 {
            return Err(error(ErrorCode::InvalidRequest, "invalid revision"));
        }
        binding.source.revision.clone_from(&scope.revision);
        binding.dirty = dirty;
        Ok(())
    }
}
