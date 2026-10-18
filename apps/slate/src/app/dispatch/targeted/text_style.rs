//! Strict text/style adapters reuse composition and property owners on clones.
use super::*;
use crate::app::board::board_text_place::{document_text_layout, inline_text_layout};
use crate::app::board_properties::Property;
use eframe::egui::Context;
use slate_doc::scene::NodeKind;

pub(super) fn prepare(
    app: &SlateApp,
    ctx: Option<&Context>,
    command: &str,
    ids: &[NodeId],
    arguments: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    let ctx = ctx.ok_or("Text and style preparation needs the active application context")?;
    prepare::validate_keys(arguments, schema::arguments(command)?)?;
    if ids.is_empty() || ids.len() > 128 {
        return Err("Use 1–128 explicit targets".into());
    }
    let mut cmds = Vec::new();
    if command == "board.objects.set_text" {
        if ids.len() != 1 {
            return Err("Text replacement requires exactly one target".into());
        }
        let text = arguments["text"]
            .as_str()
            .filter(|text| text.chars().count() <= 16_384)
            .ok_or("Supply authored text, up to 16384 characters; empty text is allowed")?;
        let before = app
            .doc()
            .scene
            .node(ids[0])
            .ok_or("Text target is missing")?;
        if !matches!(&before.kind, NodeKind::Text(_))
            && !matches!(&before.kind, NodeKind::Shape(shape) if slate_doc::scene::shape_hosts_text(shape))
            && !matches!(&before.kind, NodeKind::Image(image) if image.text_override.is_some())
        {
            return Err(
                "Use authored text, an eligible shape, or an existing authored card override"
                    .into(),
            );
        }
        let rect = match &before.kind {
            NodeKind::Image(image) => document_text_layout(
                before.rect,
                image.corner,
                &app.board_xf(),
                app.node_item_path(before),
            )
            .grow(ctx, before.rect, text),
            _ => inline_text_layout(ctx, before, None)
                .map_or(before.rect, |layout| layout.grow(ctx, before.rect, text)),
        };
        let mut after = before.clone();
        if matches!(after.kind, NodeKind::Image(_)) {
            slate_doc::authored_text::apply_document_text(&mut after, text, rect);
        } else {
            slate_doc::authored_text::apply_inline_edit(&mut after, text, Some(rect));
        }
        if &after == before {
            return Err("Text replacement would make no changes".into());
        }
        cmds.push(prepare::patch(&app.doc().scene, ids[0], |node| {
            *node = after
        })?);
    } else {
        let raw = arguments["edits"]
            .as_array()
            .filter(|edits| (1..=16).contains(&edits.len()))
            .ok_or("Use 1–16 property edits")?;
        let edits: Vec<Property> = serde_json::from_value(Value::Array(raw.clone()))
            .map_err(|_| "Use a bounded array of supported typed property edits")?;
        for id in ids {
            let before = app.doc().scene.node(*id).ok_or("Style target is missing")?;
            if command == "board.wire.edit" && !matches!(before.kind, NodeKind::Connector(_)) {
                return Err("Wire edits require connector targets".into());
            }
            let mut after = before.clone();
            for edit in &edits {
                check_property(edit, &after)?;
                edit.apply_at(&mut after, None, &[]);
            }
            if &after == before {
                return Err("Style edits would make no changes".into());
            }
            cmds.push(prepare::patch(&app.doc().scene, *id, |node| *node = after)?);
        }
    }
    Ok((cmds, ids.to_vec()))
}

fn check_property(property: &Property, node: &Node) -> Result<(), String> {
    let text_host = matches!(&node.kind, NodeKind::Text(_))
        || matches!(&node.kind, NodeKind::Shape(shape) if slate_doc::scene::shape_hosts_text(shape));
    let supported = match property {
        Property::FillRgb(_) => slate_doc::scene::supports_fill(node),
        Property::StrokeRgb(_) => slate_doc::scene::supports_stroke(node),
        Property::StrokeWidth(width) => {
            width.is_finite()
                && (0.0..=10_000.0).contains(width)
                && slate_doc::scene::supports_stroke(node)
        }
        Property::TextSize(size) => text_host && size.is_finite() && (1.0..=512.0).contains(size),
        Property::TextFamily(_)
        | Property::TextAlign(_)
        | Property::TextRgb(_)
        | Property::TextBold(_)
        | Property::TextItalic(_)
        | Property::TextUnderline(_) => text_host,
        _ => false,
    };
    if supported {
        Ok(())
    } else {
        Err("This property is unsupported for the target or outside its bounds".into())
    }
}

#[cfg(test)]
#[path = "text_style_tests.rs"]
mod tests;
