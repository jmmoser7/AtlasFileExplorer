//! Reject preview sources and expensive authored payloads before the production painter sees them.
use super::*;

const MAX_PATH_ELEMENTS: usize = 1_024;
const MAX_TEXT_BYTES: usize = 16 * 1024;

pub(super) fn validate_source(app: &SlateApp, node: &Node) -> Result<(), String> {
    if let NodeKind::Image(image) = &node.kind {
        let source = app.image_item_path(node.id);
        if source
            .as_deref()
            .is_some_and(atlas_core::secrets::is_machine_private)
        {
            return Err(
                "Machine-private source content is never available to board previews".into(),
            );
        }
        if image.agent.is_some() && image.item.is_none() {
            if let Some(path) = app.agent_shown_path(node.id) {
                if atlas_core::secrets::is_machine_private(&path) {
                    return Err(
                        "Machine-private generated content is never available to board previews"
                            .into(),
                    );
                }
            }
        }
    }
    Ok(())
}

pub(super) fn validate_node(node: &Node) -> Result<(), String> {
    let mut path_elements = node.clip.as_ref().map_or(0, path_elements_count);
    if let NodeKind::Shape(shape) = &node.kind {
        if let Some(path) = shape.path.as_deref() {
            path_elements = path_elements.saturating_add(path_elements_count(path));
        }
        if let Some(text) = &shape.text {
            validate_text(&text.body)?;
        }
    }
    if let NodeKind::Text(text) = &node.kind {
        validate_text(&text.text)?;
    }
    if path_elements > MAX_PATH_ELEMENTS {
        return Err(
            "Preview path complexity limit reached; simplify the path or choose a smaller frame"
                .into(),
        );
    }
    Ok(())
}

fn validate_text(text: &str) -> Result<(), String> {
    if text.len() > MAX_TEXT_BYTES {
        Err("Preview text limit reached; choose a smaller frame".into())
    } else {
        Ok(())
    }
}

fn path_elements_count(path: &slate_doc::scene::PathData) -> usize {
    // point_count is the geometry owner's canonical count; ancillary arrays
    // and erase strokes are counted too because the painter walks them.
    let mut count = path.point_count();
    for contour in &path.extra {
        count = count.saturating_add(1 + contour.segs.len());
    }
    count = count.saturating_add(path.tips.len());
    count = count.saturating_add(path.field_colors.len());
    count = count.saturating_add(path.corner_amounts.len());
    for erase in &path.erase {
        count = count
            .saturating_add(erase.points.len())
            .saturating_add(erase.tips.len());
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{PathData, PathSeg, ShapeKind, ShapeNode, Stroke, TextNode};

    fn path_node(segments: usize) -> Node {
        let mut scene = slate_doc::scene::Scene::default();
        let path = PathData {
            segs: vec![PathSeg::Line { to: [0.0, 0.0] }; segments],
            ..PathData::default()
        };
        scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: Stroke::default(),
                corner: Default::default(),
                sides: 4,
                phase_deg: 0.0,
                flip: false,
                path: Some(std::sync::Arc::new(path)),
                text: None,
            }),
        )
    }

    fn text_node(text: String) -> Node {
        let mut scene = slate_doc::scene::Scene::default();
        scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Text(TextNode {
                text,
                style: Default::default(),
                family: Default::default(),
                size: 12.0,
                color: slate_doc::scene::Rgba::BLACK,
                align: Default::default(),
                fill: None,
                stroke: Stroke::default(),
                agent: None,
            }),
        )
    }

    #[test]
    fn path_complexity_accepts_limit_and_rejects_one_over() {
        // point_count includes the starting point, so 1,023 segments is 1,024 elements.
        assert!(validate_node(&path_node(MAX_PATH_ELEMENTS - 1)).is_ok());
        let err = validate_node(&path_node(MAX_PATH_ELEMENTS)).unwrap_err();
        assert!(err.contains("path complexity"));
    }

    #[test]
    fn authored_text_accepts_byte_limit_and_rejects_one_over() {
        assert!(validate_node(&text_node("x".repeat(MAX_TEXT_BYTES))).is_ok());
        let err = validate_node(&text_node("x".repeat(MAX_TEXT_BYTES + 1))).unwrap_err();
        assert!(err.contains("text limit"));
    }
}
