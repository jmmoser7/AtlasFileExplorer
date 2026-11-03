//! Explicit empty .3dm creation through existing worker, media and agent owners.
use super::*;
use atlas_connector_runtime::transport::Cancellation;
use slate_doc::scene::{NodeKind, PortalNode, WorldRect};

pub(super) struct BlankJob {
    tab: u64,
    rect: WorldRect,
    rx: Receiver<Result<super::super::super::items::ReadyFile, String>>,
    cancel: Cancellation,
    accepted: crossbeam_channel::Sender<bool>,
    downstream: Option<Downstream>,
}

struct Downstream {
    source: NodeId,
    from: (slate_doc::scene::Side, f32),
    source_kind: slate_doc::agent_inputs::InputKind,
    provider: String,
    model: String,
    history: Option<slate_doc::agent_inputs::GeneratorMemoryPin>,
}

impl SlateApp {
    pub(crate) fn connector_stop_empty_headless(&mut self) {
        for job in self.connectors.headless.blanks.drain(..) {
            job.cancel.cancel();
        }
    }
    pub(crate) fn connector_new_headless(&mut self) -> Result<(), String> {
        let center = self.board_xf().s2w(self.canvas_rect.center());
        self.start_blank_model(
            WorldRect::new(center.x - 320., center.y - 240., 640., 480.),
            None,
        )
    }

    /// Only a chosen downstream builder requires an admitted native agent.
    /// Root New 3D remains usable as an ordinary local model without an agent.
    pub(crate) fn connector_new_headless_from(
        &mut self,
        source: NodeId,
        at: Option<[f32; 2]>,
        from: Option<(slate_doc::scene::Side, f32)>,
    ) -> Result<(), String> {
        let program = self.blank_model_program()?;
        let model = self
            .agents
            .catalog(&program.id)
            .iter()
            .find(|model| model.is_default)
            .or_else(|| self.agents.catalog(&program.id).first())
            .ok_or("Choose an available native model before creating a downstream 3D builder")?
            .id
            .clone();
        let parent = self
            .doc()
            .scene
            .node(source)
            .ok_or("The source is missing")?;
        if parent.locked || !self.is_agent_media(source) {
            return Err("Choose an unlocked media source".into());
        }
        let source_kind = slate_doc::agent_inputs::wire_kind(&self.doc().scene, source, &|item| {
            self.doc().item(item).map(|file| file.path.as_path())
        })
        .ok_or("The source has no compatible output")?;
        let point = at.unwrap_or([parent.rect.x + parent.rect.w + 180., parent.rect.y + 48.]);
        if point
            .iter()
            .any(|value| !value.is_finite() || value.abs() > 1.0e9)
        {
            return Err("Model placement must use finite board coordinates".into());
        }
        let history = if self.image_history_preserved(source) {
            Some(slate_doc::agent_inputs::GeneratorMemoryPin {
                source_revision: slate_doc::agent_chat::agent(parent)
                    .map_or(String::new(), |agent| agent.session.clone()),
                turns: self.public_image_history(source)?,
            })
        } else {
            None
        };
        self.start_blank_model(
            WorldRect::new(point[0], point[1] - 48., 640., 480.),
            Some(Downstream {
                source,
                from: from.unwrap_or((
                    slate_doc::scene::Side::Right,
                    slate_doc::agent_inputs::OUTPUT_T,
                )),
                source_kind,
                provider: program.id,
                model,
                history,
            }),
        )
    }

    fn blank_model_program(&self) -> Result<atlas_ai::agent::AgentProvider, String> {
        self.registered_agent_programs()
            .iter()
            .find(|program| Self::native_model_harness(&program.id))
            .cloned()
            .ok_or_else(|| {
                "Set up a supported native model agent before creating a downstream 3D builder"
                    .into()
            })
    }

    fn start_blank_model(
        &mut self,
        rect: WorldRect,
        downstream: Option<Downstream>,
    ) -> Result<(), String> {
        if self.at_home || self.tab().read_only {
            return Err("Open an editable board to create a 3D builder".into());
        }
        if !self.connectors.headless.blanks.is_empty() {
            return Err("An empty 3D model is already being created".into());
        }
        #[cfg(not(test))]
        let (rx, cancel, accepted) = worker::create_empty_headless();
        #[cfg(test)]
        let (rx, cancel, accepted) = EMPTY_PREPARATION_FOR_TEST
            .with(|fixture| fixture.borrow_mut().take())
            .unwrap_or_else(worker::create_empty_headless);
        self.connectors.headless.blanks.push(BlankJob {
            tab: self.tab().id,
            rect,
            rx,
            cancel,
            accepted,
            downstream,
        });
        self.toast("Creating an empty Rhino model headlessly");
        Ok(())
    }

    /// Resolve every late dependency and prepare all nodes before item registration
    /// or the single linked-media commit. Canonical slots keep their identities.
    fn prepare_blank_model_nodes(
        &self,
        job: &BlankJob,
        file: &super::super::super::items::ReadyFile,
    ) -> Result<Vec<slate_doc::Node>, String> {
        use slate_doc::scene::{ConnectorEnd, Side};
        if self.tab().read_only {
            return Err("Workbook is read-only".into());
        }
        let mut scratch = self.doc().scene.node_preparation();
        let mut node = self.prepare_ready_media_node(&mut scratch, job.rect, file);
        if let Some(link) = &job.downstream {
            let program = self
                .registered_agent_programs()
                .iter()
                .find(|program| {
                    program.id == link.provider && Self::native_model_harness(&program.id)
                })
                .ok_or("The chosen native model agent is no longer available")?;
            if !self
                .agents
                .catalog(&link.provider)
                .iter()
                .any(|model| model.id == link.model)
            {
                return Err("The chosen native model is no longer available".into());
            }
            let parent = self
                .doc()
                .scene
                .node(link.source)
                .filter(|node| !node.locked)
                .ok_or("The model source is no longer available")?;
            let current_kind =
                slate_doc::agent_inputs::wire_kind(&self.doc().scene, link.source, &|item| {
                    self.doc().item(item).map(|file| file.path.as_path())
                })
                .ok_or("The model source has no compatible output")?;
            if current_kind != link.source_kind {
                return Err("The model source output changed while the blank was prepared".into());
            }
            self.configure_blank_model_agent(&mut node, program, Some(link.model.clone()));
            if let Some(agent) = slate_doc::agent_chat::agent_mut(&mut node) {
                agent.generator.preserve_history = Some(link.history.is_some());
                agent.generator.history_source = link.history.as_ref().map(|_| link.source);
                agent.generator.inherited_turn_count = Some(0);
            }
            let mut endpoints = slate_doc::Scene::default();
            endpoints.nodes = vec![parent.clone(), node.clone()];
            let port =
                slate_doc::agent_inputs::default_port(&endpoints, node.id, link.source, &|item| {
                    self.doc().item(item).map(|file| file.path.as_path())
                })
                .ok_or("The model has no compatible input for this source")?;
            let template = self.connector_template_bound(
                ConnectorEnd::Anchored {
                    node: link.source,
                    side: link.from.0,
                    t: link.from.1,
                },
                ConnectorEnd::Anchored {
                    node: node.id,
                    side: Side::Left,
                    t: port.t,
                },
                std::slice::from_ref(&node),
                None,
            );
            if !matches!(&template.kind, NodeKind::Connector(conn) if conn.binding.is_some()) {
                return Err("The model input binding could not be prepared".into());
            }
            let data = scratch.build_node(template.rect, template.kind);
            let memory = if let Some(pin) = &link.history {
                Some(
                    self.prepare_generator_memory_pin_wire(
                        &mut scratch,
                        link.source,
                        &node,
                        pin.clone(),
                    )
                    .ok_or("The carried history could not be prepared")?,
                )
            } else {
                None
            };
            Ok(std::iter::once(node)
                .chain(std::iter::once(data))
                .chain(memory)
                .collect())
        } else {
            if let Ok(program) = self.blank_model_program() {
                let model = self
                    .agents
                    .catalog(&program.id)
                    .iter()
                    .find(|model| model.is_default)
                    .or_else(|| self.agents.catalog(&program.id).first())
                    .map(|model| model.id.clone());
                self.configure_blank_model_agent(&mut node, &program, model);
            }
            Ok(vec![node])
        }
    }

    fn configure_blank_model_agent(
        &self,
        node: &mut slate_doc::Node,
        program: &atlas_ai::agent::AgentProvider,
        model: Option<String>,
    ) {
        let rect = node.rect;
        if let NodeKind::Image(image) = &mut node.kind {
            image.agent = PortalNode::unbound_agent("3D builder", &program.id).agent;
        }
        let binding = self.program_binding(Some(node.id));
        super::super::super::board_agent::bind_program(node, program, &binding);
        node.rect = rect;
        if let Some(agent) = slate_doc::agent_chat::agent_mut(node) {
            agent.view = atlas_agent::PortalView::Chat;
            agent.chat.train = false;
            agent.model = model;
        }
    }

    pub(super) fn pump_blank_models(&mut self, ctx: &egui::Context) {
        let pending = std::mem::take(&mut self.connectors.headless.blanks);
        for job in pending {
            if !self.tabs.iter().any(|tab| tab.id == job.tab) {
                job.cancel.cancel();
                continue;
            }
            if self.at_home || self.tab().id != job.tab {
                self.connectors.headless.blanks.push(job);
                continue;
            }
            match job.rx.try_recv() {
                Ok(Ok(file)) => {
                    let prepared = self.prepare_blank_model_nodes(&job, &file);
                    let result = prepared.and_then(|nodes| {
                        self.link_prepared_ready_media(
                            file,
                            nodes,
                            slate_doc::scene::CmdAuthor::Human,
                        )
                    });
                    match result {
                        Ok(node) => {
                            let _ = job.accepted.send(true);
                            if let Err(error) = self.connector_activate_headless(node) {
                                self.toast(error);
                            }
                            if let Some(link) = job.downstream {
                                self.board_sel.remove(&link.source);
                                self.board_sel.remove(&node);
                                if self.shape_properties.ids.contains(&link.source) {
                                    self.shape_properties.panel = None;
                                }
                            } else {
                                self.board_sel.clear();
                                self.board_sel.insert(node);
                            }
                        }
                        Err(error) => {
                            let _ = job.accepted.send(false);
                            self.toast(error);
                        }
                    }
                }
                Ok(Err(error)) => self.toast(error),
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    self.connectors.headless.blanks.push(job)
                }
                Err(_) => self.toast("Empty model preparation stopped before completion"),
            }
        }
        if !self.connectors.headless.blanks.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    pub(super) fn bind_model_agent(&mut self, node: NodeId) -> Result<(), String> {
        let before = self
            .doc()
            .scene
            .node(node)
            .cloned()
            .ok_or("Model node is missing")?;
        if slate_doc::agent_chat::agent(&before).is_some() {
            return Ok(());
        }
        // Native headless activation does not depend on an AI provider.
        // Unbound graph metadata retains native output history without a
        // fabricated provider or a full Agent presentation.
        let after = if let Ok(program) = self.blank_model_program() {
            let model = self
                .agents
                .catalog(&program.id)
                .iter()
                .find(|model| model.is_default)
                .or_else(|| self.agents.catalog(&program.id).first())
                .map(|model| model.id.clone());
            let mut after = before.clone();
            self.configure_blank_model_agent(&mut after, &program, model);
            after
        } else {
            self.graph_settings_node(node)?
        };
        if !self.fold_into_last_step(&after) {
            self.patch_nodes(&[node], |value| *value = after.clone());
        }
        Ok(())
    }
}

// One test-thread-owned worker reply lets root dispatch exercise the real job
// owner without starting a Rhino process; production still uses the same worker.
#[cfg(test)]
type EmptyPreparation = (
    Receiver<Result<super::super::super::items::ReadyFile, String>>,
    Cancellation,
    crossbeam_channel::Sender<bool>,
);
#[cfg(test)]
thread_local! {
    static EMPTY_PREPARATION_FOR_TEST: std::cell::RefCell<Option<EmptyPreparation>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
mod dispatch_tests;
