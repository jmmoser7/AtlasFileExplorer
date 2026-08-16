//! Agent staging contract: agents propose ordinary scene commands, and humans
//! accept or reject them as one attributed journal group.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use crate::scene::{CmdAuthor, Scene, SceneCmd, SceneJournal};
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalResult {
    Accepted,
    Rejected,
    Stale { reason: StaleReason },
}

pub fn accept(
    proposal: &Proposal,
    scene: &mut Scene,
    journal: &mut SceneJournal,
) -> Result<(), StaleReason> {
    if proposal.target.format_version != SlateDoc::CURRENT {
        return Err(StaleReason::UnsupportedFormat {
            found: proposal.target.format_version,
            expected: SlateDoc::CURRENT,
        });
    }
    if proposal.cmds.is_empty() {
        return Err(StaleReason::EmptyProposal);
    }

    let cmds = normalized_cmds(&proposal.cmds, scene);
    let mut check = scene.clone();
    if !check.apply_all(&cmds) {
        return Err(StaleReason::CommandRejected);
    }
    if journal.commit_as(scene, cmds, CmdAuthor::Agent(proposal.author.clone())) {
        Ok(())
    } else {
        Err(StaleReason::CommandRejected)
    }
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
    let dir = stage_dir(ai_workspace);
    std::fs::create_dir_all(&dir)?;
    let path = result_path(ai_workspace, id);
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(result)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(tmp, path)
}

fn normalized_cmds(cmds: &[SceneCmd], scene: &Scene) -> Vec<SceneCmd> {
    cmds.iter()
        .cloned()
        .map(|cmd| match cmd {
            SceneCmd::Add { node, .. } => SceneCmd::Add {
                index: scene.nodes.len(),
                node,
            },
            other => other,
        })
        .collect()
}

#[derive(Debug, Default)]
pub struct StageWatcher {
    last_read_attempt: Option<Instant>,
    mtimes: BTreeMap<PathBuf, SystemTime>,
}

impl StageWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Polls `<ai-workspace>/.atlas-ai/stage/*.json` at most once per second.
    /// Result files are ignored. Returns proposals whose file appeared or whose
    /// mtime changed since the last successful load.
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
            if self.mtimes.get(&path).copied() == Some(mtime) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(proposal) = serde_json::from_str::<Proposal>(&text) else {
                continue;
            };
            self.mtimes.insert(path, mtime);
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
                workbook: None,
                format_version: SlateDoc::CURRENT,
            },
            cmds,
            session: Some("agent-test".into()),
            status: ProposalStatus::Pending,
        }
    }

    #[test]
    fn accept_commits_as_agent_author() {
        let mut scene = Scene::default();
        let node = sample_node(&mut scene);
        let p = proposal(vec![SceneCmd::Add { index: 99, node }]);
        let mut journal = SceneJournal::default();

        accept(&p, &mut scene, &mut journal).unwrap();
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

        let err = accept(&p, &mut scene, &mut journal).unwrap_err();
        assert_eq!(err, StaleReason::CommandRejected);
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
        let err = accept(&p, &mut scene, &mut journal).unwrap_err();
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
        accept(&p, &mut scene, &mut journal).unwrap();
        assert_eq!(scene.nodes.last().map(|n| n.id), Some(id));
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
