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
use atlas_shell::sidebar::{sidebar_subtle_divider, SidebarTheme, SidebarTokens};
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
            ),
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
    let clicked = floating_dock(
        ctx,
        "slate_tools",
        canvas,
        &palette,
        app.dock_side,
        &items,
        |ui, id| match id {
            "tool.frame" => frame_flyout(app, ui, theme),
            "tool.shapes" => shapes_flyout(app, ui, theme),
            "tool.curve" => curve_flyout(app, ui, theme),
            "tags" => tags_body(app, ui, theme),
            _ => {}
        },
    );

    match clicked {
        Some("tool.frame") => app.board_tool = BoardTool::Frame,
        Some("tool.shapes") => app.board_tool = BoardTool::RectShape,
        Some("tool.curve") => app.board_tool = BoardTool::Line,
        Some("tool.text") => app.board_tool = BoardTool::Text,
        Some("board.grid") => app.board_show_grid = !app.board_show_grid,
        Some("board.snap") => app.board_snap_grid = !app.board_snap_grid,
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
            app.board_tool = BoardTool::Frame;
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
        app.board_tool = BoardTool::Frame;
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
            app.board_tool = shape;
        }
    }
}

fn curve_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    for curve in [
        BoardTool::Line,
        BoardTool::Pen,
        BoardTool::Arc,
        BoardTool::Polyline,
        BoardTool::BezierSpan,
    ] {
        let hotkey = match curve {
            BoardTool::Line => Some(curve.hotkey()),
            BoardTool::Pen => Some("P"),
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
            app.board_tool = curve;
        }
    }
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
