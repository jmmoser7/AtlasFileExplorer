//! Typed argument validation and translation into ordinary scene commands.
use super::*;
use slate_doc::scene::{ConnectorEnd, NodeKind, Side, WorldRect};
use std::collections::HashSet;

const MAX_TARGETS: usize = 128;
const WORLD_LIMIT: f32 = 1.0e9;

pub(super) fn crop(
    app: &SlateApp,
    ids: &[NodeId],
    args: &Value,
) -> Result<
    (
        Vec<SceneCmd>,
        Vec<NodeId>,
        crate::app::board_crop::CropGesture,
    ),
    String,
> {
    use crate::app::board_crop::{apply_gesture, CropGesture};
    validate_keys(args, super::schema::arguments("board.objects.crop")?)?;
    let object = args.as_object().ok_or("Arguments must be an object")?;
    let gesture = match args.get("mode").and_then(Value::as_str) {
        Some("handle") if object.len() == 4 => CropGesture::Handle {
            handle: args
                .get("handle")
                .and_then(Value::as_u64)
                .filter(|handle| *handle < 8)
                .ok_or("Crop handle must be an integer 0–7")? as u8,
            world: (number(args, "x")?, number(args, "y")?),
        },
        Some("pan") if object.len() == 3 => CropGesture::Pan {
            delta_world: (number(args, "dx")?, number(args, "dy")?),
        },
        _ => {
            return Err(
                "Use exactly mode/handle/x/y or mode/dx/dy; coordinates are world units".into(),
            )
        }
    };
    if ids.len() != 1 || !app.croppable_image(ids[0]) {
        return Err("Crop requires one existing croppable image".into());
    }
    let before = app
        .doc()
        .scene
        .node(ids[0])
        .ok_or("Crop target is missing")?;
    let after =
        apply_gesture(before, gesture).ok_or("The crop gesture cannot apply to this image")?;
    if before == &after {
        return Err("The crop gesture would make no changes".into());
    }
    let cmd = patch(&app.doc().scene, ids[0], |node| *node = after)?;
    Ok((vec![cmd], ids.to_vec(), gesture))
}

pub(super) fn target_ids(scene: &Scene, values: &[String]) -> Result<Vec<NodeId>, String> {
    if values.len() > MAX_TARGETS {
        return Err("At most 128 targets are allowed per action".into());
    }
    let mut ids = Vec::new();
    for value in values {
        let id = value
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .map(NodeId)
            .ok_or("Targets must be positive stable object IDs, never file paths")?;
        let node = scene
            .node(id)
            .ok_or_else(|| format!("Object {} is missing", id.0))?;
        if node.locked {
            return Err(format!("Object {} is locked", id.0));
        }
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

pub(super) fn commands(
    app: &SlateApp,
    command: &str,
    ids: &[NodeId],
    args: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    if app.tab().read_only {
        return Err("This workbook is read-only".into());
    }
    let scene = &app.doc().scene;
    let allowed = super::schema::arguments(command)?;
    validate_keys(args, allowed)?;
    if super::creation::recognizes(command) {
        return super::creation::commands(app, command, ids, args);
    }
    match command {
        "board.objects.move" => {
            require_targets(ids)?;
            let mut all = ids.to_vec();
            for id in ids {
                if scene.node(*id).is_some_and(Node::is_frame) {
                    for member in scene.members_of(*id) {
                        if !all.contains(&member) {
                            all.push(member);
                        }
                    }
                }
            }
            if all.len() > MAX_TARGETS {
                return Err(
                    "Split this frame move into smaller actions (128 objects maximum)".into(),
                );
            }
            let (dx, dy) = if args.get("x").is_some() || args.get("y").is_some() {
                if ids.len() != 1 || args.get("dx").is_some() || args.get("dy").is_some() {
                    return Err("Absolute placement requires one target and x/y only".into());
                }
                let node = scene.node(ids[0]).ok_or("Target is missing")?;
                (
                    number(args, "x")? - node.rect.x,
                    number(args, "y")? - node.rect.y,
                )
            } else {
                (number(args, "dx")?, number(args, "dy")?)
            };
            let cmds = all
                .iter()
                .map(|id| patch(scene, *id, |node| node.rect = node.rect.translated(dx, dy)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((cmds, all))
        }
        "board.objects.resize" => {
            require_targets(ids)?;
            let width = dimension(args, "width")?;
            let height = dimension(args, "height")?;
            let x = optional_number(args, "x")?;
            let y = optional_number(args, "y")?;
            let cmds = ids
                .iter()
                .map(|id| {
                    patch(scene, *id, |node| {
                        node.rect.w = width;
                        node.rect.h = height;
                        if let Some(x) = x {
                            node.rect.x = x;
                        }
                        if let Some(y) = y {
                            node.rect.y = y;
                        }
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((cmds, ids.to_vec()))
        }
        "board.objects.order" => order(scene, ids, args),
        "board.objects.create_frame"
        | "board.objects.create_text"
        | "board.objects.create_shape" => {
            if !ids.is_empty() {
                return Err("Creation commands have no existing targets".into());
            }
            let rect = WorldRect::new(
                number(args, "x")?,
                number(args, "y")?,
                dimension(args, "width")?,
                dimension(args, "height")?,
            );
            let recipe = if command == "board.objects.create_text" {
                let text = bounded_text(args, "text", 16_384)?;
                let shape: slate_kit::ShapeRecipe = serde_json::from_value(serde_json::json!({
                    "node": "text", "create_style": "pinned", "text": {"text": text}
                }))
                .map_err(|e| e.to_string())?;
                slate_kit::Recipe::Shape(shape)
            } else {
                let kit = if command == "board.objects.create_frame" {
                    "frame"
                } else {
                    match args.get("shape").and_then(Value::as_str).unwrap_or("rect") {
                        "rect" => "rect",
                        "ellipse" => "ellipse",
                        "polygon" => "polygon",
                        _ => return Err("Shape must be rect, ellipse, or polygon".into()),
                    }
                };
                app.kits
                    .recipe_for_id(kit)
                    .cloned()
                    .ok_or("The board kit recipe is unavailable")?
            };
            let mut nodes = app.prepare_recipe_nodes(&recipe, rect);
            if command == "board.objects.create_frame" {
                let title = bounded_text(args, "title", 256)?;
                if !title.is_empty() {
                    for node in &mut nodes {
                        if let NodeKind::Frame(frame) = &mut node.kind {
                            frame.title.clone_from(&title);
                        }
                    }
                }
            }
            let targets = nodes.iter().map(|node| node.id).collect();
            let cmds = nodes
                .into_iter()
                .enumerate()
                .map(|(offset, node)| SceneCmd::Add {
                    index: scene.nodes.len() + offset,
                    node,
                })
                .collect();
            Ok((cmds, targets))
        }
        "board.objects.wire" => {
            if ids.len() != 2 {
                return Err("A wire requires explicit source and target IDs".into());
            }
            let a = ConnectorEnd::Anchored {
                node: ids[0],
                side: side(args, "source_side", Side::Right)?,
                t: wire_t(args, "source_t")?,
            };
            let b = ConnectorEnd::Anchored {
                node: ids[1],
                side: side(args, "target_side", Side::Left)?,
                t: wire_t(args, "target_t")?,
            };
            let node = app.connector_template_bound(a, b, &[], None);
            let mut cmds = Vec::new();
            if let Some(cmd) = app.image_wire_target_command(&node) {
                cmds.push(cmd);
            }
            let mut targets = ids.to_vec();
            targets.push(node.id);
            cmds.push(SceneCmd::Add {
                index: scene.nodes.len(),
                node,
            });
            Ok((cmds, targets))
        }
        "board.objects.delete" => {
            require_targets(ids)?;
            let (cmds, removed, ended) = app.prepare_board_node_deletion(ids);
            let mut targets: HashSet<NodeId> = removed.into_iter().chain(ended).collect();
            targets.extend(cmds.iter().filter_map(|cmd| match cmd {
                SceneCmd::Patch { before, .. } => Some(before.id),
                _ => None,
            }));
            if targets.len() > MAX_TARGETS {
                return Err(
                    "Split this deletion into smaller actions (128 objects maximum)".into(),
                );
            }
            let mut targets: Vec<_> = targets.into_iter().collect();
            targets.sort_by_key(|id| id.0);
            Ok((cmds, targets))
        }
        _ => unreachable!(),
    }
}

fn patch(scene: &Scene, id: NodeId, edit: impl FnOnce(&mut Node)) -> Result<SceneCmd, String> {
    let before = scene.node(id).ok_or("Target is missing")?;
    if before.locked {
        return Err(format!("Object {} is locked", id.0));
    }
    let mut after = before.clone();
    edit(&mut after);
    for value in [after.rect.x, after.rect.y, after.rect.w, after.rect.h] {
        if !value.is_finite() || value.abs() > WORLD_LIMIT {
            return Err("Resulting geometry is outside the board bounds".into());
        }
    }
    Ok(SceneCmd::Patch {
        before: Box::new(before.clone()),
        after: Box::new(after),
    })
}

fn order(
    scene: &Scene,
    ids: &[NodeId],
    args: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    require_targets(ids)?;
    if args.get("order").is_some() {
        if args.get("z").is_some() {
            return Err("Choose deck order or board z-order".into());
        }
        let order = args
            .get("order")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or("Deck order must be a nonnegative integer")?;
        let mut cmds = Vec::new();
        for id in ids {
            if !scene.node(*id).is_some_and(Node::is_frame) {
                return Err("Deck order requires frame targets".into());
            }
            cmds.push(patch(scene, *id, |node| {
                if let NodeKind::Frame(frame) = &mut node.kind {
                    frame.order = order;
                }
            })?);
        }
        return Ok((cmds, ids.to_vec()));
    }
    if ids.len() != 1 {
        return Err("Board z-order requires one target".into());
    }
    let z = args
        .get("z")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .filter(|z| *z < scene.nodes.len())
        .ok_or("Z-order is outside the board")?;
    let id = ids[0];
    let node = scene.node(id).cloned().ok_or("Target is missing")?;
    let index = scene.index_of(id).ok_or("Target is missing")?;
    Ok((
        vec![
            SceneCmd::Remove {
                index,
                node: node.clone(),
            },
            SceneCmd::Add { index: z, node },
        ],
        ids.to_vec(),
    ))
}

pub(super) fn validate_keys(args: &Value, allowed: &[(&str, &str)]) -> Result<(), String> {
    if args.is_null() && allowed.is_empty() {
        return Ok(());
    }
    let object = args.as_object().ok_or("Arguments must be an object")?;
    if object
        .keys()
        .any(|key| !allowed.iter().any(|(name, _)| key == name))
    {
        return Err(
            "An unsupported argument was supplied; source filesystem paths are never accepted"
                .into(),
        );
    }
    Ok(())
}
fn require_targets(ids: &[NodeId]) -> Result<(), String> {
    if ids.is_empty() {
        Err("Explicit stable target IDs are required".into())
    } else {
        Ok(())
    }
}
pub(super) fn number(args: &Value, key: &str) -> Result<f32, String> {
    let value = args
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("{key} must be a finite number"))? as f32;
    if !value.is_finite() || value.abs() > WORLD_LIMIT {
        return Err(format!("{key} is outside the board bounds"));
    }
    Ok(value)
}
fn optional_number(args: &Value, key: &str) -> Result<Option<f32>, String> {
    args.get(key).map(|_| number(args, key)).transpose()
}
pub(super) fn dimension(args: &Value, key: &str) -> Result<f32, String> {
    let value = number(args, key)?;
    if value <= 0.0 {
        return Err(format!("{key} must be greater than zero"));
    }
    Ok(value)
}
pub(super) fn bounded_text(args: &Value, key: &str, limit: usize) -> Result<String, String> {
    let value = args
        .get(key)
        .map(|v| v.as_str().ok_or_else(|| format!("{key} must be text")))
        .transpose()?
        .unwrap_or("");
    if value.chars().count() > limit {
        return Err(format!("{key} is too long"));
    }
    Ok(value.to_owned())
}
fn side(args: &Value, key: &str, default: Side) -> Result<Side, String> {
    match args.get(key).and_then(Value::as_str) {
        None if args.get(key).is_none() => Ok(default),
        Some("top") => Ok(Side::Top),
        Some("right") => Ok(Side::Right),
        Some("bottom") => Ok(Side::Bottom),
        Some("left") => Ok(Side::Left),
        Some("start") => Ok(Side::Start),
        Some("mid") => Ok(Side::Mid),
        Some("end") => Ok(Side::End),
        _ => Err(format!("Invalid wire side {key}")),
    }
}
fn wire_t(args: &Value, key: &str) -> Result<f32, String> {
    let value = optional_number(args, key)?.unwrap_or(0.5);
    if !(0.0..=1.0).contains(&value) {
        return Err(format!("{key} must be between zero and one"));
    }
    Ok(value)
}
pub(super) fn removed_ids(cmds: &[SceneCmd]) -> Vec<NodeId> {
    cmds.iter().filter_map(|cmd| match cmd { SceneCmd::Remove { node, .. } if !cmds.iter().any(|other| matches!(other, SceneCmd::Add { node: added, .. } if added.id == node.id)) => Some(node.id), _ => None }).collect()
}
pub(super) fn added_wire_ids(cmds: &[SceneCmd]) -> Vec<NodeId> {
    cmds.iter()
        .filter_map(|cmd| match cmd {
            SceneCmd::Add { node, .. } if matches!(node.kind, NodeKind::Connector(_)) => {
                Some(node.id)
            }
            _ => None,
        })
        .collect()
}
