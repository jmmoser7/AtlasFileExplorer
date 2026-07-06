//! Left tools rail — the hierarchical tagging editor, display settings, and
//! workbook operations. Layout primitives come from `atlas_shell::sidebar`.

use super::super::chrome::ToolPanel;
use super::super::SlateApp;
use atlas_shell::sidebar::{
    sidebar_mode_row, sidebar_region, sidebar_section, sidebar_slider_block,
    sidebar_subtle_divider, sidebar_toolbar_row, SidebarTheme,
};
use atlas_shell::widgets::{gear_menu, thin_sidebar_slider};
use eframe::egui::{self, Color32, Id, RichText};
use slate_doc::{GroupId, TagId, ViewKind};

fn tools_gear(app: &mut SlateApp, ui: &mut egui::Ui) {
    gear_menu(ui, "slate_tools_gear", |ui| {
        ui.label(RichText::new("Visible tool panels").small().strong());
        ui.separator();
        for panel in ToolPanel::ALL {
            let mut on = app.tab().chrome.tool(panel);
            if ui.checkbox(&mut on, panel.label()).changed() {
                app.tab_mut().chrome.set_tool(panel, on);
            }
        }
        ui.separator();
        if ui.button("Advanced settings…").clicked() {
            app.tab_mut().chrome.advanced_open = true;
            ui.close_menu();
        }
    });
}

pub fn left_panel(app: &mut SlateApp, ctx: &egui::Context) {
    let theme = app.palette().sidebar_theme();
    let chrome = app.tab().chrome.clone();
    egui::SidePanel::left("slate_tools_rail")
        .resizable(true)
        .default_width(210.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                tools_gear(app, ui);
                ui.label(RichText::new("Tools").small().color(theme.sub));
            });
            ui.add_space(4.0);

            if chrome.tool(ToolPanel::Tags) {
                tags_panel(app, ui, theme);
            }
            if chrome.tool(ToolPanel::Display) {
                display_panel(app, ui, theme);
            }
            if chrome.tool(ToolPanel::Selection) {
                super::inspector::selection_panel(app, ui, theme);
            }
            if chrome.tool(ToolPanel::Workbook) {
                workbook_panel(app, ui, theme);
            }
            if chrome.tool(ToolPanel::Ai) {
                ai_panel(app, ui, theme);
            }
        });
}

/// AI / Cursor panel — the body is shared with File Atlas (`atlas_ai::ui`),
/// so the assistant toolbar looks and behaves identically in both apps.
fn ai_panel(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let mut expanded = app.tab().chrome.tool_expanded(ToolPanel::Ai);
    if sidebar_section(
        ui,
        Id::new("slate_ai"),
        "AI",
        Some("Cursor"),
        &mut expanded,
        theme,
        |ui| atlas_ai::ui::ai_body(&mut app.ai, ui, theme),
    ) {
        app.tab_mut()
            .chrome
            .set_tool_expanded(ToolPanel::Ai, expanded);
    }
}

// ----- Tags panel -------------------------------------------------------------

fn tags_panel(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let mut expanded = app.tab().chrome.tool_expanded(ToolPanel::Tags);
    if sidebar_section(
        ui,
        Id::new("slate_tags"),
        "Tags",
        Some("groups are exclusive"),
        &mut expanded,
        theme,
        |ui| tags_body(app, ui, theme),
    ) {
        app.tab_mut()
            .chrome
            .set_tool_expanded(ToolPanel::Tags, expanded);
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

// ----- Display panel ------------------------------------------------------------

fn display_panel(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let mut expanded = app.tab().chrome.tool_expanded(ToolPanel::Display);
    if sidebar_section(
        ui,
        Id::new("slate_display"),
        "Display",
        None,
        &mut expanded,
        theme,
        |ui| display_body(app, ui, theme),
    ) {
        app.tab_mut()
            .chrome
            .set_tool_expanded(ToolPanel::Display, expanded);
    }
}

fn display_body(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    sidebar_region(ui, "Presentation", theme, |ui| {
        let current = app.doc().view.active_view;
        if sidebar_mode_row(
            ui,
            current == ViewKind::Board,
            "Board",
            "open-world canvas",
            "An authored canvas: slide frames, shapes, text, and freely placed \
             images. Presents as slides and exports an HTML artifact.",
            theme,
        )
        .clicked()
        {
            app.doc_mut().view.active_view = ViewKind::Board;
        }
        if sidebar_mode_row(
            ui,
            current == ViewKind::Grid,
            "Grid",
            "grouped by tags",
            "Thumbnails grouped into sections by their tag combination.",
            theme,
        )
        .clicked()
        {
            app.doc_mut().view.active_view = ViewKind::Grid;
        }
        if sidebar_mode_row(
            ui,
            current == ViewKind::Venn,
            "Venn",
            "overlapping circles",
            "Each focused tag becomes a circle; files sharing tags sit in the overlaps. \
             Thumbnails render as packed circles.",
            theme,
        )
        .clicked()
        {
            app.doc_mut().view.active_view = ViewKind::Venn;
        }
    });

    sidebar_subtle_divider(ui, theme);
    sidebar_region(ui, "Thumbnails", theme, |ui| {
        sidebar_slider_block(ui, |ui| {
            let mut cell = app.cell as usize;
            if thin_sidebar_slider(
                ui,
                &mut cell,
                72..=240,
                "Cell size",
                "px",
                "Grid cell size in world units",
                theme.sub,
            ) {
                app.cell = cell as f32;
            }
        });
    });

    sidebar_subtle_divider(ui, theme);
    let mut dark = app.dark_mode;
    if ui.checkbox(&mut dark, "Dark mode").changed() {
        app.dark_mode = dark;
        app.apply_theme(ui.ctx());
        if let Some(sess) = &app.atlas {
            if let Ok(mut s) = sess.shared.lock() {
                s.dark_mode = dark;
            }
        }
    }
}

// ----- Workbook panel -------------------------------------------------------------

fn workbook_panel(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let mut expanded = app.tab().chrome.tool_expanded(ToolPanel::Workbook);
    if sidebar_section(
        ui,
        Id::new("slate_workbook"),
        "Workbook",
        None,
        &mut expanded,
        theme,
        |ui| workbook_body(app, ui, theme),
    ) {
        app.tab_mut()
            .chrome
            .set_tool_expanded(ToolPanel::Workbook, expanded);
    }
}

fn workbook_body(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    sidebar_toolbar_row(ui, |ui| {
        if ui.button("Open…").on_hover_text("Ctrl+O").clicked() {
            app.open_doc_dialog();
        }
        if ui.button("Save").on_hover_text("Ctrl+S").clicked() {
            app.save_doc();
        }
        if ui
            .button("Save as…")
            .on_hover_text("Ctrl+Shift+S")
            .clicked()
        {
            app.save_doc_as_dialog();
        }
    });
    sidebar_toolbar_row(ui, |ui| {
        if ui
            .button("Add files…")
            .on_hover_text("Add files to this workbook (or drop them onto the window)")
            .clicked()
        {
            app.add_files_dialog();
        }
    });

    sidebar_subtle_divider(ui, theme);
    sidebar_region(ui, "Artifact", theme, |ui| {
        if ui
            .button("Export artifact…")
            .on_hover_text(
                "Write the board as an HTML slide deck (Ctrl+E). Frames become \
                 slides; what you see on the board is what the file shows.",
            )
            .clicked()
        {
            app.export_artifact_dialog();
        }
        ui.checkbox(&mut app.export_inline, "Single file (inline assets)")
            .on_hover_text(
                "Embed images as base64 inside index.html — one portable file, \
                 larger size — instead of an assets/ folder.",
            );
    });

    sidebar_subtle_divider(ui, theme);
    sidebar_region(ui, "File Atlas", theme, |ui| {
        if app.atlas.is_some() {
            ui.label(
                RichText::new("Linked — right-click files in Atlas to tag them")
                    .small()
                    .color(theme.sub),
            );
            if ui.button("Close linked Atlas").clicked() {
                app.close_atlas();
            }
        } else if ui
            .button("Open File Atlas…")
            .on_hover_text(
                "Opens File Atlas in a linked window. This workbook's tags appear \
                 in Atlas's right-click menu, and tagged or dragged files flow \
                 straight onto this slate.",
            )
            .clicked()
        {
            app.open_atlas(ui.ctx());
        }
    });
}
