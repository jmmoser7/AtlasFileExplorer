//! Collapsed composer and context blurbs.

use super::*;

impl SlateApp {
    /// A collapsed tail keeps its composer under the three lines.
    pub(super) fn paint_collapsed_composer(
        &mut self,
        ui: &egui::Ui,
        layout: &super::super::board_portal_chrome::PortalChromeLayout,
        node: &Node,
        z: f32,
    ) {
        if self.agent_has_child(node.id) {
            return;
        }
        let body = layout.body;
        let (inset_l, inset_r, wrap_w) = text_column(node.rect.w);
        let text_x = body.left() + inset_l * z;
        let text_right = body.right() - inset_r * z;
        let prompt = self
            .agents
            .prompts
            .get(&node.id)
            .cloned()
            .unwrap_or_default();
        let wrap = (wrap_w * z).max(1.0);
        let prompt_h = composer_text_height(ui.ctx(), &prompt, wrap, 14.0 * z);
        let bottom = body.bottom() - COMPOSER_BOTTOM * z;
        let field = Rect::from_min_max(
            Pos2::new(text_x, bottom - prompt_h - 2.0 * z),
            Pos2::new(text_right, bottom),
        );
        self.paint_agent_composer(ui, node.id, field, z, false);
    }
    /// Reserve both message views once. Source history is replayed into a fresh
    /// branch so sending from an old card cannot contaminate its siblings.
    pub(in super::super) fn prepare_agent_train_send(
        &mut self,
        id: NodeId,
        ws: &std::path::Path,
    ) -> Option<(NodeId, Vec<AgentTurn>)> {
        use slate_doc::agent_chat::{
            ChatView, Detail, CARD_GAP, CARD_HEIGHT, CARD_WIDTH, LANE_GAP,
        };
        let original = self.doc().scene.node(id)?.clone();
        let a = slate_doc::agent_chat::agent(&original)?;
        if !a.chat.train && !a.chat.draft && a.chat.end.is_none() && a.chat.bundled.is_empty() {
            return Some((id, Vec::new()));
        }
        let linear = self.agent_linear(id);
        let collapsed = slate_doc::crosstalk::collapses_new_cards(&self.doc().scene, &a.session);
        if linear
            && self.doc().scene.nodes.iter().any(|n| {
                slate_doc::agent_chat::agent(n).is_some_and(|other| other.chat.parent == Some(id))
            })
        {
            self.toast("Continue at the end of this conversation; coding agents have one stream.");
            return None;
        }
        let source = self.agent_all_turns(id);
        let through = a
            .chat
            .end
            .or_else(|| (source.len() < a.chat.start).then_some(a.chat.start));
        let history = match atlas_ai::agent::checkpoint(&source, through) {
            Ok(history) => history.to_vec(),
            Err(error) => {
                self.toast(error);
                return None;
            }
        };
        let count = history.len();
        let draft = a.chat.draft || (linear && source.is_empty());
        let children: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
            .map(|n| n.id)
            .collect();
        let branches = children.len();
        let lane_height = slate_doc::agent_chat::subtree(&self.doc().scene, &children)
            .iter()
            .filter_map(|id| self.doc().scene.node(*id))
            .map(|n| n.rect.h)
            .fold(CARD_HEIGHT, f32::max)
            + LANE_GAP;
        let mut user = if draft {
            original.clone()
        } else {
            self.doc_mut().scene.build_duplicate(&original, 0.0, 0.0)
        };
        user.rect = slate_doc::WorldRect::new(
            original.rect.x + original.rect.w + CARD_GAP,
            original.rect.y
                + if branches == 0 {
                    0.0
                } else {
                    (branches as f32 - 0.5) * lane_height
                },
            CARD_WIDTH,
            CARD_HEIGHT,
        );
        if draft {
            user.rect.x = original.rect.x;
            user.rect.y = original.rect.y;
        }
        let session = if linear {
            a.session.clone()
        } else {
            slate_doc::scene::new_agent_session_id()
        };
        let manifest = slate_doc::SourceUri {
            locator: super::super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &atlas_ai::agent::agent_dir(ws, &session).join("session.json"),
            ),
        };
        let pair = a.chat.detail == Detail::Pair;
        if !a.chat.train || (!linear && a.chat.detail == Detail::Full) || (pair && draft) {
            let mut after = original.clone();
            if let NodeKind::Portal(p) = &mut after.kind {
                if let Some(child) = &mut p.agent {
                    child.session = session;
                    child.bundle = Some(manifest);
                    if !linear {
                        child.channel = None;
                    }
                    child.chat.linear = linear;
                    child.chat.end = None;
                    child.chat.draft = false;
                }
            }
            if !self.commit_scene(vec![slate_doc::SceneCmd::Patch {
                before: Box::new(original),
                after: Box::new(after),
            }]) {
                return None;
            }
            return Some((id, if linear { Vec::new() } else { history }));
        }
        if let NodeKind::Portal(p) = &mut user.kind {
            if let Some(a) = &mut p.agent {
                a.session = session;
                a.bundle = Some(manifest);
                if !linear {
                    a.channel = None;
                }
                a.chat = ChatView {
                    linear,
                    train: true,
                    parent: if draft { a.chat.parent } else { Some(id) },
                    start: count,
                    end: if pair { None } else { Some(count + 1) },
                    detail: if pair { Detail::Pair } else { Detail::Summary },
                    custom_fill: a.chat.custom_fill,
                    stroke: a.chat.stroke,
                    collapsed,
                    ..Default::default()
                };
            }
        }
        if pair {
            user.rect.h = slate_doc::agent_chat::PAIR_HEIGHT;
        }
        let mut before_end = original.clone();
        if let NodeKind::Portal(p) = &mut before_end.kind {
            if let Some(a) = &mut p.agent {
                a.chat.end = Some(count);
            }
        }
        let base = self.doc().scene.nodes.len();
        let (reply_id, mut commands) = if pair {
            let focus = user.id;
            let commands = if draft {
                vec![slate_doc::SceneCmd::Patch {
                    before: Box::new(original),
                    after: Box::new(user),
                }]
            } else {
                vec![
                    slate_doc::SceneCmd::Patch {
                        before: Box::new(original),
                        after: Box::new(before_end),
                    },
                    slate_doc::SceneCmd::Add {
                        index: base,
                        node: user,
                    },
                ]
            };
            (focus, commands)
        } else {
            let mut reply = self
                .doc_mut()
                .scene
                .build_duplicate(&user, CARD_WIDTH + CARD_GAP, 0.0);
            if let NodeKind::Portal(p) = &mut reply.kind {
                if let Some(a) = &mut p.agent {
                    a.chat.parent = Some(user.id);
                    a.chat.start = count + 1;
                    a.chat.end = None;
                    a.chat.detail = Detail::Summary;
                }
            }
            let focus = reply.id;
            let commands = if draft {
                vec![
                    slate_doc::SceneCmd::Patch {
                        before: Box::new(original),
                        after: Box::new(user),
                    },
                    slate_doc::SceneCmd::Add {
                        index: base,
                        node: reply,
                    },
                ]
            } else {
                vec![
                    slate_doc::SceneCmd::Patch {
                        before: Box::new(original),
                        after: Box::new(before_end),
                    },
                    slate_doc::SceneCmd::Add {
                        index: base,
                        node: user,
                    },
                    slate_doc::SceneCmd::Add {
                        index: base + 1,
                        node: reply,
                    },
                ]
            };
            (focus, commands)
        };
        if !draft && branches == 1 {
            for child in slate_doc::agent_chat::subtree(&self.doc().scene, &children) {
                if let Some(before) = self.doc().scene.node(child) {
                    let mut after = before.clone();
                    after.rect.y -= lane_height * 0.5;
                    commands.push(slate_doc::SceneCmd::Patch {
                        before: Box::new(before.clone()),
                        after: Box::new(after),
                    });
                }
            }
        }
        if !self.commit_scene(commands) {
            return None;
        }
        self.board_sel.clear();
        self.board_sel.insert(reply_id);
        Some((reply_id, if linear { Vec::new() } else { history }))
    }
    /// Project, conversation, and first-load phases have no train handles yet.
    pub(super) fn agent_in_choose_phase(&self, id: NodeId) -> bool {
        self.agents.project_picker == Some(id)
            || self.agents.pending_chat_pick == Some(id)
            || self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|p| p.portal == id)
            || (self.agents.connection_pending == Some(id) && !self.agents.connection_background)
    }
    /// Train presentation is the default place context and document handles live.
    pub(super) fn agent_context_handles_visible(&self, id: NodeId) -> bool {
        if self.agent_in_choose_phase(id) {
            return false;
        }
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| {
                !a.provider.is_empty()
                    && a.chat.train
                    && a.chat.parent.is_some()
                    && a.view == atlas_ai::agent::PortalView::Chat
            })
    }
    pub(crate) fn agent_manual_context_at(&self, screen: Pos2, xf: &BoardXf) -> Option<NodeId> {
        self.agents
            .context_human
            .iter()
            .find(|(_, p)| screen.distance(**p) <= 10.0 * xf.z)
            .map(|(id, _)| *id)
    }
    pub(crate) fn context_auto_under(&self, screen: Pos2, xf: &BoardXf) -> Option<NodeId> {
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find(|n| {
                if n.hidden || n.locked || !self.agent_context_handles_visible(n.id) {
                    return false;
                }
                let Some(p) = self.agents.context_auto.get(&n.id).copied() else {
                    return false;
                };
                screen.distance(p) <= 10.0 * xf.z
            })
            .map(|n| n.id)
    }
    pub(super) fn agent_context_blurb(&self, id: NodeId) -> String {
        let mut lines = Vec::new();
        let refs = self.agent_artifacts(id, false);
        if refs.is_empty() {
            lines.push("No linked references.".into());
        } else {
            lines.extend(
                refs.iter()
                    .take(6)
                    .map(|a| format!("{} · {}", a.kind.label(), a.title)),
            );
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
            let Some(label) = self.doc().scene.node(src).map(context_label) else {
                continue;
            };
            lines.push(label);
        }
        lines.join("\n")
    }
    /// What the human context handle holds: the card's pocketed context.
    pub(super) fn agent_pocket_blurb(&self, id: NodeId) -> String {
        let hidden = slate_doc::agent_inputs::pocketed(&self.doc().scene, id);
        if hidden.is_empty() {
            return "Click to pocket this context again".into();
        }
        let mut lines = vec![format!(
            "Pocketed context · {} · click to show",
            hidden.len()
        )];
        lines.extend(
            hidden
                .iter()
                .filter_map(|n| self.doc().scene.node(*n))
                .take(8)
                .map(context_label),
        );
        lines.join("\n")
    }
    pub(crate) fn agent_artifact_at(&self, point: Pos2, xf: &BoardXf) -> Option<(NodeId, bool)> {
        for n in self
            .doc()
            .scene
            .nodes
            .iter()
            .rev()
            .filter(|n| !n.hidden && !n.locked && self.agent_context_handles_visible(n.id))
        {
            let r = xf.rect_w2s(n.rect);
            let left = self.agents.context_auto.get(&n.id);
            if let Some(left) = left {
                if point.distance(*left) <= 10.0 * xf.z {
                    return Some((n.id, false));
                }
            }
            if self.agent_has_outputs(n.id) {
                let right = Pos2::new(
                    r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                    r.center().y,
                );
                if point.distance(right) <= 10.0 * xf.z {
                    return Some((n.id, true));
                }
            }
        }
        None
    }
}
