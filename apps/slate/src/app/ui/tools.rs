//! Slate's unified bottom dock — one centered row of floating squircle icons
//! over the canvas. Board creation tools (Board view only), grid/snap toggles,
//! and the Tags dashboard; dock chrome is painted by `atlas_shell::dock`.
//!
//! Ordering: Tools → Actions → Dashboards (see `crates/atlas-shell/DOCK.md`).

use super::super::board::{BoardTool, FrameCustomDraft, FramePreset};
use super::super::board_icons::{self, ToolIcon};
use super::super::chrome::ToolPanel;
use super::super::SlateApp;
use atlas_shell::dock::{floating_dock, DockIcon, DockItem, DockItemKind};
use atlas_shell::sidebar::{sidebar_subtle_divider, SidebarTheme};
use eframe::egui::{self, Color32, Rect, RichText};
use slate_doc::{GroupId, TagId, ViewKind};

macro_rules! board_dock_icon {
    ($name:ident, $icon:expr) => {
        fn $name(p: &egui::Painter, r: Rect, c: Color32) {
            board_icons::paint_tool_icon(p, r, $icon, c);
        }
    };
}
board_dock_icon!(icon_frame, ToolIcon::Frame);
board_dock_icon!(icon_shapes, ToolIcon::Shapes);
board_dock_icon!(icon_curve, ToolIcon::Curve);
board_dock_icon!(icon_text, ToolIcon::Text);
board_dock_icon!(icon_grid, ToolIcon::Grid);
board_dock_icon!(icon_snap, ToolIcon::Snap);
board_dock_icon!(icon_colors, ToolIcon::Colors);

/// The single floating toolbar: simplified board tools + tags + grid/snap.
pub fn floating_tools_dock(app: &mut SlateApp, ctx: &egui::Context) {
    let theme = app.palette().sidebar_theme();
    let view = app.doc().view.active_view;
    let board = view == ViewKind::Board;
    let tool = app.board_tool;

    let items = [
        DockItem {
            id: "tool.frame",
            label: "Frame",
            description: "",
            icon: DockIcon::Custom(icon_frame),
            kind: DockItemKind::Tool,
            active: tool == BoardTool::Frame,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.shapes",
            label: "Shapes",
            description: "",
            icon: DockIcon::Custom(icon_shapes),
            kind: DockItemKind::Tool,
            active: matches!(tool, BoardTool::RectShape | BoardTool::Ellipse),
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.curve",
            label: "Curve",
            description: "",
            icon: DockIcon::Custom(icon_curve),
            kind: DockItemKind::Tool,
            active: matches!(
                tool,
                BoardTool::Line
                    | BoardTool::Arc
                    | BoardTool::Polyline
                    | BoardTool::BezierSpan
                    | BoardTool::Pen
                    | BoardTool::Brush
                    | BoardTool::Eraser
            ),
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.colors",
            label: "Colors",
            description: "",
            icon: DockIcon::Custom(icon_colors),
            kind: DockItemKind::Tool,
            active: false,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.text",
            label: "Text",
            description: "",
            icon: DockIcon::Custom(icon_text),
            kind: DockItemKind::Action,
            active: tool == BoardTool::Text,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "board.grid",
            label: "Grid",
            description: "",
            icon: DockIcon::Custom(icon_grid),
            kind: DockItemKind::Action,
            active: app.board_show_grid,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "board.snap",
            label: "Snap",
            description: "",
            icon: DockIcon::Custom(icon_snap),
            kind: DockItemKind::Action,
            active: app.board_snap_grid,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tags",
            label: "Tags",
            description: "Faceted tag groups for this workbook.",
            icon: DockIcon::Tags,
            kind: DockItemKind::Dashboard,
            active: false,
            visible: app.chrome().tool(ToolPanel::Tags),
            gap_before: false,
        },
    ];

    let palette = app.palette();
    let canvas = app.canvas_rect;
    let restore = app.dock_pins.clone();
    let clicked = floating_dock(
        ctx,
        "slate_tools",
        canvas,
        &palette,
        app.dock_side,
        &items,
        &restore,
        |ui, id| match id {
            "tool.frame" => frame_flyout(app, ui, theme),
            "tool.shapes" => shapes_flyout(app, ui, theme),
            "tool.curve" => curve_flyout(app, ui, theme),
            "tool.colors" => colors_body(app, ui, theme),
            "tags" => tags_body(app, ui, theme),
            _ => {}
        },
    );

    // Persist pinned palettes (e.g. Tags) across sessions.
    if let Some(pins) = atlas_shell::dock::pinned_ids(ctx, "slate_tools") {
        if pins != app.dock_pins {
            app.dock_pins = pins;
            app.save_chrome_prefs();
        }
    }

    // Dock buttons dispatch the same registry commands the keyboard uses
    // (one command surface — Art. VIII), so each click lands in the F2
    // history and feeds Space/Enter repeat.
    use atlas_commands::CommandId;
    match clicked {
        Some("tool.frame") => {
            app.dispatch(ctx, CommandId("board.tool.frame"), Some("dock".into()));
        }
        Some("tool.shapes") => {
            app.dispatch(ctx, CommandId("board.tool.rect"), Some("dock".into()));
        }
        Some("tool.curve") => {
            app.dispatch(ctx, CommandId("board.tool.line"), Some("dock".into()));
        }
        Some("tool.text") => {
            app.dispatch(ctx, CommandId("board.tool.text"), Some("dock".into()));
        }
        Some("board.grid") => {
            app.dispatch(ctx, CommandId("board.grid"), Some("dock".into()));
        }
        Some("board.snap") => {
            app.dispatch(ctx, CommandId("board.snap_grid"), Some("dock".into()));
        }
        _ => {}
    }
}

fn frame_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    for preset in [
        FramePreset::Letter,
        FramePreset::Tabloid,
        FramePreset::Wide169,
    ] {
        if board_icons::tool_menu_row(
            ui,
            ToolIcon::Frame,
            preset.label(),
            None,
            app.board_frame_preset == preset,
            theme.ink,
            theme.sub,
        )
        .clicked()
        {
            app.board_frame_preset = preset;
            app.set_board_tool(BoardTool::Frame);
        }
    }
    if board_icons::tool_menu_row(
        ui,
        ToolIcon::Frame,
        "Custom…",
        None,
        matches!(app.board_frame_preset, FramePreset::Custom { .. }),
        theme.ink,
        theme.sub,
    )
    .clicked()
    {
        app.set_board_tool(BoardTool::Frame);
        app.board_frame_custom
            .get_or_insert_with(|| FrameCustomDraft {
                w: "612".into(),
                h: "792".into(),
            });
    }
}

fn shapes_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    for shape in [BoardTool::RectShape, BoardTool::Ellipse] {
        if board_icons::tool_menu_row(
            ui,
            shape.tool_icon(),
            shape.label(),
            Some(shape.hotkey()),
            app.board_tool == shape,
            theme.ink,
            theme.sub,
        )
        .clicked()
        {
            app.set_board_tool(shape);
        }
    }
}

fn curve_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
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
        let resp = board_icons::tool_menu_row(
            ui,
            curve.tool_icon(),
            curve.label(),
            hotkey,
            app.board_tool == curve,
            theme.ink,
            theme.sub,
        );
        if resp.clicked() {
            app.set_board_tool(curve);
        }
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
        if ui.button("Delete group").clicked() {
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
            if ui.button("Remove tag").clicked() {
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
