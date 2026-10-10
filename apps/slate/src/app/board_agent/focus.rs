//! Wheel capture, focus, and project binding.

use super::*;

impl SlateApp {
    /// The wheel scrolls the album once a generator note holds more than one image.
    pub(crate) fn pointer_over_image_album(&self, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        let world = xf.s2w(p);
        // The index strip under an album browses it as well.
        let on_strip = self.doc().scene.nodes.iter().any(|n| {
            let count = self.agent_images(n.id).len();
            !n.hidden
                && count > 1
                && self.image_wheel(n.id) == ImageWheel::Album
                && atlas_shell::home::album_strip_rect(xf.rect_w2s(n.rect), count, xf.z).contains(p)
        });
        on_strip
            || self
                .doc()
                .scene
                .nodes
                .iter()
                .rev()
                .find(|n| !n.hidden && n.rect.contains(world.x, world.y))
                .is_some_and(|n| self.image_wheel(n.id) != ImageWheel::Board)
    }
    /// Who takes the wheel over an image portal: its album, the model a focused
    /// live generator flies, or the board. Paint and the board camera ask here.
    pub(crate) fn image_wheel(&self, id: NodeId) -> ImageWheel {
        if !slate_doc::agent_inputs::is_image_generator(&self.doc().scene, id) {
            return ImageWheel::Board;
        }
        let live = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.live);
        let focused = self.contents_focused() == Some(id) || self.portal_is_maximized(id);
        if live {
            if focused && self.generator_view(id).geometry().is_some() {
                ImageWheel::Model
            } else {
                ImageWheel::Board
            }
        } else if self.agent_images(id).len() > 1
            && (!matches!(
                self.doc().scene.node(id).map(|n| &n.kind),
                Some(NodeKind::Image(_))
            ) || (self.board_sel.len() == 1 && self.board_sel.contains(&id)))
        {
            ImageWheel::Album
        } else {
            // Unselected, a picture zooms with the board; hover never flows.
            ImageWheel::Board
        }
    }
    /// The wheel zooms the board while the pointer is over an agent card.
    pub(crate) fn pointer_over_agent_card(&self, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        let world = xf.s2w(p);
        self.doc().scene.nodes.iter().rev().any(|n| {
            !n.hidden
                && n.rect.contains(world.x, world.y)
                && matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent)
        })
    }
    /// The folder list before a chat starts. The wheel scrolls that list.
    pub(crate) fn pointer_over_project_picker(&self, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        let Some(id) = self.agents.project_picker else {
            return false;
        };
        let world = xf.s2w(p);
        self.doc()
            .scene
            .node(id)
            .is_some_and(|n| !n.hidden && n.rect.contains(world.x, world.y))
    }
    /// Cover Flow / bound poster owns the pointer only in contents-focus
    /// (or when maximized and unbound). Selection alone leaves the wheel
    /// on the board — same rule as the web portal (P1.portal.contents-focus).
    pub(crate) fn agent_shelf_captures(&self, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        if let Some(id) = self.portal_chrome.maximized {
            return self.agent_portal_unbound(id);
        }
        let Some(id) = self.agents.focused else {
            return false;
        };
        if !self.is_agent_portal(id) {
            return false;
        }
        let Some(node) = self.doc().scene.node(id) else {
            return false;
        };
        let srect = xf.rect_w2s(node.rect);
        let layout = layout_for_portal(
            PortalKind::Agent,
            srect,
            false,
            false,
            self.node_resolved_corner(node),
            xf.z,
        );
        if layout.pointer_on_chrome(p) {
            return false;
        }
        srect.contains(p)
    }
    /// Text editing owns the pointer. The rest of a train card drags as a node.
    pub(crate) fn agent_text_editing_captures(&self, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        if let Some((id, _)) = &self.agents.title_edit {
            if let Some(node) = self.doc().scene.node(*id) {
                let r = xf.rect_w2s(node.rect);
                let band = Rect::from_min_max(r.min, Pos2::new(r.right(), r.top() + 32.0 * xf.z));
                if band.contains(p) {
                    return true;
                }
            }
        }
        if self
            .agents
            .key_rect
            .is_some_and(|(_, rect)| rect.contains(p))
        {
            return true;
        }
        self.agents.composer_editing.is_some_and(|id| {
            self.agents
                .composer_rects
                .get(&id)
                .is_some_and(|rect| rect.contains(p))
        })
    }
    /// The tail composer under the pointer, when its card is the topmost node there.
    pub(super) fn composer_under(&self, p: Pos2, xf: &BoardXf) -> Option<NodeId> {
        let top = self.doc().scene.nodes.iter().rev().find(|n| {
            !n.hidden
                && !n.is_frame()
                && !matches!(n.kind, NodeKind::Connector(_))
                && xf.rect_w2s(n.rect).contains(p)
        })?;
        self.agents
            .composer_rects
            .get(&top.id)
            .is_some_and(|rect| rect.contains(p))
            .then_some(top.id)
    }
    /// Paint registers composer fields and scroll ranges again for what it draws.
    pub(crate) fn begin_agent_paint(&mut self) {
        self.agents.composer_rects.clear();
        self.agents.card_overflow.clear();
    }
    /// A user-sized card with more content than room scrolls instead of zooming.
    pub(crate) fn pointer_over_scrolling_agent_card(
        &self,
        xf: &BoardXf,
        pointer: Option<Pos2>,
    ) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        let world = xf.s2w(p);
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find(|n| !n.hidden && !n.is_frame() && n.rect.contains(world.x, world.y))
            .is_some_and(|n| {
                slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.size.is_some())
                    && self
                        .agents
                        .card_overflow
                        .get(&n.id)
                        .is_some_and(|max| *max > 0.5)
            })
    }
    pub(crate) fn agent_focus(&mut self, id: NodeId) {
        self.portal_enter_interactive(id);
    }
    pub(crate) fn agent_enter_contents(&mut self, id: NodeId) {
        if self.is_agent_portal(id) {
            if let Some((session, _)) = self.agent_session_for(id) {
                let selected = self
                    .board_sel
                    .iter()
                    .copied()
                    .filter(|n| *n != id)
                    .collect::<Vec<_>>();
                if !selected.is_empty() {
                    self.agents.context_selection.insert(session, selected);
                }
            }
            self.agents.focused = Some(id);
        }
    }
    pub(crate) fn agent_blur(&mut self) -> bool {
        if self.agents.focused.is_some() {
            self.contents_blur()
        } else {
            false
        }
    }
    pub(crate) fn agent_leave_contents(&mut self) -> bool {
        self.agents.focused.take().is_some()
    }
    pub(crate) fn agent_toggle_focus(&mut self) -> bool {
        if self.agents.focused.is_some() {
            return self.agent_blur();
        }
        match self.selected_agent_portal() {
            Some(id) => {
                self.agent_focus(id);
                true
            }
            None => false,
        }
    }
    pub(super) fn is_agent_portal(&self, id: NodeId) -> bool {
        self.doc()
            .scene
            .node(id)
            .is_some_and(|n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent))
    }
    pub(crate) fn pick_agent_project(&mut self, portal: NodeId) {
        self.picker.open(
            PickRequest::folder().title("Choose project folder"),
            move |picked| PickerMsg::AgentPortalSource {
                portal,
                path: file_picker::first(picked),
            },
        );
    }
    pub(crate) fn pick_selected_agent_project(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        self.pick_agent_project(id);
        true
    }
    pub(crate) fn agent_build_folder(&self) -> (std::path::PathBuf, bool) {
        train_ux::agent_build_dir(self.tab().path.as_deref(), &atlas_core::index::data_dir())
    }
    /// Bind Cursor or Codex to `<workbook>/assets/agent/` and start a new chat.
    pub(crate) fn agent_just_build(&mut self, portal: NodeId) {
        let (dir, fallback) = self.agent_build_folder();
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.toast(format!("Could not create the agent folder: {err}"));
            return;
        }
        if fallback {
            self.toast(format!(
                "This workbook isn't saved yet, so the agent is using {}.",
                dir.display()
            ));
        }
        self.agents.project_picker = None;
        self.agents.chat_picker = None;
        self.agents.pending_chat_pick = None;
        self.bind_portal_source(portal, dir);
        self.set_agent_channel(portal, None);
    }
    pub(crate) fn bind_agent_project(&mut self, portal: NodeId, path: PathBuf) {
        if self.agent_linear(portal)
            && (self
                .agents
                .session(portal)
                .is_some_and(|s| !s.conversation.is_empty())
                || self
                    .doc()
                    .scene
                    .node(portal)
                    .and_then(slate_doc::agent_chat::agent)
                    .is_some_and(|a| {
                        a.channel.is_some() || a.chat.parent.is_some() || a.chat.end.is_some()
                    }))
        {
            self.toast("Create a new agent portal to choose another project; this conversation keeps its workspace.");
            return;
        }
        if self.agent_is_running(portal) {
            self.toast("Stop the current response before changing folders.");
            return;
        }
        if !path.is_dir() {
            self.toast("Choose a folder for this agent portal.");
            return;
        }
        self.agents.project_picker = None;
        self.bind_portal_source(portal, path.clone());
        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Project")
            .to_string();
        if !cfg!(test) {
            let mut recents = RecentList::load(AGENT_RECENTS_KEY);
            recents.entries =
                atlas_ai::projects::without_temporary(std::mem::take(&mut recents.entries));
            recents.record(path.clone(), title.clone());
            recents.save(AGENT_RECENTS_KEY);
        }
        let used = RecentEntry {
            path: path.clone(),
            title,
            opened_at: atlas_ai::context::now_secs(),
            cover: None,
        };
        for list in std::iter::once(&mut self.agents.recents)
            .chain(self.agents.provider_recents.values_mut())
        {
            list.retain(|e| !atlas_ai::projects::same_folder(&e.path, &path));
            list.insert(0, used.clone());
        }
        self.agents.pending_chat_pick = Some(portal);
        self.agent_focus(portal);
        #[cfg(not(test))]
        {
            let provider = self
                .agent_session_for(portal)
                .map(|v| v.1)
                .unwrap_or_default();
            self.agents
                .chats
                .remove(&(provider.clone(), chat_key(&path)));
            self.request_agent_chats(path, &provider);
        }
    }
    pub(super) fn ensure_agent_recents(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.agents.recents_rx {
            match rx.try_recv() {
                Ok(list) => {
                    self.agents.provider_recents = list;
                    self.agents.recents_rx = None;
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.agents.recents_rx = None;
                }
            }
        }
        if self.agents.recents_started {
            return;
        }
        self.agents.recents_started = true;
        let workspace = self.ai.config.workspace_dir.clone();
        let workbook: Vec<PathBuf> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| slate_doc::agent_chat::agent(n).is_some())
            .filter_map(|n| self.agent_folder_for(n.id))
            .collect();
        let used = self.agents.recents.clone();
        let (tx, rx) = unbounded();
        self.agents.recents_rx = Some(rx);
        std::thread::spawn(move || {
            let mut slate = used;
            if !cfg!(test) {
                slate.extend(atlas_ai::projects::without_temporary(
                    RecentList::load(AGENT_RECENTS_KEY).entries,
                ));
            }
            if let Some(ws) = &workspace {
                slate.extend(atlas_ai::projects::session_projects(ws));
            }
            slate.extend(
                workbook
                    .into_iter()
                    .map(|p| atlas_ai::projects::entry(p, 0)),
            );
            let list = ["codex", "cursor"]
                .into_iter()
                .map(|provider| {
                    let merged = atlas_ai::projects::merge([
                        slate.clone(),
                        collect_agent_project_recents(provider),
                    ]);
                    (provider.to_string(), merged)
                })
                .collect();
            let _ = tx.send(list);
        });
    }
}
