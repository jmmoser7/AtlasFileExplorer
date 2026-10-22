//! File Atlas's floating tools dock — a vertical stack of squircle icons
//! centered on the canvas's left edge. Panel popovers open to the right;
//! dock chrome is painted by `atlas_shell::dock`.
//!
//! To add a tool: add a `DockItem` in [`floating_tools_dock`] and an arm in
//! the panel-body match. Renaming a tool = changing its `label`. Icon
//! visibility is toggled from the app-icon portal (Preferences).

use super::super::{AtlasApp, DateFilterField, EditMode, FilterMode};
#[cfg(test)]
use super::super::{MapView, ViewCmd};
use crate::app::chrome::ToolPanel;
use atlas_core::types::{ExtGroup, FAMILIES};
use atlas_shell::dock::{floating_dock, DockIcon, DockItem, DockItemKind};
use atlas_shell::sidebar::{
    sidebar_checkbox_row, sidebar_family_master_row, sidebar_fold_region, sidebar_mode_row,
    sidebar_nested_checkbox_row, sidebar_subtle_divider, SidebarTheme, SidebarTokens,
};
use eframe::egui::{self, Id, RichText};

fn sidebar_theme(app: &AtlasApp) -> SidebarTheme {
    let p = app.palette();
    SidebarTheme {
        card: p.card,
        border: p.border,
        ink: p.ink,
        sub: p.sub,
    }
}

pub fn floating_tools_dock(app: &mut AtlasApp, ctx: &egui::Context) {
    let chrome = app.active_chrome().clone();
    // All File Atlas dock icons are Dashboard forms, including Filters. List them
    // as one contiguous group — no visible separator (see DOCK.md).
    let items = [
        DockItem {
            id: "filters",
            label: "Basic filters",
            description: "Search, file types, owners, dates, and duplicate hiding.",
            icon: DockIcon::Filters,
            kind: DockItemKind::Dashboard,
            active: app.any_filter,
            visible: chrome.tool(ToolPanel::BasicFilters),
            gap_before: false,
        },
        DockItem {
            id: "display",
            label: "Display settings",
            description: "Tree or packed image sheet, density, group previews, and fit controls.",
            icon: DockIcon::Display,
            kind: DockItemKind::Dashboard,
            active: false,
            visible: chrome.tool(ToolPanel::DisplaySettings),
            gap_before: false,
        },
        DockItem {
            id: "mode",
            label: "Mode",
            description: "Switch between safe browsing and Explorer-style file edits.",
            icon: if app.edit_mode == EditMode::Edit {
                DockIcon::ModeEdit
            } else {
                DockIcon::Mode
            },
            kind: DockItemKind::Dashboard,
            active: app.edit_mode == EditMode::Edit,
            visible: chrome.tool(ToolPanel::Mode),
            gap_before: false,
        },
        DockItem {
            id: "workflow",
            label: "Workflow",
            description: "Focus the canvas on unassigned files during export prep.",
            icon: DockIcon::Workflow,
            kind: DockItemKind::Dashboard,
            active: app.only_unassigned,
            visible: chrome.tool(ToolPanel::Workflow),
            gap_before: false,
        },
        DockItem {
            id: "ai",
            label: "AI · Cursor",
            description: "Launch Cursor against the AI workspace and live link.",
            icon: DockIcon::Ai,
            kind: DockItemKind::Dashboard,
            active: false,
            visible: chrome.tool(ToolPanel::Ai),
            gap_before: false,
        },
    ];
    let palette = app.palette();
    let theme = sidebar_theme(app);
    let canvas = app.canvas_rect;
    let restore = app.dock_pins.clone();
    let restore_strips = app.dock_icon_strips.clone();
    let restore_hidden = app.dock_strip_hidden.clone();
    let restore_order = app.dock_strip_order.clone();
    let _ = floating_dock(
        ctx,
        "file_atlas_tools",
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
            "filters" => basic_filters_body(app, ui, theme),
            "display" => super::super::display_settings::display_settings_body(app, ui, ctx, theme),
            "mode" => mode_body(app, ui, theme),
            "workflow" => workflow_body(app, ui),
            "ai" => {
                atlas_ai::ui::ai_body(&mut app.ai, ui, theme);
            }
            _ => {}
        },
    );

    // Persist pinned palettes and flyout layout across sessions.
    let mut prefs_dirty = false;
    if let Some(pins) = atlas_shell::dock::pinned_ids(ctx, "file_atlas_tools") {
        if pins != app.dock_pins {
            app.dock_pins = pins;
            prefs_dirty = true;
        }
    }
    if let Some(strips) = atlas_shell::dock::icon_strip_ids(ctx, "file_atlas_tools") {
        if strips != app.dock_icon_strips {
            app.dock_icon_strips = strips;
            prefs_dirty = true;
        }
    }
    if let Some(hidden) = atlas_shell::dock::strip_hidden(ctx, "file_atlas_tools") {
        if hidden != app.dock_strip_hidden {
            app.dock_strip_hidden = hidden;
            prefs_dirty = true;
        }
    }
    if let Some(order) = atlas_shell::dock::strip_order(ctx, "file_atlas_tools") {
        if order != app.dock_strip_order {
            app.dock_strip_order = order;
            prefs_dirty = true;
        }
    }
    if let Some(collapsed) = atlas_shell::dock::bar_collapsed(ctx, "file_atlas_tools") {
        if collapsed != app.dock_bar_collapsed {
            app.dock_bar_collapsed = collapsed;
            prefs_dirty = true;
        }
    }
    if prefs_dirty {
        app.save_chrome_prefs();
    }
}

fn mode_body(app: &mut AtlasApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    ui.label(
        RichText::new("View is safe browsing. Edit unlocks real filesystem changes.")
            .small()
            .color(theme.sub),
    );
    ui.add_space(6.0);
    if sidebar_mode_row(
        ui,
        app.edit_mode == EditMode::View,
        "view",
        "browse only",
        "No rename, move, copy, folder creation, or delete operations are allowed.",
        theme,
    )
    .clicked()
    {
        app.set_edit_mode(EditMode::View);
        app.push_history("atlas.mode_view", None);
    }
    if sidebar_mode_row(
        ui,
        app.edit_mode == EditMode::Edit,
        "edit",
        "Explorer-style file edits",
        "Allows human-directed rename, move, copy, new folder, and delete operations.",
        theme,
    )
    .clicked()
    {
        app.set_edit_mode(EditMode::Edit);
        app.push_history("atlas.mode_edit", None);
    }
}

fn basic_filters_body(app: &mut AtlasApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    // Stable id: Ctrl+F (`canvas.search`) targets this field; Esc while it
    // has focus hands focus back to the canvas (see `hotkeys`).
    let search = egui::TextEdit::singleline(&mut app.search)
        .id(Id::new("atlas_filters_search"))
        .hint_text("Search names…")
        .desired_width(ui.available_width());
    let resp = ui.add(search);
    if resp.changed() {
        app.filter_dirty = true;
    }
    app.search_field_frame = app.frame_no;
    if app.focus_search_field {
        resp.request_focus();
        app.focus_search_field = false;
        app.search_popup_open = false;
    }
    ui.add_space(4.0);

    // Large dashboard: subsections start collapsed (□); expand with the
    // Windows-style caption control (see TOOLBARS.md).
    sidebar_fold_region(
        ui,
        Id::new("filters_file_types"),
        "Filter by file types",
        false,
        theme,
        |ui| {
            let mut family_counts = [0usize; 10];
            let mut group_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for e in app.entries.iter().filter(|e| !e.dead) {
                family_counts[e.family.idx()] += 1;
                if let Some(label) = e.family.ext_group_label(&e.ext) {
                    *group_counts
                        .entry(format!("{}:{}", e.family.idx(), label))
                        .or_insert(0) += 1;
                }
            }

            for fam in FAMILIES {
                let i = fam.idx();
                if family_counts[i] == 0 {
                    continue;
                }
                let visible_groups: Vec<(&ExtGroup, usize)> = fam
                    .ext_groups()
                    .iter()
                    .filter_map(|group| {
                        let count = group_counts
                            .get(&format!("{}:{}", i, group.label))
                            .copied()
                            .unwrap_or(0);
                        (count > 0).then_some((group, count))
                    })
                    .collect();
                let has_subtypes = !visible_groups.is_empty();
                let expand_id = ui.id().with("fam_expand").with(i);
                let mut expanded = ui.data(|d| d.get_temp::<bool>(expand_id)).unwrap_or(false);

                let label = format!(
                    "{} ({})",
                    fam.label(),
                    super::group_digits(family_counts[i] as u64)
                );
                if sidebar_family_master_row(
                    ui,
                    &mut expanded,
                    has_subtypes,
                    &mut app.family_on[i],
                    fam.color(),
                    &label,
                    theme,
                ) {
                    if app.family_on[i] {
                        app.set_family_ext_groups(fam, true);
                    }
                    app.filter_dirty = true;
                }
                ui.data_mut(|d| d.insert_temp(expand_id, expanded));

                if has_subtypes && expanded {
                    ui.indent(expand_id, |ui| {
                        for (group, count) in visible_groups {
                            let mut on = app.ext_group_enabled(fam, group);
                            let sub_label =
                                format!("{} ({})", group.label, super::group_digits(count as u64));
                            if sidebar_nested_checkbox_row(ui, &mut on, sub_label) {
                                app.set_ext_group(fam, group, on);
                                app.filter_dirty = true;
                            }
                        }
                    });
                }
                ui.add_space(2.0);
            }

            ui.horizontal(|ui| {
                if ui.small_button("all").clicked() {
                    app.family_on = [true; 10];
                    app.set_all_ext_groups(true);
                    app.filter_dirty = true;
                }
                if ui.small_button("none").clicked() {
                    app.family_on = [false; 10];
                    app.filter_dirty = true;
                }
            });
        },
    );

    if !app.all_owners.is_empty() {
        sidebar_subtle_divider(ui, theme);
        sidebar_fold_region(
            ui,
            Id::new("filters_owners"),
            "Filter by owner",
            false,
            theme,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = SidebarTokens::OPTION_GAP;
                    ui.spacing_mut().item_spacing.y = SidebarTokens::ROW_GAP;
                    let owners: Vec<(String, usize)> = app
                        .all_owners
                        .iter()
                        .map(|(o, c)| (o.clone(), *c))
                        .collect();
                    for (owner, count) in owners {
                        let mut on = app.owner_filter.contains(&owner);
                        let label = format!("{owner} ({})", super::group_digits(count as u64));
                        if ui.checkbox(&mut on, label).changed() {
                            if on {
                                app.owner_filter.insert(owner);
                            } else {
                                app.owner_filter.remove(&owner);
                            }
                            app.filter_dirty = true;
                        }
                    }
                });
                if !app.owner_filter.is_empty() && ui.small_button("clear owner filter").clicked() {
                    app.owner_filter.clear();
                    app.filter_dirty = true;
                }
            },
        );
    }

    sidebar_subtle_divider(ui, theme);

    sidebar_fold_region(
        ui,
        Id::new("filters_dates"),
        "Filter by dates",
        false,
        theme,
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = SidebarTokens::OPTION_GAP;
                if ui
                    .selectable_label(app.date_field == DateFilterField::Created, "created")
                    .on_hover_text("Filter by file creation date")
                    .clicked()
                {
                    app.date_field = DateFilterField::Created;
                    app.filter_dirty = true;
                }
                if ui
                    .selectable_label(app.date_field == DateFilterField::Modified, "modified")
                    .on_hover_text("Filter by last modified date")
                    .clicked()
                {
                    app.date_field = DateFilterField::Modified;
                    app.filter_dirty = true;
                }
            });
            ui.add_space(2.0);
            // One time axis only: the window itself is chosen on the bottom
            // bar's activity timeline, where the handles sit under the buckets
            // they select. A second slider here would be a second scale for
            // the same state.
            ui.label(
                RichText::new(if app.time_selection_active() {
                    format!(
                        "window: {}",
                        atlas_core::types::timeline_range_caption(
                            app.date_range_lo,
                            app.date_range_hi,
                            atlas_core::types::SECS_PER_DAY,
                        )
                    )
                } else {
                    "window: all dates".to_string()
                })
                .small()
                .color(theme.sub),
            );
            ui.label(
                RichText::new("set it on the activity timeline (bottom bar)")
                    .small()
                    .color(theme.sub),
            );
            if app.time_selection_active() && ui.small_button("clear selection").clicked() {
                app.reset_time_selection();
            }
        },
    );

    sidebar_subtle_divider(ui, theme);

    if sidebar_checkbox_row(ui, &mut app.auto_zoom_matches, "Zoom to matches") {
        // Switching this on frames the current result immediately instead of
        // waiting for the next filter change.
        app.refit_matches_now();
    }
    ui.label(
        RichText::new("camera follows the filter, and fits the map again when it clears")
            .small()
            .color(theme.sub),
    );

    sidebar_subtle_divider(ui, theme);

    sidebar_fold_region(
        ui,
        Id::new("filters_duplicates"),
        "Duplicates",
        false,
        theme,
        |ui| {
            if sidebar_checkbox_row(
                ui,
                &mut app.dedupe_twins,
                "Hide duplicate files (same name & size, keep newest)",
            ) {
                app.filter_dirty = true;
            }
        },
    );

    sidebar_subtle_divider(ui, theme);

    sidebar_fold_region(
        ui,
        Id::new("filters_display_mode"),
        "Display",
        false,
        theme,
        |ui| {
            if sidebar_mode_row(
                ui,
                app.filter_mode == FilterMode::Ghost,
                "ghost",
                "Dim unchecked items on the canvas",
                "Keep every file and folder in place, but fade items that fail the current filters. \
             Useful when you want spatial context while focusing on a subset.",
                theme,
            )
            .clicked()
            {
                app.filter_mode = FilterMode::Ghost;
                app.filter_dirty = true;
            }
            if sidebar_mode_row(
                ui,
                app.filter_mode == FilterMode::Hide,
                "hide",
                "Remove unchecked items from the layout",
                "Collapse the tree around items that pass the filters so hidden files no longer \
             consume space. Folders with no visible children shrink away until filters change.",
                theme,
            )
            .clicked()
            {
                app.filter_mode = FilterMode::Hide;
                app.filter_dirty = true;
            }
        },
    );
}

fn workflow_body(app: &mut AtlasApp, ui: &mut egui::Ui) {
    if sidebar_checkbox_row(ui, &mut app.only_unassigned, "Unassigned only") {
        app.filter_dirty = true;
    }
}

#[cfg(test)]
mod tests;
