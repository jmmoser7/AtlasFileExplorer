//! One blank Images downstream creator for the real data drag and history action.
use super::*;

impl SlateApp {
    pub(crate) fn prepare_blank_image_child(
        &self,
        source: NodeId,
        at: Option<Pos2>,
    ) -> Result<(Vec<SceneCmd>, NodeId), String> {
        self.prepare_blank_image_child_positioned(source, at, false)
    }

    fn prepare_blank_image_child_positioned(
        &self,
        source: NodeId,
        at: Option<Pos2>,
        at_input: bool,
    ) -> Result<(Vec<SceneCmd>, NodeId), String> {
        let Some(parent) = self.doc().scene.node(source).cloned() else {
            return Err("The image source is missing, locked or unavailable".into());
        };
        if self.tab().read_only
            || parent.locked
            || self.generator_source_kind(source) != Some(PendingHost::Image)
        {
            return Err("The image source is missing, locked or unavailable".into());
        }
        // Capture before preparing any child nodes. An unloaded saved session
        // must not become a permanently empty immutable history pin.
        let pin = self.downstream_generator_history(source)?;
        let preserve = pin.is_some();
        let selected = slate_doc::agent_chat::agent(&parent)
            .filter(|a| !a.provider.is_empty())
            .map(|a| {
                (
                    a.provider.clone(),
                    a.model.clone().unwrap_or_default(),
                    a.model_parameters.clone(),
                )
            })
            .or_else(|| {
                self.pending_generator_choice(source)
                    .map(|(p, m, parameters)| (p.into(), m.into(), parameters.clone()))
            });
        let siblings = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| {
                slate_doc::agent_chat::agent(n)
                    .is_some_and(|a| a.generator.history_source == Some(source))
            })
            .count();
        let at = at.unwrap_or_else(|| {
            Pos2::new(
                parent.rect.x + parent.rect.w + 180.0,
                parent.rect.y + siblings as f32 * 620.0,
            )
        });
        if !at.x.is_finite() || !at.y.is_finite() {
            return Err("The image source is missing, locked or unavailable".into());
        }
        let rect = slate_doc::WorldRect::new(at.x, at.y, parent.rect.w, parent.rect.h);
        let binding = self.program_binding(None);
        let mut scratch = self.doc().scene.node_preparation();
        let mut child = if let Some((provider, model, parameters)) =
            selected.filter(|(p, _, _)| atlas_ai::agent::local_image_engine(p))
        {
            let program = atlas_ai::agent::AgentProvider {
                view: PortalView::Images,
                ..atlas_ai::agent::provider_by_id(&provider)
            };
            let mut node = Self::prepare_flow_media_node(
                &mut scratch,
                SpawnKind::Image,
                rect,
                &program,
                &binding,
            );
            let agent = slate_doc::agent_chat::agent_mut(&mut node).unwrap();
            agent.model = (!model.is_empty()).then_some(model);
            agent.model_parameters = parameters;
            let agent = agent.clone();
            node.kind = NodeKind::Image(slate_doc::scene::ImageNode::new(slate_doc::ItemId::NONE));
            if let NodeKind::Image(image) = &mut node.kind {
                image.agent = Some(Box::new(agent));
            }
            node
        } else {
            let mut image = slate_doc::scene::ImageNode::new(slate_doc::ItemId::NONE);
            image.agent = Some(Box::new(slate_doc::scene::AgentPortalRef::unbound(
                PortalView::Images,
            )));
            scratch.build_node(rect, NodeKind::Image(image))
        };
        let agent = slate_doc::agent_chat::agent_mut(&mut child).unwrap();
        agent.instruction.clear();
        agent.bundle = None;
        agent.seed = None;
        agent.chat = Default::default();
        agent.generator = Default::default();
        agent.image = Default::default();
        agent.generator.count = Some(1);
        agent.generator.preserve_history = Some(preserve);
        agent.generator.inherited_turn_count = Some(0);
        agent.generator.history_source = preserve.then_some(source);
        let child_id = child.id;
        let port = agent_inputs::GENERATOR_PORTS[0];
        if at_input {
            let site = self.wire_input_host(&child).anchor(Side::Left, port.t);
            child.rect.y -= site[1] - child.rect.y;
        }
        let data_template = self.connector_template_bound(
            ConnectorEnd::Anchored {
                node: source,
                side: Side::Right,
                t: OUTPUT_T,
            },
            ConnectorEnd::Anchored {
                node: child.id,
                side: Side::Left,
                t: port.t,
            },
            std::slice::from_ref(&child),
            Some(agent_inputs::WireBinding {
                input_b: true,
                kind: InputKind::Images,
                output: None,
                order: vec![],
                all_images: false,
                consumed: false,
                slot: Some("media".into()),
            }),
        );
        let mut data = scratch.build_node(data_template.rect, data_template.kind);
        if let NodeKind::Connector(conn) = &mut data.kind {
            // The Studio data edge follows the parent's selected image;
            // only the separate history pin is an immutable conversation copy.
            if let Some(binding) = &mut conn.binding {
                binding.output = None;
            }
        }
        let memory = pin.and_then(|pin| {
            self.prepare_generator_memory_pin_wire(&mut scratch, source, &child, pin)
        });
        let nodes = std::iter::once(child)
            .chain(std::iter::once(data))
            .chain(memory)
            .collect::<Vec<_>>();
        let base = self.doc().scene.nodes.len();
        let commands = nodes
            .into_iter()
            .enumerate()
            .map(|(i, node)| SceneCmd::Add {
                index: base + i,
                node,
            })
            .collect();
        Ok((commands, child_id))
    }

    pub(crate) fn spawn_blank_image_child(&mut self, source: NodeId, at: Option<Pos2>) -> bool {
        let (commands, child_id) =
            match self.prepare_blank_image_child_positioned(source, at, at.is_some()) {
                Ok(prepared) => prepared,
                Err(error) => {
                    self.toast(error);
                    return false;
                }
            };
        if !self.commit_scene(commands) {
            return false;
        }
        self.board_sel.remove(&source);
        self.board_sel.remove(&child_id);
        if self.shape_properties.ids.contains(&source) {
            self.shape_properties.panel = None;
        }
        true
    }
}
