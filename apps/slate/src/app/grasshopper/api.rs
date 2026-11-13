use super::*;

pub(super) fn grasshopper_item(
    tab: &SlateTab,
    id: NodeId,
) -> Option<(&ImageNode, &slate_doc::SlateItem)> {
    let NodeKind::Image(image) = &tab.doc.scene.node(id)?.kind else {
        return None;
    };
    let item = tab.doc.item(image.item)?;
    slate_doc::media::is_grasshopper(&item.path).then_some((image, item))
}

impl SlateApp {
    pub(crate) fn grasshopper_viewer(&self, id: NodeId) -> bool {
        grasshopper_item(self.tab(), id).is_some()
    }

    pub(crate) fn request_grasshopper(&mut self, id: NodeId, ctx: &egui::Context) {
        let tab = self.tab();
        let key = (tab.id, id);
        let Some((_, item)) = grasshopper_item(tab, id) else {
            return;
        };
        if self
            .grasshopper
            .views
            .get(&key)
            .is_some_and(|view| view.source.matches(tab, item))
        {
            return;
        }
        let source = Source {
            path: slate_doc::scene::resolve_source(
                tab.path.as_deref(),
                &item.path.to_string_lossy(),
            ),
            key: item.cache_key.clone(),
            item_path: item.path.clone(),
            workbook_path: tab.path.clone(),
        };
        if let Some(view) = self.grasshopper.views.get_mut(&key) {
            if view.source.path == source.path && view.source.workbook_path == source.workbook_path
            {
                view.source = source;
                view.source_revision = None;
                view.invalidate(Duration::ZERO);
                self.grasshopper.links_dirty = true;
                ctx.request_repaint();
                return;
            }
        }
        if self.grasshopper.views.contains_key(&key) {
            self.clear_grasshopper_preview(key.0, id);
            self.grasshopper.remove(key);
        }
        self.grasshopper.next_instance += 1;
        self.grasshopper
            .views
            .insert(key, View::new(source, self.grasshopper.next_instance));
        self.grasshopper.links_dirty = true;
        ctx.request_repaint();
    }

    pub(crate) fn grasshopper_preview(
        &self,
        doc: u64,
        id: NodeId,
        source_key: &str,
    ) -> Option<(&str, &Path)> {
        let view = self.grasshopper.views.get(&(doc, id))?;
        if view.source.key != source_key {
            return None;
        }
        let preview = view.preview.as_ref()?;
        Some((&preview.key, &preview.path))
    }

    pub(crate) fn grasshopper_status(&self, id: NodeId) -> Option<&str> {
        self.grasshopper
            .views
            .get(&(self.tab().id, id))?
            .status
            .as_deref()
    }

    pub(crate) fn grasshopper_working_label(&self, id: NodeId) -> Option<&'static str> {
        let key = (self.tab().id, id);
        let view = self.grasshopper.views.get(&key)?;
        if view.bake.is_some() {
            return Some("Baking geometry");
        }
        let solving = self
            .grasshopper
            .background
            .as_ref()
            .is_some_and(|worker| worker.busy == Some(key));
        if view.due.is_none() && !solving {
            return None;
        }
        if view.wait.is_some() {
            return view
                .wait
                .as_deref()
                .filter(|status| status.starts_with("Waiting"))
                .map(|_| "Waiting for connected inputs");
        }
        Some(match view.status.as_deref() {
            Some(status) if status.starts_with("Baking") => "Baking geometry",
            Some(status) if status.starts_with("Starting") => "Starting Grasshopper",
            _ => "Solving Grasshopper",
        })
    }

    /// Keep one rendered poster per viewer while a replacement is being drawn.
    pub(crate) fn grasshopper_displayed_poster(
        &mut self,
        id: NodeId,
        fresh: Option<egui::TextureHandle>,
    ) -> Option<egui::TextureId> {
        let key = (self.tab().id, id);
        let view = self.grasshopper.views.get_mut(&key)?;
        if let Some(fresh) = fresh {
            view.displayed_poster = Some(fresh);
        }
        view.displayed_poster.as_ref().map(egui::TextureHandle::id)
    }

    pub(crate) fn grasshopper_panel(&self, id: NodeId) -> Option<PanelState> {
        let view = self.grasshopper.views.get(&(self.tab().id, id))?;
        let mut controls = view.controls.clone();
        for control in &mut controls {
            if let Some(value) = view.desired.get(&control.id) {
                control.value = Some(value.clone());
            }
        }
        Some(PanelState {
            status: view.status.clone(),
            controls,
            can_bake: view.ready_revision.is_some()
                && view.preview.is_some()
                && view.bake.is_none()
                && !self.tab().read_only,
            can_retry: view.ready_revision.is_none()
                && view.status.is_some()
                && view.due.is_none()
                && self
                    .grasshopper
                    .background
                    .as_ref()
                    .is_none_or(|bg| bg.busy != Some((self.tab().id, id))),
        })
    }

    pub(crate) fn grasshopper_control_count(&self, id: NodeId) -> usize {
        self.grasshopper
            .views
            .get(&(self.tab().id, id))
            .map_or(0, |view| view.controls.len())
    }

    pub(crate) fn grasshopper_control(
        &self,
        node: NodeId,
        id: &GrasshopperControlId,
    ) -> Option<&GrasshopperControl> {
        self.grasshopper
            .views
            .get(&(self.tab().id, node))?
            .controls
            .iter()
            .find(|control| &control.id == id)
    }

    pub(crate) fn grasshopper_accepts_plugin_drop(&self, id: NodeId) -> bool {
        let key = (self.tab().id, id);
        self.grasshopper.views.get(&key).is_some_and(|view| {
            view.needs_plugins && view.due.is_none() && view.local_plugins.is_empty()
        }) && self
            .grasshopper
            .background
            .as_ref()
            .is_none_or(|bg| bg.busy != Some(key))
    }

    pub(crate) fn grasshopper_drop_plugins(&mut self, id: NodeId, mut paths: Vec<PathBuf>) -> bool {
        if !self.grasshopper_accepts_plugin_drop(id) || !self.grasshopper_viewer(id) {
            return false;
        }
        paths.retain(|path| is_grasshopper_plugin(path));
        paths.sort_unstable_by(|a, b| {
            packages::is_raw_plugin(a)
                .cmp(&packages::is_raw_plugin(b))
                .then_with(|| a.cmp(b))
        });
        paths.dedup();
        if paths.is_empty() {
            return false;
        }
        let key = (self.tab().id, id);
        let view = self
            .grasshopper
            .views
            .get_mut(&key)
            .expect("missing plug-in viewer");
        view.generation += 1;
        view.local_plugins = paths;
        view.retry_startup = true;
        view.due = Some(Instant::now());
        view.status = Some("Checking dropped Grasshopper plug-ins…".into());
        true
    }

    pub(crate) fn grasshopper_edit(
        &mut self,
        id: NodeId,
        control: GrasshopperControlId,
        value: GrasshopperValue,
        phase: EditPhase,
    ) {
        if self.tab().read_only {
            return;
        }
        let key = (self.tab().id, id);
        let Some(view) = self.grasshopper.views.get_mut(&key) else {
            return;
        };
        view.draft_changed = true;
        match phase {
            EditPhase::Preview => view.draft = Some((control, value)),
            EditPhase::Cancel => view.draft = None,
            EditPhase::Commit => {
                view.draft = None;
                self.last_board_edit = None;
                self.patch_nodes(&[id], |node| {
                    if let NodeKind::Image(image) = &mut node.kind {
                        let overrides = image.grasshopper.get_or_insert_with(Box::default);
                        let _ = overrides.set(control.clone(), value.clone());
                    }
                });
                self.last_board_edit = None;
            }
        }
    }

    pub(crate) fn grasshopper_retry(&mut self, id: NodeId) {
        if let Some(view) = self.grasshopper.views.get_mut(&(self.tab().id, id)) {
            view.invalidate(Duration::ZERO);
            view.retry_startup = true;
            view.status = Some("Retrying Grasshopper…".into());
            self.grasshopper.links_dirty = true;
        }
    }

    pub(crate) fn dispatch_grasshopper_bake(&mut self, detail: Option<&str>) -> bool {
        let id = match detail {
            Some(id) => id.parse().ok().map(NodeId),
            None if self.board_sel.len() == 1 => self.board_sel.iter().next().copied(),
            None => None,
        };
        let Some(id) = id else {
            return false;
        };
        let tab = self.tab();
        let Some((_, item)) = grasshopper_item(tab, id) else {
            return false;
        };
        let key = (tab.id, id);
        if tab.read_only
            || !self.grasshopper.views.get(&key).is_some_and(|view| {
                view.source.matches(tab, item)
                    && view.preview.is_some()
                    && view.bake.is_none()
                    && view.draft.is_none()
                    && view.ready_revision.is_some()
            })
        {
            return false;
        }
        let destination = output::capture_path(tab.path.as_deref(), id, "3dm");
        let view = self.grasshopper.views.get_mut(&key).expect("active viewer");
        let Some(revision) = view.ready_revision.take() else {
            return false;
        };
        view.bake = Some(BakeRequest {
            destination,
            revision,
            overrides: view.desired.clone(),
            inputs: view.linked.clone().unwrap_or_default(),
            generation: view.generation,
        });
        view.status = Some("Baking visible geometry…".into());
        true
    }
}
