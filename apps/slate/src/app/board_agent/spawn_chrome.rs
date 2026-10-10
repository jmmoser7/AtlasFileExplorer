//! Spawn preview, rename, collapse, and full-access grant.

use super::*;

impl SlateApp {
    pub(crate) fn paint_agent_spawn_preview(&self, painter: &egui::Painter, xf: &BoardXf) {
        self.paint_flow_ports(painter, xf);
        self.paint_agent_output_drag(painter, xf);
        let palette = self.palette();
        let pointer = painter.ctx().pointer_latest_pos();
        if let (Some((id, _)), Some(p)) = (self.agents.artifact_drag, pointer) {
            if let Some(n) = self.doc().scene.node(id) {
                let r = xf.rect_w2s(n.rect);
                let from = Pos2::new(
                    r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                    r.center().y,
                );
                painter.line_segment(
                    [from, p],
                    egui::Stroke::new((1.25 * xf.z).max(1.0), palette.sub.gamma_multiply(0.7)),
                );
            }
        }
        for n in &self.doc().scene.nodes {
            let Some(a) = slate_doc::agent_chat::agent(n) else {
                continue;
            };
            if self.agent_stop_shown(n) {
                self.paint_agent_stop(painter, xf, n, pointer);
                continue;
            }
            if n.hidden
                || n.locked
                || a.chat.draft
                || a.provider.is_empty()
                || a.chat.parent.is_none()
                || self.agent_in_choose_phase(n.id)
            {
                continue;
            }
            let r = xf.rect_w2s(n.rect);
            let p = Pos2::new(
                r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                r.top() + slate_doc::agent_chat::RAIL_INSET * xf.z,
            );
            let color = palette.sub.gamma_multiply(
                if pointer.is_some_and(|q| q.distance(p) < 16.0 * xf.z) {
                    0.9
                } else {
                    0.35
                },
            );
            let near = pointer.is_some_and(|q| q.distance(p) <= 10.0 * xf.z);
            paint_handle_dot(painter, p, xf.z, near, color);
        }
        if let (Some((id, _)), Some(pos)) = (self.agents.spawn_drag, self.board_point_snap) {
            {
                let lane = 0;
                let rect = slate_doc::WorldRect::new(
                    pos.x,
                    pos.y
                        + lane as f32
                            * (slate_doc::agent_chat::DRAFT_HEIGHT
                                + slate_doc::agent_chat::LANE_GAP),
                    self.agent_draft_width(id),
                    slate_doc::agent_chat::DRAFT_HEIGHT,
                );
                let r = xf.rect_w2s(rect);
                painter.rect_filled(r, 8.0 * xf.z, palette.card.gamma_multiply(0.45));
                painter.rect_stroke(
                    r,
                    8.0 * xf.z,
                    egui::Stroke::new(xf.z, palette.sub.gamma_multiply(0.5)),
                    egui::StrokeKind::Inside,
                );
                if let Some(parent) = self.doc().scene.node(id) {
                    if let NodeKind::Portal(p) = &parent.kind {
                        canvas_text::text(
                            painter,
                            r.min + egui::vec2(16.0, 10.0) * xf.z,
                            Align2::LEFT_CENTER,
                            &p.title,
                            FontId::proportional(13.0 * xf.z),
                            palette.sub.gamma_multiply(0.55),
                        );
                        painter.line_segment(
                            [
                                r.min + egui::vec2(12.0, 30.0) * xf.z,
                                r.min + egui::vec2(12.0, 45.0) * xf.z,
                            ],
                            egui::Stroke::new(xf.z, palette.sub.gamma_multiply(0.5)),
                        );
                    }
                    let b = slate_doc::agent_chat::rail(parent.rect, rect);
                    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                        [b.p0, b.c1, b.c2, b.p3].map(|p| xf.w2s(Pos2::new(p[0], p[1]))),
                        false,
                        Color32::TRANSPARENT,
                        egui::Stroke::new(xf.z, palette.sub.gamma_multiply(0.5)),
                    ));
                }
            }
        }
    }
    pub(crate) fn agent_rename(&mut self, title: &str) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let title = title.trim();
        if title.is_empty() {
            return false;
        }
        let ids = slate_doc::agent_chat::segments(&self.doc().scene, id)
            .into_iter()
            .find(|path| path.contains(&id))
            .unwrap_or_else(|| vec![id]);
        self.patch_nodes(&ids, |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                p.title = title.into();
            }
        });
        true
    }
    /// Selected chat cards that can collapse or be sized: not drafts, not bundles.
    fn selected_chat_cards(&self) -> Vec<NodeId> {
        self.board_sel
            .iter()
            .copied()
            .filter(|id| {
                self.doc()
                    .scene
                    .node(*id)
                    .and_then(slate_doc::agent_chat::agent)
                    .is_some_and(|a| {
                        a.view == atlas_ai::agent::PortalView::Chat
                            && !a.provider.is_empty()
                            && !a.chat.draft
                            && !(a.chat.train && !a.chat.bundled.is_empty())
                    })
            })
            .collect()
    }
    /// Collapse every selected card to three lines, or expand them all when
    /// every one is already collapsed.
    pub(crate) fn agent_toggle_collapse(&mut self, ctx: &egui::Context) -> bool {
        let ids = self.selected_chat_cards();
        let collapse = ids.iter().any(|id| {
            self.doc()
                .scene
                .node(*id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| !a.chat.collapsed)
        });
        let released = self.release_stream_open(&ids);
        self.refit_agent_cards(ctx, &ids, |chat| {
            chat.collapsed = collapse;
            chat.partial = false;
        }) || released
    }
    /// One chevron steps a level; the double jumps two. A missing detail keeps the keyboard toggle. Folding a
    /// card that is open only for streaming starts from what the person sees
    /// and makes that fold theirs.
    pub(crate) fn agent_fold(&mut self, ctx: &egui::Context, detail: Option<&str>) -> bool {
        let Some((dir, step)) = detail.and_then(train_ux::parse_chevron) else {
            return self.agent_toggle_collapse(ctx);
        };
        let ids = self.selected_chat_cards();
        let streaming: HashSet<NodeId> = ids
            .iter()
            .copied()
            .filter(|id| self.agents.stream_open.contains(id))
            .collect();
        let released = self.release_stream_open(&ids);
        self.refit_agent_cards_by(ctx, &ids, |id, chat| {
            let fold = card_fold(chat, streaming.contains(&id));
            write_fold(chat, train_ux::apply_fold(fold, dir, step));
        }) || released
    }
    /// A collapsed card shows twice the capsule while a reply streams. The
    /// fold is display state, not a person's edit: the card stays collapsed,
    /// its rect follows the ordinary auto-fit, and the capsule returns when
    /// the stream ends ([`Self::settle_stream_open`]).
    pub(super) fn open_streaming_card(&mut self, id: NodeId) {
        let open = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| {
                a.chat.collapsed && !a.chat.draft && !(a.chat.train && !a.chat.bundled.is_empty())
            });
        if open && self.agents.stream_open.insert(id) {
            self.agents.fit_revision = None;
        }
    }
    /// Drop streaming opens whose reply is no longer streaming, so the card
    /// refits to its capsule. Cheap when nothing is open.
    pub(crate) fn settle_stream_open(&mut self) {
        let awaiting = &self.agents.awaiting;
        if !self.agents.responding.is_empty() {
            self.agents
                .responding
                .retain(|id, _| matches!(awaiting.get(id), Some(AgentAwait::Responding { .. })));
        }
        if self.agents.stream_open.is_empty() {
            return;
        }
        let before = self.agents.stream_open.len();
        self.agents
            .stream_open
            .retain(|id| matches!(awaiting.get(id), Some(AgentAwait::Responding { .. })));
        if self.agents.stream_open.len() != before {
            self.agents.fit_revision = None;
        }
    }
    fn release_stream_open(&mut self, ids: &[NodeId]) -> bool {
        let mut released = false;
        for id in ids {
            released |= self.agents.stream_open.remove(id);
        }
        if released {
            self.agents.fit_revision = None;
        }
        released
    }
    /// Per-user grants that must survive a relaunch and never ride in a
    /// workbook: agent full access here, web origin consent in `board_web`.
    pub(crate) fn install_local_grants(&mut self) {
        let path = atlas_ai::access::store_path();
        self.agents.full_access = atlas_ai::access::load_in(&path);
        self.agents.crosstalk.trust = self
            .agents
            .full_access
            .iter()
            .filter(|s| atlas_ai::access::relay_granted_in(&path, s))
            .cloned()
            .collect();
        self.agents.access_path = Some(path);
        self.web
            .use_consent_file(atlas_core::index::data_dir().join("web-consent.json"));
    }
    pub(crate) fn agent_full_access(&self, session: &str) -> bool {
        self.agents.full_access.contains(session)
    }
    /// The person's explicit grant for one conversation: the provider runs
    /// commands and edits files without asking. Applies from the next message.
    pub(crate) fn agent_toggle_full_access(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some((session, provider)) = self.agent_session_for(id) else {
            return false;
        };
        if !atlas_ai::runtime::linear_provider(&provider) {
            return false;
        }
        let on = !self.agent_full_access(&session);
        if on && self.crosstalk_policy(&session) == atlas_agent::TurnPolicy::ReadOnly {
            let name = if provider == "codex" {
                "Codex"
            } else {
                "Cursor"
            };
            self.toast(format!(
                "{name} reviews read-only in this crosstalk, so Full access cannot apply. To let {name} build, pause the crosstalk and swap roles in the wire's capsule."
            ));
            return false;
        }
        let running = self.agent_is_running(id);
        if let Some(path) = self.agents.access_path.clone() {
            if let Err(error) = atlas_ai::access::set_in(&path, &session, on) {
                self.toast(error);
                return false;
            }
        }
        if on {
            self.agents.full_access.insert(session.clone());
        } else {
            self.agents.full_access.remove(&session);
        }
        // Cursor reads the grant when its sidecar starts; the next Send restarts it.
        if provider == "cursor" {
            self.restart_cursor_sidecar(&session);
        }
        let what = if on {
            "Full access on. This conversation runs commands and edits files without asking"
        } else {
            "Full access off. The agent asks before risky actions"
        };
        // A running reply keeps the permissions it started with; providers
        // fix them when a turn starts.
        self.toast(if running {
            format!("{what}, from the next message. The reply in progress keeps its permissions.")
        } else {
            format!("{what}.")
        });
        true
    }
    /// Forget the size a person gave these cards; they hug their text again.
    pub(crate) fn agent_fit_to_text(&mut self, ctx: &egui::Context) -> bool {
        let ids = self.selected_chat_cards();
        self.refit_agent_cards(ctx, &ids, |chat| {
            chat.size = None;
            chat.partial = false;
        })
    }
    /// Edit each card's view and refit its rect in the same journal step, so
    /// one Undo returns both.
    fn refit_agent_cards(
        &mut self,
        ctx: &egui::Context,
        ids: &[NodeId],
        edit: impl Fn(&mut slate_doc::agent_chat::ChatView),
    ) -> bool {
        self.refit_agent_cards_by(ctx, ids, |_, chat| edit(chat))
    }
    fn refit_agent_cards_by(
        &mut self,
        ctx: &egui::Context,
        ids: &[NodeId],
        edit: impl Fn(NodeId, &mut slate_doc::agent_chat::ChatView),
    ) -> bool {
        let mut commands = Vec::new();
        for id in ids {
            let Some(before) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let mut edited = before.clone();
            if let NodeKind::Portal(p) = &mut edited.kind {
                if let Some(a) = &mut p.agent {
                    edit(*id, &mut a.chat);
                }
            }
            if edited == before {
                continue;
            }
            let after = self.fitted_agent_card(ctx, &edited);
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        !commands.is_empty() && self.commit_scene(commands)
    }
    /// Unsent text and focus belong to the card; they leave with it.
    pub(crate) fn forget_deleted_agent_cards(&mut self, ids: &[NodeId]) {
        for id in ids {
            self.agents.prompts.remove(id);
            self.agents.pocket_open.remove(id);
            self.agents.composer_rects.remove(id);
            if self.agents.composer_editing == Some(*id) {
                self.agents.composer_editing = None;
            }
            if self.agents.composer_focus == Some(*id) {
                self.agents.composer_focus = None;
            }
        }
        self.agents.prompt_epoch = self.agents.prompt_epoch.wrapping_add(1);
    }
    /// Which Delete keys remove the selected chat cards while a card holds the
    /// keyboard: `(Delete, Backspace)`. Typed text keeps both keys. An empty
    /// composer lets Delete through; Backspace stays with the field.
    pub(crate) fn agent_delete_keys(&self, wants_kb: bool) -> (bool, bool) {
        let cards = !self.board_sel.is_empty()
            && self
                .board_sel
                .iter()
                .all(|id| slate_doc::agent_inputs::is_chat_card(&self.doc().scene, *id));
        if !cards || self.agents.title_edit.is_some() || self.agents.key_focus.is_some() {
            return (false, false);
        }
        match self.agents.composer_editing {
            Some(id) => {
                let empty = self.agents.prompts.get(&id).is_none_or(|t| t.is_empty());
                (empty && self.board_sel.contains(&id), false)
            }
            None if wants_kb => (false, false),
            None => (true, true),
        }
    }
}
