//! Journaled creation for explicitly scoped portal targets.
use super::*;
use slate_doc::scene::{NodeKind, PortalNode, WorldRect};
use std::path::PathBuf;

pub(super) fn recognizes(command: &str) -> bool {
    command == "board.objects.create_portal"
}

pub(super) fn prepare(
    app: &SlateApp,
    command: &str,
    ids: &[NodeId],
    args: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    if !recognizes(command) || !ids.is_empty() {
        return Err("Create one portal without existing targets".into());
    }
    SlateApp::validate_targeted_arguments(command, args)?;
    let kind = args["kind"]
        .as_str()
        .ok_or("Portal kind must be atlas or web")?;
    let source = args["source"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 4096)
        .ok_or("Portal source must be an explicit bounded locator")?;
    let rect = WorldRect::new(
        prepare::number(args, "x")?,
        prepare::number(args, "y")?,
        prepare::dimension(args, "width")?,
        prepare::dimension(args, "height")?,
    );
    if [rect.x, rect.y, rect.w, rect.h]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1.0e9)
    {
        return Err("Portal geometry is outside the board bounds".into());
    }
    let node = match kind {
        "application" => {
            let reference = slate_doc::application_surface::ApplicationSurface::parse(source)
                .ok_or("Invalid application surface locator")?;
            if app.model_node_info(reference.parent).is_none() {
                return Err("Application surface needs an existing model parent".into());
            }
            let mut scratch = app.doc().scene.node_preparation();
            scratch.build_node(rect, NodeKind::Portal(reference.portal(&reference.surface)))
        }
        "atlas" => {
            let path = PathBuf::from(source);
            let portal = app.bound_atlas_portal(&path, Vec::new());
            let mut scratch = app.doc().scene.node_preparation();
            scratch.build_node(rect, NodeKind::Portal(portal))
        }
        "web" => {
            if let Some(url) = super::super::super::board_web::web_url_text(source) {
                let mut scratch = app.doc().scene.node_preparation();
                scratch.build_node(
                    rect,
                    NodeKind::Portal(PortalNode::bound_web("Web portal", url)),
                )
            } else {
                app.build_bound_web(rect, &PathBuf::from(source))
            }
        }
        _ => return Err("Portal kind must be atlas or web".into()),
    };
    let id = node.id;
    Ok((
        vec![SceneCmd::Add {
            index: app.doc().scene.nodes.len(),
            node,
        }],
        vec![id],
    ))
}
