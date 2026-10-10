//! Open artifacts, build nodes, and provenance wires.

use super::*;

impl SlateApp {
    pub(crate) fn open_agent_artifact(&mut self, detail: Option<&str>) -> bool {
        self.open_agent_artifact_slot(detail, 0, None)
    }
    pub(super) fn open_agent_artifact_slot(
        &mut self,
        detail: Option<&str>,
        slot: usize,
        at: Option<Pos2>,
    ) -> bool {
        let Some((id, artifact_id, face)) = detail.and_then(parse_artifact_open) else {
            return false;
        };
        let id = NodeId(id);
        let Some(original) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let Some(artifact) = self
            .agents
            .session(id)
            .and_then(|s| s.artifacts.iter().find(|a| a.id == artifact_id))
            .cloned()
        else {
            // An output capsule, answering the text-or-graphic chooser.
            let face = (face != atlas_agent::Face::Auto).then_some(face);
            return self.spawn_agent_output(id, &artifact_id, face, at);
        };
        if artifact.kind == atlas_ai::agent::ArtifactKind::Deleted {
            self.toast("This file was deleted in this exchange.");
            return true;
        }
        if let Some(existing) = self.live_spawn(id, &artifact_id) {
            if let Some(at) = at {
                self.move_node_to(existing, at);
            } else {
                self.begin_context_retract(&[existing]);
            }
            return true;
        }
        let output = artifact.kind.is_output();
        let rect = if let Some(at) = at {
            slate_doc::WorldRect::new(at.x, at.y, 480.0, 320.0)
        } else {
            self.clear_spawn_rect(slate_doc::WorldRect::new(
                if output {
                    original.rect.x + original.rect.w + 72.0
                } else {
                    original.rect.x - 480.0 - 72.0
                },
                original.rect.y + slot as f32 * 376.0,
                480.0,
                320.0,
            ))
        };
        let source = if artifact.kind == atlas_ai::agent::ArtifactKind::Web {
            if !(artifact.source.starts_with("https://") || artifact.source.starts_with("http://"))
            {
                return false;
            }
            artifact.source.clone()
        } else {
            let path = PathBuf::from(&artifact.source);
            if path.is_absolute() {
                artifact.source.clone()
            } else {
                self.agent_folder_for(id)
                    .unwrap_or_default()
                    .join(path)
                    .to_string_lossy()
                    .into_owned()
            }
        };
        let portal = match self.build_artifact_node(&source, face, rect) {
            ArtifactBuild::Node(node) => node,
            ArtifactBuild::Ask => {
                self.agents.preview_ask = Some((id, artifact_id, at));
                self.agents.preview_ask_ready = false;
                return true;
            }
            ArtifactBuild::Unavailable => return false,
        };
        let portal_id = portal.id;
        let wire = self.provenance_wire(id, &portal, output);
        self.add_nodes(vec![portal, wire]);
        self.agents.spawned.insert((id, artifact_id), portal_id);
        self.board_sel = std::iter::once(portal_id).collect();
        true
    }
    /// The node an artifact becomes, and nothing else: a web portal for a URL
    /// or a page shown as a graphic, a File Atlas portal for a folder, the
    /// placed file otherwise, and File Atlas on the parent folder, filtered to
    /// the name, for a file that is not there. A file with more than one face
    /// and none chosen asks. Wires, selection and tracking are the caller's.
    pub(crate) fn build_artifact_node(
        &mut self,
        source: &str,
        face: atlas_agent::Face,
        rect: slate_doc::WorldRect,
    ) -> ArtifactBuild {
        use atlas_agent::Face;
        use slate_doc::media::PreviewFace;
        if source.starts_with("https://") || source.starts_with("http://") {
            return ArtifactBuild::Node(self.build_web_portal(rect, Some(source.into()), None));
        }
        let path = std::path::Path::new(source);
        let chosen = match face {
            Face::Graphic => Some(PreviewFace::Graphic),
            Face::Text => Some(PreviewFace::Text),
            Face::Auto | Face::Images | Face::Folder => None,
        };
        let choices = slate_doc::media::preview_choices(path);
        if choices.len() > 1 && chosen.is_none() && face != Face::Folder {
            return ArtifactBuild::Ask;
        }
        let graphic = chosen == Some(PreviewFace::Graphic)
            || (chosen.is_none() && choices == [PreviewFace::Graphic]);
        let node = if graphic && path.is_file() {
            self.build_web_portal(rect, Some(path.to_string_lossy().into_owned()), None)
        } else if path.is_dir() {
            self.build_bound_atlas(rect, path, Vec::new())
        } else if path.is_file() && face != Face::Folder {
            let Some(item) = self.item_for_path(path) else {
                return ArtifactBuild::Unavailable;
            };
            self.doc_mut().scene.build_node(
                rect,
                NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
            )
        } else {
            let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
                return ArtifactBuild::Unavailable;
            };
            self.build_bound_atlas(rect, parent, vec![name.to_string_lossy().into_owned()])
        };
        ArtifactBuild::Node(node)
    }
    pub(super) fn live_spawn(&self, agent: NodeId, artifact: &str) -> Option<NodeId> {
        let id = *self.agents.spawned.get(&(agent, artifact.to_string()))?;
        self.doc().scene.node(id).is_some().then_some(id)
    }
    /// Context cards tied to this agent by a faint provenance wire.
    /// `output` selects the right-hand (created) side; otherwise the left.
    pub(super) fn provenance_portals(&self, agent: NodeId, output: bool) -> Vec<NodeId> {
        use slate_doc::scene::{ConnectorEnd, Side, WireDisplay};
        let want = if output { Side::Right } else { Side::Left };
        let mut found = Vec::new();
        for n in &self.doc().scene.nodes {
            let NodeKind::Connector(c) = &n.kind else {
                continue;
            };
            if c.binding.is_some() || c.display != WireDisplay::Faint {
                continue;
            }
            let (
                ConnectorEnd::Anchored {
                    node: a, side: sa, ..
                },
                ConnectorEnd::Anchored { node: b, .. },
            ) = (&c.a, &c.b)
            else {
                continue;
            };
            let portal = if *a == agent && *sa == want {
                *b
            } else if *b == agent {
                if let ConnectorEnd::Anchored { side, .. } = &c.b {
                    if *side == want {
                        *a
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            } else {
                continue;
            };
            if self.doc().scene.node(portal).is_some() {
                found.push(portal);
            }
        }
        found
    }
    /// Provenance wire and the agent-dot position a spawned card retracts toward.
    pub(crate) fn machine_context_link(&self, portal: NodeId) -> Option<(NodeId, Pos2)> {
        use slate_doc::scene::{ConnectorEnd, WireDisplay};
        for n in &self.doc().scene.nodes {
            let NodeKind::Connector(c) = &n.kind else {
                continue;
            };
            if c.binding.is_some() || c.display != WireDisplay::Faint {
                continue;
            }
            let (a, b) = match (&c.a, &c.b) {
                (
                    ConnectorEnd::Anchored {
                        node: na,
                        side: sa,
                        t: ta,
                    },
                    ConnectorEnd::Anchored {
                        node: nb,
                        side: sb,
                        t: tb,
                    },
                ) => ((*na, *sa, *ta), (*nb, *sb, *tb)),
                _ => continue,
            };
            let agent = if a.0 == portal {
                b
            } else if b.0 == portal {
                a
            } else {
                continue;
            };
            if !self.is_agent_portal(agent.0) {
                continue;
            }
            let anchor = self
                .doc()
                .scene
                .node(agent.0)
                .map(|nn| slate_doc::connector_anchor_on(nn, agent.1, agent.2))
                .unwrap_or([0.0, 0.0]);
            return Some((n.id, Pos2::new(anchor[0], anchor[1])));
        }
        None
    }
    /// The faint wire [`Self::machine_context_link`] follows: from the card's
    /// right midpoint to a spawned output, or from spawned context into the
    /// card's left midpoint. Provenance is never model input, so it has no
    /// binding. `node` may still be waiting for the same `add_nodes` call.
    pub(crate) fn provenance_wire(&mut self, agent: NodeId, node: &Node, output: bool) -> Node {
        use slate_doc::scene::{ConnectorEnd, Side};
        let agent_end = ConnectorEnd::Anchored {
            node: agent,
            side: if output { Side::Right } else { Side::Left },
            t: 0.5,
        };
        let node_end = ConnectorEnd::Anchored {
            node: node.id,
            side: if output { Side::Left } else { Side::Right },
            t: 0.5,
        };
        let (a, b) = if output {
            (agent_end, node_end)
        } else {
            (node_end, agent_end)
        };
        let mut wire = self.build_connector_with(a, b, std::slice::from_ref(node));
        if let NodeKind::Connector(c) = &mut wire.kind {
            c.binding = None;
            c.display = slate_doc::scene::WireDisplay::Faint;
        }
        wire
    }
    pub(super) fn clear_spawn_rect(&self, mut rect: slate_doc::WorldRect) -> slate_doc::WorldRect {
        let hits = |a: slate_doc::WorldRect, b: slate_doc::WorldRect| {
            a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
        };
        for _ in 0..16 {
            let blocked = self
                .doc()
                .scene
                .nodes
                .iter()
                .any(|n| !n.hidden && n.rect.w > 2.0 && n.rect.h > 2.0 && hits(n.rect, rect));
            if !blocked {
                break;
            }
            rect.y += rect.h + 48.0;
        }
        rect
    }
    pub(super) fn move_node_to(&mut self, id: NodeId, at: Pos2) {
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return;
        };
        let mut after = before.clone();
        after.rect.x = at.x;
        after.rect.y = at.y;
        let _ = self.commit_scene(vec![slate_doc::SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]);
    }
}
