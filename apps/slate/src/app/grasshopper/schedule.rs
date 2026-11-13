use super::*;

impl SlateApp {
    pub(in crate::app) fn grasshopper_frame(&mut self, ctx: &egui::Context) {
        if self.grasshopper.views.is_empty() && self.grasshopper.background.is_none() {
            return;
        }
        let active_doc = self.tab().id;
        self.request_grasshopper_dependencies(ctx);
        let mut links_changed = std::mem::take(&mut self.grasshopper.links_dirty);
        links_changed |= self.grasshopper.poll_sources(ctx, active_doc);
        let keys: Vec<_> = self.grasshopper.views.keys().copied().collect();
        for key in keys {
            let image = self
                .tabs
                .iter()
                .find(|tab| tab.id == key.0)
                .and_then(|tab| {
                    let (image, item) = grasshopper_item(tab, key.1)?;
                    self.grasshopper.views[&key]
                        .source
                        .matches(tab, item)
                        .then_some(image)
                });
            if let Some(image) = image {
                let view = self.grasshopper.views.get_mut(&key).expect("viewer key");
                let generation = view.generation;
                if key.0 != active_doc && view.draft.take().is_some() {
                    view.draft_changed = true;
                }
                view.update_inputs(image.grasshopper.as_deref());
                links_changed |= generation != view.generation;
            } else {
                self.clear_grasshopper_preview(key.0, key.1);
                self.grasshopper.remove(key);
                links_changed = true;
            }
        }
        if links_changed {
            self.update_grasshopper_links();
        }
        let mut finished = false;
        while let Some(event) = self
            .grasshopper
            .background
            .as_ref()
            .and_then(|bg| bg.results.try_recv().ok())
        {
            match event {
                WorkerEvent::Stage {
                    viewer,
                    instance,
                    generation,
                    message,
                } => {
                    if let Some(view) = self.grasshopper.views.get_mut(&viewer) {
                        if (view.instance, view.generation) == (instance, generation)
                            && view.wait.is_none()
                        {
                            view.status = Some(message.into());
                        }
                    }
                }
                WorkerEvent::Finished(completion) => {
                    self.grasshopper.background.as_mut().expect("worker").busy = None;
                    if completion.was_bake {
                        ctx.request_repaint();
                    }
                    self.complete_grasshopper(*completion);
                    finished = true;
                }
            }
        }
        if finished {
            self.update_grasshopper_links();
        }
        for (id, bake) in self.grasshopper.take_completed_bakes(active_doc) {
            self.place_grasshopper_bake(id, bake);
        }

        if self
            .grasshopper
            .background
            .as_ref()
            .is_some_and(|bg| bg.busy.is_some())
        {
            return;
        }
        let now = Instant::now();
        let next = self
            .grasshopper
            .views
            .iter()
            .filter(|(key, view)| {
                key.0 == active_doc
                    && view.wait.is_none()
                    && (view.due.is_some() || view.bake.is_some())
            })
            .min_by_key(|(_, view)| (view.bake.is_none(), view.due.unwrap_or(now)))
            .map(|(key, view)| {
                (
                    *key,
                    if view.bake.is_some() {
                        now
                    } else {
                        view.due.unwrap_or(now)
                    },
                )
            });
        let Some((key, due)) = next else {
            return;
        };
        if due > now {
            ctx.request_repaint_after(due - now);
            return;
        }
        let view = self.grasshopper.views.get_mut(&key).expect("viewer key");
        let bake = view.bake.take();
        if bake.is_none() {
            view.due = None;
        }
        let job = Job {
            viewer: key,
            instance: view.instance,
            source: view.source.clone(),
            overrides: bake
                .as_ref()
                .map_or(&view.desired, |bake| &bake.overrides)
                .clone(),
            inputs: bake.as_ref().map_or_else(
                || view.linked.clone().unwrap_or_default(),
                |bake| bake.inputs.clone(),
            ),
            generation: bake
                .as_ref()
                .map_or(view.generation, |bake| bake.generation),
            bake,
            retry_startup: std::mem::take(&mut view.retry_startup),
            local_plugins: std::mem::take(&mut view.local_plugins),
        };
        let background = self
            .grasshopper
            .background
            .get_or_insert_with(|| Background::start(ctx));
        if background.requests.send(Work::Solve(Box::new(job))).is_ok() {
            background.busy = Some(key);
        } else {
            view.status = Some("The Grasshopper worker stopped. Retry to start it again.".into());
            self.grasshopper.background = None;
        }
    }

    pub(super) fn complete_grasshopper(&mut self, completion: Completion) {
        let key = completion.viewer;
        let Some(view) = self.grasshopper.views.get_mut(&key) else {
            return;
        };
        if view.instance != completion.instance
            || view.source != completion.source
            || view.generation != completion.generation
        {
            return;
        }
        if let Ok(solved) = &completion.result {
            view.source_revision = Some(solved.revision.clone());
        }
        if completion.was_bake {
            match completion.result {
                Ok(solved) if solved.bake.is_some() => {
                    view.completed_bake = solved.bake;
                    view.ready_revision = Some(solved.revision);
                    view.status = solved.output.recovery_warning().map(str::to_owned);
                }
                Ok(solved) => view.status = Some(response_status(&solved.output)),
                Err(SolveError::Failed(error)) => view.status = Some(error),
                Err(SolveError::SourceChanged) => view.invalidate(Duration::ZERO),
            }
            return;
        }
        let solved = match completion.result {
            Ok(solved) => solved,
            Err(SolveError::Failed(error)) => {
                view.status = Some(error);
                return;
            }
            Err(SolveError::SourceChanged) => {
                view.invalidate(Duration::ZERO);
                return;
            }
        };
        if let Ok(controls) = serde_json::from_value(solved.output.response.controls.clone()) {
            view.controls = controls;
        }
        if let Ok(ports) = serde_json::from_value(solved.output.response.ports.clone()) {
            if view.ports != ports {
                self.connector_pick_bounds = None;
            }
            view.ports = ports;
            view.port_revision = Some(solved.revision.clone());
        }
        view.needs_plugins = solved.output.response.status == "missing_packages";
        if solved.output.response.status != "ready" {
            view.status = Some(response_status(&solved.output));
            return;
        }
        view.outputs = Some(
            serde_json::from_value(solved.output.response.outputs.clone()).unwrap_or_default(),
        );
        match solved.preview {
            Some(Ok((scene, hash))) => {
                let cache_key = format!("grasshopper-{}-{}-{hash}", key.0, view.instance);
                let path = solved.output.preview_path().expect("preview path");
                view.status = solved.output.response.diagnostics.first().cloned();
                view.ready_revision = Some(solved.revision);
                view.preview = Some(Preview {
                    key: cache_key.clone(),
                    path,
                    _owner: solved.output,
                });
                self.install_grasshopper_preview(key.0, key.1, cache_key, scene, hash);
            }
            Some(Err(error)) => view.status = Some(error),
            None => {
                view.preview = None;
                view.displayed_poster = None;
                view.status = Some(response_status(&solved.output));
                self.clear_grasshopper_preview(key.0, key.1);
            }
        }
    }

    pub(super) fn place_grasshopper_bake(&mut self, source: NodeId, mut bake: BakedFile) {
        if self.tab().read_only || !self.grasshopper_viewer(source) {
            return;
        }
        let Some(rect) = self.doc().scene.node(source).map(|node| node.rect) else {
            return;
        };
        let previous = self
            .doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find_map(|node| match &node.kind {
                NodeKind::Image(image) if image.grasshopper_bake_source == Some(source) => {
                    Some(node.rect)
                }
                _ => None,
            });
        let (x, y) = previous.map_or((rect.x + rect.w + 24.0, rect.y), |previous| {
            (previous.x, previous.y + previous.h + 24.0)
        });
        let Some(item) = bake.0.as_deref().and_then(|path| self.item_for_path(path)) else {
            self.toast("Could not add the Grasshopper bake to the workbook");
            return;
        };
        let mut image = ImageNode::new(item);
        image.grasshopper_bake_source = Some(source);
        let node = self
            .doc()
            .scene
            .node_preparation()
            .build_node(WorldRect::new(x, y, rect.w, rect.h), NodeKind::Image(image));
        if !self.add_nodes(vec![node]).is_empty() {
            bake.0.take();
            self.toast("Baked Grasshopper preview to a 3D model");
        }
    }
}
