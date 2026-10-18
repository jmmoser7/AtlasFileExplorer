//! Artifact generation adapts the existing Web host, input graph and sidecars.
use super::*;
use atlas_agent::{
    ArtifactCheckpoint, ArtifactProvenance, ArtifactRevision, ArtifactRevisionRef, ArtifactRun,
};
use slate_doc::scene::{artifact, artifact_mut, SceneCmd, SourceUri};

mod editor;
mod export;
mod generation;
mod pump;
mod spawn;
pub(crate) use export::{materialize_artifact_source, prepare_artifact_export_sources};
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct ArtifactUi {
    preparing: HashMap<NodeId, Preparing>,
    running: HashMap<NodeId, Running>,
    pending: HashMap<NodeId, Pending>,
    work: Option<(Sender<WorkDone>, Receiver<WorkDone>)>,
    inspect: HashSet<NodeId>,
    picks: HashMap<NodeId, String>,
    active_workers: usize,
}
struct Preparing {
    plan: Plan,
    inputs: super::input_prep::InputPreparation,
    started: bool,
}
#[derive(Clone)]
struct Plan {
    tab: u64,
    node: NodeId,
    request: AgentRequest,
    workspace: PathBuf,
    link_dir: PathBuf,
    history_dir: PathBuf,
    history_root: Option<PathBuf>,
    session: String,
    provider: String,
    family: String,
    parent: Option<ArtifactRevisionRef>,
    memory: Option<(ArtifactRevisionRef, usize)>,
    resume: bool,
    previous: Option<String>,
    tables: HashMap<u64, PathBuf>,
}
struct Prepared {
    plan: Plan,
    run: ArtifactRun,
    input_provenance: Vec<atlas_agent::ArtifactInputProvenance>,
}
struct Running {
    prepared: Prepared,
    publishing: bool,
}
struct Pending {
    tab: u64,
    previous: Option<String>,
    reference: ArtifactRevisionRef,
    revision: ArtifactRevision,
}
enum WorkDone {
    Prepared(u64, NodeId, String, Result<Prepared, String>),
    Published(u64, NodeId, String, Result<Pending, String>),
    Picked(
        u64,
        NodeId,
        String,
        Option<String>,
        Result<(ArtifactRevisionRef, ArtifactRevision), String>,
    ),
}

impl SlateApp {
    pub(crate) fn is_artifact_node(&self, id: NodeId) -> bool {
        self.doc().scene.node(id).and_then(artifact).is_some()
    }
    pub(crate) fn artifact_is_busy(&self, id: NodeId) -> bool {
        self.agents.artifact.preparing.contains_key(&id)
            || self.agents.artifact.running.contains_key(&id)
            || self.artifact_revision_loading(id)
    }
    pub(crate) fn artifact_revision_loading(&self, id: NodeId) -> bool {
        self.agents.artifact.picks.contains_key(&id)
    }
    pub(crate) fn artifact_request_submitted(&self, id: NodeId) -> bool {
        self.agents.artifact.running.contains_key(&id)
    }
    fn artifact_reference(
        &self,
        id: NodeId,
        revision: &str,
    ) -> Result<ArtifactRevisionRef, String> {
        let binding = self
            .doc()
            .scene
            .node(id)
            .and_then(artifact)
            .ok_or("Artifact source is missing")?;
        let source = binding
            .history_root
            .as_ref()
            .ok_or("Artifact history is missing")?;
        atlas_ai::versions::artifact_revision_reference(
            &resolve_source(self.tab().path.as_deref(), &source.locator),
            revision,
        )
        .map_err(|e| e.to_string())
    }
    fn artifact_work_tx(&mut self) -> Sender<WorkDone> {
        self.agents
            .artifact
            .work
            .get_or_insert_with(unbounded)
            .0
            .clone()
    }
    pub(crate) fn artifact_preview_source(&self, id: NodeId) -> Option<SourceUri> {
        let pending = self
            .agents
            .artifact
            .pending
            .get(&id)
            .filter(|p| p.tab == self.tab().id)?;
        Some(SourceUri {
            locator: super::super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &pending.reference.materialized_entry,
            ),
        })
    }
}
