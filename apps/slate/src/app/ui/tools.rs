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

mod palettes;
mod primaries;
pub(crate) use palettes::palette_strip_items;
mod flyouts;
pub(crate) mod tags;
pub(crate) use flyouts::osnap_command_id;
use flyouts::*;

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
board_dock_icon!(icon_polygon, ToolIcon::Polygon);
board_dock_icon!(icon_line, ToolIcon::Line);
board_dock_icon!(icon_arc, ToolIcon::Arc);
board_dock_icon!(icon_polyline, ToolIcon::Polyline);
board_dock_icon!(icon_bezier, ToolIcon::Bezier);
board_dock_icon!(icon_pen, ToolIcon::Pen);
board_dock_icon!(icon_brush, ToolIcon::Brush);
board_dock_icon!(icon_eraser, ToolIcon::Eraser);
board_dock_icon!(icon_smooth, ToolIcon::Smooth);
board_dock_icon!(icon_web, ToolIcon::WebPortal);
board_dock_icon!(icon_atlas, ToolIcon::AtlasLens);
board_dock_icon!(icon_trim, ToolIcon::Trim);
board_dock_icon!(icon_split, ToolIcon::Split);
board_dock_icon!(icon_join, ToolIcon::Join);

/// Two triangles facing each other across a dashed axis: the filled one is
/// the original, the outlined one its mirror image.
fn paint_mirror_glyph(p: &egui::Painter, r: Rect, c: Color32, vertical: bool) {
    let at = |u: f32, v: f32| {
        let (u, v) = if vertical { (v, u) } else { (u, v) };
        egui::pos2(r.left() + r.width() * u, r.top() + r.height() * v)
    };
    let stroke = Stroke::new(1.2_f32, c);
    p.add(egui::Shape::convex_polygon(
        vec![at(0.44, 0.24), at(0.44, 0.76), at(0.16, 0.76)],
        c,
        Stroke::NONE,
    ));
    p.add(egui::Shape::closed_line(
        vec![at(0.56, 0.24), at(0.84, 0.76), at(0.56, 0.76)],
        stroke,
    ));
    p.extend(egui::Shape::dashed_line(
        &[at(0.5, 0.12), at(0.5, 0.88)],
        Stroke::new(1.0_f32, c),
        2.0,
        2.0,
    ));
}

fn icon_mirror_h(p: &egui::Painter, r: Rect, c: Color32) {
    paint_mirror_glyph(p, r, c, false);
}

fn icon_mirror_v(p: &egui::Painter, r: Rect, c: Color32) {
    paint_mirror_glyph(p, r, c, true);
}

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
        BoardTool::Polygon => DockIcon::Custom(icon_polygon),
        BoardTool::Line => DockIcon::Custom(icon_line),
        BoardTool::Arc => DockIcon::Custom(icon_arc),
        BoardTool::Polyline => DockIcon::Custom(icon_polyline),
        BoardTool::BezierSpan => DockIcon::Custom(icon_bezier),
        BoardTool::Pen => DockIcon::Custom(icon_pen),
        BoardTool::Brush => DockIcon::Custom(icon_brush),
        BoardTool::Eraser => DockIcon::Custom(icon_eraser),
        BoardTool::Smooth => DockIcon::Custom(icon_smooth),
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
        BoardTool::Polygon => "Click to place a hexagon, or drag to size; Shift locks aspect.",
        BoardTool::Line => "Draw a straight line.",
        BoardTool::Pen => "Freehand path.",
        BoardTool::Brush => {
            "Brush — [ ] size, Shift+[ ] softness, Shift+drag straight line from the last stroke's end (45° steps, Tab locks its direction), Shift+click connects to the last stroke, Shift+right-drag scrubs opacity, Alt+right-drag scrubs size and softness from the press point, Ctrl+right-drag opens the color wheel."
        }
        BoardTool::Eraser => "Eraser — erase painted ink; vector strokes go whole. [ ] size, Shift+[ ] softness, Alt+right-drag size and softness, Shift+right-drag strength, Shift+drag (45° steps, Tab locks) or Shift+click for a straight pass.",
        BoardTool::Smooth => "Smooth — Laplacian on vector strokes; whole-stroke blur on painted ink. [ ] size, Shift+[ ] softness, Alt+right-drag size and softness, Shift+right-drag strength, Shift+drag for a straight pass.",
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
    let items = primaries::items(app);
    app.feedback.canvas_rect = Some(app.canvas_rect);

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
            "tool.tags" => tags::show(app, ui),
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
    if clicked == Some("app.feedback.open") {
        app.dispatch(ctx, atlas_commands::CommandId("app.feedback.open"), None);
    }
}

/// Activate the baseline recipe from a canvas copy without changing dock state.
pub(crate) fn activate_flyout_id(app: &mut SlateApp, ctx: &egui::Context, id: &str) {
    if tags::activate_encoded(app, ctx, id) {
        return;
    }
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
        "action.trim" | "action.split" | "action.join" | "action.mirror_h" | "action.mirror_v" => {
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
        "shape.polygon" => {
            app.set_board_tool(BoardTool::Polygon);
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
        "shape.smooth" => {
            app.set_board_tool(BoardTool::Smooth);
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
        "tool.tags" => "Tags",
        "object.properties" => "Object properties",
        "document.settings" => "Document settings",
        _ => "Palette",
    }
}
