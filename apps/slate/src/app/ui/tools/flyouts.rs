use super::*;

pub(super) fn media_flyout(app: &mut SlateApp, ui: &mut egui::Ui) {
    let items = palette_strip_items(app, "tool.media", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        activate_flyout_id(app, ui.ctx(), id);
    }
}

pub(super) fn frame_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let _ = theme;
    let items = palette_strip_items(app, "tool.frame", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        apply_frame_choice(app, id);
    }
}

pub(super) fn frame_preset_icon(preset: FramePreset) -> DockIcon {
    match preset {
        FramePreset::Letter => DockIcon::Custom(icon_frame_letter),
        FramePreset::Tabloid => DockIcon::Custom(icon_frame_tabloid),
        FramePreset::Wide169 => DockIcon::Custom(icon_frame_wide),
        FramePreset::Custom { .. } => DockIcon::Custom(icon_frame_custom),
    }
}

pub(super) fn apply_frame_choice(app: &mut SlateApp, id: &str) {
    match id {
        "frame.letter" => app.board_frame_preset = FramePreset::Letter,
        "frame.tabloid" => app.board_frame_preset = FramePreset::Tabloid,
        "frame.wide" => app.board_frame_preset = FramePreset::Wide169,
        "frame.custom" => {
            app.board_frame_custom
                .get_or_insert_with(|| FrameCustomDraft {
                    w: "612".into(),
                    h: "792".into(),
                });
        }
        _ => {}
    }
    app.set_board_tool(BoardTool::Frame);
}

pub(super) fn portals_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let _ = theme;
    let items = palette_strip_items(app, "tool.portals", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        arm_kit_tool(app, id);
    }
}

pub(super) fn arm_kit_tool(app: &mut SlateApp, id: &str) -> bool {
    let Some(tool) = app.kits.board_tool_for(id) else {
        return false;
    };
    let derived = app.kits.is_derived(id);
    let kit_id = if derived { Some(id.to_string()) } else { None };
    app.set_board_tool(tool);
    app.armed_kit_id = kit_id;
    true
}

pub(super) fn kit_dock_icon(app: &SlateApp, id: &str) -> DockIcon {
    app.kits
        .board_tool_for(id)
        .map(tool_dock_icon)
        .unwrap_or(DockIcon::Grid)
}

pub(super) fn apply_portal_choice(app: &mut SlateApp, id: &str) {
    let _ = arm_kit_tool(app, id);
}

pub(super) fn text_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let _ = theme;
    let items = palette_strip_items(app, "tool.text", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        apply_text_choice(app, id);
    }
}

pub(super) fn apply_shape_choice(app: &mut SlateApp, id: &str) {
    let tool = match id {
        "shape.rect" => BoardTool::RectShape,
        "shape.ellipse" => BoardTool::Ellipse,
        "shape.polygon" => BoardTool::Polygon,
        "shape.line" => BoardTool::Line,
        "shape.arc" => BoardTool::Arc,
        "shape.polyline" => BoardTool::Polyline,
        "shape.bezier" => BoardTool::BezierSpan,
        "shape.pen" => BoardTool::Pen,
        "shape.brush" => BoardTool::Brush,
        "shape.eraser" => BoardTool::Eraser,
        "shape.smooth" => BoardTool::Smooth,
        _ => return,
    };
    app.set_board_tool(tool);
}

pub(super) fn apply_text_choice(app: &mut SlateApp, id: &str) {
    let tool = match id {
        "text.block" => BoardTool::Text,
        "text.sticky" => BoardTool::Sticky,
        _ => return,
    };
    app.set_board_tool(tool);
}

pub(super) fn actions_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let _ = theme;
    let items = palette_strip_items(app, "tool.actions", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        apply_action_choice(app, ui.ctx(), id);
    }
}

pub(super) fn apply_action_choice(app: &mut SlateApp, ctx: &egui::Context, id: &str) {
    match id {
        "action.trim" => app.set_board_tool(BoardTool::Trim),
        "action.split" => app.set_board_tool(BoardTool::Split),
        "action.join" => {
            app.dispatch(
                ctx,
                atlas_commands::CommandId("board.path.join"),
                Some("dock".into()),
            );
        }
        "action.mirror_h" | "action.mirror_v" => {
            let command = if id == "action.mirror_h" {
                "board.mirror.horizontal"
            } else {
                "board.mirror.vertical"
            };
            app.dispatch(ctx, atlas_commands::CommandId(command), None);
        }
        _ => {}
    }
}

pub(super) fn shapes_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let _ = theme;
    let items = palette_strip_items(app, "tool.shapes", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        if !arm_kit_tool(app, id) {
            apply_shape_choice(app, id);
        }
    }
}

pub(super) fn snap_dock_icon(kind: slate_doc::SnapKind) -> DockIcon {
    match kind {
        slate_doc::SnapKind::End => DockIcon::SnapEnd,
        slate_doc::SnapKind::Mid => DockIcon::SnapMid,
        slate_doc::SnapKind::Center => DockIcon::SnapCenter,
        slate_doc::SnapKind::Near => DockIcon::SnapNear,
        slate_doc::SnapKind::Intersection => DockIcon::SnapInt,
        slate_doc::SnapKind::Quadrant => DockIcon::SnapQuad,
        slate_doc::SnapKind::Perpendicular => DockIcon::SnapPerp,
        slate_doc::SnapKind::Tangent => DockIcon::SnapTan,
    }
}

pub(super) fn osnap_item_id(kind: slate_doc::SnapKind) -> &'static str {
    match kind {
        slate_doc::SnapKind::End => "settings.osnap.end",
        slate_doc::SnapKind::Mid => "settings.osnap.mid",
        slate_doc::SnapKind::Center => "settings.osnap.center",
        slate_doc::SnapKind::Near => "settings.osnap.near",
        slate_doc::SnapKind::Intersection => "settings.osnap.int",
        slate_doc::SnapKind::Quadrant => "settings.osnap.quad",
        slate_doc::SnapKind::Perpendicular => "settings.osnap.perp",
        slate_doc::SnapKind::Tangent => "settings.osnap.tan",
    }
}

pub(crate) fn osnap_command_id(kind: slate_doc::SnapKind) -> &'static str {
    match kind {
        slate_doc::SnapKind::End => "board.osnap.end",
        slate_doc::SnapKind::Mid => "board.osnap.mid",
        slate_doc::SnapKind::Center => "board.osnap.center",
        slate_doc::SnapKind::Near => "board.osnap.near",
        slate_doc::SnapKind::Intersection => "board.osnap.int",
        slate_doc::SnapKind::Quadrant => "board.osnap.quad",
        slate_doc::SnapKind::Perpendicular => "board.osnap.perp",
        slate_doc::SnapKind::Tangent => "board.osnap.tan",
    }
}
