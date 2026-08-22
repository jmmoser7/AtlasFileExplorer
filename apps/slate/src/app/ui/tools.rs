//! Slate's unified bottom dock — one condensed row of floating squircle icons
//! over the canvas. Board creation tools (Board view only) plus object /
//! document dashboards; dock chrome is painted by `atlas_shell::dock`.
//!
//! Ordering: Tools → Actions → Dashboards (see `crates/atlas-shell/DOCK.md`).

use super::super::board::{BoardTool, FrameCustomDraft, FramePreset};
use super::super::board_icons::{self, ToolIcon};
use super::super::chrome::ToolPanel;
use super::super::SlateApp;
use atlas_shell::dock::{
    current_body_layout, floating_dock, flyout_items, paint_dock_icon, DockBodyLayout, DockIcon,
    DockItem, DockItemKind, DockOutcome, FlyoutItem, FlyoutRole,
};
use atlas_shell::sidebar::{
    sidebar_choice_chips, sidebar_fold_region, sidebar_heading, sidebar_icon_row,
    sidebar_segmented, sidebar_subtle_divider, ChoiceChip, SegmentedItem, SidebarTheme,
};
use eframe::egui::{self, Color32, Id, Rect, RichText};
use slate_doc::{GroupId, TagId, ViewKind};

macro_rules! board_dock_icon {
    ($name:ident, $icon:expr) => {
        fn $name(p: &egui::Painter, r: Rect, c: Color32) {
            board_icons::paint_tool_icon(p, r, $icon, c);
        }
    };
}
board_dock_icon!(icon_frame, ToolIcon::Frame);
board_dock_icon!(icon_portals, ToolIcon::Portals);
board_dock_icon!(icon_shapes, ToolIcon::Shapes);
board_dock_icon!(icon_text, ToolIcon::Text);
board_dock_icon!(icon_sticky, ToolIcon::Sticky);
board_dock_icon!(icon_grid, ToolIcon::Grid);
board_dock_icon!(icon_colors, ToolIcon::Colors);
board_dock_icon!(icon_rect, ToolIcon::Rect);
board_dock_icon!(icon_ellipse, ToolIcon::Ellipse);
board_dock_icon!(icon_line, ToolIcon::Line);
board_dock_icon!(icon_arc, ToolIcon::Arc);
board_dock_icon!(icon_polyline, ToolIcon::Polyline);
board_dock_icon!(icon_bezier, ToolIcon::Bezier);
board_dock_icon!(icon_pen, ToolIcon::Pen);
board_dock_icon!(icon_brush, ToolIcon::Brush);
board_dock_icon!(icon_eraser, ToolIcon::Eraser);
board_dock_icon!(icon_repo, ToolIcon::RepoLens);
board_dock_icon!(icon_status, ToolIcon::StatusBoard);
board_dock_icon!(icon_web, ToolIcon::WebPortal);
board_dock_icon!(icon_atlas, ToolIcon::AtlasLens);
board_dock_icon!(icon_trim, ToolIcon::Trim);
board_dock_icon!(icon_split, ToolIcon::Split);
board_dock_icon!(icon_join, ToolIcon::Join);

fn icon_wire_bezier(p: &egui::Painter, r: Rect, c: Color32) {
    let a = egui::pos2(r.left() + r.width() * 0.16, r.center().y);
    let b = egui::pos2(r.right() - r.width() * 0.16, r.center().y);
    let c1 = egui::pos2(r.center().x, r.top() + r.height() * 0.18);
    let c2 = egui::pos2(r.center().x, r.bottom() - r.height() * 0.18);
    p.add(egui::Shape::CubicBezier(
        egui::epaint::CubicBezierShape::from_points_stroke(
            [a, c1, c2, b],
            false,
            Color32::TRANSPARENT,
            egui::Stroke::new(1.4_f32, c),
        ),
    ));
}

fn icon_wire_ortho(p: &egui::Painter, r: Rect, c: Color32) {
    let pts = [
        egui::pos2(
            r.left() + r.width() * 0.16,
            r.center().y + r.height() * 0.22,
        ),
        egui::pos2(r.center().x, r.center().y + r.height() * 0.22),
        egui::pos2(r.center().x, r.center().y - r.height() * 0.22),
        egui::pos2(
            r.right() - r.width() * 0.16,
            r.center().y - r.height() * 0.22,
        ),
    ];
    atlas_shell::dock::rounded_route(p, &pts, 3.0, egui::Stroke::new(1.4_f32, c));
}

fn tool_dock_icon(tool: BoardTool) -> DockIcon {
    match tool {
        BoardTool::RectShape => DockIcon::Custom(icon_rect),
        BoardTool::Ellipse => DockIcon::Custom(icon_ellipse),
        BoardTool::Line => DockIcon::Custom(icon_line),
        BoardTool::Arc => DockIcon::Custom(icon_arc),
        BoardTool::Polyline => DockIcon::Custom(icon_polyline),
        BoardTool::BezierSpan => DockIcon::Custom(icon_bezier),
        BoardTool::Pen => DockIcon::Custom(icon_pen),
        BoardTool::Brush => DockIcon::Custom(icon_brush),
        BoardTool::Eraser => DockIcon::Custom(icon_eraser),
        BoardTool::RepoLens => DockIcon::Custom(icon_repo),
        BoardTool::StatusBoard => DockIcon::Custom(icon_status),
        BoardTool::AgentPortal => DockIcon::Custom(icon_portals),
        BoardTool::WebPortal => DockIcon::Custom(icon_web),
        BoardTool::AtlasPortal => DockIcon::Custom(icon_atlas),
        BoardTool::Trim => DockIcon::Custom(icon_trim),
        BoardTool::Split => DockIcon::Custom(icon_split),
        BoardTool::Frame => DockIcon::Custom(icon_frame),
        BoardTool::Text => DockIcon::Custom(icon_text),
        BoardTool::Sticky => DockIcon::Custom(icon_sticky),
        _ => DockIcon::Custom(icon_shapes),
    }
}

fn tool_flyout_desc(tool: BoardTool) -> &'static str {
    match tool {
        BoardTool::RectShape => "Click to place a rectangle, or drag to size.",
        BoardTool::Ellipse => "Click to place a circle, or drag to size.",
        BoardTool::Line => "Draw a straight line.",
        BoardTool::Pen => "Freehand path.",
        BoardTool::Brush => "Expressive ink stroke.",
        BoardTool::Eraser => "Erase path strokes.",
        BoardTool::Arc => "Draw an arc.",
        BoardTool::Polyline => "Draw a polyline.",
        BoardTool::BezierSpan => "Draw a bezier span.",
        _ => tool.label(),
    }
}

/// The single floating toolbar: Frame, Portals, Shapes (incl. curves), Text,
/// Actions (Trim, Split, Join), Object properties (colors + tags), Document settings
/// (grid + snap).
pub fn floating_tools_dock(app: &mut SlateApp, ctx: &egui::Context) {
    let theme = app.palette().sidebar_theme();
    let view = app.doc().view.active_view;
    let board = view == ViewKind::Board;
    let tool = app.board_tool;

    let shape_active = matches!(
        tool,
        BoardTool::RectShape
            | BoardTool::Ellipse
            | BoardTool::Line
            | BoardTool::Arc
            | BoardTool::Polyline
            | BoardTool::BezierSpan
            | BoardTool::Pen
            | BoardTool::Brush
            | BoardTool::Eraser
    );

    let items = [
        DockItem {
            id: "tool.frame",
            label: "Frame",
            description: "Place a slide frame — Letter, Tabloid, 16:9, or a custom size.",
            icon: DockIcon::Custom(icon_frame),
            kind: DockItemKind::Tool,
            active: tool == BoardTool::Frame,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.portals",
            label: "Portals",
            description: "Drop a generated or host portal onto the board (Repository Lens, File Atlas, Status Board, Agent, or Web).",
            icon: DockIcon::Custom(icon_portals),
            kind: DockItemKind::Tool,
            active: matches!(
                tool,
                BoardTool::RepoLens
                    | BoardTool::StatusBoard
                    | BoardTool::AgentPortal
                    | BoardTool::WebPortal
                    | BoardTool::AtlasPortal
            ),
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.shapes",
            label: "Shapes",
            description: "Rectangles, ellipses, and curve tools (line, pen, brush, eraser…).",
            icon: DockIcon::Custom(icon_shapes),
            kind: DockItemKind::Tool,
            active: shape_active,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.text",
            label: "Text",
            description: "Place a text block or sticky note on the board.",
            icon: DockIcon::Custom(icon_text),
            kind: DockItemKind::Tool,
            active: matches!(tool, BoardTool::Text | BoardTool::Sticky),
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.actions",
            label: "Actions",
            description: "Edit commands — Trim (Ctrl+T), Split (Ctrl+Shift+T), Join (Ctrl+J).",
            icon: DockIcon::Custom(icon_trim),
            kind: DockItemKind::Tool,
            active: matches!(tool, BoardTool::Trim | BoardTool::Split),
            visible: board,
            gap_before: true,
        },
        DockItem {
            id: "object.properties",
            label: "Object properties",
            description: "Ink / paper colors and the workbook’s faceted tag groups.",
            icon: DockIcon::Custom(icon_colors),
            kind: DockItemKind::Dashboard,
            active: false,
            visible: board || app.chrome().tool(ToolPanel::Tags),
            gap_before: board,
        },
        DockItem {
            id: "document.settings",
            label: "Document settings",
            description: "Board grid, object snaps, smart-guide reach, and wire routing.",
            icon: DockIcon::Custom(icon_grid),
            kind: DockItemKind::Dashboard,
            active: app.board_show_grid
                || app.board_snap_grid
                || app.board_smart_guides
                || app.board_osnap.any_kind_on()
                || app.board_wire_routing != slate_doc::WireRouting::Bezier,
            visible: board,
            gap_before: false,
        },
    ];

    let palette = app.palette();
    let canvas = app.canvas_rect;
    let restore = app.dock_pins.clone();
    let restore_strips = app.dock_icon_strips.clone();
    let restore_hidden = app.dock_strip_hidden.clone();
    let DockOutcome {
        clicked,
        drop_to_canvas,
    } = floating_dock(
        ctx,
        "slate_tools",
        canvas,
        &palette,
        app.dock_side,
        &items,
        &restore,
        &restore_strips,
        &restore_hidden,
        app.dock_bar_collapsed,
        |ui, id| match id {
            "tool.frame" => frame_flyout(app, ui, theme),
            "tool.portals" => portals_flyout(app, ui, theme),
            "tool.shapes" => shapes_flyout(app, ui, theme),
            "tool.text" => text_flyout(app, ui, theme),
            "tool.actions" => actions_flyout(app, ui, theme),
            "object.properties" => object_properties_body(app, ui, theme),
            "document.settings" => document_settings_body(app, ui, theme),
            _ => {}
        },
    );

    // Persist pinned palettes and flyout layout across sessions.
    let mut prefs_dirty = false;
    if let Some(pins) = atlas_shell::dock::pinned_ids(ctx, "slate_tools") {
        if pins != app.dock_pins {
            app.dock_pins = pins;
            prefs_dirty = true;
        }
    }
    if let Some(strips) = atlas_shell::dock::icon_strip_ids(ctx, "slate_tools") {
        if strips != app.dock_icon_strips {
            app.dock_icon_strips = strips;
            prefs_dirty = true;
        }
    }
    if let Some(hidden) = atlas_shell::dock::strip_hidden(ctx, "slate_tools") {
        if hidden != app.dock_strip_hidden {
            app.dock_strip_hidden = hidden;
            prefs_dirty = true;
        }
    }
    if let Some(collapsed) = atlas_shell::dock::bar_collapsed(ctx, "slate_tools") {
        if collapsed != app.dock_bar_collapsed {
            app.dock_bar_collapsed = collapsed;
            prefs_dirty = true;
        }
    }
    if prefs_dirty {
        app.save_chrome_prefs();
    }
    if let Some(ids) = atlas_shell::dock::take_catalog_duplicate(ctx) {
        let _ = app.kits.duplicate_catalog_ids(&ids);
    }
    if let Some(palette_id) = drop_to_canvas {
        app.drop_dock_strip(ctx, palette_id);
    }

    // Dock buttons dispatch the same registry commands the keyboard uses
    // (one command surface — Art. VIII), so each click lands in the F2
    // history and feeds Space/Enter repeat.
    use atlas_commands::CommandId;
    match clicked {
        Some("tool.frame") => {
            app.dispatch(ctx, CommandId("board.tool.frame"), Some("dock".into()));
        }
        Some("tool.portals") => {
            // Flyout is the picker; do not auto-arm a subtype now that two
            // generated portals share the chip (P1.portal).
        }
        Some("tool.shapes") => {
            // Open on the last shape-family tool, defaulting to rect.
            if !shape_active {
                app.dispatch(ctx, CommandId("board.tool.rect"), Some("dock".into()));
            }
        }
        Some("tool.text") => {
            // Flyout picks Text vs Sticky; do not auto-arm a subtype.
        }
        Some("tool.actions") => {
            app.dispatch(ctx, CommandId("board.tool.trim"), Some("dock".into()));
        }
        _ => {}
    }
}

fn frame_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let _ = theme;
    let items = palette_strip_items(app, "tool.frame", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        apply_frame_choice(app, id);
    }
}

fn apply_frame_choice(app: &mut SlateApp, id: &str) {
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

fn portals_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let _ = theme;
    let items = palette_strip_items(app, "tool.portals", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        arm_kit_tool(app, id);
    }
}

fn arm_kit_tool(app: &mut SlateApp, id: &str) -> bool {
    let Some(tool) = app.kits.board_tool_for(id) else {
        return false;
    };
    let derived = app.kits.is_derived(id);
    let kit_id = if derived { Some(id.to_string()) } else { None };
    app.set_board_tool(tool);
    app.armed_kit_id = kit_id;
    true
}

fn kit_dock_icon(app: &SlateApp, id: &str) -> DockIcon {
    app.kits
        .board_tool_for(id)
        .map(tool_dock_icon)
        .unwrap_or(DockIcon::Grid)
}

fn apply_portal_choice(app: &mut SlateApp, id: &str) {
    let _ = arm_kit_tool(app, id);
}

fn text_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let _ = theme;
    let items = palette_strip_items(app, "tool.text", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        apply_text_choice(app, id);
    }
}

fn apply_shape_choice(app: &mut SlateApp, id: &str) {
    let tool = match id {
        "shape.rect" => BoardTool::RectShape,
        "shape.ellipse" => BoardTool::Ellipse,
        "shape.line" => BoardTool::Line,
        "shape.arc" => BoardTool::Arc,
        "shape.polyline" => BoardTool::Polyline,
        "shape.bezier" => BoardTool::BezierSpan,
        "shape.pen" => BoardTool::Pen,
        "shape.brush" => BoardTool::Brush,
        "shape.eraser" => BoardTool::Eraser,
        _ => return,
    };
    app.set_board_tool(tool);
}

fn apply_text_choice(app: &mut SlateApp, id: &str) {
    let tool = match id {
        "text.block" => BoardTool::Text,
        "text.sticky" => BoardTool::Sticky,
        _ => return,
    };
    app.set_board_tool(tool);
}

fn actions_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let _ = theme;
    let items = palette_strip_items(app, "tool.actions", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        apply_action_choice(app, ui.ctx(), id);
    }
}

fn apply_action_choice(app: &mut SlateApp, ctx: &egui::Context, id: &str) {
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
        _ => {}
    }
}

/// Shapes + curves in one flyout (collapsed curve section by default).
fn shapes_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    if current_body_layout(ui.ctx()) != DockBodyLayout::List {
        let items = palette_strip_items(app, "tool.shapes", &[]);
        if let Some(id) = flyout_items(ui, &items) {
            if !arm_kit_tool(app, id) {
                apply_shape_choice(app, id);
            }
        }
        return;
    }
    for shape in [BoardTool::RectShape, BoardTool::Ellipse] {
        if stack_tool_row(
            ui,
            shape,
            Some(shape.hotkey()),
            app.board_tool == shape,
            theme,
        ) {
            app.set_board_tool(shape);
        }
    }

    sidebar_subtle_divider(ui, theme);
    sidebar_fold_region(
        ui,
        Id::new("slate_shapes_curves"),
        "Curves & ink",
        false,
        theme,
        |ui| {
            for curve in [
                BoardTool::Line,
                BoardTool::Pen,
                BoardTool::Brush,
                BoardTool::Eraser,
                BoardTool::Arc,
                BoardTool::Polyline,
                BoardTool::BezierSpan,
            ] {
                let hotkey = match curve {
                    BoardTool::Line => Some(curve.hotkey()),
                    BoardTool::Pen => Some("P"),
                    BoardTool::Brush => Some("B"),
                    BoardTool::Eraser => Some("E"),
                    _ => None,
                };
                if stack_tool_row(ui, curve, hotkey, app.board_tool == curve, theme) {
                    app.set_board_tool(curve);
                }
            }
        },
    );
}

fn stack_tool_row(
    ui: &mut egui::Ui,
    tool: BoardTool,
    hotkey: Option<&str>,
    active: bool,
    theme: SidebarTheme,
) -> bool {
    sidebar_icon_row(ui, tool.label(), hotkey, active, theme, |p, r, c| {
        board_icons::paint_tool_icon(p, r, tool.tool_icon(), c)
    })
    .clicked()
}

fn object_properties_body(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let board = app.doc().view.active_view == ViewKind::Board;
    if current_body_layout(ui.ctx()) != DockBodyLayout::List {
        object_properties_icons(app, ui, board);
        return;
    }
    if board {
        sidebar_fold_region(
            ui,
            Id::new("slate_obj_colors"),
            "Colors",
            true,
            theme,
            |ui| colors_body(app, ui, theme),
        );
    }
    if app.chrome().tool(ToolPanel::Tags) {
        sidebar_fold_region(ui, Id::new("slate_obj_tags"), "Tags", !board, theme, |ui| {
            tags_body(app, ui, theme)
        });
    }
}

fn object_properties_icons(app: &mut SlateApp, ui: &mut egui::Ui, board: bool) {
    let _ = board;
    let items = palette_strip_items(app, "object.properties", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        let ctx = ui.ctx().clone();
        match id {
            "prop.swap" => {
                app.dispatch(
                    &ctx,
                    atlas_commands::CommandId("board.colors.swap"),
                    Some("dock".into()),
                );
            }
            "prop.reset" => {
                app.dispatch(
                    &ctx,
                    atlas_commands::CommandId("board.colors.default"),
                    Some("dock".into()),
                );
            }
            _ => {}
        }
    }
}

fn document_settings_body(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    if current_body_layout(ui.ctx()) != DockBodyLayout::List {
        document_settings_icons(app, ui);
        return;
    }
    if sidebar_icon_row(
        ui,
        "Show grid",
        Some("G"),
        app.board_show_grid,
        theme,
        |p, r, c| paint_dock_icon(p, r, DockIcon::Grid, c),
    )
    .on_hover_text("Toggle the 20-unit board grid.")
    .clicked()
    {
        let ctx = ui.ctx().clone();
        app.dispatch(
            &ctx,
            atlas_commands::CommandId("board.grid"),
            Some("dock".into()),
        );
    }

    sidebar_subtle_divider(ui, theme);
    let bezier = app.board_wire_routing == slate_doc::WireRouting::Bezier;
    if let Some(i) = sidebar_choice_chips(
        ui,
        "wires",
        theme,
        &[
            ChoiceChip {
                label: "bezier",
                selected: bezier,
                hint: Some("Cubic span — leaves each side perpendicular."),
            },
            ChoiceChip {
                label: "orthogonal",
                selected: !bezier,
                hint: Some(
                    "Axis-aligned wrap — shortest path around hosts; ties go right, then down.",
                ),
            },
        ],
    ) {
        let ctx = ui.ctx().clone();
        let cmd = if i == 0 {
            "board.wire.bezier"
        } else {
            "board.wire.orthogonal"
        };
        app.dispatch(&ctx, atlas_commands::CommandId(cmd), Some("dock".into()));
    }

    sidebar_subtle_divider(ui, theme);
    sidebar_heading(ui, "Object snaps", theme);
    osnap_palette(app, ui, theme);
}

fn document_settings_icons(app: &mut SlateApp, ui: &mut egui::Ui) {
    use slate_doc::SnapKind;
    let items = palette_strip_items(app, "document.settings", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        let ctx = ui.ctx().clone();
        let cmd = match id {
            "settings.grid" => "board.grid",
            "settings.snap_grid" => "board.snap_grid",
            "settings.osnap" => "board.osnap",
            "settings.smart_guides" => "board.smart_guides",
            "settings.wire.bezier" => "board.wire.bezier",
            "settings.wire.orthogonal" => "board.wire.orthogonal",
            other => {
                if let Some(kind) = SnapKind::ALL
                    .iter()
                    .copied()
                    .find(|k| osnap_item_id(*k) == other)
                {
                    osnap_command_id(kind)
                } else {
                    return;
                }
            }
        };
        app.dispatch(&ctx, atlas_commands::CommandId(cmd), Some("dock".into()));
    }
}

fn snap_dock_icon(kind: slate_doc::SnapKind) -> DockIcon {
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

fn osnap_item_id(kind: slate_doc::SnapKind) -> &'static str {
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

fn osnap_palette(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    use slate_doc::SnapKind;

    if sidebar_icon_row(
        ui,
        "Enable object snaps",
        None,
        app.board_osnap.enabled,
        theme,
        |p, r, c| paint_dock_icon(p, r, DockIcon::Osnap, c),
    )
    .on_hover_text(
        "Master switch (Rhino Disable inverted). Checked kinds stay remembered while this is off. Alt suspends snaps for one pick.",
    )
    .clicked()
    {
        let ctx = ui.ctx().clone();
        app.dispatch(
            &ctx,
            atlas_commands::CommandId("board.osnap"),
            Some("dock".into()),
        );
    }

    if sidebar_icon_row(
        ui,
        "Snap to grid",
        Some("F9"),
        app.board_snap_grid,
        theme,
        |p, r, c| paint_dock_icon(p, r, DockIcon::SnapGrid, c),
    )
    .on_hover_text(
        "F9 — snap point picks and moves to the 20-unit board grid. Object snaps override grid when both fire.",
    )
    .clicked()
    {
        let ctx = ui.ctx().clone();
        app.dispatch(
            &ctx,
            atlas_commands::CommandId("board.snap_grid"),
            Some("dock".into()),
        );
    }

    if sidebar_icon_row(
        ui,
        "Smart guides",
        None,
        app.board_smart_guides,
        theme,
        |p, r, c| paint_dock_icon(p, r, DockIcon::SnapGrid, c),
    )
    .on_hover_text(
        "Align to nearby objects in the same row or column. A closer neighbor blocks objects behind it. Alt suspends.",
    )
    .clicked()
    {
        let ctx = ui.ctx().clone();
        app.dispatch(
            &ctx,
            atlas_commands::CommandId("board.smart_guides"),
            Some("dock".into()),
        );
    }

    if let Some(i) = sidebar_segmented(
        ui,
        "REACH",
        theme,
        &[
            SegmentedItem {
                label: "tight",
                selected: app.board_snap_reach == super::super::settings::SnapReach::Tight,
                hint: Some(super::super::settings::SnapReach::Tight.hint()),
                paint_icon: None,
            },
            SegmentedItem {
                label: "nearby",
                selected: app.board_snap_reach == super::super::settings::SnapReach::Nearby,
                hint: Some(super::super::settings::SnapReach::Nearby.hint()),
                paint_icon: None,
            },
            SegmentedItem {
                label: "wide",
                selected: app.board_snap_reach == super::super::settings::SnapReach::Wide,
                hint: Some(super::super::settings::SnapReach::Wide.hint()),
                paint_icon: None,
            },
        ],
    ) {
        let ctx = ui.ctx().clone();
        let cmd = match i {
            0 => "board.snap_reach.tight",
            2 => "board.snap_reach.wide",
            _ => "board.snap_reach.nearby",
        };
        app.dispatch(&ctx, atlas_commands::CommandId(cmd), Some("dock".into()));
    }

    let muted = !app.board_osnap.enabled;
    let kind_theme = if muted {
        SidebarTheme {
            ink: theme.sub,
            ..theme
        }
    } else {
        theme
    };
    ui.columns(2, |cols| {
        for (i, kind) in SnapKind::ALL.iter().copied().enumerate() {
            let ui = &mut cols[i % 2];
            let on = app.board_osnap.is_kind_remembered(kind);
            if sidebar_icon_row(ui, kind.label(), None, on, kind_theme, |p, r, c| {
                paint_dock_icon(p, r, snap_dock_icon(kind), c)
            })
            .on_hover_text(kind.hint())
            .clicked()
            {
                let ctx = ui.ctx().clone();
                app.dispatch(
                    &ctx,
                    atlas_commands::CommandId(osnap_command_id(kind)),
                    Some("dock".into()),
                );
            }
        }
    });
}

fn osnap_command_id(kind: slate_doc::SnapKind) -> &'static str {
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

/// Colors panel: the fg/bg chip pair (click a chip = the standard color
/// picker), swap (X) and reset (D) — same commands the keyboard drives.
fn colors_body(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    use super::super::board::{rgba32, to_rgba};
    ui.horizontal(|ui| {
        ui.label(RichText::new("Ink").small().color(theme.sub));
        let mut fg = rgba32(app.board_colors.fg);
        if ui.color_edit_button_srgba(&mut fg).changed() {
            app.board_colors.fg = to_rgba(fg);
            app.save_board_colors();
        }
        ui.label(RichText::new("Paper").small().color(theme.sub));
        let mut bg = rgba32(app.board_colors.bg);
        if ui.color_edit_button_srgba(&mut bg).changed() {
            app.board_colors.bg = to_rgba(bg);
            app.save_board_colors();
        }
    });
    ui.horizontal(|ui| {
        if ui
            .small_button("Swap")
            .on_hover_text("Swap foreground ⇄ background (X)")
            .clicked()
        {
            let ctx = ui.ctx().clone();
            app.dispatch(
                &ctx,
                atlas_commands::CommandId("board.colors.swap"),
                Some("dock".into()),
            );
        }
        if ui
            .small_button("Reset")
            .on_hover_text("Reset to the theme ink/paper (D)")
            .clicked()
        {
            let ctx = ui.ctx().clone();
            app.dispatch(
                &ctx,
                atlas_commands::CommandId("board.colors.default"),
                Some("dock".into()),
            );
        }
    });
    ui.label(
        RichText::new(format!(
            "Brush {:.1}u · Eraser {:.1}u — [ and ] step widths",
            app.brush_width, app.eraser_width
        ))
        .small()
        .color(theme.sub),
    );
}

fn tags_body(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let groups: Vec<(GroupId, String)> = app
        .doc()
        .groups
        .iter()
        .map(|g| (g.id, g.name.clone()))
        .collect();

    let mut structure_changed = false;

    for (gi, (group_id, group_name)) in groups.iter().enumerate() {
        if gi > 0 {
            sidebar_subtle_divider(ui, theme);
        }
        group_rows(
            app,
            ui,
            theme,
            *group_id,
            group_name,
            &mut structure_changed,
        );
    }

    if !groups.is_empty() {
        sidebar_subtle_divider(ui, theme);
    }

    if let Some((None, buf)) = &mut app.new_tag_edit {
        let resp = ui.add(
            egui::TextEdit::singleline(buf)
                .hint_text("Group name…")
                .desired_width(ui.available_width()),
        );
        resp.request_focus();
        if resp.lost_focus() {
            let name = buf.trim().to_string();
            app.new_tag_edit = None;
            if !name.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                app.doc_mut().add_group(name);
                structure_changed = true;
            }
        }
    } else if ui
        .button(RichText::new("＋ Add tag group").small())
        .on_hover_text(
            "A tag group holds mutually exclusive tags (e.g. Big / Medium / Small). \
             A file can hold one tag from each group.",
        )
        .clicked()
    {
        app.new_tag_edit = Some((None, String::new()));
    }

    if structure_changed {
        app.publish_session_tags();
    }
}

fn group_rows(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    theme: SidebarTheme,
    group_id: GroupId,
    group_name: &str,
    structure_changed: &mut bool,
) {
    let header =
        ui.horizontal(|ui| ui.label(RichText::new(group_name).small().strong().color(theme.ink)));
    header.inner.context_menu(|ui| {
        let dark = app.dark_mode;
        atlas_shell::menu::prepare(ui, dark);
        if atlas_shell::menu::item_danger(
            ui,
            atlas_shell::menu::MenuIcon::Trash,
            "Delete group",
            dark,
        )
        .clicked()
        {
            app.doc_mut().remove_group(group_id);
            *structure_changed = true;
            ui.close_menu();
        }
    });

    let tags: Vec<(TagId, String, [u8; 3], usize)> = app
        .doc()
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .map(|g| {
            g.tags
                .iter()
                .map(|t| {
                    let count = app.doc().items_with_tag(t.id).len();
                    (t.id, t.name.clone(), t.color, count)
                })
                .collect()
        })
        .unwrap_or_default();

    for (tag_id, name, color, count) in &tags {
        let focused = app.tab().venn_focus.is_empty() || app.tab().venn_focus.contains(tag_id);
        let row = ui.horizontal(|ui| {
            let accent = Color32::from_rgb(color[0], color[1], color[2]);
            ui.label(RichText::new("●").color(if focused {
                accent
            } else {
                accent.gamma_multiply(0.35)
            }));
            let resp = ui.selectable_label(false, RichText::new(name).small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{count}")).small().color(theme.sub));
            });
            resp
        });
        let resp = row.inner.on_hover_text(
            "Click to focus/unfocus this tag in the Venn view · right-click for actions",
        );
        if resp.clicked() {
            toggle_focus(app, &tags, *tag_id);
        }
        resp.context_menu(|ui| {
            let dark = app.dark_mode;
            atlas_shell::menu::prepare(ui, dark);
            if atlas_shell::menu::item_danger(
                ui,
                atlas_shell::menu::MenuIcon::Trash,
                "Remove tag",
                dark,
            )
            .clicked()
            {
                app.doc_mut().remove_tag(*tag_id);
                *structure_changed = true;
                ui.close_menu();
            }
        });
    }

    if let Some((Some(g), buf)) = &mut app.new_tag_edit {
        if *g == group_id {
            let resp = ui.add(
                egui::TextEdit::singleline(buf)
                    .hint_text("Tag name…")
                    .desired_width(ui.available_width()),
            );
            resp.request_focus();
            if resp.lost_focus() {
                let name = buf.trim().to_string();
                app.new_tag_edit = None;
                if !name.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let color = app.next_tag_color();
                    app.doc_mut().add_tag(group_id, name, color);
                    *structure_changed = true;
                }
            }
            return;
        }
    }
    if ui
        .button(RichText::new("＋ tag").small().color(theme.sub))
        .on_hover_text("Add a tag to this group (exclusive with its siblings)")
        .clicked()
    {
        app.new_tag_edit = Some((Some(group_id), String::new()));
    }
}

/// Activate a flyout tool from a canvas-embedded toolbar copy. Create
/// tools arm only — they do not place. Instant actions (join, grid,
/// snaps, color swap) still fire. Does not pin or unpin the baseline dock.
pub(crate) fn activate_flyout_id(app: &mut SlateApp, ctx: &egui::Context, id: &str) {
    if arm_kit_tool(app, id) {
        return;
    }
    match id {
        "frame.letter" | "frame.tabloid" | "frame.wide" | "frame.custom" => {
            apply_frame_choice(app, id);
            return;
        }
        "portal.repo" | "portal.status" | "portal.agent" | "portal.web" | "portal.atlas" => {
            apply_portal_choice(app, id);
            return;
        }
        "text.block" | "text.sticky" => {
            apply_text_choice(app, id);
            return;
        }
        "action.trim" | "action.split" | "action.join" => {
            apply_action_choice(app, ctx, id);
            return;
        }
        "shape.rect" => {
            app.set_board_tool(BoardTool::RectShape);
            return;
        }
        "shape.ellipse" => {
            app.set_board_tool(BoardTool::Ellipse);
            return;
        }
        "shape.line" => {
            app.set_board_tool(BoardTool::Line);
            return;
        }
        "shape.pen" => {
            app.set_board_tool(BoardTool::Pen);
            return;
        }
        "shape.brush" => {
            app.set_board_tool(BoardTool::Brush);
            return;
        }
        "shape.eraser" => {
            app.set_board_tool(BoardTool::Eraser);
            return;
        }
        "shape.arc" => {
            app.set_board_tool(BoardTool::Arc);
            return;
        }
        "shape.polyline" => {
            app.set_board_tool(BoardTool::Polyline);
            return;
        }
        "shape.bezier" => {
            app.set_board_tool(BoardTool::BezierSpan);
            return;
        }
        _ => {}
    }
    let cmd = match id {
        "prop.swap" => Some("board.colors.swap"),
        "prop.reset" => Some("board.colors.default"),
        "settings.grid" => Some("board.grid"),
        "settings.snap_grid" => Some("board.snap_grid"),
        "settings.osnap" => Some("board.osnap"),
        "settings.smart_guides" => Some("board.smart_guides"),
        "settings.wire.bezier" => Some("board.wire.bezier"),
        "settings.wire.orthogonal" => Some("board.wire.orthogonal"),
        other => slate_doc::SnapKind::ALL
            .iter()
            .copied()
            .find(|k| osnap_item_id(*k) == other)
            .map(osnap_command_id),
    };
    if let Some(cmd) = cmd {
        app.dispatch(
            ctx,
            atlas_commands::CommandId(cmd),
            Some("canvas-dock".into()),
        );
    }
}

pub(crate) fn palette_title(id: &str) -> &'static str {
    match id {
        "tool.frame" => "Frame",
        "tool.portals" => "Portals",
        "tool.shapes" => "Shapes",
        "tool.text" => "Text",
        "tool.actions" => "Actions",
        "object.properties" => "Object properties",
        "document.settings" => "Document settings",
        _ => "Palette",
    }
}

/// One recipe for the docked flyout and a canvas `DockStrip`. `visible`
/// empty means the full default set; otherwise it is the drop snapshot.
pub(crate) fn palette_strip_items<'a>(
    app: &'a SlateApp,
    palette_id: &str,
    visible: &[String],
) -> Vec<FlyoutItem<'a>> {
    let mut items = match palette_id {
        "tool.frame" => {
            let preset = app.board_frame_preset;
            vec![
                FlyoutItem {
                    id: "frame.letter",
                    label: FramePreset::Letter.label(),
                    description: "Letter slide frame (8.5 × 11).",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame),
                    active: preset == FramePreset::Letter,
                    group: Some("frame"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "frame.tabloid",
                    label: FramePreset::Tabloid.label(),
                    description: "Tabloid slide frame (11 × 17).",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame),
                    active: preset == FramePreset::Tabloid,
                    group: Some("frame"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "frame.wide",
                    label: FramePreset::Wide169.label(),
                    description: "16:9 slide frame.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame),
                    active: preset == FramePreset::Wide169,
                    group: Some("frame"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "frame.custom",
                    label: "Custom…",
                    description: "Type a custom frame size.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame),
                    active: matches!(preset, FramePreset::Custom { .. }),
                    group: Some("frame"),
                    role: FlyoutRole::Icon,
                },
            ]
        }
        "tool.portals" => {
            let tool = app.board_tool;
            let mut items = vec![
                FlyoutItem {
                    id: "portal.repo",
                    label: "Repository Lens",
                    description: "Generated git history graph for a local repository.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_repo),
                    active: tool == BoardTool::RepoLens && app.armed_kit_id.is_none(),
                    group: Some("portals"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "portal.status",
                    label: "Status Board",
                    description: "Generated project-state instrument from a JSON snapshot.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_status),
                    active: tool == BoardTool::StatusBoard && app.armed_kit_id.is_none(),
                    group: Some("portals"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "portal.agent",
                    label: "Agent portal",
                    description: "Host portal for a local Cursor agent session.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_portals),
                    active: tool == BoardTool::AgentPortal && app.armed_kit_id.is_none(),
                    group: Some("portals"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "portal.web",
                    label: "Web portal",
                    description: "Host a URL or local HTML dashboard on the board.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_web),
                    active: tool == BoardTool::WebPortal && app.armed_kit_id.is_none(),
                    group: Some("portals"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "portal.atlas",
                    label: "File Atlas",
                    description: "Live folder map on the board — File Atlas's canvas, not its window.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_atlas),
                    active: tool == BoardTool::AtlasPortal && app.armed_kit_id.is_none(),
                    group: Some("portals"),
                    role: FlyoutRole::Icon,
                },
            ];
            for extra in app.kits.derived_in_group("portals") {
                items.push(FlyoutItem {
                    id: extra.id,
                    label: super::super::kits::intern_label(&extra.name),
                    description: super::super::kits::intern_label(&extra.description),
                    hotkey: None,
                    icon: kit_dock_icon(app, extra.id),
                    active: app.armed_kit_id.as_deref() == Some(extra.id),
                    group: Some(extra.group),
                    role: FlyoutRole::Icon,
                });
            }
            items
        }
        "tool.shapes" => {
            let tools = [
                (
                    BoardTool::RectShape,
                    "shape.rect",
                    Some(BoardTool::RectShape.hotkey()),
                    "shapes",
                ),
                (
                    BoardTool::Ellipse,
                    "shape.ellipse",
                    Some(BoardTool::Ellipse.hotkey()),
                    "shapes",
                ),
                (BoardTool::Line, "shape.line", Some(BoardTool::Line.hotkey()), "curves"),
                (BoardTool::Arc, "shape.arc", None, "curves"),
                (BoardTool::Polyline, "shape.polyline", None, "curves"),
                (BoardTool::BezierSpan, "shape.bezier", None, "curves"),
                (BoardTool::Pen, "shape.pen", Some("P"), "curves"),
                (BoardTool::Brush, "shape.brush", Some("B"), "ink"),
                (BoardTool::Eraser, "shape.eraser", Some("E"), "ink"),
            ];
            let mut items: Vec<FlyoutItem<'_>> = tools
                .into_iter()
                .map(|(tool, id, hotkey, group)| FlyoutItem {
                    id,
                    label: tool.label(),
                    description: tool_flyout_desc(tool),
                    hotkey,
                    icon: tool_dock_icon(tool),
                    active: app.board_tool == tool,
                    group: Some(group),
                    role: FlyoutRole::Icon,
                })
                .collect();
            for extra in app.kits.derived_in_group("shapes") {
                items.push(FlyoutItem {
                    id: extra.id,
                    label: super::super::kits::intern_label(&extra.name),
                    description: super::super::kits::intern_label(&extra.description),
                    hotkey: None,
                    icon: kit_dock_icon(app, extra.id),
                    active: app.armed_kit_id.as_deref() == Some(extra.id),
                    group: Some(extra.group),
                    role: FlyoutRole::Icon,
                });
            }
            items
        }
        "tool.text" => {
            let tool = app.board_tool;
            vec![
                FlyoutItem {
                    id: "text.block",
                    label: "Text",
                    description: "Place a text block on the board.",
                    hotkey: Some("T"),
                    icon: DockIcon::Custom(icon_text),
                    active: tool == BoardTool::Text,
                    group: Some("text"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "text.sticky",
                    label: "Sticky note",
                    description: "Place a sticky note on the board.",
                    hotkey: Some("N"),
                    icon: DockIcon::Custom(icon_sticky),
                    active: tool == BoardTool::Sticky,
                    group: Some("text"),
                    role: FlyoutRole::Icon,
                },
            ]
        }
        "tool.actions" => vec![
            FlyoutItem {
                id: "action.trim",
                label: "Trim",
                description: "Pick cutters, then click the part to delete.",
                hotkey: Some("Ctrl+T"),
                icon: DockIcon::Custom(icon_trim),
                active: app.board_tool == BoardTool::Trim,
                group: Some("actions"),
                role: FlyoutRole::Icon,
            },
            FlyoutItem {
                id: "action.split",
                label: "Split",
                description: "Pick cutters, then click an object to keep every piece.",
                hotkey: Some("Ctrl+Shift+T"),
                icon: DockIcon::Custom(icon_split),
                active: app.board_tool == BoardTool::Split,
                group: Some("actions"),
                role: FlyoutRole::Icon,
            },
            FlyoutItem {
                id: "action.join",
                label: "Join",
                description: "Join open paths, or union closed regions.",
                hotkey: Some("Ctrl+J"),
                icon: DockIcon::Custom(icon_join),
                active: false,
                group: Some("actions"),
                role: FlyoutRole::Icon,
            },
        ],
        "object.properties" => {
            let mut items = Vec::new();
            if app.doc().view.active_view == ViewKind::Board {
                items.push(FlyoutItem {
                    id: "prop.colors",
                    label: "Colors",
                    description: "Ink and paper — switch to the stacked list to edit the picker.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_colors),
                    active: false,
                    group: Some("colors"),
                    role: FlyoutRole::Icon,
                });
                items.push(FlyoutItem {
                    id: "prop.swap",
                    label: "Swap",
                    description: "Swap foreground ⇄ background (X).",
                    hotkey: Some("X"),
                    icon: DockIcon::Swap,
                    active: false,
                    group: Some("colors"),
                    role: FlyoutRole::Icon,
                });
                items.push(FlyoutItem {
                    id: "prop.reset",
                    label: "Reset",
                    description: "Reset to the theme ink/paper (D).",
                    hotkey: Some("D"),
                    icon: DockIcon::Reset,
                    active: false,
                    group: Some("colors"),
                    role: FlyoutRole::Icon,
                });
            }
            if app.chrome().tool(ToolPanel::Tags) {
                items.push(FlyoutItem {
                    id: "prop.tags",
                    label: "Tags",
                    description: "Workbook tag groups — switch to the stacked list to edit.",
                    hotkey: None,
                    icon: DockIcon::Tags,
                    active: !app.doc().groups.is_empty(),
                    group: Some("tags"),
                    role: FlyoutRole::Icon,
                });
            }
            items
        }
        "document.settings" => {
            use slate_doc::SnapKind;
            let mut items = vec![
                FlyoutItem {
                    id: "settings.grid",
                    label: "grid",
                    description: "Toggle the 20-unit board grid.",
                    hotkey: None,
                    icon: DockIcon::Grid,
                    active: app.board_show_grid,
                    group: Some("grid"),
                    role: FlyoutRole::Toggle,
                },
                FlyoutItem {
                    id: "settings.snap_grid",
                    label: "snap",
                    description: "F9 — snap picks and moves to the board grid.",
                    hotkey: Some("F9"),
                    icon: DockIcon::SnapGrid,
                    active: app.board_snap_grid,
                    group: Some("grid"),
                    role: FlyoutRole::Toggle,
                },
                FlyoutItem {
                    id: "settings.wire.bezier",
                    label: "Bezier wires",
                    description: "Cubic span — leaves each side perpendicular.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_wire_bezier),
                    active: app.board_wire_routing == slate_doc::WireRouting::Bezier,
                    group: Some("wires"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "settings.wire.orthogonal",
                    label: "Orthogonal wires",
                    description: "Axis-aligned wrap around hosts (File Atlas PCB-trace style).",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_wire_ortho),
                    active: app.board_wire_routing == slate_doc::WireRouting::Orthogonal,
                    group: Some("wires"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "settings.osnap",
                    label: "osnap",
                    description: "Master switch. Alt suspends snaps for one pick.",
                    hotkey: None,
                    icon: DockIcon::Osnap,
                    active: app.board_osnap.enabled,
                    group: Some("object snaps"),
                    role: FlyoutRole::Toggle,
                },
                FlyoutItem {
                    id: "settings.smart_guides",
                    label: "guides",
                    description: "Align to nearby objects in the same row or column.",
                    hotkey: None,
                    icon: DockIcon::SnapGrid,
                    active: app.board_smart_guides,
                    group: Some("object snaps"),
                    role: FlyoutRole::Toggle,
                },
            ];
            for kind in SnapKind::ALL {
                items.push(FlyoutItem {
                    id: osnap_item_id(kind),
                    label: kind.label(),
                    description: kind.hint(),
                    hotkey: None,
                    icon: snap_dock_icon(kind),
                    active: app.board_osnap.is_kind_remembered(kind),
                    group: Some("object snaps"),
                    role: FlyoutRole::Toggle,
                });
            }
            items
        }
        _ => Vec::new(),
    };
    if !visible.is_empty() {
        items.retain(|it| visible.iter().any(|v| v == it.id));
        items.sort_by_key(|it| visible.iter().position(|v| v == it.id).unwrap_or(usize::MAX));
    }
    items
}

fn toggle_focus(app: &mut SlateApp, siblings: &[(TagId, String, [u8; 3], usize)], tag: TagId) {
    let all: Vec<TagId> = app
        .doc()
        .groups
        .iter()
        .flat_map(|g| g.tags.iter().map(|t| t.id))
        .collect();
    let _ = siblings;
    let focus = &mut app.tab_mut().venn_focus;
    if focus.is_empty() {
        focus.extend(all);
    }
    if !focus.remove(&tag) {
        focus.insert(tag);
    }
}
