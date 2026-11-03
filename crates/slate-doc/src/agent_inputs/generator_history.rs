//! Optional generator context travels on ordinary, explicitly typed wires.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratorMemoryPin {
    pub source_revision: String,
    pub turns: Vec<atlas_agent::AgentTurn>,
}

/// Endpoint admission reads identity metadata without materializing any turns.
/// A quoted execution snapshot does not itself grant conversation authority.
pub fn generator_memory_endpoint(binding: &WireBinding) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Header {
        source_revision: String,
    }
    let text = binding
        .output
        .as_deref()
        .ok_or("The inherited history snapshot is missing")?;
    serde_json::from_str::<Header>(text)
        .map(|header| header.source_revision)
        .map_err(|_| "The inherited history identity is unreadable".into())
}

/// Replace only the inherited prefix of the public image dialogue. A legacy
/// loaded transcript with an unknown prefix must not be guessed or deduplicated.
pub fn compose_image_history(
    inherited: &[atlas_agent::AgentTurn],
    visible: &[atlas_agent::AgentTurn],
    previous_prefix: Option<u32>,
) -> Result<(Vec<atlas_agent::AgentTurn>, u32), &'static str> {
    if inherited
        .iter()
        .chain(visible)
        .any(|turn| !matches!(turn.role.as_str(), "user" | "assistant"))
    {
        return Err("Only public user and assistant messages can be carried.");
    }
    if previous_prefix.is_none() && !inherited.is_empty() && !visible.is_empty() {
        return Err("The inherited boundary of this legacy conversation is unknown. Cut its history before reconnecting.");
    }
    let previous = previous_prefix.unwrap_or(0) as usize;
    let own = visible
        .get(previous..)
        .ok_or("Wait for the saved image conversation to load before sending again.")?;
    let prefix =
        u32::try_from(inherited.len()).map_err(|_| "This history is too large to carry.")?;
    let mut turns = inherited.to_vec();
    turns.extend_from_slice(own);
    Ok((turns, prefix))
}

/// Transcript manifests for inspection only. Readers must not replay these
/// into inference; active memory is resolved exclusively from live bindings.
pub fn archived_generator_history(node: &Node) -> &[SourceUri] {
    crate::agent_chat::agent(node)
        .map(|agent| agent.generator.archived_history.as_slice())
        .unwrap_or(&[])
}

/// Chat Train checkpoints are immutable rails and never generator memory cuts.
pub fn generator_memory_target(scene: &Scene, conn: &ConnectorNode) -> Option<NodeId> {
    let binding = conn
        .binding
        .as_ref()
        .filter(|b| b.kind == InputKind::Memory)?;
    let target = endpoint_node(binding.target_end(conn))?;
    let node = scene.node(target)?;
    crate::agent_chat::agent(node)?;
    (!crate::agent_chat::is_chat_host(node)).then_some(target)
}

/// A target with a downstream data or history consumer is an interior checkpoint.
pub fn generator_history_is_terminal(scene: &Scene, target: NodeId) -> bool {
    !scene.nodes.iter().any(|node| {
        let NodeKind::Connector(conn) = &node.kind else {
            return false;
        };
        conn.binding.as_ref().is_some_and(|binding| {
            !matches!(binding.kind, InputKind::Control)
                && endpoint_node(binding.source_end(conn)) == Some(target)
                && endpoint_node(binding.target_end(conn)).is_some()
        })
    })
}

pub fn generator_memory_snapshot(scene: &Scene, target: NodeId) -> Option<GeneratorMemoryPin> {
    generator_memory_snapshot_checked(scene, target)
        .ok()
        .flatten()
}

pub fn generator_memory_snapshot_checked(
    scene: &Scene,
    target: NodeId,
) -> Result<Option<GeneratorMemoryPin>, String> {
    let mut incoming = scene.nodes.iter().filter_map(|node| {
        let NodeKind::Connector(conn) = &node.kind else {
            return None;
        };
        let binding = conn.binding.as_ref()?;
        if generator_memory_target(scene, conn) != Some(target)
            || binding.slot.as_deref() != Some("generator_memory")
        {
            return None;
        }
        Some(binding)
    });
    let Some(binding) = incoming.next() else {
        return Ok(None);
    };
    if incoming.next().is_some() {
        return Err("Choose one inherited history connection before sending".into());
    }
    let text = binding
        .output
        .as_deref()
        .ok_or("The inherited history snapshot is missing")?;
    serde_json::from_str(text)
        .map(Some)
        .map_err(|_| "The inherited history snapshot is invalid".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_metadata_does_not_materialize_or_authorize_quoted_turns() {
        let mut binding = WireBinding {
            input_b: true,
            kind: InputKind::Memory,
            output: Some(
                r#"{"source_revision":"old-session","turns":{"not":"a public-turn array"}}"#.into(),
            ),
            order: vec![],
            all_images: false,
            consumed: false,
            slot: Some("generator_memory".into()),
        };
        assert_eq!(generator_memory_endpoint(&binding).unwrap(), "old-session");
        assert!(
            serde_json::from_str::<GeneratorMemoryPin>(binding.output.as_deref().unwrap()).is_err()
        );
        binding.output = Some(r#"{"turns":[]}"#.into());
        assert!(generator_memory_endpoint(&binding).is_err());
    }
    #[test]
    fn refresh_replaces_only_known_inherited_prefix_and_rejects_legacy_guessing() {
        let turn = |s: &str| atlas_agent::AgentTurn {
            role: "user".into(),
            text: s.into(),
            at: 1,
        };
        let visible = vec![turn("old parent"), turn("own first"), turn("own next")];
        let fresh = vec![turn("new parent one"), turn("new parent two")];
        let (history, prefix) = compose_image_history(&fresh, &visible, Some(1)).unwrap();
        assert_eq!(prefix, 2);
        assert_eq!(
            history.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(),
            ["new parent one", "new parent two", "own first", "own next"]
        );
        assert!(compose_image_history(&fresh, &visible, None).is_err());
        assert!(compose_image_history(&fresh, &[], Some(1)).is_err());
        let mut private = fresh.clone();
        private[0].role = "analysis".into();
        assert!(compose_image_history(&private, &visible, Some(1)).is_err());
        assert_eq!(visible[0].text, "old parent");
    }
    #[test]
    fn any_downstream_data_or_memory_connection_makes_a_checkpoint_interior() {
        let mut scene = Scene::default();
        for kind in [
            InputKind::Images,
            InputKind::Text,
            InputKind::Memory,
            InputKind::Artifact,
            InputKind::Table,
        ] {
            let node = scene.build_node(
                WorldRect::new(0.0, 0.0, 1.0, 1.0),
                NodeKind::Connector(ConnectorNode {
                    a: ConnectorEnd::Anchored {
                        node: NodeId(20),
                        side: Side::Right,
                        t: 0.5,
                    },
                    b: ConnectorEnd::Anchored {
                        node: NodeId(21),
                        side: Side::Left,
                        t: 0.5,
                    },
                    binding: Some(WireBinding {
                        input_b: true,
                        kind,
                        output: None,
                        order: vec![],
                        all_images: false,
                        consumed: false,
                        slot: None,
                    }),
                    stroke: Stroke::default(),
                    color: None,
                    routing: None,
                    arrow_a: false,
                    arrow_b: false,
                    label: None,
                    display: Default::default(),
                    crosstalk: None,
                }),
            );
            scene.nodes.push(node);
            assert!(!generator_history_is_terminal(&scene, NodeId(20)));
            assert!(generator_history_is_terminal(&scene, NodeId(21)));
            scene.nodes.clear();
        }
    }
}
