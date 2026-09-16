//! Agent staging contract: agents propose ordinary scene commands, and humans
//! accept or reject them as one attributed journal group.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use crate::scene::{CmdAuthor, NodeId, Scene, SceneCmd, SceneJournal};
use crate::SlateDoc;

const READ_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalTarget {
    pub workbook: Option<PathBuf>,
    pub format_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    #[default]
    Pending,
    Accepted,
    Rejected,
    Stale,
    /// An interrupted acceptance must be reviewed, never automatically replayed.
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub author: String,
    pub title: String,
    pub created_at: i64,
    pub target: ProposalTarget,
    pub cmds: Vec<SceneCmd>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default)]
    pub status: ProposalStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaleReason {
    UnsupportedFormat { found: u32, expected: u32 },
    EmptyProposal,
    CommandRejected,
    UnsavedWorkbook,
    WorkbookMismatch,
    NotPending,
    NodeChanged { node: NodeId },
}

impl std::fmt::Display for StaleReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat { found, expected } => {
                write!(f, "format {found} is unsupported (expected {expected})")
            }
            Self::EmptyProposal => f.write_str("the proposal contains no commands"),
            Self::CommandRejected => f.write_str("a command no longer applies to this board"),
            Self::UnsavedWorkbook => {
                f.write_str("save the workbook and request a new proposal with its saved path")
            }
            Self::WorkbookMismatch => f.write_str("the proposal targets a different workbook"),
            Self::NotPending => f.write_str("the proposal is no longer pending"),
            Self::NodeChanged { node } => write!(
                f,
                "node {} changed after this proposal was prepared",
                node.0
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalResult {
    /// Durable replay barrier; a crash here leaves acceptance unconfirmed.
    Applying,
    Accepted,
    Rejected,
    Stale {
        reason: StaleReason,
    },
}

pub fn accept(
    proposal: &Proposal,
    workbook: Option<&Path>,
    scene: &mut Scene,
    journal: &mut SceneJournal,
) -> Result<(), StaleReason> {
    let cmds = prepared_cmds(proposal, workbook, scene)?;
    if journal.commit_as(scene, cmds, CmdAuthor::Agent(proposal.author.clone())) {
        Ok(())
    } else {
        Err(StaleReason::CommandRejected)
    }
}

fn prepared_cmds(
    proposal: &Proposal,
    workbook: Option<&Path>,
    scene: &Scene,
) -> Result<Vec<SceneCmd>, StaleReason> {
    if proposal.target.format_version != SlateDoc::CURRENT {
        return Err(StaleReason::UnsupportedFormat {
            found: proposal.target.format_version,
            expected: SlateDoc::CURRENT,
        });
    }
    let (Some(target), Some(workbook)) = (proposal.target.workbook.as_deref(), workbook) else {
        return Err(StaleReason::UnsavedWorkbook);
    };
    if !same_workbook(target, workbook) {
        return Err(StaleReason::WorkbookMismatch);
    }
    if proposal.status != ProposalStatus::Pending {
        return Err(StaleReason::NotPending);
    }
    if proposal.cmds.is_empty() {
        return Err(StaleReason::EmptyProposal);
    }

    let mut check = scene.clone();
    let mut cmds = Vec::with_capacity(proposal.cmds.len());
    for original in &proposal.cmds {
        // Check the evolving proposal state: a later command may legitimately
        // address a node added or patched by an earlier command in this group.
        let cmd = match original {
            SceneCmd::Add { node, .. } => SceneCmd::Add {
                index: check.nodes.len(),
                node: node.clone(),
            },
            SceneCmd::Patch { before, .. } => {
                if check.node(before.id) != Some(before.as_ref()) {
                    return Err(StaleReason::NodeChanged { node: before.id });
                }
                original.clone()
            }
            SceneCmd::Remove { node, .. } => {
                if check.node(node.id) != Some(node) {
                    return Err(StaleReason::NodeChanged { node: node.id });
                }
                original.clone()
            }
        };
        if !check.apply(&cmd) {
            return Err(StaleReason::CommandRejected);
        }
        cmds.push(cmd);
    }
    Ok(cmds)
}

// Identity comparison is lexical and never touches a potentially remote source.
// Agents should echo the path in context.json; aliases through symlinks are not
// resolved here. Unsaved documents have no usable identity in the v1 contract.
fn same_workbook(a: &Path, b: &Path) -> bool {
    let normalize = |path: &Path| {
        let mut out = PathBuf::new();
        for part in path.components() {
            match part {
                Component::CurDir => {}
                Component::ParentDir
                    if matches!(out.components().next_back(), Some(Component::Normal(_))) =>
                {
                    out.pop();
                }
                other => out.push(other.as_os_str()),
            }
        }
        out
    };
    let (a, b) = (normalize(a), normalize(b));
    if cfg!(windows) {
        a.as_os_str()
            .as_encoded_bytes()
            .eq_ignore_ascii_case(b.as_os_str().as_encoded_bytes())
    } else {
        a == b
    }
}

#[derive(Debug)]
pub struct AcceptRecordError {
    /// True only when the journal commit completed but final confirmation failed.
    pub applied: bool,
    pub error: io::Error,
}

/// Persist a replay barrier before committing, then confirm the decision.
/// Disk and the in-memory journal are not an atomic transaction: interruption
/// leaves `Applying`, which requires human review instead of automatic replay.
pub fn accept_and_record(
    proposal: &Proposal,
    workbook: Option<&Path>,
    scene: &mut Scene,
    journal: &mut SceneJournal,
    ai_workspace: &Path,
) -> Result<ProposalResult, AcceptRecordError> {
    accept_and_record_with_writer(proposal, workbook, scene, journal, ai_workspace, |result| {
        write_result(ai_workspace, &proposal.id, result)
    })
}

fn accept_and_record_with_writer(
    proposal: &Proposal,
    workbook: Option<&Path>,
    scene: &mut Scene,
    journal: &mut SceneJournal,
    ai_workspace: &Path,
    mut write: impl FnMut(&ProposalResult) -> io::Result<()>,
) -> Result<ProposalResult, AcceptRecordError> {
    let before = |error| AcceptRecordError {
        applied: false,
        error,
    };
    if read_result(ai_workspace, &proposal.id)
        .map_err(before)?
        .is_some()
    {
        return Err(before(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "a decision already exists; review the workbook before dismissing this proposal",
        )));
    }
    let cmds = match prepared_cmds(proposal, workbook, scene) {
        Ok(cmds) => cmds,
        Err(reason @ (StaleReason::WorkbookMismatch | StaleReason::UnsavedWorkbook)) => {
            // Changing tabs or saving is a recoverable UI action. Do not
            // consume a proposal that may still apply in its intended workbook.
            return Err(before(io::Error::new(
                io::ErrorKind::InvalidInput,
                reason.to_string(),
            )));
        }
        Err(reason) => {
            let result = ProposalResult::Stale { reason };
            write(&result).map_err(before)?;
            return Ok(result);
        }
    };
    write(&ProposalResult::Applying).map_err(before)?;
    if !journal.commit_as(scene, cmds, CmdAuthor::Agent(proposal.author.clone())) {
        return Err(before(io::Error::other(
            "acceptance could not finish; review the workbook before dismissing this proposal",
        )));
    }
    write(&ProposalResult::Accepted).map_err(|error| AcceptRecordError {
        applied: true,
        error,
    })?;
    Ok(ProposalResult::Accepted)
}

pub fn reject(_proposal: &Proposal) -> ProposalResult {
    ProposalResult::Rejected
}

pub fn stage_dir(ai_workspace: &Path) -> PathBuf {
    ai_workspace.join(".atlas-ai").join("stage")
}

pub fn result_path(ai_workspace: &Path, id: &str) -> PathBuf {
    stage_dir(ai_workspace).join(format!("{id}.result.json"))
}

pub fn write_result(ai_workspace: &Path, id: &str, result: &ProposalResult) -> std::io::Result<()> {
    validate_id(id)?;
    let dir = stage_dir(ai_workspace);
    std::fs::create_dir_all(&dir)?;
    let path = result_path(ai_workspace, id);
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(result)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(tmp, path)
}

fn validate_id(id: &str) -> io::Result<()> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "proposal id must be a file name using letters, digits, dots, hyphens or underscores",
        ));
    }
    Ok(())
}

pub fn read_result(ai_workspace: &Path, id: &str) -> io::Result<Option<ProposalResult>> {
    validate_id(id)?;
    match std::fs::read(result_path(ai_workspace, id)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[derive(Debug, Default)]
pub struct StageWatcher {
    last_read_attempt: Option<Instant>,
    mtimes: BTreeMap<PathBuf, (SystemTime, Option<SystemTime>)>,
}

impl StageWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Polls `<ai-workspace>/.atlas-ai/stage/*.json` at most once per second.
    /// Final decisions suppress replay, including after restart. An unfinished
    /// or unreadable decision is returned as `RecoveryRequired`, never Pending.
    pub fn tick_read(&mut self, ai_workspace: &Path) -> Vec<Proposal> {
        if let Some(t) = self.last_read_attempt {
            if t.elapsed() < READ_INTERVAL {
                return Vec::new();
            }
        }
        self.last_read_attempt = Some(Instant::now());

        let dir = stage_dir(ai_workspace);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            self.mtimes.clear();
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if name.ends_with(".result.json") || name.ends_with(".tmp") {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let Ok(mtime) = metadata.modified() else {
                continue;
            };
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let result_mtime = std::fs::metadata(result_path(ai_workspace, id))
                .and_then(|m| m.modified())
                .ok();
            let stamp = (mtime, result_mtime);
            if self.mtimes.get(&path).copied() == Some(stamp) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(mut proposal) = serde_json::from_str::<Proposal>(&text) else {
                continue;
            };
            if proposal.id != id || validate_id(id).is_err() {
                continue;
            }
            let decision = read_result(ai_workspace, id);
            self.mtimes.insert(path, stamp);
            match decision {
                Ok(Some(ProposalResult::Applying)) | Err(_) => {
                    proposal.status = ProposalStatus::RecoveryRequired
                }
                Ok(Some(_)) => continue,
                Ok(None) => {}
            }
            out.push(proposal);
        }
        out
    }

    #[cfg(test)]
    fn force_elapsed(&mut self) {
        self.last_read_attempt = Some(Instant::now() - READ_INTERVAL - Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Corner, NodeKind, Rgba, ShapeKind, ShapeNode, Stroke, WorldRect};

    fn temp_workspace(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "slate_stage_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_node(scene: &mut Scene) -> crate::scene::Node {
        scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 80.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: Some(Rgba([30, 40, 50, 255])),
                stroke: Stroke::none(),
                corner: Corner::default(),
                flip: false,
                path: None,
            }),
        )
    }

    fn proposal(cmds: Vec<SceneCmd>) -> Proposal {
        Proposal {
            id: "p1".into(),
            author: "cursor-agent".into(),
            title: "Test proposal".into(),
            created_at: 1,
            target: ProposalTarget {
                workbook: Some(target_workbook().to_path_buf()),
                format_version: SlateDoc::CURRENT,
            },
            cmds,
            session: Some("agent-test".into()),
            status: ProposalStatus::Pending,
        }
    }

    fn target_workbook() -> &'static Path {
        Path::new("/audit/test.slate")
    }

    #[test]
    fn accept_commits_as_agent_author() {
        let mut scene = Scene::default();
        let node = sample_node(&mut scene);
        let p = proposal(vec![SceneCmd::Add { index: 99, node }]);
        let mut journal = SceneJournal::default();

        accept(&p, Some(target_workbook()), &mut scene, &mut journal).unwrap();
        assert_eq!(scene.nodes.len(), 1);
        assert_eq!(
            journal.last_author(),
            Some(&CmdAuthor::Agent("cursor-agent".into()))
        );
    }

    #[test]
    fn accept_is_all_or_nothing() {
        let mut scene = Scene::default();
        let node = sample_node(&mut scene);
        let mut stale = node.clone();
        stale.id = crate::scene::NodeId(9999);
        let p = proposal(vec![
            SceneCmd::Add { index: 0, node },
            SceneCmd::Remove {
                index: 0,
                node: stale,
            },
        ]);
        let mut journal = SceneJournal::default();

        let err = accept(&p, Some(target_workbook()), &mut scene, &mut journal).unwrap_err();
        assert_eq!(err, StaleReason::NodeChanged { node: NodeId(9999) });
        assert!(scene.nodes.is_empty());
        assert!(journal.last_author().is_none());
    }

    #[test]
    fn reject_leaves_the_scene_untouched() {
        let mut scene = Scene::default();
        let p = proposal(vec![SceneCmd::Add {
            index: 0,
            node: sample_node(&mut scene),
        }]);
        assert_eq!(reject(&p), ProposalResult::Rejected);
        assert!(scene.nodes.is_empty());
    }

    #[test]
    fn proposal_round_trips_through_json() {
        let mut scene = Scene::default();
        let p = proposal(vec![SceneCmd::Add {
            index: 0,
            node: sample_node(&mut scene),
        }]);
        let mut value = serde_json::to_value(&p).unwrap();
        value["unknown_future"] = serde_json::json!(true);
        let parsed: Proposal = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.id, p.id);
        assert_eq!(parsed.status, ProposalStatus::Pending);
    }

    #[test]
    fn stale_format_version_is_refused_with_a_reason() {
        let mut scene = Scene::default();
        let node = sample_node(&mut scene);
        let mut p = proposal(vec![SceneCmd::Add { index: 0, node }]);
        p.target.format_version = SlateDoc::CURRENT + 1;
        let mut journal = SceneJournal::default();
        let err = accept(&p, Some(target_workbook()), &mut scene, &mut journal).unwrap_err();
        assert_eq!(
            err,
            StaleReason::UnsupportedFormat {
                found: SlateDoc::CURRENT + 1,
                expected: SlateDoc::CURRENT
            }
        );
    }

    #[test]
    fn add_index_is_normalized_to_the_top() {
        let mut scene = Scene::default();
        let first = sample_node(&mut scene);
        scene.apply(&SceneCmd::Add {
            index: 0,
            node: first,
        });
        let node = sample_node(&mut scene);
        let id = node.id;
        let p = proposal(vec![SceneCmd::Add { index: 0, node }]);
        let mut journal = SceneJournal::default();
        accept(&p, Some(target_workbook()), &mut scene, &mut journal).unwrap();
        assert_eq!(scene.nodes.last().map(|n| n.id), Some(id));
    }

    #[test]
    fn wrong_workbook_and_unsaved_targets_never_mutate_the_scene() {
        let mut scene = Scene::default();
        let node = sample_node(&mut scene);
        let mut p = proposal(vec![SceneCmd::Add { index: 0, node }]);
        let mut journal = SceneJournal::default();
        assert_eq!(
            accept(
                &p,
                Some(Path::new("/audit/other.slate")),
                &mut scene,
                &mut journal
            ),
            Err(StaleReason::WorkbookMismatch)
        );
        assert_eq!(
            accept(&p, None, &mut scene, &mut journal),
            Err(StaleReason::UnsavedWorkbook)
        );
        p.target.workbook = None;
        assert_eq!(
            accept(&p, None, &mut scene, &mut journal),
            Err(StaleReason::UnsavedWorkbook)
        );
        assert!(scene.nodes.is_empty());
        assert!(!journal.can_undo());
    }

    #[test]
    fn wrong_workbook_refusal_can_be_retried_in_the_correct_saved_workbook() {
        let ws = temp_workspace("recoverable_target");
        let mut scene = Scene::default();
        let node = sample_node(&mut scene);
        let p = proposal(vec![SceneCmd::Add { index: 0, node }]);
        let mut journal = SceneJournal::default();
        for workbook in [Some(Path::new("/audit/other.slate")), None] {
            let error = accept_and_record(&p, workbook, &mut scene, &mut journal, &ws).unwrap_err();
            assert!(!error.applied);
            assert_eq!(error.error.kind(), io::ErrorKind::InvalidInput);
            assert_eq!(read_result(&ws, &p.id).unwrap(), None);
            assert!(scene.nodes.is_empty());
            assert!(!journal.can_undo());
        }
        assert_eq!(
            accept_and_record(&p, Some(target_workbook()), &mut scene, &mut journal, &ws).unwrap(),
            ProposalResult::Accepted
        );
        assert_eq!(scene.nodes.len(), 1);
        std::fs::remove_dir_all(ws).unwrap();
    }

    #[test]
    fn workbook_identity_normalizes_lexically_without_io() {
        assert!(same_workbook(
            Path::new("/missing/sub/../board.slate"),
            Path::new("/missing/./board.slate")
        ));
        assert!(!same_workbook(
            Path::new("/a/board.slate"),
            Path::new("/b/board.slate")
        ));
        #[cfg(windows)]
        assert!(same_workbook(
            Path::new(r"C:\MISSING\sub\..\Board.slate"),
            Path::new("c:/missing/board.slate")
        ));
    }

    #[test]
    fn stale_patch_preserves_intervening_human_edit_and_undo() {
        let mut scene = Scene::default();
        let original = sample_node(&mut scene);
        let id = original.id;
        assert!(scene.apply(&SceneCmd::Add {
            index: 0,
            node: original.clone()
        }));
        let mut agent_after = original.clone();
        agent_after.rect = agent_after.rect.translated(250.0, 0.0);
        let stale = proposal(vec![SceneCmd::Patch {
            before: Box::new(original.clone()),
            after: Box::new(agent_after),
        }]);
        let mut human_after = original.clone();
        if let NodeKind::Shape(shape) = &mut human_after.kind {
            shape.fill = Some(Rgba([200, 100, 50, 255]));
        }
        let mut journal = SceneJournal::default();
        assert!(journal.commit(
            &mut scene,
            vec![SceneCmd::Patch {
                before: Box::new(original.clone()),
                after: Box::new(human_after.clone())
            }]
        ));
        assert_eq!(
            accept(&stale, Some(target_workbook()), &mut scene, &mut journal),
            Err(StaleReason::NodeChanged { node: id })
        );
        assert_eq!(scene.node(id), Some(&human_after));
        assert_eq!(journal.last_author(), Some(&CmdAuthor::Human));

        // A freshly based proposal remains valid; undo restores the exact
        // human state immediately before acceptance, including the new color.
        let mut fresh_after = human_after.clone();
        fresh_after.rect = fresh_after.rect.translated(250.0, 0.0);
        let fresh = proposal(vec![SceneCmd::Patch {
            before: Box::new(human_after.clone()),
            after: Box::new(fresh_after),
        }]);
        accept(&fresh, Some(target_workbook()), &mut scene, &mut journal).unwrap();
        assert!(journal.undo(&mut scene));
        assert_eq!(scene.node(id), Some(&human_after));
        assert!(journal.undo(&mut scene));
        assert_eq!(scene.node(id), Some(&original));
    }

    #[test]
    fn stale_remove_does_not_delete_a_changed_node_or_apply_earlier_commands() {
        let mut scene = Scene::default();
        let original = sample_node(&mut scene);
        assert!(scene.apply(&SceneCmd::Add {
            index: 0,
            node: original.clone()
        }));
        let mut changed = original.clone();
        changed.rect = changed.rect.translated(30.0, 0.0);
        assert!(scene.apply(&SceneCmd::Patch {
            before: Box::new(original.clone()),
            after: Box::new(changed.clone())
        }));
        let added = sample_node(&mut scene);
        let p = proposal(vec![
            SceneCmd::Add {
                index: 0,
                node: added,
            },
            SceneCmd::Remove {
                index: 0,
                node: original.clone(),
            },
        ]);
        let mut journal = SceneJournal::default();
        assert_eq!(
            accept(&p, Some(target_workbook()), &mut scene, &mut journal),
            Err(StaleReason::NodeChanged { node: original.id })
        );
        assert_eq!(scene.nodes, vec![changed]);
        assert!(!journal.can_undo());
    }

    #[test]
    fn proposal_preflight_follows_commands_in_order() {
        let mut scene = Scene::default();
        let first = sample_node(&mut scene);
        let second = sample_node(&mut scene);
        let mut patched = first.clone();
        patched.rect = patched.rect.translated(40.0, 0.0);
        let p = proposal(vec![
            SceneCmd::Add {
                index: 99,
                node: first.clone(),
            },
            SceneCmd::Add {
                index: 99,
                node: second.clone(),
            },
            SceneCmd::Patch {
                before: Box::new(first),
                after: Box::new(patched.clone()),
            },
        ]);
        let mut journal = SceneJournal::default();
        accept(&p, Some(target_workbook()), &mut scene, &mut journal).unwrap();
        assert_eq!(scene.nodes, vec![patched, second]);
        assert!(journal.undo(&mut scene));
        assert!(scene.nodes.is_empty());
    }

    #[test]
    fn recorded_acceptance_survives_watcher_restart_without_replay() {
        let ws = temp_workspace("decisions");
        std::fs::create_dir_all(stage_dir(&ws)).unwrap();
        let mut scene = Scene::default();
        let node = sample_node(&mut scene);
        let accepted = proposal(vec![SceneCmd::Add { index: 0, node }]);
        let mut rejected = proposal(Vec::new());
        rejected.id = "rejected".into();
        for p in [&accepted, &rejected] {
            std::fs::write(
                stage_dir(&ws).join(format!("{}.json", p.id)),
                serde_json::to_vec(p).unwrap(),
            )
            .unwrap();
        }
        assert_eq!(StageWatcher::new().tick_read(&ws).len(), 2);
        let mut journal = SceneJournal::default();
        assert_eq!(
            accept_and_record(
                &accepted,
                Some(target_workbook()),
                &mut scene,
                &mut journal,
                &ws
            )
            .unwrap(),
            ProposalResult::Accepted
        );
        write_result(&ws, &rejected.id, &reject(&rejected)).unwrap();
        assert!(StageWatcher::new().tick_read(&ws).is_empty());
        assert_eq!(
            read_result(&ws, &accepted.id).unwrap(),
            Some(ProposalResult::Accepted)
        );
        assert!(journal.undo(&mut scene));
        assert!(scene.nodes.is_empty());
        let replay = accept_and_record(
            &accepted,
            Some(target_workbook()),
            &mut scene,
            &mut journal,
            &ws,
        )
        .unwrap_err();
        assert!(!replay.applied);
        assert_eq!(replay.error.kind(), io::ErrorKind::AlreadyExists);
        assert!(scene.nodes.is_empty());
        std::fs::remove_dir_all(ws).unwrap();
    }

    #[test]
    fn interrupted_or_unreadable_acceptance_requires_review_without_reapplying() {
        let ws = temp_workspace("interrupted");
        std::fs::create_dir_all(stage_dir(&ws)).unwrap();
        let mut scene = Scene::default();
        let node = sample_node(&mut scene);
        let p = proposal(vec![SceneCmd::Add { index: 0, node }]);
        std::fs::write(
            stage_dir(&ws).join("p1.json"),
            serde_json::to_vec(&p).unwrap(),
        )
        .unwrap();
        write_result(&ws, &p.id, &ProposalResult::Applying).unwrap();
        let mut journal = SceneJournal::default();
        for unreadable in [false, true] {
            if unreadable {
                std::fs::write(result_path(&ws, &p.id), b"partial-json").unwrap();
            }
            let loaded = StageWatcher::new().tick_read(&ws);
            assert_eq!(loaded.len(), 1);
            assert_eq!(loaded[0].status, ProposalStatus::RecoveryRequired);
            let error =
                accept_and_record(&p, Some(target_workbook()), &mut scene, &mut journal, &ws)
                    .unwrap_err();
            assert!(!error.applied);
            assert!(scene.nodes.is_empty());
            assert!(!journal.can_undo());
        }
        write_result(&ws, &p.id, &reject(&p)).unwrap();
        assert!(StageWatcher::new().tick_read(&ws).is_empty());
        std::fs::remove_dir_all(ws).unwrap();
    }

    #[test]
    fn failed_final_confirmation_keeps_barrier_and_exactly_one_undoable_commit() {
        let ws = temp_workspace("confirmation_failure");
        let mut scene = Scene::default();
        let node = sample_node(&mut scene);
        let p = proposal(vec![SceneCmd::Add {
            index: 0,
            node: node.clone(),
        }]);
        let mut journal = SceneJournal::default();
        let error = accept_and_record_with_writer(
            &p,
            Some(target_workbook()),
            &mut scene,
            &mut journal,
            &ws,
            |result| {
                if *result == ProposalResult::Accepted {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "injected final confirmation failure",
                    ))
                } else {
                    write_result(&ws, &p.id, result)
                }
            },
        )
        .unwrap_err();
        assert!(error.applied);
        assert_eq!(error.error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(scene.nodes, vec![node]);
        assert_eq!(
            journal.last_author(),
            Some(&CmdAuthor::Agent(p.author.clone()))
        );
        assert_eq!(
            read_result(&ws, &p.id).unwrap(),
            Some(ProposalResult::Applying)
        );
        let replay = accept_and_record(&p, Some(target_workbook()), &mut scene, &mut journal, &ws)
            .unwrap_err();
        assert!(!replay.applied);
        assert_eq!(scene.nodes.len(), 1);
        assert!(journal.undo(&mut scene));
        assert!(scene.nodes.is_empty());
        assert!(!journal.can_undo());
        std::fs::remove_dir_all(ws).unwrap();
    }

    #[test]
    fn result_write_failure_and_invalid_id_never_apply_the_proposal() {
        let ws = temp_workspace("unwritable");
        std::fs::create_dir_all(ws.join(".atlas-ai")).unwrap();
        std::fs::write(stage_dir(&ws), "a file blocks the stage directory").unwrap();
        let mut scene = Scene::default();
        let node = sample_node(&mut scene);
        let mut p = proposal(vec![SceneCmd::Add { index: 0, node }]);
        let mut journal = SceneJournal::default();
        let error = accept_and_record(&p, Some(target_workbook()), &mut scene, &mut journal, &ws)
            .unwrap_err();
        assert!(!error.applied);
        p.id = "../../outside".into();
        let error = accept_and_record(&p, Some(target_workbook()), &mut scene, &mut journal, &ws)
            .unwrap_err();
        assert_eq!(error.error.kind(), io::ErrorKind::InvalidInput);
        assert!(!error.applied);
        assert!(scene.nodes.is_empty());
        assert!(!journal.can_undo());
        std::fs::remove_dir_all(ws).unwrap();
    }

    #[test]
    fn watcher_reads_changed_proposals() {
        let ws = temp_workspace("watcher");
        let dir = stage_dir(&ws);
        std::fs::create_dir_all(&dir).unwrap();
        let p = proposal(Vec::new());
        std::fs::write(
            dir.join("p1.json"),
            serde_json::to_string_pretty(&p).unwrap(),
        )
        .unwrap();

        let mut watcher = StageWatcher::new();
        let got = watcher.tick_read(&ws);
        assert_eq!(got.len(), 1);
        assert!(watcher.tick_read(&ws).is_empty());

        std::thread::sleep(Duration::from_millis(1100));
        let mut changed = p.clone();
        changed.title = "Changed".into();
        std::fs::write(
            dir.join("p1.json"),
            serde_json::to_string_pretty(&changed).unwrap(),
        )
        .unwrap();
        watcher.force_elapsed();
        let got = watcher.tick_read(&ws);
        assert_eq!(got[0].title, "Changed");

        let _ = std::fs::remove_dir_all(&ws);
    }
}
