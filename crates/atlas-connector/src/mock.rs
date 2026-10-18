//! Deterministic conformance adapter. It models a headless external application,
//! performs no filesystem reads/edits, and requires real transfer descriptors
//! supplied by its runtime before advertising capture/publication.

use crate::*;

pub const MOCK_INSPECT_COMMAND: &str = "org.slate.mock.inspect";
pub const MOCK_WRITE_COMMAND: &str = "org.slate.mock.write_back";

#[derive(Debug)]
pub struct MockConnector {
    identity: ConnectorIdentity,
    session: Option<Session>,
    artifact: Option<ArtifactTransfer>,
    adversarial_write_offer: bool,
}

impl MockConnector {
    pub fn new(identity: ConnectorIdentity) -> Self {
        Self {
            identity,
            session: None,
            artifact: None,
            adversarial_write_offer: false,
        }
    }

    /// Negative-test mode deliberately advertises an overreaching command.
    /// Admission must deny agents regardless of this claim; mock human calls
    /// receive UnsupportedOperation, since this adapter never performs edits.
    pub fn enable_adversarial_write_offer(&mut self) -> Result<(), ProtocolError> {
        if self.session.is_some() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "configure negative-test offers before negotiation",
            ));
        }
        self.adversarial_write_offer = true;
        Ok(())
    }

    pub fn set_artifact(&mut self, artifact: ArtifactTransfer) -> Result<(), ProtocolError> {
        artifact.validate()?;
        if self.session.is_some() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "configure transfers before negotiation",
            ));
        }
        self.artifact = Some(artifact);
        Ok(())
    }

    pub fn hello(&self) -> Hello {
        let mut capabilities = CapabilityOffer::default();
        if self.artifact.is_none() {
            capabilities
                .offered
                .retain(|s| s != "capture" && s != "publication");
        }
        let mut hello = Hello {
            protocol: PROTOCOL_FAMILY.into(),
            versions: vec![V0_1],
            identity: self.identity.clone(),
            capabilities,
            surface_modes: vec![SurfaceMode::Headless],
            commands: vec![CommandDescriptor {
                id: MOCK_INSPECT_COMMAND.into(),
                title: "Inspect mock source".into(),
                effect: CommandEffect::Inspect,
                undo: UndoOwner::None,
                cancellable: true,
                agent_allowed: true,
                parameters: serde_json::json!({"type":"object", "properties":{}, "additionalProperties":false}),
            }],
            view_schemas: vec![ViewSchema {
                namespace: format!("{}.view", self.identity.application_id),
                version: 1,
            }],
        };
        if self.adversarial_write_offer {
            let mut command = hello.commands[0].clone();
            command.id = MOCK_WRITE_COMMAND.into();
            command.title = "Adversarial authority test; execution unsupported".into();
            command.effect = CommandEffect::SourceWriteBack;
            command.undo = UndoOwner::NativeApplication;
            hello.commands.push(command);
        }
        hello
    }

    pub fn negotiate(
        &mut self,
        policy: &HostPolicy,
        session_id: String,
        generation: u64,
    ) -> Result<Welcome, ProtocolError> {
        let (session, welcome) = Session::negotiate(policy, &self.hello(), session_id, generation)?;
        self.session = Some(session);
        Ok(welcome)
    }

    /// External plugin-side acceptance of the host's response to our Hello.
    /// Runtime must first verify the host process/channel identity.
    pub fn accept_welcome(&mut self, welcome: &Welcome) -> Result<(), ProtocolError> {
        if welcome.identity != self.identity
            || welcome.protocol != PROTOCOL_FAMILY
            || welcome.version != V0_1
        {
            return Err(ProtocolError::new(
                ErrorCode::IncompatibleProtocol,
                "welcome does not match connector identity/version",
            ));
        }
        let policy = HostPolicy {
            versions: vec![welcome.version],
            capabilities: welcome.capabilities.clone(),
            ..HostPolicy::default()
        };
        let (session, actual) = Session::negotiate(
            &policy,
            &self.hello(),
            welcome.session_id.clone(),
            welcome.generation,
        )?;
        if actual != *welcome {
            return Err(ProtocolError::new(
                ErrorCode::RequiredCapabilityMissing,
                "welcome includes unoffered capabilities",
            ));
        }
        self.session = Some(session);
        Ok(())
    }

    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }
    pub fn session_mut(&mut self) -> Option<&mut Session> {
        self.session.as_mut()
    }

    pub fn handle(
        &mut self,
        request: RequestEnvelope,
        authority: &CallerAuthority,
    ) -> ReplyEnvelope {
        let Some(session) = self.session.as_mut() else {
            return ReplyEnvelope::failure(
                &request,
                ProtocolError::new(ErrorCode::SessionMismatch, "negotiate first"),
            );
        };
        match session.begin(request.clone(), authority) {
            Ok(Begin::Replay(reply)) => return reply,
            Ok(Begin::Dispatch(_)) => {}
            Err(error) => return ReplyEnvelope::failure(&request, error),
        }
        let outcome = match &request.operation {
            Operation::BindSource { source, mode, .. } => ReplyBody::SourceBound {
                binding: SourceBinding {
                    source: source.clone(),
                    document_id: "mock-document".into(),
                    mode: *mode,
                    dirty: false,
                },
            },
            Operation::DescribeCommands => ReplyBody::Commands {
                commands: session.commands().cloned().collect(),
            },
            Operation::DescribeSurfaces => ReplyBody::Surfaces {
                surfaces: Vec::new(),
            },
            Operation::InvokeCommand { command_id, .. } if command_id != MOCK_INSPECT_COMMAND => {
                ReplyBody::Failure {
                    error: ProtocolError::new(
                        ErrorCode::UnsupportedOperation,
                        "adversarial mock descriptor is a refusal fixture; no native edits are implemented",
                    ),
                }
            }
            Operation::InvokeCommand { .. } => ReplyBody::CommandResult {
                value: serde_json::json!({
                "application_id": self.identity.application_id, "source": request.source }),
                revision: None,
            },
            Operation::DescribeViewSchemas => ReplyBody::ViewSchemas {
                schemas: session.view_schemas().to_vec(),
            },
            Operation::SnapshotView { schema } => ReplyBody::View {
                view: mock_view(&request, schema),
            },
            Operation::Capture { schema } => ReplyBody::Captured {
                capture: CaptureResult {
                    metadata: CaptureMetadata {
                        source: request.source.clone().expect("admitted scope"),
                        view: mock_view(&request, schema),
                        captured_at: "2026-10-16T12:00:00Z".into(),
                        width_px: 1,
                        height_px: 1,
                        consistency: CaptureConsistency::Atomic,
                    },
                    artifact: self
                        .artifact
                        .clone()
                        .expect("capture capability requires configured artifact"),
                },
            },
            Operation::Publish => ReplyBody::Published {
                publication: PublicationResult {
                    source: request.source.clone().expect("admitted scope"),
                    behavior: PublicationBehavior::PosterAndPointer,
                    artifact: self
                        .artifact
                        .clone()
                        .expect("publication capability requires configured artifact"),
                },
            },
            Operation::Detach { .. } => ReplyBody::Detached {
                preserved_unsaved: true,
                application_terminated: false,
            },
            Operation::Cancel { request_id } => ReplyBody::CancellationRequested {
                target_request_id: request_id.clone(),
                rollback: false,
            },
            Operation::OperationStatus { request_id } => ReplyBody::OperationState {
                target_request_id: request_id.clone(),
                state: session.state(request_id).expect("admitted status target"),
            },
            Operation::UnbindSource | Operation::Surface { .. } | Operation::RestoreView { .. } => {
                ReplyBody::Ack
            }
        };
        session
            .complete(&request.request_id, outcome)
            .unwrap_or_else(|error| ReplyEnvelope::failure(&request, error))
    }
}

fn mock_view(request: &RequestEnvelope, schema: &ViewSchema) -> ViewSnapshot {
    ViewSnapshot {
        source: request.source.clone().expect("admitted scope"),
        schema: schema.namespace.clone(),
        schema_version: schema.version,
        fidelity: ViewFidelity::ExactNative,
        state: serde_json::json!({"mock_view": "front"}),
    }
}
