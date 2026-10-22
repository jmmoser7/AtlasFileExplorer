//! Preparation only: creation uses the same train, generator and linked-item
//! owners as direct manipulation. No imports, provider starts or new grants.
use super::*;
use eframe::egui;
use slate_doc::scene::{NodeKind, WorldRect};

pub(super) fn recognizes(command: &str) -> bool {
    matches!(
        command,
        "board.objects.create_train"
            | "board.objects.create_generator"
            | "board.objects.place_images"
    )
}

pub(super) fn commands(
    app: &SlateApp,
    command: &str,
    ids: &[NodeId],
    args: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    if command == "board.objects.place_images" {
        return place_images(app, ids, args);
    }
    if !ids.is_empty() {
        return Err("Create a train or generator without existing targets; wire inputs in a separate witnessed action".into());
    }
    let rect = WorldRect::new(
        prepare::number(args, "x")?,
        prepare::number(args, "y")?,
        prepare::dimension(args, "width")?,
        prepare::dimension(args, "height")?,
    );
    let train = command == "board.objects.create_train";
    let (aspect, count) = if train {
        (None, None)
    } else {
        image_options(args)?
    };
    let provider = match args.get("provider") {
        Some(value) => value
            .as_str()
            .ok_or("Provider must be an existing engine identifier")?,
        None => {
            if train {
                "codex"
            } else {
                "codex-image"
            }
        }
    };
    let allowed = if train {
        matches!(provider, "codex" | "cursor" | "local")
    } else {
        atlas_ai::agent::local_image_engine(provider)
    };
    if !allowed {
        return Err("Choose an existing chat harness or image engine for this kind of node".into());
    }
    if app.ai.config.workspace_dir.is_none() {
        return Err("Establish the human's AI workspace before creating agent resources".into());
    }
    let model = prepare::bounded_text(args, "model", 256)?;
    let title = prepare::bounded_text(args, "title", 256)?;
    let prompt = prepare::bounded_text(args, "prompt", 16_384)?;
    let binding = app.program_binding(None);
    let program = atlas_ai::agent::provider_by_id(provider);
    let mut scratch = app.doc().scene.node_preparation();
    let mut node = if train {
        SlateApp::prepare_agent_train_node(&mut scratch, rect, &program, &binding)
    } else {
        SlateApp::prepare_flow_media_node(
            &mut scratch,
            super::super::super::board_flow::SpawnKind::Image,
            rect,
            &program,
            &binding,
        )
    };
    if let Some(agent) = slate_doc::agent_chat::agent_mut(&mut node) {
        agent.model = (!model.is_empty()).then_some(model);
        agent.instruction = prompt;
        agent.live = false;
        if let Some(aspect) = aspect {
            agent.image.aspect = aspect;
        }
        if let Some(count) = count {
            agent.image.count = count;
        }
    }
    if !title.is_empty() {
        if let NodeKind::Portal(portal) = &mut node.kind {
            portal.title = title;
        }
    }
    additions(app, vec![node], Vec::new())
}

pub(super) fn image_options(
    args: &Value,
) -> Result<(Option<atlas_agent::Aspect>, Option<u8>), String> {
    let aspect = args
        .get("aspect")
        .map(|value| {
            serde_json::from_value::<atlas_agent::Aspect>(value.clone()).map_err(|_| {
                "Aspect must be source, square, landscape, portrait, or wide".to_owned()
            })
        })
        .transpose()?;
    let count = args
        .get("count")
        .map(|value| {
            value
                .as_u64()
                .filter(|count| (1..=8).contains(count))
                .map(|count| count as u8)
                .ok_or_else(|| "Image count must be an integer from 1 to 8".to_owned())
        })
        .transpose()?;
    Ok((aspect, count))
}

fn place_images(
    app: &SlateApp,
    ids: &[NodeId],
    args: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    if ids.is_empty() || ids.len() > 64 {
        return Err("Place 1–64 explicitly readable image nodes per action".into());
    }
    let x = prepare::number(args, "x")?;
    let y = prepare::number(args, "y")?;
    let mut items = Vec::new();
    let mut sizes = Vec::new();
    for id in ids {
        let node = app.doc().scene.node(*id).ok_or("Image source is missing")?;
        let NodeKind::Image(_) = &node.kind else {
            return Err(
                "Place images accepts linked image nodes or completed image generators".into(),
            );
        };
        // Results must already have been imported by the asynchronous image
        // output owner. A provider path is never accepted as an item locator.
        let item = app.registered_agent_image_item(*id).ok_or_else(|| {
            app.agent_image_import_problem(*id)
                .unwrap_or("Wait for the selected graphic to finish importing before placing it")
                .to_owned()
        })?;
        let linked = app
            .doc()
            .item(item)
            .ok_or("Wait for this graphic to finish importing before placing it")?;
        if slate_doc::media_kind(&linked.path) != slate_doc::MediaKind::Image {
            return Err("Only registered image items can be placed with this command".into());
        }
        items.push(item);
        sizes.push((node.rect.w, node.rect.h));
    }
    // Explicit coordinates denote the group centre, matching a human media
    // drop. Existing authored source geometry supplies aspect without I/O.
    let rects = super::super::super::board::grid_drop_rects(&sizes, egui::pos2(x, y));
    let nodes = app.prepare_item_placements(&items, &rects);
    // Frame tagging changes linked workbook metadata. This scene-only action
    // cannot silently mutate that separate journal, so refuse a missing tag
    // inheritance rather than produce a non-reversible side effect.
    let result = additions(app, nodes, ids.to_vec())?;
    check_placement_tags(app, &result.0)?;
    Ok(result)
}

pub(super) fn check_placement_tags(app: &SlateApp, cmds: &[SceneCmd]) -> Result<(), String> {
    for cmd in cmds {
        let SceneCmd::Add { node, .. } = cmd else {
            continue;
        };
        let NodeKind::Image(image) = &node.kind else {
            continue;
        };
        let (cx, cy) = node.rect.center();
        if let Some(frame) = app
            .doc()
            .scene
            .frame_at(cx, cy)
            .and_then(|id| app.doc().scene.node(id))
        {
            if let NodeKind::Frame(frame) = &frame.kind {
                let linked = app.doc().item(image.item).ok_or("Image item is missing")?;
                if frame
                    .assignments
                    .iter()
                    .any(|(group, tag)| linked.assignments.get(group) != Some(tag))
                {
                    return Err("Assign this frame's tags to the source images with the human tag controls before placing them here".into());
                }
            }
        }
    }
    Ok(())
}

fn additions(
    app: &SlateApp,
    nodes: Vec<Node>,
    mut targets: Vec<NodeId>,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    if nodes.iter().any(|node| {
        [node.rect.x, node.rect.y, node.rect.w, node.rect.h]
            .iter()
            .any(|value| !value.is_finite() || value.abs() > 1.0e9)
            || node.rect.w <= 0.0
            || node.rect.h <= 0.0
    }) {
        return Err("Created geometry is outside the board bounds".into());
    }
    targets.extend(nodes.iter().map(|node| node.id));
    Ok((
        nodes
            .into_iter()
            .enumerate()
            .map(|(offset, node)| SceneCmd::Add {
                index: app.doc().scene.nodes.len() + offset,
                node,
            })
            .collect(),
        targets,
    ))
}

pub(super) fn linked_items(
    doc: &slate_doc::SlateDoc,
    cmds: &[SceneCmd],
) -> Vec<slate_doc::SlateItem> {
    let mut items = Vec::new();
    for cmd in cmds {
        if let SceneCmd::Add { node, .. } = cmd {
            if let NodeKind::Image(image) = &node.kind {
                if let Some(item) = doc.item(image.item).filter(|item| {
                    !items
                        .iter()
                        .any(|old: &slate_doc::SlateItem| old.id == item.id)
                }) {
                    items.push(item.clone());
                }
            }
        }
    }
    items
}

#[cfg(test)]
mod tests;
