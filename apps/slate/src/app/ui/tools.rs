//! Slate's unified bottom dock — one centered row of floating squircle icons
//! over the canvas. Board creation tools (Board view only), grid/snap/align,
//! and the Tags / Selection / View / Lens panels all live here; the dock
//! chrome itself is painted by `atlas_shell::dock`.
//!
//! To add a tool: add a `DockItem` in [`floating_tools_dock`], an arm in the
//! panel-body match (for Panel items), and an arm in the click match (for
//! Action items). Renaming a tool = changing its `label`.

use super::super::board::{BoardAlign, BoardTool, DistributeAxis, FrameCustomDraft, FramePreset};
use super::super::board_icons::{self, ToolIcon};
use super::super::chrome::ToolPanel;
use super::super::SlateApp;
use atlas_shell::dock::{floating_dock, DockIcon, DockItem, DockItemKind, DockSide};
use atlas_shell::sidebar::{sidebar_subtle_divider, SidebarTheme, SidebarTokens};
use eframe::egui::{self, Color32, Rect, RichText};
use slate_doc::{GroupId, TagId, ViewKind};

// Custom dock-icon painters bridging the shared dock to Slate's board icons.
macro_rules! board_dock_icon {
    ($name:ident, $icon:expr) => {
        fn $name(p: &egui::Painter, r: Rect, c: Color32) {
            board_icons::paint_tool_icon(p, r, $icon, c);
        }
    };
}
board_dock_icon!(icon_select, ToolIcon::Select);
board_dock_icon!(icon_pan, ToolIcon::Pan);
board_dock_icon!(icon_frame, ToolIcon::Frame);
board_dock_icon!(icon_shapes, ToolIcon::Shapes);
board_dock_icon!(icon_curve, ToolIcon::Curve);
board_dock_icon!(icon_text, ToolIcon::Text);
board_dock_icon!(icon_grid, ToolIcon::Grid);
board_dock_icon!(icon_snap, ToolIcon::Snap);
board_dock_icon!(icon_align, ToolIcon::Align);

/// The single floating toolbar: board tools + panel launchers, one dock.
pub fn floating_tools_dock(app: &mut SlateApp, ctx: &egui::Context) {
    let theme = app.palette().sidebar_theme();
    let chrome = app.tab().chrome.clone();
    let view = app.doc().view.active_view;
    let board = view == ViewKind::Board;

    // Keep the combined nav button in sync with hotkey switches (V / H).
    if matches!(app.board_tool, BoardTool::Select | BoardTool::Pan) {
        app.board_nav_tool = app.board_tool;
    }
    let tool = app.board_tool;
    let nav_tool = app.board_nav_tool;

    let items = [
        // ----- board creation tools (Board view only) -----
        DockItem {
            id: "tool.nav",
            label: "Select / Pan",
            icon: DockIcon::Custom(if nav_tool == BoardTool::Pan {
                icon_pan
            } else {
                icon_select
            }),
            kind: DockItemKind::Panel,
            active: matches!(tool, BoardTool::Select | BoardTool::Pan),
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.frame",
            label: "Frame",
            icon: DockIcon::Custom(icon_frame),
            kind: DockItemKind::Panel,
            active: tool == BoardTool::Frame,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.shapes",
            label: "Shapes",
            icon: DockIcon::Custom(icon_shapes),
            kind: DockItemKind::Panel,
            active: matches!(tool, BoardTool::RectShape | BoardTool::Ellipse),
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.curve",
            label: "Curve",
            icon: DockIcon::Custom(icon_curve),
            kind: DockItemKind::Panel,
            active: matches!(
                tool,
                BoardTool::Line | BoardTool::Arc | BoardTool::Polyline | BoardTool::BezierSpan
            ),
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.text",
            label: "Text (T)",
            icon: DockIcon::Custom(icon_text),
            kind: DockItemKind::Action,
            active: tool == BoardTool::Text,
            visible: board,
            gap_before: false,
        },
        // ----- board view options -----
        DockItem {
            id: "board.grid",
            label: "Board grid",
            icon: DockIcon::Custom(icon_grid),
            kind: DockItemKind::Action,
            active: app.board_show_grid,
            visible: board,
            gap_before: true,
        },
        DockItem {
            id: "board.snap",
            label: "Snap to grid",
            icon: DockIcon::Custom(icon_snap),
            kind: DockItemKind::Action,
            active: app.board_snap_grid,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "board.align",
            label: "Align",
            icon: DockIcon::Custom(icon_align),
            kind: DockItemKind::Panel,
            active: false,
            visible: board && app.board_sel.len() >= 2,
            gap_before: false,
        },
        // ----- workspace panels (all views) -----
        DockItem {
            id: "tags",
            label: "Tags",
            icon: DockIcon::Tags,
            kind: DockItemKind::Panel,
            active: false,
            visible: chrome.tool(ToolPanel::Tags),
            gap_before: board,
        },
        DockItem {
            id: "selection",
            label: "Selection",
            icon: DockIcon::Selection,
            kind: DockItemKind::Panel,
            active: !app.board_sel.is_empty(),
            visible: chrome.tool(ToolPanel::Selection),
            gap_before: false,
        },
        DockItem {
            id: "view",
            label: "View",
            icon: DockIcon::View,
            kind: DockItemKind::Panel,
            active: false,
            visible: chrome.tool(ToolPanel::Display),
            gap_before: false,
        },
        DockItem {
            id: "lens",
            label: "Lens",
            icon: DockIcon::Lens,
            kind: DockItemKind::Panel,
            active: view == ViewKind::Lens,
            visible: chrome.tool(ToolPanel::Lens) && view == ViewKind::Lens,
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
        DockSide::BottomCenter,
        &items,
        |ui, id| match id {
            "tool.nav" => nav_flyout(app, ui, theme),
            "tool.frame" => frame_flyout(app, ui, theme),
            "tool.shapes" => shapes_flyout(app, ui, theme),
            "tool.curve" => curve_flyout(app, ui, theme),
            "board.align" => align_flyout(app, ui),
            "tags" => tags_body(app, ui, theme),
            "selection" => super::inspector::selection_body(app, ui, theme),
            "view" => display_body(app, ui),
            "lens" => app.lens_sidebar(ui, theme),
            _ => {}
        },
    );

    // Icon clicks: tool activation and toggles.
    match clicked {
        Some("tool.nav") => {
            let other = if nav_tool == BoardTool::Select {
                BoardTool::Pan
            } else {
                BoardTool::Select
            };
            app.board_tool = if tool == nav_tool { other } else { nav_tool };
            app.board_nav_tool = app.board_tool;
        }
        Some("tool.frame") => app.board_tool = BoardTool::Frame,
        Some("tool.shapes") => app.board_tool = BoardTool::RectShape,
        Some("tool.curve") => app.board_tool = BoardTool::Line,
        Some("tool.text") => app.board_tool = BoardTool::Text,
        Some("board.grid") => app.board_show_grid = !app.board_show_grid,
        Some("board.snap") => app.board_snap_grid = !app.board_snap_grid,
        _ => {}
    }
}

// ----- board tool flyouts -------------------------------------------------------

fn nav_flyout(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    for nav in [BoardTool::Select, BoardTool::Pan] {
        if board_icons::tool_menu_row(
            ui,
            nav.tool_icon(),
            nav.label(),
            Some(nav.hotkey()),
            app.board_tool == nav,
            theme.ink,
            theme.sub,
        )
        .clicked()
        {
            app.board_tool = nav;
            app.board_nav_tool = nav;
        }
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
        BoardTool::Arc,
        BoardTool::Polyline,
        BoardTool::BezierSpan,
    ] {
        let hotkey = (curve == BoardTool::Line).then_some(curve.hotkey());
        let resp = board_icons::tool_menu_row(
            ui,
            curve.tool_icon(),
            curve.label(),
            hotkey,
            app.board_tool == curve,
            theme.ink,
            theme.sub,
        );
        let resp = if curve.is_implemented() {
            resp
        } else {
            resp.on_hover_text("Coming soon")
        };
        if resp.clicked() {
            if curve.is_implemented() {
                app.board_tool = curve;
            } else {
                app.toast(format!(
                    "{} is not available yet — use Line for now.",
                    curve.label()
                ));
            }
        }
    }
}

fn align_flyout(app: &mut SlateApp, ui: &mut egui::Ui) {
    for (label, align) in [
        ("Left", BoardAlign::Left),
        ("Center", BoardAlign::CenterH),
        ("Right", BoardAlign::Right),
        ("Top", BoardAlign::Top),
        ("Middle", BoardAlign::CenterV),
        ("Bottom", BoardAlign::Bottom),
    ] {
        if ui.button(label).clicked() {
            app.align_board_selection(align);
        }
    }
    if app.board_sel.len() >= 3 {
        ui.separator();
        if ui.button("Distribute horizontally").clicked() {
            app.distribute_board_selection(DistributeAxis::Horizontal);
        }
        if ui.button("Distribute vertically").clicked() {
            app.distribute_board_selection(DistributeAxis::Vertical);
        }
    }
}

// ----- Tags panel body ----------------------------------------------------------

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

    // "+ Add group" — the top-level sub-sub-menu creator.
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
    // Group header with rename/delete context menu.
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

    // Inline "+" for a new mutually exclusive tag inside this group.
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

/// Focus toggling: an empty focus set means "all tags shown"; clicking a tag
/// in that state focuses everything *except* nothing — i.e. materializes the
/// full set first, then toggles the clicked tag.
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

// ----- Presentation Mode panel --------------------------------------------------

fn view_kind_label(kind: ViewKind) -> &'static str {
    match kind.normalized() {
        ViewKind::Board | ViewKind::Branch => "Board",
        ViewKind::Grid | ViewKind::Unknown => "Grid",
        ViewKind::Venn => "Venn",
        ViewKind::Lens => "Lens",
    }
}

fn display_body(app: &mut SlateApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SidebarTokens::OPTION_GAP;
        ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);

        let mut dark = app.dark_mode;
        let theme_label = if dark { "Dark" } else { "Light" };
        if ui.checkbox(&mut dark, theme_label).changed() {
            app.dark_mode = dark;
            app.apply_theme(ui.ctx());
            if let Some(sess) = &app.atlas {
                if let Ok(mut s) = sess.shared.lock() {
                    s.dark_mode = dark;
                }
            }
        }

        let current = match app.doc().view.active_view.normalized() {
            ViewKind::Branch => ViewKind::Board,
            other => other,
        };
        egui::ComboBox::from_id_salt("slate_presentation_view")
            .selected_text(view_kind_label(current))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for kind in [
                    ViewKind::Board,
                    ViewKind::Grid,
                    ViewKind::Venn,
                    ViewKind::Lens,
                ] {
                    if ui
                        .selectable_label(current == kind, view_kind_label(kind))
                        .clicked()
                    {
                        app.doc_mut().view.active_view = kind;
                    }
                }
            });
    });
}

// Workbook file operations and the File Atlas link now live in the app-icon
// portal (File menu); AI/Cursor lives in Preferences. See `ui/menubar.rs`.
