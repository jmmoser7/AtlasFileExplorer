//! Make a separate, reversible cleanup copy of the Rhino validation workbook.
//! Uses the ordinary document encoder and authored scene journal; never edits
//! an open workbook or a source model. The input checkpoint remains untouched.
use slate_doc::scene::CmdAuthor;
use slate_doc::{NodeId, NodeKind, SceneCmd, SceneJournal, SlateDoc, WireRouting, WorldRect};
use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: clean_validation_board CHECKPOINT.slate NEW-CLEAN.slate".into());
    }
    let input = std::fs::canonicalize(PathBuf::from(&args[0]))?;
    let output_arg = PathBuf::from(&args[1]);
    let parent = std::fs::canonicalize(output_arg.parent().ok_or("output needs a parent")?)?;
    let output = parent.join(output_arg.file_name().ok_or("output needs a file name")?);
    let receipt = output.with_extension("cleanup.json");
    if input.parent() != Some(parent.as_path()) || output.exists() || receipt.exists() {
        return Err("output must be a new file beside the preserved checkpoint".into());
    }
    let original_bytes = std::fs::read(&input)?;
    let mut doc = SlateDoc::load_from(&input)?;
    let original_scene = serde_json::to_value(&doc.scene)?;
    let keep = [1, 2, 3, 4, 5, 7, 8, 10, 11, 12];
    if keep.iter().any(|id| doc.scene.node(NodeId(*id)).is_none()) {
        return Err("checkpoint is missing an expected source, train, or wire".into());
    }
    let mut journal = SceneJournal::default();
    let author = CmdAuthor::Agent("Rhino workspace cleanup".into());
    let mut groups = Vec::new();
    let remove: Vec<_> = doc
        .scene
        .nodes
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, node)| !keep.contains(&node.id.0))
        .map(|(index, node)| SceneCmd::Remove {
            index,
            node: node.clone(),
        })
        .collect();
    if remove.is_empty() || !journal.commit_as(&mut doc.scene, remove.clone(), author.clone()) {
        return Err("cleanup removal journal group was rejected".into());
    }
    groups.push(remove);
    let layout = [
        (1, WorldRect::new(0.0, 0.0, 320.0, 240.0)),
        (2, WorldRect::new(380.0, 0.0, 1000.0, 760.0)),
        (3, WorldRect::new(0.0, 280.0, 320.0, 240.0)),
        (4, WorldRect::new(0.0, 560.0, 320.0, 226.0)),
        (5, WorldRect::new(1440.0, 0.0, 450.0, 260.0)),
        (7, WorldRect::new(1440.0, 300.0, 450.0, 460.0)),
    ];
    let patches: Vec<_> = layout
        .iter()
        .map(|(id, rect)| {
            let before = doc.scene.node(NodeId(*id)).unwrap().clone();
            let mut after = before.clone();
            after.rect = *rect;
            after.hidden = false;
            if let Some(agent) = slate_doc::agent_chat::agent_mut(&mut after) {
                agent.chat.size = Some([rect.w, rect.h]);
            }
            SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            }
        })
        .collect();
    if !journal.commit_as(&mut doc.scene, patches.clone(), author.clone()) {
        return Err("cleanup layout journal group was rejected".into());
    }
    groups.push(patches);
    let wires: Vec<_> = doc
        .scene
        .nodes
        .iter()
        .filter_map(|before| {
            let NodeKind::Connector(conn) = &before.kind else {
                return None;
            };
            let mut after = before.clone();
            if let NodeKind::Connector(conn) = &mut after.kind {
                conn.routing = Some(WireRouting::Orthogonal);
            }
            after.rect = slate_doc::connector_route_in_scene(
                &doc.scene,
                Some(before.id),
                &conn.a,
                &conn.b,
                WireRouting::Orthogonal,
            )?
            .aabb();
            Some(SceneCmd::Patch {
                before: Box::new(before.clone()),
                after: Box::new(after),
            })
        })
        .collect();
    if wires.len() != 4 || !journal.commit_as(&mut doc.scene, wires.clone(), author) {
        return Err("cleanup wire routing journal group was rejected".into());
    }
    groups.push(wires);
    let cleaned_scene = serde_json::to_value(&doc.scene)?;
    for _ in 0..groups.len() {
        if !journal.undo(&mut doc.scene) {
            return Err("cleanup undo failed".into());
        }
    }
    if serde_json::to_value(&doc.scene)? != original_scene {
        return Err("undo changed checkpoint data".into());
    }
    for _ in 0..groups.len() {
        if !journal.redo(&mut doc.scene) {
            return Err("cleanup redo failed".into());
        }
    }
    if serde_json::to_value(&doc.scene)? != cleaned_scene {
        return Err("redo changed cleaned data".into());
    }
    doc.name = "Rhino workspace — clean".into();
    doc.view.cam_x = 945.0;
    doc.view.cam_y = 393.0;
    doc.view.zoom = 0.7;
    doc.save_to(&output)?;
    let loaded = SlateDoc::load_from(&output)?;
    if serde_json::to_value(&loaded.scene)? != cleaned_scene
        || std::fs::read(&input)? != original_bytes
    {
        return Err("saved cleanup or preserved checkpoint did not roundtrip".into());
    }
    std::fs::write(
        &receipt,
        serde_json::to_vec_pretty(&serde_json::json!({
            "input": input, "output": output, "author": "Rhino workspace cleanup",
            "kept_node_ids": keep, "groups": groups,
            "undo_redo_verified": true, "checkpoint_bytes_unchanged": true,
            "note": "Separate authored copy; source models and source chat history retained."
        }))?,
    )?;
    println!(
        "Created {} with {} nodes; preserved checkpoint; undo/redo and reload verified.",
        output.display(),
        loaded.scene.nodes.len()
    );
    Ok(())
}
