//! Registered Images history actions share the human SceneCmd owners.
use super::*;

impl SlateApp {
    pub(crate) fn finish_image_history_action(&mut self, action: Option<&(NodeId, String)>) {
        if let Some((source, action)) = action {
            if matches!(action.as_str(), "refresh" | "reconnect") {
                self.sync_pending_generator_preserve(*source, true);
            }
        }
    }
}

pub(super) fn prepare(
    app: &SlateApp,
    ids: &[NodeId],
    args: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>, Option<NodeId>), String> {
    if ids.len() != 1 {
        return Err("Choose exactly one Image history target".into());
    }
    prepare::validate_keys(args, schema::arguments("board.image.history")?)?;
    let action = args["action"]
        .as_str()
        .ok_or("Choose refresh, reconnect, downstream or cut")?;
    if !matches!(action, "refresh" | "reconnect" | "downstream" | "cut") {
        return Err("Choose refresh, reconnect, downstream or cut".into());
    }
    let id = ids[0];
    let dependencies = app.image_history_dependencies(id, action)?;
    let point = match (args.get("x"), args.get("y")) {
        (None, None) => None,
        (Some(_), Some(_)) if action == "downstream" => Some(eframe::egui::pos2(
            prepare::number(args, "x")?,
            prepare::number(args, "y")?,
        )),
        _ => return Err("Only downstream accepts a complete explicit world x/y point".into()),
    };
    let (commands, cut) = match action {
        "refresh" | "reconnect" => (app.prepare_refresh_image_history(id)?, None),
        "downstream" => (app.prepare_blank_image_child(id, point)?.0, None),
        "cut" => (app.prepare_generator_history_cut(id)?, Some(id)),
        _ => unreachable!(),
    };
    Ok((commands, dependencies, cut))
}
