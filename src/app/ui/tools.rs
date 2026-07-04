//! Left tools rail — canvas actions, filters, display settings.
//! Optional sub-panels are toggled from the gear menu (`chrome::ToolPanel`).

use super::super::{
    AtlasApp, DateFilterField, DateSliderMode, DragChip, FilterMode, LeaderStyle, Orient, ViewCmd,
};
use super::sidebar::{
    sidebar_action_block, sidebar_actions_column, sidebar_checkbox_row, sidebar_control_group,
    sidebar_family_row, sidebar_mode_row, sidebar_option_group, sidebar_region, sidebar_section,
    sidebar_sliders_group, sidebar_subtle_divider, SidebarTheme, SidebarTokens,
};
use super::widgets::{chip, gear_menu, sidebar_date_timeline, thin_sidebar_slider};
use crate::app::chrome::ToolPanel;
use crate::types::{ExtGroup, FAMILIES};
use eframe::egui::{self, Color32, Id};

fn sidebar_theme(app: &AtlasApp) -> SidebarTheme {
    let p = app.palette();
    SidebarTheme {
        card: p.card,
        border: p.border,
        ink: p.ink,
        sub: p.sub,
        line: p.line,
    }
}

fn tools_gear(app: &mut AtlasApp, ui: &mut egui::Ui) {
    gear_menu(ui, "tools_gear", |ui| {
        ui.label(egui::RichText::new("Visible tool panels").small().strong());
        ui.separator();
        for panel in ToolPanel::ALL {
            let mut on = app.active_chrome().tool(panel);
            if ui.checkbox(&mut on, panel.label()).changed() {
                app.active_chrome_mut().set_tool(panel, on);
            }
        }
        ui.separator();
        if ui.button("Advanced settings…").clicked() {
            app.active_chrome_mut().advanced_open = true;
            ui.close_menu();
        }
    });
}

pub fn left_panel(app: &mut AtlasApp, ctx: &egui::Context) {
    let chrome = app.active_chrome().clone();
    let theme = sidebar_theme(app);
    let mut first = true;
    egui::SidePanel::left("tools_rail")
        .resizable(true)
        .default_width(200.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                tools_gear(app, ui);
                ui.label(egui::RichText::new("Tools").small().color(theme.sub));
            });
            ui.add_space(1.0);

            if chrome.tool(ToolPanel::BasicFilters) {
                basic_filters(app, ui, theme, &mut first);
            }
            if chrome.tool(ToolPanel::DisplaySettings) {
                display_settings(app, ui, ctx, theme, &mut first);
            }
            if chrome.tool(ToolPanel::Workflow) {
                workflow(app, ui, theme, &mut first);
            }
            if chrome.tool(ToolPanel::Tags) {
                tags_panel(app, ui, theme, &mut first);
            }
        });
}

fn basic_filters(app: &mut AtlasApp, ui: &mut egui::Ui, theme: SidebarTheme, first: &mut bool) {
    let mut expanded = app.active_chrome().tool_expanded(ToolPanel::BasicFilters);
    if sidebar_section(
        ui,
        Id::new("tools_basic_filters"),
        "Basic filters",
        None,
        &mut expanded,
        theme,
        *first,
        |ui| basic_filters_body(app, ui, theme),
    ) {
        app.active_chrome_mut()
            .set_tool_expanded(ToolPanel::BasicFilters, expanded);
    }
    *first = false;
}

fn basic_filters_body(app: &mut AtlasApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let search = egui::TextEdit::singleline(&mut app.search)
        .hint_text("Search names…")
        .desired_width(ui.available_width());
    if ui.add(search).changed() {
        app.filter_dirty = true;
    }
    ui.add_space(4.0);

    sidebar_region(ui, "File types", theme, |ui| {
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
            let label = format!(
                "{} ({})",
                fam.label(),
                super::group_digits(family_counts[i] as u64)
            );
            if sidebar_family_row(ui, &mut app.family_on[i], fam.color(), &label) {
                if app.family_on[i] {
                    app.set_family_ext_groups(fam, true);
                }
                app.filter_dirty = true;
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
            if visible_groups.is_empty() {
                continue;
            }

            ui.indent(format!("fam_ext_{i}"), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = SidebarTokens::OPTION_GAP;
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (group, count) in visible_groups {
                        let mut on = app.ext_group_enabled(fam, group);
                        let sub_label =
                            format!("{} ({})", group.label, super::group_digits(count as u64));
                        if ui.checkbox(&mut on, sub_label).changed() {
                            app.set_ext_group(fam, group, on);
                            app.filter_dirty = true;
                        }
                    }
                });
            });
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
    });

    if !app.all_owners.is_empty() {
        sidebar_subtle_divider(ui, theme);
        sidebar_region(ui, "Owner", theme, |ui| {
            egui::ScrollArea::vertical()
                .max_height(72.0)
                .show(ui, |ui| {
                    let owners: Vec<(String, usize)> = app
                        .all_owners
                        .iter()
                        .map(|(o, c)| (o.clone(), *c))
                        .collect();
                    for (owner, count) in owners {
                        let active = app.owner_filter.contains(&owner);
                        let label = format!("{owner} ({})", super::group_digits(count as u64));
                        let resp = chip(ui, &label, active, Color32::from_rgb(0x5c, 0x6b, 0x8a));
                        if resp.clicked() {
                            if active {
                                app.owner_filter.remove(&owner);
                            } else {
                                app.owner_filter.insert(owner);
                            }
                            app.filter_dirty = true;
                        }
                    }
                    if !app.owner_filter.is_empty()
                        && ui.small_button("clear owner filter").clicked()
                    {
                        app.owner_filter.clear();
                        app.filter_dirty = true;
                    }
                });
        });
    }

    sidebar_subtle_divider(ui, theme);

    sidebar_region(ui, "Dates", theme, |ui| {
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
        if sidebar_date_timeline(
            ui,
            Id::new("basic_date_timeline"),
            app.date_span_min,
            app.date_span_max,
            &mut app.date_mode,
            &mut app.date_single_day,
            &mut app.date_range_lo,
            &mut app.date_range_hi,
            &mut app.date_filter_engaged,
            theme,
        ) {
            app.filter_dirty = true;
        }
    });

    sidebar_subtle_divider(ui, theme);

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
}

fn display_settings(
    app: &mut AtlasApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    theme: SidebarTheme,
    first: &mut bool,
) {
    let mut expanded = app
        .active_chrome()
        .tool_expanded(ToolPanel::DisplaySettings);
    if sidebar_section(
        ui,
        Id::new("tools_display_settings"),
        "Display settings",
        None,
        &mut expanded,
        theme,
        *first,
        |ui| display_settings_body(app, ui, ctx, theme),
    ) {
        app.active_chrome_mut()
            .set_tool_expanded(ToolPanel::DisplaySettings, expanded);
    }
    *first = false;
}

fn display_settings_body(
    app: &mut AtlasApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    theme: SidebarTheme,
) {
    let mut layout_changed = false;

    sidebar_control_group(ui, theme, false, |ui| {
        sidebar_actions_column(ui, |ui| {
            sidebar_action_block(
                ui,
                theme,
                "Fit the entire canvas in the current view (F)",
                |ui| {
                    if ui
                        .button("Fit")
                        .on_hover_text("Fit the entire canvas in the current view (F)")
                        .clicked()
                    {
                        app.pending_view = Some(ViewCmd::Fit);
                    }
                },
            );
            let orient_txt = match app.orient {
                Orient::V => "Flow →",
                Orient::H => "Flow ↓",
            };
            sidebar_action_block(
                ui,
                theme,
                "Toggle branch flow direction (horizontal ↔ vertical)",
                |ui| {
                    if ui
                        .button(orient_txt)
                        .on_hover_text("Toggle branch flow direction (horizontal ↔ vertical)")
                        .clicked()
                    {
                        app.orient = match app.orient {
                            Orient::V => Orient::H,
                            Orient::H => Orient::V,
                        };
                        app.relayout();
                        app.pending_view = Some(ViewCmd::Fit);
                    }
                },
            );
        });
    });

    sidebar_control_group(ui, theme, true, |ui| {
        sidebar_sliders_group(ui, |ui| {
            let domains = &mut app.display_slider_domains;
            layout_changed |= thin_sidebar_slider(
                ui,
                Id::new("slider_grid_cols"),
                &mut app.grid_cols,
                &mut domains.grid_cols,
                "grid columns",
                "wide",
                "Maximum controlled dimension of thumbnail grids",
                theme.sub,
            );
            layout_changed |= thin_sidebar_slider(
                ui,
                Id::new("slider_portal"),
                &mut app.portal_threshold,
                &mut domains.portal_threshold,
                "portal threshold",
                "items",
                "Child-count threshold where collapsed folders become group previews",
                theme.sub,
            );
            layout_changed |= thin_sidebar_slider(
                ui,
                Id::new("slider_row_spacing"),
                &mut app.row_spacing,
                &mut domains.row_spacing,
                "row spacing",
                "%",
                "Offset between row datums. Right-click to raise the max above 300%.",
                theme.sub,
            );
        });
    });

    sidebar_control_group(ui, theme, true, |ui| {
        let mut dark = app.dark_mode;
        if ui
            .checkbox(&mut dark, "Dark")
            .on_hover_text("Switch between dark and light interface theme")
            .changed()
        {
            app.dark_mode = dark;
            ctx.set_theme(if dark {
                egui::ThemePreference::Dark
            } else {
                egui::ThemePreference::Light
            });
            ctx.set_visuals(if dark {
                crate::app::dark_visuals()
            } else {
                crate::app::light_visuals()
            });
        }
        if ui
            .checkbox(
                &mut app.align_groups_to_lowest,
                "align image groups to lowest datum",
            )
            .on_hover_text(
                "Create a clean horizontal datum from the lowest image group in each branch",
            )
            .changed()
        {
            layout_changed = true;
        }
    });

    sidebar_control_group(ui, theme, true, |ui| {
        sidebar_option_group(ui, "leader lines", theme, |ui| {
            if ui
                .selectable_label(app.leader_style == LeaderStyle::Bezier, "bezier")
                .clicked()
            {
                app.leader_style = LeaderStyle::Bezier;
            }
            if ui
                .selectable_label(app.leader_style == LeaderStyle::Orthogonal, "orthogonal")
                .clicked()
            {
                app.leader_style = LeaderStyle::Orthogonal;
            }
        });
    });

    if layout_changed {
        let d = &app.display_slider_domains;
        app.grid_cols = app
            .grid_cols
            .clamp(*d.grid_cols.start(), *d.grid_cols.end());
        app.portal_threshold = app
            .portal_threshold
            .clamp(*d.portal_threshold.start(), *d.portal_threshold.end());
        app.row_spacing = app
            .row_spacing
            .clamp(*d.row_spacing.start(), *d.row_spacing.end());
        let cfg = app.layout_config();
        if let Some(t) = &mut app.tree {
            t.cfg = cfg;
            for d in t.dirs.iter_mut() {
                if d.child_dirs.len() + d.files.len() > cfg.portal_threshold {
                    d.collapsed = true;
                }
            }
        }
        app.relayout();
    }
}

fn workflow(app: &mut AtlasApp, ui: &mut egui::Ui, theme: SidebarTheme, first: &mut bool) {
    let mut expanded = app.active_chrome().tool_expanded(ToolPanel::Workflow);
    if sidebar_section(
        ui,
        Id::new("tools_workflow"),
        "Workflow",
        None,
        &mut expanded,
        theme,
        *first,
        |ui| workflow_body(app, ui),
    ) {
        app.active_chrome_mut()
            .set_tool_expanded(ToolPanel::Workflow, expanded);
    }
    *first = false;
}

fn workflow_body(app: &mut AtlasApp, ui: &mut egui::Ui) {
    if sidebar_checkbox_row(ui, &mut app.only_untagged, "Untagged only") {
        app.filter_dirty = true;
    }
    if sidebar_checkbox_row(ui, &mut app.only_unassigned, "Unassigned only") {
        app.filter_dirty = true;
    }
}

fn tags_panel(app: &mut AtlasApp, ui: &mut egui::Ui, theme: SidebarTheme, first: &mut bool) {
    let mut expanded = app.active_chrome().tool_expanded(ToolPanel::Tags);
    if sidebar_section(
        ui,
        Id::new("tools_tags"),
        "Tags",
        Some("(click filters · drag onto files)"),
        &mut expanded,
        theme,
        *first,
        |ui| tags_panel_body(app, ui),
    ) {
        app.active_chrome_mut()
            .set_tool_expanded(ToolPanel::Tags, expanded);
    }
    *first = false;
}

fn tags_panel_body(app: &mut AtlasApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        let tags: Vec<(String, usize)> =
            app.all_tags.iter().map(|(t, c)| (t.clone(), *c)).collect();
        for (tag, count) in tags {
            let active = app.tag_filter.contains(&tag);
            let resp = chip(
                ui,
                &format!("{tag} ({count})"),
                active,
                Color32::from_rgb(0x37, 0x5a, 0x7a),
            );
            if resp.drag_started() {
                app.drag_chip = Some(DragChip::Tag(tag.clone()));
            }
            if resp.clicked() {
                if active {
                    app.tag_filter.remove(&tag);
                } else {
                    app.tag_filter.insert(tag.clone());
                }
                app.filter_dirty = true;
            }
        }
        if !app.tag_filter.is_empty() && ui.small_button("clear tag filter").clicked() {
            app.tag_filter.clear();
            app.filter_dirty = true;
        }
    });
}
