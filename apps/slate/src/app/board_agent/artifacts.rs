//! Artifact paint and hit-testing.

use super::*;

impl SlateApp {
    pub(crate) fn agent_artifacts(
        &self,
        id: NodeId,
        output: bool,
    ) -> Vec<atlas_ai::agent::AgentArtifact> {
        let Some(a) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return vec![];
        };
        let start = self
            .doc()
            .scene
            .node(id)
            .and_then(|n| slate_doc::agent_chat::bundle_entry(&self.doc().scene, n))
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.chat.start)
            .unwrap_or(a.chat.start);
        self.agents
            .session(id)
            .map(|s| {
                s.artifacts
                    .iter()
                    .filter(|artifact| {
                        artifact.kind.is_output() == output
                            && artifact.turn >= start
                            && a.chat.end.is_none_or(|end| artifact.turn < end)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
    pub(crate) fn toggle_agent_artifacts(&mut self, detail: Option<&str>) -> bool {
        let Some((id, output)) = detail.and_then(|s| serde_json::from_str::<(u64, bool)>(s).ok())
        else {
            return false;
        };
        let key = (NodeId(id), output);
        self.agents.artifact_popup = if self.agents.artifact_popup == Some(key) {
            None
        } else {
            Some(key)
        };
        self.agents.artifact_popup_rect = None;
        true
    }
    pub(super) fn paint_agent_artifacts(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
    ) {
        let r = xf.rect_w2s(node.rect);
        let z = xf.z;
        let palette = self.palette();
        let rest = Pos2::new(
            r.left() + slate_doc::agent_chat::PORT_INSET * z,
            r.center().y,
        );
        let travel = canvas_scale::px(dot_split_travel(HANDLE_DOT), z);
        let reach = canvas_scale::px(11.0, z);
        let pointer = ui.ctx().pointer_hover_pos();
        let refs = self.agent_artifacts(node.id, false);
        let has_refs = !refs.is_empty();
        let raised = rest + egui::vec2(0.0, -travel);
        let lowered = rest + egui::vec2(0.0, travel);
        let near = |p: Pos2| pointer.is_some_and(|q| q.distance(p) <= reach);
        let on_rest = near(rest);
        let outward = pointer.is_some_and(|p| {
            let d = p - rest;
            d.x < -canvas_scale::px(2.0, z)
                && d.length() < canvas_scale::px(42.0, z)
                && !near(rest)
                && !near(raised)
        });
        let split_on = if has_refs {
            on_rest || near(raised) || near(lowered)
        } else {
            outward || on_rest
        };
        let split = ui.ctx().animate_bool_with_time(
            Id::new(("agent-context-split", node.id.0)),
            split_on,
            0.18,
        );
        // Split halves swell together and part by the swollen radius, so the
        // drawn gap stays DOT_SPLIT_GAP × the drawn radius.
        let grow = if split_on { HANDLE_HOVER_GROW } else { 1.0 };
        let open = split * canvas_scale::px(dot_split_travel(HANDLE_DOT * grow), z);
        let auto = Pos2::new(rest.x, rest.y - if has_refs { open } else { 0.0 });
        let human = Pos2::new(rest.x, rest.y + if has_refs { open } else { 0.0 });
        let pocket = self.agent_has_pocket(node.id);
        self.agents.context_human.remove(&node.id);
        let gray = palette.sub;
        if has_refs {
            self.agents.context_auto.insert(node.id, auto);
            let swell = if split > 0.02 { split_on } else { near(auto) };
            if split > 0.02 {
                self.paint_context_wire_slide(painter, node.id, rest, auto, human, gray, z);
                paint_handle_dot(
                    painter,
                    human,
                    z,
                    swell,
                    gray.gamma_multiply(0.8 * split.min(1.0)),
                );
                self.agents.context_human.insert(node.id, human);
            }
            paint_handle_dot(painter, auto, z, swell, gray.gamma_multiply(0.8));
        } else {
            self.agents.context_auto.remove(&node.id);
            let fade = ui.ctx().animate_bool_with_time(
                Id::new(("agent-context-fade", node.id.0)),
                split_on,
                0.12,
            );
            // A pocket keeps its handle findable at rest, a shade quieter.
            let fade = if pocket { fade.max(0.6) } else { fade };
            if fade > 0.04 {
                paint_handle_dot(painter, rest, z, on_rest, gray.gamma_multiply(0.8 * fade));
                self.agents.context_human.insert(node.id, rest);
            }
        }
        if let Some(handle) = self
            .agents
            .context_human
            .get(&node.id)
            .copied()
            .filter(|_| pocket)
        {
            let blurb = self.agent_pocket_blurb(node.id);
            ui.interact(
                Rect::from_center_size(handle, egui::Vec2::splat(canvas_scale::px(16.0, z))),
                Id::new(("agent-pocket", node.id.0)),
                Sense::hover(),
            )
            .on_hover_text(blurb);
        }
        if self.agents.context_preview == Some(node.id) {
            let over = pointer.is_some_and(|p| {
                p.distance(auto) <= reach * 1.6
                    || self
                        .agents
                        .context_preview_rect
                        .is_some_and(|rect| rect.expand(canvas_scale::px(12.0, z)).contains(p))
            });
            if self.agents.context_preview_rect.is_some() && !over {
                self.agents.context_preview = None;
                self.agents.context_preview_rect = None;
            } else {
                self.paint_context_preview(painter, xf, node, auto);
            }
        }
        let mut open = None;
        for output in [false, true] {
            if !output && !has_refs {
                continue;
            }
            if output && !self.agent_has_outputs(node.id) {
                continue;
            }
            let artifacts = if output {
                Vec::new()
            } else {
                self.agent_artifacts(node.id, false)
            };
            let anchor = if output {
                Pos2::new(
                    r.right() - slate_doc::agent_chat::PORT_INSET * z,
                    r.center().y,
                )
            } else {
                auto
            };
            if output {
                let near = pointer.is_some_and(|p| p.distance(anchor) <= canvas_scale::px(12.0, z));
                paint_handle_dot(painter, anchor, z, near, gray.gamma_multiply(0.8));
            }
            let response = ui.interact(
                Rect::from_center_size(anchor, egui::vec2(16.0, 16.0) * z),
                Id::new(("agent-artifact", node.id.0, output)),
                Sense::hover(),
            );
            if output {
                if self.agents.artifact_popup == Some((node.id, true)) {
                    self.paint_agent_output_stack(ui, xf, node.id, anchor);
                } else {
                    let count = self.agent_outputs(node.id).list.items.len();
                    response.on_hover_text(format!("Outputs · {count}"));
                }
                continue;
            }
            let blurb = self.agent_context_blurb(node.id);
            response.on_hover_text(blurb);
            if self.agents.artifact_popup == Some((node.id, output)) {
                let title = "Linked context";
                let dark = palette.dark_mode;
                let shown = atlas_shell::menu::anchored(
                    ui.ctx(),
                    Id::new(("agent-artifact-list", node.id.0, output)),
                    ui.layer_id(),
                    anchor + egui::vec2(0.0, 14.0 * z),
                    dark,
                    360.0,
                    None,
                    |ui| {
                        atlas_shell::menu::heading(ui, title, dark);
                        if artifacts.is_empty() {
                            atlas_shell::menu::note(
                                ui,
                                "No reported artifacts for this exchange.",
                                dark,
                            );
                        }
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                for a in &artifacts {
                                    let label = format!("{} · {}", a.kind.label(), a.title);
                                    if ui
                                        .add_enabled(
                                            a.kind != atlas_ai::agent::ArtifactKind::Deleted,
                                            egui::Button::new(label),
                                        )
                                        .on_hover_text(&a.source)
                                        .clicked()
                                    {
                                        open = Some(a.id.clone());
                                    }
                                }
                            });
                        atlas_shell::menu::item(
                            ui,
                            atlas_shell::menu::MenuIcon::None,
                            "Close",
                            dark,
                        )
                        .clicked()
                    },
                );
                if shown.inner {
                    self.agents.artifact_popup = None;
                    self.agents.artifact_popup_rect = None;
                }
                if self.agents.artifact_popup.is_some() {
                    self.agents.artifact_popup_rect = Some(shown.rect);
                }
            }
        }
        if let Some(id) = open {
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("portal.agent.open_artifact"),
                Some(serde_json::to_string(&(node.id.0, id)).unwrap()),
            );
            self.agents.artifact_popup = None;
            self.agents.artifact_popup_rect = None;
        }
        self.paint_preview_ask(ui, node, r);
    }
    fn paint_preview_ask(&mut self, ui: &egui::Ui, node: &Node, rect: Rect) {
        let Some((agent, artifact, at)) = self.agents.preview_ask.clone() else {
            return;
        };
        if agent != node.id {
            return;
        }
        let dot = Pos2::new(
            rect.right() - slate_doc::agent_chat::PORT_INSET * 2.0,
            rect.center().y,
        );
        let dark = self.palette().dark_mode;
        let mut chosen = None;
        let shown = atlas_shell::menu::anchored(
            ui.ctx(),
            Id::new(("agent-preview-ask", node.id.0)),
            ui.layer_id(),
            dot + egui::vec2(8.0, 12.0),
            dark,
            260.0,
            Some(&mut self.agents.preview_ask_ready),
            |ui| {
                atlas_shell::menu::heading(ui, "Open this file", dark);
                atlas_shell::menu::note(
                    ui,
                    "This file can be read as source or drawn as a page.",
                    dark,
                );
                for face in [
                    slate_doc::media::PreviewFace::Text,
                    slate_doc::media::PreviewFace::Graphic,
                ] {
                    if atlas_shell::menu::item(
                        ui,
                        atlas_shell::menu::MenuIcon::None,
                        face.label(),
                        dark,
                    )
                    .clicked()
                    {
                        chosen = Some(face);
                    }
                }
            },
        );
        if let Some(face) = chosen {
            self.agents.preview_ask = None;
            self.agents.preview_ask_ready = false;
            let _ = self.open_agent_artifact_slot(
                Some(
                    &serde_json::json!({
                        "portal": agent.0,
                        "artifact": artifact,
                        "face": face.id(),
                    })
                    .to_string(),
                ),
                0,
                at,
            );
        } else if shown.dismissed {
            self.agents.preview_ask = None;
            self.agents.preview_ask_ready = false;
        }
    }
    fn context_preview_rows(&self, id: NodeId) -> Vec<(String, Option<String>)> {
        let mut rows = Vec::new();
        for artifact in self.agent_artifacts(id, false) {
            rows.push((
                format!("{} · {}", artifact.kind.label(), artifact.title),
                Some(artifact.id),
            ));
        }
        for node in &self.doc().scene.nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            let Some(binding) = &conn.binding else {
                continue;
            };
            // TWIN: slate_doc::agent_inputs::WireBinding::source 2026-09-25
            let (source, target) = if binding.input_b {
                (&conn.a, &conn.b)
            } else {
                (&conn.b, &conn.a)
            };
            if slate_doc::agent_inputs::endpoint_node(target) != Some(id) {
                continue;
            }
            let Some(src) = slate_doc::agent_inputs::endpoint_node(source) else {
                continue;
            };
            let Some(label) = self.doc().scene.node(src).map(|n| match &n.kind {
                NodeKind::Text(t) => t
                    .text
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(64)
                    .collect::<String>(),
                NodeKind::Portal(p) => p.title.clone(),
                NodeKind::Image(_) => "Image".into(),
                _ => "Linked context".into(),
            }) else {
                continue;
            };
            if label.is_empty() || rows.iter().any(|(title, _)| title == &label) {
                continue;
            }
            rows.push((label, None));
        }
        if rows.is_empty() {
            rows.push(("No linked context".into(), None));
        }
        rows
    }
    /// Unjournaled cards. They last only while the pointer stays with them.
    fn paint_context_preview(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        anchor: Pos2,
    ) {
        use atlas_shell::selection_tools::{self as tools, Capsule, StackSide};
        let z = xf.z;
        let rows = self.context_preview_rows(node.id);
        let palette = self.palette();
        let mut bounds: Option<Rect> = None;
        let rects = tools::capsule_stack_rects(anchor, StackSide::Left, rows.len(), z);
        for ((label, _), rect) in rows.iter().zip(rects) {
            bounds = Some(bounds.map_or(rect, |b| b.union(rect)));
            let row = Capsule {
                label,
                ..Default::default()
            };
            tools::paint_capsule_row(painter, rect, &row, false, false, z, palette);
        }
        self.agents.context_preview_rect = bounds;
    }
    pub(super) fn pin_agent_context(&mut self, id: NodeId) {
        let existing = self.provenance_portals(id, false);
        if !existing.is_empty() {
            self.begin_context_retract(&existing);
            return;
        }
        let ids: Vec<_> = self
            .agent_artifacts(id, false)
            .into_iter()
            .filter(|a| a.kind != atlas_ai::agent::ArtifactKind::Deleted)
            .map(|a| a.id)
            .collect();
        if ids.is_empty() {
            self.toast("No linked context to pin.");
            return;
        }
        for (slot, artifact_id) in ids.into_iter().enumerate() {
            let _ = self.open_agent_artifact_slot(
                Some(&serde_json::to_string(&(id.0, artifact_id)).unwrap()),
                slot,
                None,
            );
        }
    }
    /// Short screen stub so a provenance wire follows the raised context mark,
    /// and a human context wire follows the lower mark. The cached curve stays put.
    #[allow(clippy::too_many_arguments)]
    fn paint_context_wire_slide(
        &self,
        painter: &egui::Painter,
        node: NodeId,
        rest: Pos2,
        auto: Pos2,
        human: Pos2,
        color: Color32,
        z: f32,
    ) {
        use slate_doc::scene::{ConnectorEnd, Side};
        let scene = &self.doc().scene;
        for n in &scene.nodes {
            let NodeKind::Connector(conn) = &n.kind else {
                continue;
            };
            // A wire to pocketed context is not painted, so neither is its stub.
            if [&conn.a, &conn.b].into_iter().any(|end| {
                slate_doc::agent_inputs::endpoint_node(end)
                    .and_then(|id| scene.node(id))
                    .is_some_and(|other| other.hidden)
            }) {
                continue;
            }
            for end in [&conn.a, &conn.b] {
                let ConnectorEnd::Anchored {
                    node: id,
                    side: Side::Left,
                    t,
                } = end
                else {
                    continue;
                };
                if *id != node || (*t - 0.5).abs() > 0.08 {
                    continue;
                }
                let tip = if conn.binding.is_some() { human } else { auto };
                painter.line_segment(
                    [rest, tip],
                    egui::Stroke::new((1.25 * z).max(1.0), color.gamma_multiply(0.55)),
                );
            }
        }
    }
}
