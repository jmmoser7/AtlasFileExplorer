//! Bundle, history rails, and summary paint.

use super::*;

impl SlateApp {
    pub(super) fn agent_all_turns(&self, id: NodeId) -> Vec<AgentTurn> {
        self.agents
            .local_turns
            .get(&id)
            .or_else(|| self.agents.sessions.get(&id).map(|s| &s.turns))
            .cloned()
            .unwrap_or_default()
    }
    pub(crate) fn agent_bundle_selection(&mut self) -> bool {
        let ids: Vec<_> = self.board_sel.iter().copied().collect();
        let Some(run) = slate_doc::agent_chat::bundle_run(&self.doc().scene, &ids) else {
            self.toast("Select a consecutive run on one branch, without an interior fork.");
            return true;
        };
        let last = *run.last().unwrap();
        if run.iter().any(|id| self.agent_is_running(*id)) {
            self.toast("Wait for the response before bundling.");
            return true;
        }
        self.patch_nodes(&run, |n| bundle_view(n, &run));
        self.board_sel.clear();
        self.board_sel.insert(last);
        true
    }
    pub(crate) fn agent_expand_bundle(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some(a) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return false;
        };
        if !a.chat.train || a.chat.bundled.is_empty() {
            return false;
        }
        let mut ids = a.chat.bundled.clone();
        ids.push(id);
        let origin = self.doc().scene.node(id).unwrap().rect;
        let mut x = origin.x;
        let mut positions = HashMap::new();
        for member in &ids {
            let n = self.doc().scene.node(*member).unwrap();
            positions.insert(*member, x);
            x += n.rect.w + slate_doc::agent_chat::CARD_GAP;
        }
        let mut commands = Vec::new();
        for member in &ids {
            let before = self.doc().scene.node(*member).unwrap().clone();
            let mut after = before.clone();
            let n = &mut after;
            n.hidden = false;
            n.rect.x = positions[&n.id];
            n.rect.y = origin.y;
            if n.id == id {
                if let NodeKind::Portal(p) = &mut n.kind {
                    if let Some(a) = &mut p.agent {
                        a.chat.bundled = a.chat.bundle_layers.pop().unwrap_or_default();
                        a.chat.train = true;
                    }
                }
            }
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        self.arrange_agent_projection(id, &mut commands);
        self.follow_crosswires(&mut commands);
        self.commit_scene(commands);
        true
    }
    pub(crate) fn agent_show_chat(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let segments = slate_doc::agent_chat::segments(&self.doc().scene, id);
        if segments
            .iter()
            .flatten()
            .any(|id| self.agent_is_running(*id))
        {
            self.toast("Wait for the response before changing its presentation.");
            return true;
        }
        let mut commands = Vec::new();
        let mut selected = None;
        let mut anchor = None;
        let mut absorbed = Vec::new();
        for segment in segments {
            let Some((path, draft)) = self.agent_path_split(&segment) else {
                continue;
            };
            let Some(&last) = path.last() else {
                if let Some(draft) = draft {
                    self.restyle_agent_draft(
                        draft,
                        false,
                        slate_doc::agent_chat::Detail::Full,
                        &mut commands,
                    );
                }
                continue;
            };
            anchor = anchor.or(Some(last));
            if segment.contains(&id) {
                selected = Some(last);
            }
            if let Some(draft) = draft {
                absorbed.push((draft, last));
            }
            for member in &path {
                let before = self.doc().scene.node(*member).unwrap().clone();
                let mut after = before.clone();
                after.hidden = *member != last;
                if *member == last {
                    if let NodeKind::Portal(p) = &mut after.kind {
                        if let Some(a) = &mut p.agent {
                            a.chat.bundled = path[..path.len() - 1].to_vec();
                            a.chat.train = false;
                            a.chat.detail = slate_doc::agent_chat::Detail::Full;
                            a.chat.window_height =
                                Some(before.rect.h.max(slate_doc::agent_chat::DRAFT_HEIGHT) as u32);
                        }
                    }
                }
                commands.push(slate_doc::SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                });
            }
        }
        let Some(anchor) = anchor else {
            return true;
        };
        let moved: HashMap<_, _> = absorbed.iter().copied().collect();
        self.retarget_agent_wires(&moved, &mut commands);
        let mut removed: Vec<_> = absorbed
            .iter()
            .filter_map(|(draft, _)| {
                let index = self.doc().scene.index_of(*draft)?;
                Some((index, self.doc().scene.node(*draft)?.clone()))
            })
            .collect();
        removed.sort_by_key(|(index, _)| std::cmp::Reverse(*index));
        commands.extend(
            removed
                .into_iter()
                .map(|(index, node)| slate_doc::SceneCmd::Remove { index, node }),
        );
        self.arrange_agent_projection(anchor, &mut commands);
        self.follow_crosswires(&mut commands);
        if self.commit_scene(commands) {
            self.absorb_agent_drafts(&absorbed);
            self.agents.projection_settle =
                Some((anchor, self.tab().journal.undo_depth(), self.scene_gen));
        }
        if let Some(id) = selected {
            self.board_sel.clear();
            self.board_sel.insert(id);
        }
        true
    }
    pub(crate) fn agent_set_detail(&mut self, detail: slate_doc::agent_chat::Detail) -> bool {
        let ids: Vec<_> = self.board_sel.iter().copied().collect();
        self.patch_nodes(&ids, |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                if let Some(a) = &mut p.agent {
                    if !a.chat.train && detail != slate_doc::agent_chat::Detail::Full {
                        return;
                    }
                    a.chat.detail = detail;
                    n.rect.h = match detail {
                        slate_doc::agent_chat::Detail::Identity => 48.0,
                        slate_doc::agent_chat::Detail::Summary => {
                            slate_doc::agent_chat::CARD_HEIGHT
                        }
                        slate_doc::agent_chat::Detail::Pair => slate_doc::agent_chat::PAIR_HEIGHT,
                        slate_doc::agent_chat::Detail::Full => 540.0,
                    };
                }
            }
        });
        true
    }
    /// Opacity of a train's history rails over `palette.sub`, per theme.
    fn rail_opacity(&self) -> f32 {
        if self.palette().dark_mode {
            slate_doc::agent_chat::RAIL_OPACITY
        } else {
            slate_doc::agent_chat::RAIL_LIGHT_OPACITY
        }
    }
    /// The calm gray of a train's history rails as a stored wire color. A wire
    /// into a chat card starts with it; a color the person picks later stays.
    pub(crate) fn chat_wire_color(&self) -> slate_doc::scene::Rgba {
        let sub = self.palette().sub;
        slate_doc::scene::Rgba([
            sub.r(),
            sub.g(),
            sub.b(),
            (self.rail_opacity() * 255.0).round() as u8,
        ])
    }
    pub(crate) fn paint_agent_history_rails(&self, painter: &egui::Painter, xf: &BoardXf) {
        let palette = self.palette();
        let rail_color = palette.sub.gamma_multiply(self.rail_opacity());
        let mut cache = self.agents.rail_cache.borrow_mut();
        let revision = (self.scene_gen, self.doc().scene.scene_gen());
        if cache.0 != revision || cache.1.is_empty() {
            *cache = (
                revision,
                slate_doc::agent_chat::history_rails(&self.doc().scene),
            );
        }
        for rail in &cache.1 {
            let b = &rail.curve;
            if !xf
                .rect_w2s(b.aabb())
                .expand(2.0 * xf.z)
                .intersects(painter.clip_rect())
            {
                continue;
            }
            let point = |p: [f32; 2]| xf.w2s(egui::pos2(p[0], p[1]));
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [point(b.p0), point(b.c1), point(b.c2), point(b.p3)],
                false,
                Color32::TRANSPARENT,
                egui::Stroke::new(
                    canvas_scale::px(slate_doc::agent_chat::RAIL_WIDTH, xf.z),
                    rail_color,
                ),
            ));
        }
    }
    /// What invalidates cached agent text. The camera is deliberately absent:
    /// line breaks are world-unit layouts and must not change with zoom.
    pub(super) fn agent_text_key(&self) -> (u64, u64, u64) {
        let palette = self.palette();
        (
            self.agents.output_epoch,
            self.doc().scene.scene_gen(),
            u32::from_le_bytes(palette.ink.to_array()) as u64,
        )
    }
    pub(super) fn paint_agent_bundle(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let Some(a) = portal.agent.as_ref() else {
            return;
        };
        let palette = self.palette();
        let rect = xf.rect_w2s(node.rect);
        self.paint_agent_bundle_count(painter, rect, node, a, xf.z);
        for (index, id) in a
            .chat
            .bundled
            .iter()
            .chain(std::iter::once(&node.id))
            .enumerate()
        {
            let origin = rect.min
                + egui::vec2(
                    12.0 + (index % 6) as f32 * 76.0,
                    32.0 + (index / 6) as f32 * 88.0,
                ) * xf.z;
            let mini = Rect::from_min_size(origin, egui::vec2(64.0, 72.0) * xf.z);
            atlas_shell::selection_tools::agent_card(
                painter,
                mini,
                5.0 * xf.z,
                xf.z * 0.5,
                palette,
            );
            painter.circle_filled(
                origin + egui::vec2(6.0, 8.0) * xf.z,
                2.0 * xf.z,
                palette.sub.gamma_multiply(0.5),
            );
            let Some(member) = self.doc().scene.node(*id) else {
                continue;
            };
            let Some(binding) = slate_doc::agent_chat::agent(member) else {
                continue;
            };
            let key = self.agent_text_key();
            let mut cache = self.agents.summary_cache.borrow_mut();
            if cache.get(id).is_none_or(|e| (e.0, e.1, e.2) != key) {
                let turns = self.agent_all_turns(*id);
                let text = turns
                    .iter()
                    .skip(binding.chat.start)
                    .take(
                        binding
                            .chat
                            .end
                            .unwrap_or(turns.len())
                            .saturating_sub(binding.chat.start),
                    )
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let excerpt: String = text.chars().take(100).collect();
                let miniature = canvas_text::world_layout(
                    painter.ctx(),
                    &excerpt,
                    FontId::proportional(5.0),
                    52.0,
                    egui::Align::LEFT,
                );
                cache.insert(*id, (key.0, key.1, key.2, miniature));
            }
            let miniature = canvas_text::world_text(painter.ctx(), &cache[id].3, xf.z);
            miniature.paint(
                &painter.with_clip_rect(mini.shrink(4.0 * xf.z)),
                origin + egui::vec2(6.0, 17.0) * xf.z,
                palette.ink,
            );
        }
    }
    /// `max_rows` paints a collapsed capsule. A user-sized card scrolls what overflows.
    pub(super) fn paint_agent_summary(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
        max_rows: Option<usize>,
    ) {
        let Some(agent) = portal.agent.as_ref() else {
            return;
        };
        let rect = xf.rect_w2s(node.rect);
        let palette = self.palette();
        let z = xf.z;
        let clip = rect.shrink(10.0 * z).intersect(painter.clip_rect());
        let painter = painter.with_clip_rect(clip);
        if !canvas_text::legible(canvas_text::authored_px(13.0, z)) {
            return;
        }
        // A one-message card whose message another agent wrote reads in red.
        let first_turn = slate_doc::agent_chat::bundle_entry(&self.doc().scene, node)
            .and_then(slate_doc::agent_chat::agent)
            .map_or(agent.chat.start, |a| a.chat.start);
        let from_agent = agent.chat.end == Some(first_turn + 1)
            && self
                .crosstalk_relayed()
                .contains_key(&(agent.session.clone(), first_turn));
        let ink = if from_agent {
            palette.danger
        } else {
            palette.ink
        };
        let key = self.agent_text_key();
        let key = if from_agent {
            (key.0, key.1, key.2 ^ (1 << 63))
        } else {
            key
        };
        let mut cache = self.agents.summary_cache.borrow_mut();
        if cache
            .get(&node.id)
            .is_none_or(|entry| (entry.0, entry.1, entry.2) != key)
        {
            let turns = self.visible_agent_turns(node.id);
            let excerpt = turns
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let layout = canvas_text::world_layout_rows(
                painter.ctx(),
                &excerpt,
                FontId::proportional(13.0),
                (node.rect.w - 24.0).max(1.0),
                egui::Align::LEFT,
                max_rows.unwrap_or(usize::MAX),
            );
            cache.insert(node.id, (key.0, key.1, key.2, layout));
        }
        let laid = canvas_text::world_text(painter.ctx(), &cache[&node.id].3, z);
        drop(cache);
        let mut offset = 0.0;
        if max_rows.is_none() && agent.chat.size.is_some() {
            let room = node.rect.h - SUMMARY_TEXT_TOP - 10.0;
            let max = (laid.size().y / z - room).max(0.0);
            self.agents.card_overflow.insert(node.id, max);
            offset = self
                .agents
                .transcript_scroll
                .get(&node.id)
                .copied()
                .unwrap_or(0.0);
            if max > 0.0
                && ui
                    .ctx()
                    .pointer_hover_pos()
                    .is_some_and(|p| clip.contains(p))
            {
                offset -= ui.input(|i| i.smooth_scroll_delta.y) / z;
            }
            offset = offset.clamp(0.0, max);
            self.agents.transcript_scroll.insert(node.id, offset);
        }
        let mut text_ui = egui::Ui::new(
            ui.ctx().clone(),
            Id::new(("agent-summary", node.id.0)),
            egui::UiBuilder::new()
                .layer_id(ui.layer_id())
                .max_rect(clip),
        );
        text_ui.set_clip_rect(clip);
        let at = rect.min + egui::vec2(12.0, SUMMARY_TEXT_TOP - offset) * z;
        if from_agent {
            let bar = Rect::from_min_size(
                at - egui::vec2(6.0 * z, 0.0),
                egui::vec2(
                    canvas_scale::px(crosstalk::MESSAGE_BAR, z),
                    laid.size().y.min(clip.bottom() - at.y).max(0.0),
                ),
            );
            painter.rect_filled(bar, 0.0, palette.danger);
        }
        laid.selectable(
            &text_ui,
            Id::new(("agent-summary-selection", node.id.0)),
            at,
            ink,
        );
    }
}
