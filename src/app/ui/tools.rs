//! Left tools rail — canvas actions, filters, display settings.
//! Optional sub-panels are toggled from the gear menu (`chrome::ToolPanel`).

use super::super::{AtlasApp, DragChip, FilterMode, LeaderStyle, Orient, ViewCmd};
use super::widgets::{chip, gear_menu, thin_sidebar_slider};
use crate::app::chrome::ToolPanel;
use crate::types::FAMILIES;
use eframe::egui::{self, Color32};

fn tools_gear(app: &mut AtlasApp, ui: &mut egui::Ui) {
    gear_menu(ui, "tools_gear", |ui| {
        ui.label(
            egui::RichText::new("Visible tool panels")
                .small()
                .strong(),
        );
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
    egui::SidePanel::left("tools_rail")
        .resizable(true)
        .default_width(200.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                tools_gear(app, ui);
                ui.label(
                    egui::RichText::new("Tools")
                        .small()
                        .color(Color32::from_gray(120)),
                );
            });
            ui.add_space(4.0);

            if chrome.tool(ToolPanel::BasicFilters) {
                basic_filters(app, ui);
            }
            if chrome.tool(ToolPanel::DisplaySettings) {
                display_settings(app, ui, ctx);
            }
            if chrome.tool(ToolPanel::Workflow) {
                workflow(app, ui);
            }
            if chrome.tool(ToolPanel::Tags) {
                tags_panel(app, ui);
            }
        });
}

fn basic_filters(app: &mut AtlasApp, ui: &mut egui::Ui) {
    ui.strong("Basic filters");
    ui.add_space(2.0);
    let search = egui::TextEdit::singleline(&mut app.search)
        .hint_text("Search names…")
        .desired_width(ui.available_width());
    if ui.add(search).changed() {
        app.filter_dirty = true;
    }
    ui.add_space(4.0);

    let mut counts = [0usize; 10];
    for e in app.entries.iter().filter(|e| !e.dead) {
        counts[e.family.idx()] += 1;
    }
    for fam in FAMILIES {
        let i = fam.idx();
        if counts[i] == 0 {
            continue;
        }
        ui.horizontal(|ui| {
            let mut on = app.family_on[i];
            let swatch = egui::RichText::new("■").color(fam.color());
            if ui.checkbox(&mut on, "").changed() {
                app.family_on[i] = on;
                app.filter_dirty = true;
            }
            ui.label(swatch);
            ui.label(format!(
                "{} ({})",
                fam.label(),
                super::group_digits(counts[i] as u64)
            ));
        });
    }
    ui.horizontal(|ui| {
        if ui.small_button("all").clicked() {
            app.family_on = [true; 10];
            app.filter_dirty = true;
        }
        if ui.small_button("none").clicked() {
            app.family_on = [false; 10];
            app.filter_dirty = true;
        }
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Unchecked:");
        let ghost = ui
            .selectable_label(app.filter_mode == FilterMode::Ghost, "ghost")
            .on_hover_text("Dim unchecked categories, but keep their positions");
        let hide = ui
            .selectable_label(app.filter_mode == FilterMode::Hide, "hide")
            .on_hover_text("Remove unchecked categories from the canvas layout");
        if ghost.clicked() {
            app.filter_mode = FilterMode::Ghost;
            app.filter_dirty = true;
        }
        if hide.clicked() {
            app.filter_mode = FilterMode::Hide;
            app.filter_dirty = true;
        }
    });
    ui.separator();
}

fn display_settings(app: &mut AtlasApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.strong("Display settings");
    ui.add_space(2.0);

    ui.horizontal(|ui| {
        if ui.button("Fit").on_hover_text("F").clicked() {
            app.pending_view = Some(ViewCmd::Fit);
        }
        let orient_txt = match app.orient {
            Orient::V => "Flow →",
            Orient::H => "Flow ↓",
        };
        if ui
            .button(orient_txt)
            .on_hover_text("Toggle branch direction")
            .clicked()
        {
            app.orient = match app.orient {
                Orient::V => Orient::H,
                Orient::H => Orient::V,
            };
            app.relayout();
            app.pending_view = Some(ViewCmd::Fit);
        }
        let mut dark = app.dark_mode;
        if ui.checkbox(&mut dark, "Dark").changed() {
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
    });

    let mut layout_changed = false;
    layout_changed |= thin_sidebar_slider(
        ui,
        &mut app.grid_cols,
        2..=30,
        "grid columns",
        "wide",
        "Maximum controlled dimension of thumbnail grids",
    );
    layout_changed |= thin_sidebar_slider(
        ui,
        &mut app.portal_threshold,
        10..=1000,
        "portal threshold",
        "items",
        "Child-count threshold where collapsed folders become group previews",
    );
    layout_changed |= thin_sidebar_slider(
        ui,
        &mut app.row_spacing,
        40..=300,
        "row spacing",
        "%",
        "Offset between row datums (distance between depth levels)",
    );
    if ui
        .checkbox(
            &mut app.align_groups_to_lowest,
            "align image groups to lowest datum",
        )
        .on_hover_text("Create a clean horizontal datum from the lowest image group in each branch")
        .changed()
    {
        layout_changed = true;
    }
    ui.label(
        egui::RichText::new("leader lines")
            .small()
            .color(Color32::from_gray(120)),
    );
    ui.horizontal(|ui| {
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
    if layout_changed {
        app.grid_cols = app.grid_cols.clamp(2, 30);
        app.portal_threshold = app.portal_threshold.clamp(10, 10_000);
        app.row_spacing = app.row_spacing.clamp(40, 300);
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
    ui.separator();
}

fn workflow(app: &mut AtlasApp, ui: &mut egui::Ui) {
    ui.strong("Workflow");
    if ui.checkbox(&mut app.only_untagged, "Untagged only").changed() {
        app.filter_dirty = true;
    }
    if ui
        .checkbox(&mut app.only_unassigned, "Unassigned only")
        .changed()
    {
        app.filter_dirty = true;
    }
    ui.separator();
}

fn tags_panel(app: &mut AtlasApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.strong("Tags");
        ui.label(
            egui::RichText::new("(click filters · drag onto files)")
                .small()
                .color(Color32::from_gray(120)),
        );
    });
    ui.add_space(2.0);
    egui::ScrollArea::vertical().show(ui, |ui| {
        let tags: Vec<(String, usize)> = app
            .all_tags
            .iter()
            .map(|(t, c)| (t.clone(), *c))
            .collect();
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
