//! File Atlas Display controls and their session-local layout domains.
use super::{AtlasApp, FolderHeatMode, LeaderStyle, MapView, Orient, ViewCmd};
use atlas_core::pack_sheet::SheetSort;
use atlas_core::tree::LayoutConfig;
use atlas_shell::sidebar::{
    sidebar_fold_region, sidebar_mode_row, sidebar_option_group, sidebar_slider_block,
    sidebar_toolbar_row, SidebarTheme,
};
use atlas_shell::widgets::{thin_sidebar_slider, SliderDomain};
use eframe::egui::{self, RichText};

pub(super) fn default_domains() -> [SliderDomain; 3] {
    [
        SliderDomain::new(2..=30),
        SliderDomain::new(10..=1000),
        SliderDomain::new(40..=300),
    ]
}

impl AtlasApp {
    pub(crate) fn layout_config(&self) -> LayoutConfig {
        LayoutConfig {
            grid_cols: self.grid_cols,
            grid_cols_max: self.display_slider_domains[0]
                .maximum()
                .max(LayoutConfig::default().grid_cols_max),
            portal_threshold: self.portal_threshold,
            portal_threshold_max: self.display_slider_domains[1]
                .maximum()
                .max(LayoutConfig::default().portal_threshold_max),
            align_groups_to_lowest: self.align_groups_to_lowest,
            row_spacing: self.row_spacing,
            row_spacing_max: self.display_slider_domains[2]
                .maximum()
                .max(LayoutConfig::default().row_spacing_max),
            root_folder_gap: self.root_folder_gap,
        }
        .normalized()
    }
}

pub(super) fn display_settings_body(
    app: &mut AtlasApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    theme: SidebarTheme,
) {
    sidebar_option_group(ui, "layout", theme, |ui| {
        if ui
            .selectable_label(app.map_view == MapView::Tree, "tree")
            .on_hover_text("Folder map with leaders and cards")
            .clicked()
        {
            app.set_map_view(MapView::Tree);
        }
        if ui
            .selectable_label(app.map_view == MapView::Packed, "packed")
            .on_hover_text("Flush grid of images — no borders or fillets")
            .clicked()
        {
            app.set_map_view(MapView::Packed);
        }
    });
    if app.map_view == MapView::Packed {
        sidebar_option_group(ui, "sort", theme, |ui| {
            for sort in [
                SheetSort::Name,
                SheetSort::Modified,
                SheetSort::Created,
                SheetSort::Size,
                SheetSort::Type,
            ] {
                if ui
                    .selectable_label(app.sheet_sort == sort, sort.label())
                    .clicked()
                {
                    app.set_sheet_sort(sort);
                }
            }
        });
        let mut aspect = app.sheet_aspect_pct;
        sidebar_slider_block(ui, |ui| {
            if thin_sidebar_slider(
                ui,
                &mut aspect,
                25..=400,
                "field aspect",
                "%",
                "Width of the packed field relative to its height (100% is square)",
                theme.sub,
            ) {
                app.set_sheet_aspect_pct(aspect);
            }
        });
    }

    sidebar_toolbar_row(ui, |ui| {
        if ui.button("Fit").on_hover_text("F").clicked() {
            app.pending_view = Some(ViewCmd::Fit);
        }
        let orient_txt = match app.orient {
            Orient::V => "Flow direction →",
            Orient::H => "Flow direction ↓",
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
            app.set_dark_mode(dark, ctx);
            if let Some(session) = &app.session {
                if let Ok(mut s) = session.lock() {
                    s.dark_mode = dark;
                }
            }
        }
    });

    let mut layout_changed = false;
    sidebar_slider_block(ui, |ui| {
        layout_changed |= thin_sidebar_slider(
            ui,
            &mut app.grid_cols,
            &mut app.display_slider_domains[0],
            "grid columns",
            "wide",
            "Maximum controlled dimension of thumbnail grids",
            theme.sub,
        );
    });
    sidebar_slider_block(ui, |ui| {
        layout_changed |= thin_sidebar_slider(
            ui,
            &mut app.portal_threshold,
            &mut app.display_slider_domains[1],
            "stack threshold",
            "items",
            "Collapsed folders over this many items become a group preview (tree) or one cover tile (packed)",
            theme.sub,
        );
    });
    sidebar_slider_block(ui, |ui| {
        layout_changed |= thin_sidebar_slider(
            ui,
            &mut app.row_spacing,
            &mut app.display_slider_domains[2],
            "row spacing",
            "%",
            "Offset between row datums (distance between depth levels)",
            theme.sub,
        );
    });
    sidebar_fold_region(
        ui,
        egui::Id::new("display_file_atlas_layout"),
        "File Atlas layout",
        true,
        theme,
        |ui| {
            sidebar_slider_block(ui, |ui| {
                layout_changed |= thin_sidebar_slider(
                    ui,
                    &mut app.root_folder_gap,
                    40..=300,
                    "root folder gap",
                    "%",
                    "Breadth spacing between root-level folders, independent of depth row spacing",
                    theme.sub,
                );
            });
            let mut lod_changed = false;
            sidebar_slider_block(ui, |ui| {
                lod_changed |= thin_sidebar_slider(
                    ui,
                    &mut app.lod_mid,
                    1..=100,
                    "LOD mid",
                    "%",
                    "Zoom threshold where folder/file cards add metadata",
                    theme.sub,
                );
            });
            sidebar_slider_block(ui, |ui| {
                lod_changed |= thin_sidebar_slider(
                    ui,
                    &mut app.lod_full,
                    2..=200,
                    "LOD full",
                    "%",
                    "Zoom threshold where thumbnails and creator/owner detail appear",
                    theme.sub,
                );
            });
            sidebar_slider_block(ui, |ui| {
                lod_changed |= thin_sidebar_slider(
                    ui,
                    &mut app.lod_detail,
                    50..=3200,
                    "LOD detail",
                    "%",
                    "Deep zoom: full file paths and a short paragraph of details on the tag",
                    theme.sub,
                );
            });
            if lod_changed {
                app.lod_mid = app.lod_mid.clamp(1, 100);
                app.lod_full = app.lod_full.clamp(app.lod_mid + 1, 200);
                app.lod_detail = app.lod_detail.clamp(app.lod_full + 1, 3200);
                ctx.request_repaint();
            }
        },
    );

    sidebar_fold_region(
        ui,
        egui::Id::new("display_folder_heatmap"),
        "Folder heatmap",
        true,
        theme,
        |ui| {
            ui.label(
                RichText::new(
                    "Colour folders by a robust metric. Medians and percentile scaling keep a few \
                     ancient admin files from flattening the ramp.",
                )
                .small()
                .color(theme.sub),
            );
            ui.add_space(4.0);
            let modes = [
                (
                    FolderHeatMode::Off,
                    "off",
                    "Ordinary folder chrome — no heatmap.",
                ),
                (
                    FolderHeatMode::Size,
                    "size",
                    "Total descendant bytes. Log-scaled so a single huge archive saturates high \
                     without collapsing smaller folders.",
                ),
                (
                    FolderHeatMode::Created,
                    "created",
                    "Median file creation time in each folder (P10–P90 across the tree).",
                ),
                (
                    FolderHeatMode::Modified,
                    "modified",
                    "Median file modification time in each folder (P10–P90 across the tree).",
                ),
            ];
            for (mode, label, tip) in modes {
                if sidebar_mode_row(
                    ui,
                    app.folder_heat_mode == mode,
                    label,
                    match mode {
                        FolderHeatMode::Off => "no colouring",
                        FolderHeatMode::Size => "by total bytes",
                        FolderHeatMode::Created => "by median created",
                        FolderHeatMode::Modified => "by median modified",
                    },
                    tip,
                    theme,
                )
                .clicked()
                {
                    app.set_folder_heat_mode(mode);
                    ctx.request_repaint();
                }
            }
        },
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
    ui.add_space(4.0);

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

    if layout_changed {
        app.grid_cols = app.display_slider_domains[0].clamp(app.grid_cols);
        app.portal_threshold = app.display_slider_domains[1].clamp(app.portal_threshold);
        app.row_spacing = app.display_slider_domains[2].clamp(app.row_spacing);
        app.root_folder_gap = app.root_folder_gap.clamp(40, 300);
        let cfg = app.layout_config();
        if let Some(t) = &mut app.tree {
            t.cfg = cfg;
            for d in t.dirs.iter_mut() {
                if d.child_dirs.len() + d.files.len() > cfg.portal_threshold {
                    d.collapsed = true;
                }
            }
        }
        // Moving the threshold is a deliberate re-decision, so it is recorded.
        app.record_collapse_state();
        app.relayout();
        app.rebuild_packed_sheet();
        if app.map_view == MapView::Packed {
            app.pending_view = Some(ViewCmd::Fit);
        }
    }
}
