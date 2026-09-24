//! Slate's unified bottom dock — one condensed row of floating squircle icons
//! over the canvas. Board creation tools (Board view only) share unlabeled
//! icon-strip capsules. Object properties and selection live on the per-node
//! strips. Document settings live in the home menu (Preferences). Dock chrome
//! is painted by `atlas_shell::dock`.
//!
//! Ordering: Tools → Actions (see `crates/atlas-shell/DOCK.md`).

use super::super::board::{BoardTool, FrameCustomDraft, FramePreset};
use super::super::board_icons::{self, ToolIcon};
use super::super::SlateApp;
use atlas_shell::dock::{
    floating_dock, flyout_items, DockIcon, DockItem, DockItemKind, DockOutcome, FlyoutItem,
    FlyoutRole,
};
use atlas_shell::sidebar::SidebarTheme;
use eframe::egui::{self, Color32, Rect, Stroke};
use slate_doc::ViewKind;

pub(crate) const DOCK_ID: &str = "slate_tools";

macro_rules! board_dock_icon {
    ($name:ident, $icon:expr) => {
        fn $name(p: &egui::Painter, r: Rect, c: Color32) {
            board_icons::paint_tool_icon(p, r, $icon, c);
        }
    };
}
board_dock_icon!(icon_media, ToolIcon::Media);
board_dock_icon!(icon_image, ToolIcon::Image);
board_dock_icon!(icon_model, ToolIcon::Model);
board_dock_icon!(icon_video, ToolIcon::Video);
board_dock_icon!(icon_text_doc, ToolIcon::TextDoc);
board_dock_icon!(icon_frame, ToolIcon::Frame);
board_dock_icon!(icon_frame_letter, ToolIcon::FrameLetter);
board_dock_icon!(icon_frame_tabloid, ToolIcon::FrameTabloid);
board_dock_icon!(icon_frame_wide, ToolIcon::FrameWide);
board_dock_icon!(icon_frame_custom, ToolIcon::FrameCustom);
board_dock_icon!(icon_portals, ToolIcon::Portals);
fn icon_agent(p: &egui::Painter, r: Rect, c: Color32) {
    // The face is wide, so the shared squircle inset leaves it smaller than its neighbors.
    board_icons::paint_tool_icon(p, r.expand(r.width() * 0.22), ToolIcon::Agent, c);
}
board_dock_icon!(icon_shapes, ToolIcon::Shapes);
board_dock_icon!(icon_text, ToolIcon::Text);
board_dock_icon!(icon_sticky, ToolIcon::Sticky);
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
board_dock_icon!(icon_web, ToolIcon::WebPortal);
board_dock_icon!(icon_atlas, ToolIcon::AtlasLens);
board_dock_icon!(icon_trim, ToolIcon::Trim);
board_dock_icon!(icon_split, ToolIcon::Split);
board_dock_icon!(icon_join, ToolIcon::Join);

fn icon_reach_tight(p: &egui::Painter, r: Rect, c: Color32) {
    p.circle_stroke(r.center(), r.width() * 0.16, Stroke::new(1.3_f32, c));
}

fn icon_reach_nearby(p: &egui::Painter, r: Rect, c: Color32) {
    p.circle_stroke(r.center(), r.width() * 0.22, Stroke::new(1.2_f32, c));
    p.circle_stroke(r.center(), r.width() * 0.34, Stroke::new(1.1_f32, c));
}

fn icon_reach_wide(p: &egui::Painter, r: Rect, c: Color32) {
    p.circle_stroke(r.center(), r.width() * 0.18, Stroke::new(1.2_f32, c));
    p.circle_stroke(r.center(), r.width() * 0.30, Stroke::new(1.1_f32, c));
    p.circle_stroke(r.center(), r.width() * 0.42, Stroke::new(1.0_f32, c));
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
        BoardTool::AgentPortal => DockIcon::Custom(icon_agent),
        BoardTool::WebPortal => DockIcon::Custom(icon_web),
        BoardTool::AtlasPortal => DockIcon::Custom(icon_atlas),
        BoardTool::SlatePortal => DockIcon::Custom(icon_frame),
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
        BoardTool::Brush => {
            "Brush — [ ] size, Shift+[ ] softness, Shift+click steps opacity, Shift+right-drag scrubs opacity, Alt+right-drag scrubs size and softness from the press point, Ctrl+right-drag opens the color wheel."
        }
        BoardTool::Eraser => "Eraser — erase painted ink; vector strokes go whole. [ ] size, Shift+[ ] softness, Alt+right-drag size and softness, Shift+right-drag strength, Shift+drag or Shift+click for a straight pass.",
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
            id: "tool.media",
            label: "Media",
            description: "Place images and print media, 3D models, or video. PowerPoint renders as PDF pages.",
            icon: DockIcon::Custom(icon_media),
            kind: DockItemKind::Tool,
            active: false,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.frame",
            label: "Frame",
            description: "Place a slide frame — Letter, Tabloid, 16:9, or a custom size.",
            icon: frame_preset_icon(app.board_frame_preset),
            kind: DockItemKind::Tool,
            active: tool == BoardTool::Frame,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.portals",
            label: "Portals",
            description: "Drop a portal onto the board (File Atlas, Agent, Web, or a nested Slate board).",
            icon: DockIcon::Custom(icon_portals),
            kind: DockItemKind::Tool,
            active: matches!(
                tool,
                BoardTool::AgentPortal | BoardTool::WebPortal | BoardTool::AtlasPortal | BoardTool::SlatePortal
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
            icon: DockIcon::Actions,
            kind: DockItemKind::Tool,
            active: matches!(tool, BoardTool::Trim | BoardTool::Split),
            visible: board,
            gap_before: true,
        },
    ];

    let palette = app.palette();
    let canvas = app.canvas_rect;
    let restore = app.dock_pins.clone();
    let restore_strips = app.dock_icon_strips.clone();
    let restore_hidden = app.dock_strip_hidden.clone();
    let restore_order = app.dock_strip_order.clone();
    let DockOutcome {
        clicked,
        drop_to_canvas,
    } = floating_dock(
        ctx,
        DOCK_ID,
        canvas,
        &palette,
        app.dock_side,
        &items,
        &restore,
        &restore_strips,
        &restore_hidden,
        &restore_order,
        app.dock_bar_collapsed,
        |ui, id| match id {
            "tool.media" => media_flyout(app, ui),
            "tool.frame" => frame_flyout(app, ui, theme),
            "tool.portals" => portals_flyout(app, ui, theme),
            "tool.shapes" => shapes_flyout(app, ui, theme),
            "tool.text" => text_flyout(app, ui, theme),
            "tool.actions" => actions_flyout(app, ui, theme),
            _ => {}
        },
    );

    // Persist pinned palettes and flyout layout across sessions.
    let mut prefs_dirty = false;
    if let Some(pins) = atlas_shell::dock::pinned_ids(ctx, DOCK_ID) {
        if pins != app.dock_pins {
            app.dock_pins = pins;
            prefs_dirty = true;
        }
    }
    if let Some(strips) = atlas_shell::dock::icon_strip_ids(ctx, DOCK_ID) {
        if strips != app.dock_icon_strips {
            app.dock_icon_strips = strips;
            prefs_dirty = true;
        }
    }
    if let Some(hidden) = atlas_shell::dock::strip_hidden(ctx, DOCK_ID) {
        if hidden != app.dock_strip_hidden {
            app.dock_strip_hidden = hidden;
            prefs_dirty = true;
        }
    }
    if let Some(order) = atlas_shell::dock::strip_order(ctx, DOCK_ID) {
        if order != app.dock_strip_order {
            app.dock_strip_order = order;
            prefs_dirty = true;
        }
    }
    if let Some(collapsed) = atlas_shell::dock::bar_collapsed(ctx, DOCK_ID) {
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

    // Picker primaries (Media, Frame, Portals, Shapes, Text, Actions)
    // open the flyout only. Nested icons arm or run the chosen command.
    let _ = clicked;
}

fn media_flyout(app: &mut SlateApp, ui: &mut egui::Ui) {
    let items = palette_strip_items(app, "tool.media", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        activate_flyout_id(app, ui.ctx(), id);
    }
}

fn frame_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let _ = theme;
    let items = palette_strip_items(app, "tool.frame", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        apply_frame_choice(app, id);
    }
}

fn frame_preset_icon(preset: FramePreset) -> DockIcon {
    match preset {
        FramePreset::Letter => DockIcon::Custom(icon_frame_letter),
        FramePreset::Tabloid => DockIcon::Custom(icon_frame_tabloid),
        FramePreset::Wide169 => DockIcon::Custom(icon_frame_wide),
        FramePreset::Custom { .. } => DockIcon::Custom(icon_frame_custom),
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

fn shapes_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let _ = theme;
    let items = palette_strip_items(app, "tool.shapes", &[]);
    if let Some(id) = flyout_items(ui, &items) {
        if !arm_kit_tool(app, id) {
            apply_shape_choice(app, id);
        }
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
        "portal.agent" | "portal.web" | "portal.atlas" | "portal.slate" => {
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
        "media.image" => Some("board.media.image"),
        "media.model" => Some("board.media.model"),
        "media.video" => Some("board.media.video"),
        "media.text" => Some("board.media.text"),
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
        "tool.media" => "Media",
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
        "tool.media" => vec![
            FlyoutItem { id: "media.image", label: "Image", description: "Images and print media: JPG, PNG, PDF, and PowerPoint.", hotkey: None, icon: DockIcon::Custom(icon_image), active: false, group: Some("media"), role: FlyoutRole::Icon },
            FlyoutItem { id: "media.model", label: "3D", description: "Place a 3D model. OBJ, STL, glTF, and Rhino preview in the viewport. Blender, DWG, and SketchUp use the same card.", hotkey: None, icon: DockIcon::Custom(icon_model), active: false, group: Some("media"), role: FlyoutRole::Icon },
            FlyoutItem { id: "media.video", label: "Video", description: "Place a video. Hover across it to scrub the whole clip; click to play. mp4, mov, webm, and other types the system can open.", hotkey: None, icon: DockIcon::Custom(icon_video), active: false, group: Some("media"), role: FlyoutRole::Icon },
            FlyoutItem { id: "media.text", label: "Text", description: "Word, Excel, CSV, spreadsheets, and source code. Shows an excerpt when the file can be read.", hotkey: None, icon: DockIcon::Custom(icon_text_doc), active: false, group: Some("media"), role: FlyoutRole::Icon },
        ],
        "tool.frame" => {
            let preset = app.board_frame_preset;
            vec![
                FlyoutItem {
                    id: "frame.letter",
                    label: FramePreset::Letter.label(),
                    description: "Letter slide frame (8.5 × 11).",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame_letter),
                    active: preset == FramePreset::Letter,
                    group: Some("frame"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "frame.tabloid",
                    label: FramePreset::Tabloid.label(),
                    description: "Tabloid slide frame, landscape (17 × 11).",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame_tabloid),
                    active: preset == FramePreset::Tabloid,
                    group: Some("frame"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "frame.wide",
                    label: FramePreset::Wide169.label(),
                    description: "16:9 slide frame.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame_wide),
                    active: preset == FramePreset::Wide169,
                    group: Some("frame"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "frame.custom",
                    label: "Custom…",
                    description: "Type a custom frame size.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame_custom),
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
                    id: "portal.agent",
                    label: "Agent portal",
                    description: "Host portal for a local Cursor agent session.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_agent),
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
                    description:
                        "Live folder map on the board — File Atlas's canvas, not its window.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_atlas),
                    active: tool == BoardTool::AtlasPortal && app.armed_kit_id.is_none(),
                    group: Some("portals"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "portal.slate",
                    label: "Slate board",
                    description: "Load another workbook's board inside this frame.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame),
                    active: tool == BoardTool::SlatePortal && app.armed_kit_id.is_none(),
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
                (
                    BoardTool::Line,
                    "shape.line",
                    Some(BoardTool::Line.hotkey()),
                    "curves",
                ),
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
                FlyoutItem {
                    id: "settings.reach.tight",
                    label: "tight",
                    description: super::super::settings::SnapReach::Tight.hint(),
                    hotkey: None,
                    icon: DockIcon::Custom(icon_reach_tight),
                    active: app.board_snap_reach == super::super::settings::SnapReach::Tight,
                    group: Some("reach"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "settings.reach.nearby",
                    label: "nearby",
                    description: super::super::settings::SnapReach::Nearby.hint(),
                    hotkey: None,
                    icon: DockIcon::Custom(icon_reach_nearby),
                    active: app.board_snap_reach == super::super::settings::SnapReach::Nearby,
                    group: Some("reach"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "settings.reach.wide",
                    label: "wide",
                    description: super::super::settings::SnapReach::Wide.hint(),
                    hotkey: None,
                    icon: DockIcon::Custom(icon_reach_wide),
                    active: app.board_snap_reach == super::super::settings::SnapReach::Wide,
                    group: Some("reach"),
                    role: FlyoutRole::Icon,
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
        items.sort_by_key(|it| {
            visible
                .iter()
                .position(|v| v == it.id)
                .unwrap_or(usize::MAX)
        });
    }
    items
}
