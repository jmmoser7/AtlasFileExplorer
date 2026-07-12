use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use crate::model::CodeGraph;
use crate::overlay::{lens_dir, read_overlay, LensOverlay};

const WRITE_INTERVAL: Duration = Duration::from_secs(1);
const READ_INTERVAL: Duration = Duration::from_secs(1);

const README: &str = "\
Slate writes `graph.json` here as the deterministic codebase graph for the active \
Lens view. Cursor agents should read that file and write `overlay.json` with semantic \
cluster labels, summaries, and colors. See `docs/lens-agent-contract.md` in the \
repository for the full schema and selector rules.\n";

#[derive(Debug, Default)]
pub struct LensBeacon {
    last_write_attempt: Option<Instant>,
    last_fingerprint: u64,
    last_read_attempt: Option<Instant>,
    last_overlay_mtime: Option<SystemTime>,
    graph_readme_written: bool,
}

#[derive(Serialize)]
struct GraphBeacon<'a> {
    app: &'static str,
    source_root: PathBuf,
    generated_at: u64,
    graph: &'a CodeGraph,
}

impl LensBeacon {
    pub fn new() -> Self {
        Self::default()
    }

    /// Throttled (>=1s, fingerprint-gated) atomic write of
    /// <ai_workspace>/.atlas-ai/lens/graph.json. Safe to call every frame.
    /// Returns true when a write happened.
    pub fn tick_write(
        &mut self,
        ai_workspace: &Path,
        source_root: &Path,
        graph: &CodeGraph,
    ) -> bool {
        if let Some(t) = self.last_write_attempt {
            if t.elapsed() < WRITE_INTERVAL {
                return false;
            }
        }
        self.last_write_attempt = Some(Instant::now());

        let fp = graph.fingerprint();
        if fp == self.last_fingerprint {
            return false;
        }

        let dir = lens_dir(ai_workspace);
        if std::fs::create_dir_all(&dir).is_err() {
            return false;
        }

        let source_root = source_root
            .canonicalize()
            .unwrap_or_else(|_| source_root.to_path_buf());

        let payload = GraphBeacon {
            app: "slate",
            source_root,
            generated_at: now_secs(),
            graph,
        };
        let json = match serde_json::to_string_pretty(&payload) {
            Ok(j) => j,
            Err(_) => return false,
        };

        let path = dir.join("graph.json");
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_err() {
            return false;
        }
        if std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return false;
        }

        self.last_fingerprint = fp;
        self.write_readme_if_needed(&dir);
        true
    }

    /// Polls overlay.json mtime (>=1s). Returns Some only when the file
    /// (re)appeared or changed since last successful load.
    pub fn tick_read(&mut self, ai_workspace: &Path) -> Option<LensOverlay> {
        if let Some(t) = self.last_read_attempt {
            if t.elapsed() < READ_INTERVAL {
                return None;
            }
        }
        self.last_read_attempt = Some(Instant::now());

        let path = lens_dir(ai_workspace).join("overlay.json");
        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => {
                self.last_overlay_mtime = None;
                return None;
            }
        };
        let mtime = metadata.modified().ok()?;
        if self.last_overlay_mtime == Some(mtime) {
            return None;
        }

        let overlay = read_overlay(ai_workspace)?;
        self.last_overlay_mtime = Some(mtime);
        Some(overlay)
    }

    fn write_readme_if_needed(&mut self, dir: &Path) {
        if self.graph_readme_written {
            return;
        }
        let readme = dir.join("README.md");
        if readme.exists() {
            self.graph_readme_written = true;
            return;
        }
        if std::fs::write(&readme, README).is_ok() {
            self.graph_readme_written = true;
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
impl LensBeacon {
    fn test_force_write_elapsed(&mut self) {
        self.last_write_attempt =
            Some(Instant::now() - WRITE_INTERVAL - Duration::from_millis(100));
    }

    fn test_force_read_elapsed(&mut self) {
        self.last_read_attempt = Some(Instant::now() - READ_INTERVAL - Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::model::{EdgeKind, LensEdge, LensNode, NodeKind};

    fn temp_workspace() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "code_lens_beacon_{}_{}",
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

    fn sample_graph() -> CodeGraph {
        CodeGraph {
            root: 0,
            nodes: vec![
                LensNode {
                    id: 0,
                    parent: None,
                    kind: NodeKind::Workspace,
                    name: "ws".into(),
                    path: PathBuf::new(),
                    loc: 10,
                    children: vec![1],
                },
                LensNode {
                    id: 1,
                    parent: Some(0),
                    kind: NodeKind::Package { is_app: false },
                    name: "foo".into(),
                    path: PathBuf::from("crates/foo"),
                    loc: 10,
                    children: vec![],
                },
            ],
            edges: vec![LensEdge {
                from: 0,
                to: 1,
                kind: EdgeKind::PackageDep,
                weight: 1,
            }],
            generated_at: 0,
        }
    }

    #[test]
    fn beacon_write_round_trip_and_fingerprint_gate() {
        let ws = temp_workspace();
        let source = ws.join("repo");
        std::fs::create_dir_all(&source).unwrap();
        let mut beacon = LensBeacon::new();
        let graph = sample_graph();

        assert!(beacon.tick_write(&ws, &source, &graph));
        assert!(!beacon.tick_write(&ws, &source, &graph));

        let graph_path = lens_dir(&ws).join("graph.json");
        let text = std::fs::read_to_string(&graph_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["app"], "slate");
        assert_eq!(parsed["graph"]["nodes"].as_array().unwrap().len(), 2);
        assert!(parsed["source_root"].is_string());
        assert!(parsed["generated_at"].is_u64());

        let readme = lens_dir(&ws).join("README.md");
        assert!(readme.exists());
        let readme_text = std::fs::read_to_string(readme).unwrap();
        assert!(readme_text.contains("graph.json"));
        assert!(readme_text.contains("overlay.json"));

        let mut changed = graph.clone();
        changed.nodes[1].loc = 99;
        beacon.test_force_write_elapsed();
        assert!(beacon.tick_write(&ws, &source, &changed));

        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn beacon_read_overlay_mtime_cycle() {
        let ws = temp_workspace();
        let mut beacon = LensBeacon::new();

        beacon.test_force_read_elapsed();
        assert!(beacon.tick_read(&ws).is_none());

        std::fs::create_dir_all(lens_dir(&ws)).unwrap();
        let overlay_path = lens_dir(&ws).join("overlay.json");
        std::fs::write(
            &overlay_path,
            r#"{"generated_at":1,"clusters":[{"id":"x","title":"X","members":[]}]}"#,
        )
        .unwrap();

        beacon.test_force_read_elapsed();
        let overlay = beacon.tick_read(&ws).expect("overlay loaded");
        assert_eq!(overlay.clusters[0].id, "x");

        beacon.test_force_read_elapsed();
        assert!(beacon.tick_read(&ws).is_none());

        // Coarse mtime (1s) on some filesystems — wait so the rewrite bumps mtime.
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(
            &overlay_path,
            r#"{"generated_at":2,"clusters":[{"id":"y","title":"Y","members":[]}]}"#,
        )
        .unwrap();

        beacon.test_force_read_elapsed();
        let overlay = beacon.tick_read(&ws).expect("reloaded overlay");
        assert_eq!(overlay.clusters[0].id, "y");

        let _ = std::fs::remove_dir_all(&ws);
    }
}
