//! Local adapter using the shared protocol admission/result owners. SDK bridge
//! files are internal job data; they are never connector replies or source paths.
use super::{command_descriptor, parse_parameters, HeadlessRhino, GENERATE_COMMAND};
use crate::{
    artifact::{receive_artifact, ArtifactSender, ArtifactStore},
    transport::{Cancellation, PipeAddress},
};
use atlas_connector::*;
use std::time::Duration;

pub struct HeadlessDraft {
    pub session: Session,
    runtime: HeadlessRhino,
    artifacts: ArtifactStore,
    published: Option<ArtifactTransfer>,
    source_bytes: Option<Vec<u8>>,
    source_origin: Option<std::path::PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft() -> HeadlessDraft {
        HeadlessDraft::new(
            HeadlessRhino {
                executable: "missing".into(),
                rhino_system: "missing".into(),
                workspace: "missing".into(),
                timeout: Duration::from_secs(180),
            },
            "headless-test".into(),
            1,
        )
        .unwrap()
    }
    fn request(
        id: &str,
        operation: Operation,
        source: Option<SourceScope>,
        caller: CallerAuthority,
    ) -> RequestEnvelope {
        RequestEnvelope {
            session_id: "headless-test".into(),
            generation: 1,
            request_id: id.into(),
            caller,
            source,
            operation,
        }
    }
    #[test]
    fn descriptor_uses_existing_scalar_dialect() {
        validate_parameter_schema(&command_descriptor().parameters).unwrap();
        assert!(parse_parameters(&serde_json::json!({"plan":"{}","script":"x"})).is_err());
    }
    #[test]
    fn agent_without_isolated_grant_never_launches_helper() {
        let mut draft = draft();
        let cancel = Cancellation::default();
        let source = SourceRef {
            source_id: "new-model".into(),
            revision: "empty".into(),
            locator: SourceLocator {
                root_id: None,
                relative: None,
                absolute: Some("isolated.3dm".into()),
            },
        };
        let human = CallerAuthority::Human {
            action_id: "bind".into(),
            target_source: None,
        };
        draft
            .handle(
                request(
                    "bind",
                    Operation::BindSource {
                        source: source.clone(),
                        mode: SourceMode::IsolatedDraft,
                        selection_id: "explicit".into(),
                    },
                    None,
                    human.clone(),
                ),
                &human,
                &cancel,
                |_| {},
            )
            .unwrap();
        let agent = CallerAuthority::Agent {
            agent_id: "agent".into(),
            workspace_autonomy: true,
        };
        let invoke = request(
            "generate",
            Operation::InvokeCommand {
                command_id: GENERATE_COMMAND.into(),
                parameters: serde_json::json!({"plan":"{}"}),
            },
            Some(source.scope()),
            agent.clone(),
        );
        assert!(draft
            .handle(invoke.clone(), &agent, &cancel, |_| panic!(
                "must not launch"
            ))
            .unwrap_err()
            .contains("grant"));
        draft
            .session
            .grant_draft_authority(
                "grant",
                "agent",
                &source.scope(),
                &[GENERATE_COMMAND.into()],
            )
            .unwrap();
        let reply = draft
            .handle(invoke, &agent, &cancel, |_| {
                panic!("invalid plan must not launch")
            })
            .unwrap();
        assert!(matches!(reply.outcome, ReplyBody::Failure { .. }));
    }
}

impl HeadlessDraft {
    pub fn new(
        runtime: HeadlessRhino,
        session_id: String,
        generation: u64,
    ) -> Result<Self, String> {
        let hello = Hello {
            protocol: PROTOCOL_FAMILY.into(),
            versions: vec![V0_2],
            identity: ConnectorIdentity {
                pack_id: "org.slate.rhino.headless".into(),
                application_id: "com.mcneel.rhino".into(),
                application_version: "8".into(),
                connector_version: "0.1.0".into(),
            },
            capabilities: CapabilityOffer {
                offered: vec![
                    "lifecycle".into(),
                    "sources".into(),
                    "commands".into(),
                    "publication".into(),
                ],
                required: vec!["lifecycle".into()],
            },
            surface_modes: vec![SurfaceMode::Headless],
            commands: vec![command_descriptor(), inspect_descriptor()],
            view_schemas: vec![],
        };
        let (session, _) =
            Session::negotiate(&HostPolicy::default(), &hello, session_id, generation)
                .map_err(|e| e.to_string())?;
        Ok(Self {
            session,
            runtime,
            artifacts: ArtifactStore::default(),
            published: None,
            source_bytes: None,
            source_origin: None,
        })
    }

    pub fn handle(
        &mut self,
        request: RequestEnvelope,
        authority: &CallerAuthority,
        cancel: &Cancellation,
        progress: impl FnMut(&str),
    ) -> Result<ReplyEnvelope, String> {
        match self
            .session
            .begin(request.clone(), authority)
            .map_err(|e| e.to_string())?
        {
            Begin::Replay(reply) => return Ok(reply),
            Begin::Dispatch(_) => {}
        }
        let result: Result<ReplyBody, String> = (|| {
            match &request.operation {
            Operation::BindSource{source,mode:SourceMode::IsolatedDraft,..} => Ok(ReplyBody::SourceBound { binding:SourceBinding { source:source.clone(),document_id:format!("draft-{}",request.session_id),mode:SourceMode::IsolatedDraft,dirty:false } }),
            Operation::DescribeCommands=>Ok(ReplyBody::Commands { commands:vec![command_descriptor(),inspect_descriptor()] }),
            Operation::InvokeCommand{command_id,..} if command_id=="org.slate.rhino.inspect" => {
                let report=if let Some(bytes)=self.source_bytes.as_deref() {
                    Some(self.runtime.generate_with_origin(&super::GeometryPlan{units:super::Units::Source,operations:vec![]},Some(bytes),self.source_origin.as_deref(),cancel,progress)?.report)
                } else {None};
                Ok(ReplyBody::CommandResult {value:serde_json::json!({"surface":"headless","binding":self.session.binding(),"model":report}),revision:None})
            },
            Operation::InvokeCommand{command_id,parameters} if command_id==GENERATE_COMMAND => {
                let plan=parse_parameters(parameters)?;
                let generated=self.runtime.generate_with_origin(&plan,self.source_bytes.as_deref(),self.source_origin.as_deref(),cancel,progress)?;
                let (report,artifact)=generated.retain(&mut self.artifacts,&format!("model-{}",request.request_id))?;
                let revision=artifact.sha256.clone();
                self.published=Some(artifact);
                Ok(ReplyBody::CommandResult { value:serde_json::to_value(report).map_err(|e|e.to_string())?,revision:Some(revision) })
            },
            Operation::Publish=>Ok(ReplyBody::Published { publication:PublicationResult {
                source:request.source.clone().ok_or("Missing source")?,behavior:PublicationBehavior::PortableArtifact,
                artifact:self.published.clone().ok_or("Generate a verified model before publication")?
            }}),
            Operation::UnbindSource=>Ok(ReplyBody::Ack),
            Operation::Detach{..}=>Ok(ReplyBody::Detached{preserved_unsaved:true,application_terminated:false}),
            _=>Err("Headless adapter supports isolated generation and publication only; source write-back and interactive surfaces are unavailable".into()),
        }
        })();
        let outcome = match result {
            Ok(body) => body,
            Err(message) if message.contains("uncertain") => {
                return self
                    .session
                    .timeout(&request.request_id)
                    .map_err(|e| e.to_string())
            }
            Err(message) => ReplyBody::Failure {
                error: ProtocolError::new(ErrorCode::InvalidResult, message),
            },
        };
        self.session
            .complete(&request.request_id, outcome)
            .map_err(|e| e.to_string())
    }

    /// Trusted source owner supplies a bounded immutable snapshot. Peer plans
    /// never choose or open a filesystem path.
    pub fn set_source_snapshot(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        if bytes.len() as u64 > MAX_ARTIFACT_BYTES
            || !bytes.starts_with(b"3D Geometry File Format ")
        {
            return Err("Invalid/bounded source snapshot required".into());
        }
        self.source_bytes = Some(bytes);
        Ok(())
    }
    pub fn set_source_origin(&mut self, origin: std::path::PathBuf) {
        self.source_origin = Some(origin);
    }

    /// Existing authenticated transfer and receiver receipt, even for the local
    /// adapter. Native callers consume verified bytes, never a bridge-owned path.
    pub fn receive_publication(
        &self,
        publication: &PublicationResult,
        cancel: &Cancellation,
    ) -> Result<Vec<u8>, String> {
        if self.published.as_ref() != Some(&publication.artifact)
            || self
                .session
                .binding()
                .is_none_or(|b| b.source.scope() != publication.source)
        {
            return Err("Publication source/revision/descriptor changed".into());
        }
        let bytes = self
            .artifacts
            .get(&publication.artifact.transfer_id)
            .ok_or("Publication bytes unavailable")?;
        let control = PipeAddress::fresh().map_err(|e| e.to_string())?;
        let pid = std::process::id();
        let sender = ArtifactSender::bind(&control, publication.artifact.clone(), bytes, pid)
            .map_err(|e| e.to_string())?;
        let sender_cancel = cancel.clone();
        let send = std::thread::spawn(move || {
            sender
                .send(Duration::from_secs(20), &sender_cancel)
                .map_err(|e| e.to_string())
        });
        let result = receive_artifact(
            &control,
            &publication.artifact,
            pid,
            MAX_ARTIFACT_BYTES as usize,
            Duration::from_secs(20),
            cancel,
        )
        .map_err(|e| e.to_string());
        let sent = send
            .join()
            .map_err(|_| "Publication sender stopped".to_string())?;
        sent?;
        result
    }
}

fn inspect_descriptor() -> CommandDescriptor {
    CommandDescriptor {
        id: "org.slate.rhino.inspect".into(),
        title: "Inspect headless Rhino draft".into(),
        effect: CommandEffect::Inspect,
        undo: UndoOwner::None,
        cancellable: true,
        agent_allowed: true,
        parameters: serde_json::json!({"type":"object","properties":{},"additionalProperties":false}),
    }
}
