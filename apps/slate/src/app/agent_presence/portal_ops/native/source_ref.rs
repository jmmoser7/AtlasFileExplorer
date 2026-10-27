//! Secondary native inputs stay inside both the installed root and the explicit wire grant.
use super::*;
use std::path::{Component, Path};

pub(super) struct Reference {
    wire: slate_doc::NodeId,
    node: slate_doc::NodeId,
    path: PathBuf,
    canonical: PathBuf,
}
impl Reference {
    pub(super) fn prepare(
        app: &SlateApp,
        actor: &Collaborator,
        model: slate_doc::NodeId,
        installed_root: &Path,
        parameters: &Value,
    ) -> Result<Option<Self>, String> {
        let Some(relative) = parameters.get("source_relative") else {
            return Ok(None);
        };
        let relative = relative
            .as_str()
            .ok_or("Native source_relative must be text")?;
        let root = installed_root
            .canonicalize()
            .map_err(|_| "The installed native source root is unavailable")?;
        let canonical = resolve_relative(&root, relative)?;
        let grant = actor
            .grant
            .as_ref()
            .ok_or("Native input grant was revoked")?;
        let (wire, node, path) = app.native_explicit_inputs(&actor.profile.id, model)
            .into_iter().find(|(_, node, path)| {
                grant.allows_node(&node.0.to_string())
                    && !atlas_core::cloud::is_dehydrated(path)
                    && path.canonicalize().is_ok_and(|path| path == canonical)
            }).ok_or("Wire this native source file into the chat or target model and grant input access before reading it")?;
        Ok(Some(Self {
            wire,
            node,
            path,
            canonical,
        }))
    }

    pub(super) fn validate(
        &self,
        app: &SlateApp,
        actor: &Collaborator,
        model: slate_doc::NodeId,
    ) -> Result<(), String> {
        let allowed = actor
            .grant
            .as_ref()
            .is_some_and(|grant| grant.allows_node(&self.node.0.to_string()));
        let connected = app
            .native_explicit_inputs(&actor.profile.id, model)
            .into_iter()
            .any(|(wire, node, path)| wire == self.wire && node == self.node && path == self.path);
        if !allowed
            || !connected
            || atlas_core::cloud::is_dehydrated(&self.path)
            || self
                .path
                .canonicalize()
                .ok()
                .is_none_or(|path| path != self.canonical)
        {
            return Err("The native secondary input wire, path, or read grant changed".into());
        }
        Ok(())
    }
}

fn resolve_relative(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Native source_relative must stay under the installed source root".into());
    }
    let path = root.join(path);
    if atlas_core::cloud::is_dehydrated(&path) {
        return Err("Make the wired native source available locally first".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| "The wired native source is unavailable")?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        return Err("Native source_relative left the installed source root".into());
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{ConnectorEnd, ImageNode, NodeKind, Side, WorldRect};

    #[test]
    fn secondary_native_input_requires_explicit_readable_wire_and_revalidates_it() {
        let (mut h, model, _, who) =
            super::super::tests::setup(atlas_connector::SourceMode::IsolatedDraft, true);
        let path = h.base.join("layers.csv");
        std::fs::write(&path, b"path,red,green,blue\nTest,20,30,40\n").unwrap();
        let item = h
            .app
            .doc_mut()
            .add_item(path.clone(), "layers.csv", 30, 0, "csv-fixture");
        let node = h.app.doc().scene.node_preparation().build_node(
            WorldRect::new(0.0, 0.0, 80.0, 60.0),
            NodeKind::Image(ImageNode::new(item)),
        );
        let reference = h.app.add_nodes(vec![node])[0];
        // No implicit root access, even when the file is placed on the board.
        let params = json!({"source_relative":"layers.csv"});
        let index = h
            .app
            .presence
            .agents
            .iter()
            .position(|actor| actor.profile.id == who)
            .unwrap();
        let mut actor = h.app.presence.agents.remove(index);
        assert!(Reference::prepare(&h.app, &actor, model, &h.base, &params).is_err());
        let wire = h.app.build_connector_with(
            ConnectorEnd::Anchored {
                node: reference,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: model,
                side: Side::Left,
                t: 0.5,
            },
            &[],
        );
        let wire = h.app.add_nodes(vec![wire])[0];
        let grant = actor.grant.as_mut().unwrap();
        grant.readable_board = false;
        grant.readable_nodes = vec![model.0.to_string()];
        assert!(Reference::prepare(&h.app, &actor, model, &h.base, &params).is_err());
        actor
            .grant
            .as_mut()
            .unwrap()
            .readable_nodes
            .push(reference.0.to_string());
        let captured = Reference::prepare(&h.app, &actor, model, &h.base, &params)
            .unwrap()
            .unwrap();
        assert!(captured.validate(&h.app, &actor, model).is_ok());
        h.app.delete_board_nodes_now(&[wire]);
        assert!(captured.validate(&h.app, &actor, model).is_err());
        assert!(Reference::prepare(
            &h.app,
            &actor,
            model,
            &h.base,
            &json!({"source_relative":"../outside.csv"})
        )
        .is_err());
        assert!(
            Reference::prepare(&h.app, &actor, model, &h.base, &json!({"csv":"inline"}))
                .unwrap()
                .is_none()
        );
        h.app.presence.agents.insert(index, actor);
    }
}
