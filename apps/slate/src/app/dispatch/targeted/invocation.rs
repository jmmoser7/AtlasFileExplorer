//! Deferred provider jobs reuse the generator owner after witnessed contact.
use super::*;
use slate_doc::scene::NodeKind;

#[derive(Debug, Clone)]
pub(super) struct DeferredInvocation {
    target: NodeId,
    prompt: Option<String>,
    inputs: atlas_agent::InputSnapshot,
    requires_linked_inputs: bool,
}

impl DeferredInvocation {
    pub(super) fn target(&self) -> NodeId {
        self.target
    }
    pub(super) fn requires_linked_inputs(&self) -> bool {
        self.requires_linked_inputs
    }
}

pub(super) fn prepare(
    app: &SlateApp,
    ids: &[NodeId],
    args: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>, DeferredInvocation), String> {
    prepare::validate_keys(args, schema::arguments("portal.agent.run")?)?;
    if ids.len() != 1 {
        return Err("A provider run requires one explicit image generator target".into());
    }
    let target = ids[0];
    if !slate_doc::agent_inputs::is_image_generator(&app.doc().scene, target)
        || app.is_text_block(target)
    {
        return Err("This targeted run supports image generators; use scoped conversation messaging for chats".into());
    }
    let prompt = args
        .get("prompt")
        .map(|value| {
            let prompt = value.as_str().ok_or("Prompt must be text")?;
            if prompt.trim().is_empty() || prompt.chars().count() > 16_384 {
                return Err("Prompt must contain 1–16384 characters");
            }
            Ok(prompt.to_owned())
        })
        .transpose()
        .map_err(str::to_owned)?;
    let inputs = app.generator_input_refs(target, true)?;
    // Image/depth locators and linked media/card sources are byte-bearing.
    // Authored text, frame titles, and shape text remain board-only context.
    let requires_linked_inputs = inputs.context.iter().chain(&inputs.wired).any(|input| {
        !input.images.is_empty()
            || input.depth.is_some()
            || !input.outputs.is_empty()
            || app
                .doc()
                .scene
                .node(NodeId(input.node))
                .is_some_and(|node| matches!(node.kind, NodeKind::Image(_) | NodeKind::Portal(_)))
    });
    let mut targets = vec![target];
    for input in inputs.context.iter().chain(&inputs.wired) {
        let id = NodeId(input.node);
        if app.doc().scene.node(id).is_some() && !targets.contains(&id) {
            targets.push(id);
        }
    }
    if targets.len() > 128 {
        return Err("This generator exceeds the scoped input limit".into());
    }
    let before = app.doc().scene.node(target).ok_or("Generator is missing")?;
    let mut after = before.clone();
    if let NodeKind::Image(image) = &mut after.kind {
        if !image.paint_layers.is_empty() {
            if let Some(result) = app.image_result_id(target) {
                slate_doc::image_paint::seed_result_from_legacy(image, result);
            }
        }
        image.item = slate_doc::ItemId::NONE;
    }
    let cmds = (before != &after)
        .then(|| SceneCmd::Patch {
            before: Box::new(before.clone()),
            after: Box::new(after),
        })
        .into_iter()
        .collect();
    Ok((
        cmds,
        targets,
        DeferredInvocation {
            target,
            prompt,
            inputs,
            requires_linked_inputs,
        },
    ))
}

impl SlateApp {
    pub(super) fn finish_generator_invocation(
        &mut self,
        invocation: DeferredInvocation,
        cmds: Vec<SceneCmd>,
        author: slate_doc::scene::CmdAuthor,
    ) -> Result<String, String> {
        if self.generator_input_refs(invocation.target, true)? != invocation.inputs {
            return Err("Generator inputs changed while the action was prepared".into());
        }
        let request = self.generation_request_with_prompt(
            invocation.target,
            false,
            invocation.prompt.as_deref(),
        )?;
        let request_id = request.id.clone();
        self.check_generation_queue(invocation.target)?;
        let inputs = self.begin_agent_input_preparation(request.inputs.clone())?;
        if !cmds.is_empty() && !self.commit_scene_as(cmds, author.clone()) {
            return Err("The journal refused the generator preparation".into());
        }
        if !self.enqueue_prepared_generation_as(invocation.target, request, inputs, author.clone())
        {
            return Err("The generator could not queue this request".into());
        }
        let history_author = match author {
            slate_doc::scene::CmdAuthor::Agent(name) => atlas_commands::CmdAuthor::Agent(name),
            slate_doc::scene::CmdAuthor::Human => atlas_commands::CmdAuthor::Human,
        };
        self.push_history_as(
            atlas_commands::CommandId("portal.agent.run"),
            Some(format!("object {} · queued", invocation.target.0)),
            history_author,
        );
        Ok(format!("queued:{request_id}"))
    }
}
