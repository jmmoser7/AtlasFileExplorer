//! Save As changes locator spelling, never the linked source's identity.
use super::*;
use crate::scene::{Node, NodeKind, SceneCmd, WebRefusal, WebSourceKind};
#[cfg(test)]
mod tests;

/// Rebase only linked node sources. History stores and agent sessions remain
/// external links; this function performs no I/O and never collects their bytes.
pub fn rebase_linked_scene(scene: &mut scene::Scene, from: &Path, to: &Path) -> bool {
    if from.parent() == to.parent() {
        return false;
    }
    let commands = scene
        .nodes
        .iter()
        .filter_map(|before| {
            let mut after = before.clone();
            rebase_node(&mut after, from, Some(to));
            (before != &after).then(|| SceneCmd::Patch {
                before: Box::new(before.clone()),
                after: Box::new(after),
            })
        })
        .collect::<Vec<_>>();
    !commands.is_empty() && scene.apply_all(&commands)
}

/// The saved workbook's new base also applies to every undo/redo snapshot.
pub(crate) fn rebase_linked_command(command: &mut SceneCmd, from: &Path, to: &Path) {
    match command {
        SceneCmd::Presentations { .. } => {}
        SceneCmd::Add { node, .. }
        | SceneCmd::Remove { node, .. }
        | SceneCmd::LayerNodeAdd { node, .. }
        | SceneCmd::LayerNodeRemove { node, .. } => rebase_node(node, from, Some(to)),
        SceneCmd::Patch { before, after } | SceneCmd::LayerNodePatch { before, after, .. } => {
            rebase_node(before, from, Some(to));
            rebase_node(after, from, Some(to));
        }
    }
}

/// Preserve linked identities when recovery lives outside the workbook folder.
/// Only locator strings are examined: cloud and network content is never read.
pub fn absolutize_document(doc: &mut SlateDoc, workbook: Option<&Path>) -> std::io::Result<()> {
    let from = match workbook {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => std::env::current_dir()?.join(path),
        None => std::env::current_dir()?.join("Untitled.slate"),
    };
    for item in &mut doc.items {
        item.path = scene::resolve_source(Some(&from), &item.path.to_string_lossy());
    }
    if let Some(root) = &mut doc.lens_root {
        *root = scene::resolve_source(Some(&from), &root.to_string_lossy());
    }
    for node in &mut doc.scene.nodes {
        rebase_node(node, &from, None);
    }
    doc.mark_item_paths_changed();
    Ok(())
}

fn rebase_node(node: &mut Node, from: &Path, to: Option<&Path>) {
    visit_linked_sources(node, &mut |locator| rebase_locator(locator, from, to));
}

/// The one recursive owner of linked source strings (no filesystem reads).
pub(super) fn visit_linked_sources(node: &mut Node, visit: &mut impl FnMut(&mut String)) {
    if let NodeKind::Portal(portal) = &mut node.kind {
        if let Some(source) = &mut portal.source {
            visit(&mut source.locator);
        }
    }
    if let Some(artifact) = scene::artifact_mut(node) {
        for source in [&mut artifact.history_root, &mut artifact.workspace]
            .into_iter()
            .flatten()
        {
            visit(&mut source.locator);
        }
    }
    if let Some(agent) = crate::agent_chat::agent_mut(node) {
        if let Some(bundle) = &mut agent.bundle {
            visit(&mut bundle.locator);
        }
        if let Some(seed) = &mut agent.seed {
            visit(&mut seed.source);
        }
        for source in &mut agent.generator.archived_history {
            visit(&mut source.locator);
        }
        if let Some(text) = &mut agent.generator.shown_text {
            visit(&mut text.source.locator);
        }
        if let Some(output) = &mut agent.generator.owned_text_output {
            visit(&mut output.locator);
        }
        for review in &mut agent.generator.text_reviews {
            visit(&mut review.output.source.locator);
        }
        for output in &mut agent.generator.model_outputs {
            visit(&mut output.source.locator);
        }
    }
    let answers = match &mut node.kind {
        NodeKind::Text(text) => Some(&mut text.answers),
        NodeKind::Image(image) => Some(&mut image.answers),
        _ => None,
    };
    if let Some(answers) = answers {
        for answer in answers {
            if let Some(origin) = &mut answer.origin {
                visit(&mut origin.locator);
            }
        }
    }
    if let NodeKind::Image(image) = &mut node.kind {
        for layer in image.paint_layers.iter_mut().chain(
            image
                .result_layers
                .iter_mut()
                .flat_map(|result| &mut result.layers),
        ) {
            for child in &mut layer.nodes {
                visit_linked_sources(child, visit);
            }
        }
    }
}

fn rebase_locator(locator: &mut String, from: &Path, to: Option<&Path>) {
    if locator.is_empty() || Path::new(locator).is_absolute() {
        return;
    }
    // Reuse the locator owner's scheme rules rather than parsing URLs here.
    if matches!(
        scene::classify_web_locator(locator, false),
        Ok(WebSourceKind::Remote)
            | Err(WebRefusal::DangerousScheme(_) | WebRefusal::UnsupportedScheme(_))
    ) {
        return;
    }
    let resolved = scene::resolve_source(Some(from), locator);
    *locator = scene::source_locator(to, &resolved);
}
