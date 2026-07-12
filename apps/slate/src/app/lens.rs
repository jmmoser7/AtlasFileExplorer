//! Lens view — interactive code-dependency graph over a codebase root.
//!
//! Analysis, layout, and overlay matching live in `code-lens`; this module
//! owns UI state, background workers, painting, and sidebar controls.
//! `LensState` is app-wide today (could become per-tab later).

use super::SlateApp;
use code_lens::model::EdgeStats;
use code_lens::{
    analyze_workspace, layout_graph, match_cluster, CodeGraph, EdgeKind, ItemKind, LensBeacon,
    LensLayout, LensOverlay, LensWire, NodeId, NodeKind, Rectf,
};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Pos2, Rect, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};
use slate_doc::ViewKind;
use std::collections::HashSet;
use std::path::PathBuf;

const HEADER_STRIP_H: f32 = 28.0;
const CHIP_RADIUS: f32 = 6.0;
const CONTAINER_RADIUS: f32 = 8.0;

struct LensPaintStyle<'a> {
    alpha: f32,
    search_hit: bool,
    focused: bool,
    hovered: bool,
    expandable: bool,
    expanded: bool,
    cluster: Option<&'a code_lens::OverlayCluster>,
    palette: &'a atlas_shell::theme::Palette,
    z: f32,
}

struct LensFocusInfo {
    name: String,
    kind: &'static str,
    path: PathBuf,
    loc: u32,
    can_open: bool,
    expandable: bool,
    edge_stats: [(&'static str, EdgeStats); 3],
    package_cycle: Option<String>,
}

enum LensMsg {
    Ready { root: PathBuf, graph: CodeGraph },
    Error { root: PathBuf, msg: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LensStatus {
    Idle,
    Analyzing,
    Ready,
    Error(String),
}

/// Runtime Lens state (one instance on [`SlateApp`] for now).
pub struct LensState {
    analyze_rx: Option<Receiver<LensMsg>>,
    pub status: LensStatus,
    /// Which codebase root produced the current graph.
    graph_root: Option<PathBuf>,
    pub graph: Option<CodeGraph>,
    pub expanded: HashSet<NodeId>,
    pub focus: Option<NodeId>,
    pub hover: Option<NodeId>,
    layout: Option<LensLayout>,
    layout_dirty: bool,
    pub filter_package_dep: bool,
    pub filter_use: bool,
    pub filter_impl_trait: bool,
    pub search: String,
    pub beacon: LensBeacon,
    pub overlay: Option<LensOverlay>,
    /// Fit camera to layout bounds once after the first successful analysis.
    pending_auto_fit: bool,
    /// `SLATE_LENS_FOCUS` harness: applied once after the graph is ready.
    harness_focus_applied: bool,
}

impl Default for LensState {
    fn default() -> Self {
        Self {
            analyze_rx: None,
            status: LensStatus::Idle,
            graph_root: None,
            graph: None,
            expanded: HashSet::new(),
            focus: None,
            hover: None,
            layout: None,
            layout_dirty: true,
            filter_package_dep: true,
            filter_use: true,
            filter_impl_trait: true,
            search: String::new(),
            beacon: LensBeacon::new(),
            overlay: None,
            pending_auto_fit: false,
            harness_focus_applied: false,
        }
    }
}

impl SlateApp {
    /// Drain analysis results, maintain layout cache, poll AI beacon.
    pub(crate) fn lens_pump(&mut self, ctx: &egui::Context) {
        let mut drained = Vec::new();
        if let Some(rx) = self.lens.analyze_rx.as_ref() {
            while let Ok(msg) = rx.try_recv() {
                drained.push(msg);
            }
        }
        if !drained.is_empty() {
            self.lens.analyze_rx = None;
            for msg in drained {
                match msg {
                    LensMsg::Ready { root, graph } => {
                        self.lens.graph_root = Some(root);
                        self.lens.graph = Some(graph);
                        self.lens.expanded = default_expanded(self.lens.graph.as_ref().unwrap());
                        self.lens.status = LensStatus::Ready;
                        self.lens.layout_dirty = true;
                        self.lens.pending_auto_fit = true;
                        self.lens.focus = None;
                    }
                    LensMsg::Error { root, msg } => {
                        self.lens.graph_root = Some(root);
                        self.lens.graph = None;
                        self.lens.status = LensStatus::Error(msg);
                        self.lens.layout = None;
                    }
                }
            }
        }

        if self.doc().view.active_view == ViewKind::Lens {
            if let Some(root) = self.doc().lens_root.clone() {
                let busy = self.lens.analyze_rx.is_some()
                    || matches!(self.lens.status, LensStatus::Analyzing);
                let stale = self.lens.graph_root.as_ref() != Some(&root);
                if stale && !busy {
                    self.lens_start_analysis(root);
                }
            }
        }

        if matches!(self.lens.status, LensStatus::Analyzing) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        self.lens_tick_beacon();
        self.lens_ensure_layout();
        self.lens_apply_harness_focus();
        if self.lens.pending_auto_fit {
            if let Some(layout) = self.lens.layout.clone() {
                self.lens_fit_layout(&layout);
            }
            self.lens.pending_auto_fit = false;
        }
    }

    fn lens_tick_beacon(&mut self) {
        let Some(ws) = self.ai.config.valid_workspace().map(PathBuf::from) else {
            return;
        };
        let root = self.doc().lens_root.clone();
        let graph = self.lens.graph.clone();
        if let (Some(root), Some(graph)) = (root, graph) {
            if self.lens.status == LensStatus::Ready {
                self.lens.beacon.tick_write(&ws, &root, &graph);
            }
        }
        if let Some(ov) = self.lens.beacon.tick_read(&ws) {
            self.lens.overlay = Some(ov);
        }
    }

    fn lens_ensure_layout(&mut self) {
        if !self.lens.layout_dirty {
            return;
        }
        self.lens.layout = self
            .lens
            .graph
            .as_ref()
            .map(|g| layout_graph(g, &self.lens.expanded));
        self.lens.layout_dirty = false;
    }

    pub fn lens_choose_root_dialog(&mut self) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new().pick_folder();
            let _ = tx.send(super::PickerMsg::LensRoot(picked));
        });
    }

    pub fn lens_rescan(&mut self) {
        if let Some(root) = self.doc().lens_root.clone() {
            self.lens_start_analysis(root);
        }
    }

    fn lens_start_analysis(&mut self, root: PathBuf) {
        if self.lens.analyze_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.lens.analyze_rx = Some(rx);
        self.lens.status = LensStatus::Analyzing;
        std::thread::spawn(move || {
            let msg = match analyze_workspace(&root) {
                Ok(graph) => LensMsg::Ready { root, graph },
                Err(e) => LensMsg::Error {
                    root,
                    msg: e.to_string(),
                },
            };
            let _ = tx.send(msg);
        });
    }

    pub fn lens_set_depth_packages(&mut self) {
        let Some(graph) = self.lens.graph.as_ref() else {
            return;
        };
        let mut exp = HashSet::new();
        exp.insert(graph.root);
        self.lens.expanded = exp;
        self.lens.layout_dirty = true;
    }

    pub fn lens_set_depth_modules(&mut self) {
        let Some(graph) = self.lens.graph.as_ref() else {
            return;
        };
        let mut exp = HashSet::new();
        exp.insert(graph.root);
        for node in &graph.nodes {
            if matches!(node.kind, NodeKind::Package { .. }) {
                exp.insert(node.id);
            }
        }
        self.lens.expanded = exp;
        self.lens.layout_dirty = true;
    }

    pub fn lens_set_depth_items(&mut self) {
        let Some(graph) = self.lens.graph.as_ref() else {
            return;
        };
        let mut exp = HashSet::new();
        exp.insert(graph.root);
        for node in &graph.nodes {
            match node.kind {
                NodeKind::Package { .. } | NodeKind::Module => {
                    exp.insert(node.id);
                }
                _ => {}
            }
        }
        self.lens.expanded = exp;
        self.lens.layout_dirty = true;
    }

    pub(crate) fn lens_sidebar(&mut self, ui: &mut Ui, theme: atlas_shell::sidebar::SidebarTheme) {
        use atlas_shell::sidebar::{
            sidebar_region, sidebar_subtle_divider, sidebar_toolbar_row, SidebarTokens,
        };

        sidebar_region(ui, "Code root", theme, |ui| {
            match &self.doc().lens_root {
                Some(root) => {
                    let display = root.to_string_lossy();
                    let trunc = if display.len() > 42 {
                        format!("…{}", &display[display.len().saturating_sub(39)..])
                    } else {
                        display.into_owned()
                    };
                    ui.label(egui::RichText::new(trunc).small().color(theme.sub));
                }
                None => {
                    ui.label(
                        egui::RichText::new("No code root chosen")
                            .small()
                            .color(theme.sub),
                    );
                }
            }
            sidebar_toolbar_row(ui, |ui| {
                if ui.button("Choose…").clicked() {
                    self.lens_choose_root_dialog();
                }
                if ui
                    .button("Rescan")
                    .on_hover_text("Re-run analysis on the current root")
                    .clicked()
                {
                    self.lens_rescan();
                }
            });
            ui.label(
                egui::RichText::new(self.lens_status_line())
                    .small()
                    .color(theme.sub),
            );
            ui.label(
                egui::RichText::new("Source-derived · read-only")
                    .small()
                    .color(theme.sub),
            )
            .on_hover_text(
                "Lens visualizes analyzed source. It does not move, rename, or rewrite code yet.",
            );
        });

        sidebar_subtle_divider(ui, theme);
        sidebar_region(ui, "Depth", theme, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = SidebarTokens::OPTION_GAP;
                if ui.button("Packages").clicked() {
                    self.lens_set_depth_packages();
                }
                if ui.button("Modules").clicked() {
                    self.lens_set_depth_modules();
                }
                if ui.button("Items").clicked() {
                    self.lens_set_depth_items();
                }
            });
        });

        sidebar_subtle_divider(ui, theme);
        sidebar_region(ui, "Edges", theme, |ui| {
            ui.checkbox(&mut self.lens.filter_package_dep, "Package dependencies");
            ui.checkbox(&mut self.lens.filter_use, "Use / import");
            ui.checkbox(&mut self.lens.filter_impl_trait, "Trait implementations");
        });

        sidebar_subtle_divider(ui, theme);
        self.lens_wire_legend(ui, theme);

        if matches!(self.lens.status, LensStatus::Ready) {
            if let Some(graph) = self.lens.graph.clone() {
                sidebar_subtle_divider(ui, theme);
                self.lens_workspace_pressure(ui, theme, &graph);
            }
        }

        sidebar_subtle_divider(ui, theme);
        sidebar_region(ui, "Search", theme, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.lens.search)
                    .hint_text("Highlight by name…")
                    .desired_width(ui.available_width()),
            );
        });

        let focus_info = self.lens.focus.and_then(|id| {
            self.lens.graph.as_ref().map(|graph| {
                let node = graph.node(id);
                let package_cycle = graph
                    .ancestor_where(id, |candidate| {
                        matches!(graph.node(candidate).kind, NodeKind::Package { .. })
                    })
                    .and_then(|package| graph.package_cycle_containing(package))
                    .map(|cycle| {
                        cycle
                            .iter()
                            .map(|cycle_id| graph.node(*cycle_id).name.as_str())
                            .collect::<Vec<_>>()
                            .join(" → ")
                    });
                LensFocusInfo {
                    name: node.name.clone(),
                    kind: kind_label(node.kind),
                    path: node.path.clone(),
                    loc: node.loc,
                    can_open: matches!(node.kind, NodeKind::File | NodeKind::Item { .. }),
                    expandable: can_expand(node.kind),
                    edge_stats: edge_stats_by_kind(graph, id),
                    package_cycle,
                }
            })
        });
        if let Some(info) = focus_info {
            sidebar_subtle_divider(ui, theme);
            sidebar_region(ui, "Focused node", theme, |ui| {
                ui.label(egui::RichText::new(info.name).strong());
                ui.label(
                    egui::RichText::new(format!("{} · {} LOC", info.kind, info.loc))
                        .small()
                        .color(theme.sub),
                );
                if !info.path.as_os_str().is_empty() {
                    ui.label(
                        egui::RichText::new(info.path.display().to_string())
                            .small()
                            .color(theme.sub),
                    )
                    .on_hover_text(info.path.display().to_string());
                }
                let mut has_edges = false;
                for (label, stats) in info.edge_stats {
                    if stats.incoming_edges == 0 && stats.outgoing_edges == 0 {
                        continue;
                    }
                    has_edges = true;
                    ui.label(
                        egui::RichText::new(edge_stats_line(label, stats))
                            .small()
                            .color(theme.sub),
                    );
                }
                if !has_edges {
                    ui.label(
                        egui::RichText::new("No direct analyzed dependencies")
                            .small()
                            .color(theme.sub),
                    );
                }
                if let Some(cycle) = &info.package_cycle {
                    ui.label(
                        egui::RichText::new(format!("Package cycle: {cycle}"))
                            .small()
                            .color(self.palette().staged),
                    )
                    .on_hover_text(
                        "This package participates in a circular Cargo workspace dependency.",
                    );
                }
                sidebar_toolbar_row(ui, |ui| {
                    if info.can_open && ui.button("Open source").clicked() {
                        if let Some(root) = self.doc().lens_root.clone() {
                            Self::open_path(&root.join(&info.path));
                        }
                    }
                    if ui.button("Clear focus").clicked() {
                        self.lens.focus = None;
                    }
                });
                if info.can_open {
                    ui.label(
                        egui::RichText::new("Double-click the node to open it.")
                            .small()
                            .color(theme.sub),
                    );
                } else if info.expandable {
                    ui.label(
                        egui::RichText::new("Double-click the header to expand or collapse.")
                            .small()
                            .color(theme.sub),
                    );
                }
            });
        }

        if let Some(overlay) = &self.lens.overlay {
            if !overlay.clusters.is_empty() {
                sidebar_subtle_divider(ui, theme);
                sidebar_region(ui, "Overlay", theme, |ui| {
                    for cluster in &overlay.clusters {
                        ui.horizontal(|ui| {
                            let swatch = cluster.color.unwrap_or([128, 128, 128]);
                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(2),
                                Color32::from_rgb(swatch[0], swatch[1], swatch[2]),
                            );
                            ui.label(cluster.title.as_str());
                            resp.on_hover_text(&cluster.summary);
                        });
                    }
                });
            }
        }
    }

    fn lens_workspace_pressure(
        &mut self,
        ui: &mut Ui,
        theme: atlas_shell::sidebar::SidebarTheme,
        graph: &CodeGraph,
    ) {
        use atlas_shell::sidebar::sidebar_region;

        let summary = graph.workspace_summary();
        sidebar_region(ui, "Workspace pressure", theme, |ui| {
            let cycle_note = if summary.packages_in_cycles > 0 {
                format!(" · {} in cycles", summary.packages_in_cycles)
            } else {
                String::new()
            };
            ui.label(
                egui::RichText::new(format!(
                    "{} packages · {} nodes{cycle_note}",
                    summary.packages, summary.nodes
                ))
                .small()
                .color(theme.sub),
            );
            ui.label(
                egui::RichText::new(format!(
                    "Edges: {} pkg · {} use · {} trait",
                    summary.edges_package_dep, summary.edges_use, summary.edges_impl_trait
                ))
                .small()
                .color(theme.sub),
            );
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Top package fan-in").small().strong());
            self.lens_pressure_rows(ui, theme, graph, EdgeKind::PackageDep, true);
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Top use fan-in").small().strong());
            self.lens_pressure_rows(ui, theme, graph, EdgeKind::Use, true);
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Top package fan-out").small().strong());
            self.lens_pressure_rows(ui, theme, graph, EdgeKind::PackageDep, false);
        });
    }

    fn lens_pressure_rows(
        &mut self,
        ui: &mut Ui,
        theme: atlas_shell::sidebar::SidebarTheme,
        graph: &CodeGraph,
        kind: EdgeKind,
        fan_in: bool,
    ) {
        let rows = if fan_in {
            graph.top_packages_by_fan_in(kind, 3)
        } else {
            graph.top_packages_by_fan_out(kind, 3)
        };
        if rows.is_empty() || rows.iter().all(|row| row.fan_in == 0 && row.fan_out == 0) {
            ui.label(
                egui::RichText::new("No cross-package links")
                    .small()
                    .color(theme.sub),
            );
            return;
        }
        for row in rows {
            let metric = if fan_in { row.fan_in } else { row.fan_out };
            if metric == 0 {
                continue;
            }
            let name = graph.node(row.id).name.as_str();
            let label = format!("{name} · {metric}");
            if ui
                .selectable_label(self.lens.focus == Some(row.id), label)
                .clicked()
            {
                self.lens.focus = Some(row.id);
            }
        }
    }

    fn lens_wire_legend(&self, ui: &mut Ui, theme: atlas_shell::sidebar::SidebarTheme) {
        use atlas_shell::sidebar::sidebar_region;

        let palette = self.palette();
        sidebar_region(ui, "Wire legend", theme, |ui| {
            for (label, kind) in [
                ("Package dependencies", EdgeKind::PackageDep),
                ("Use / import", EdgeKind::Use),
                ("Trait implementations", EdgeKind::ImplTrait),
            ] {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(44.0, 14.0), Sense::hover());
                    paint_wire_legend_swatch(ui.painter(), rect, kind, &palette);
                    ui.label(egui::RichText::new(label).small().color(theme.sub));
                });
            }
            ui.label(
                egui::RichText::new("Thickness grows with reference count.")
                    .small()
                    .color(theme.sub),
            );
        });
    }

    fn lens_apply_harness_focus(&mut self) {
        if self.lens.harness_focus_applied {
            return;
        }
        let Ok(name) = std::env::var("SLATE_LENS_FOCUS") else {
            return;
        };
        if !matches!(self.lens.status, LensStatus::Ready) {
            return;
        }
        let Some(graph) = self.lens.graph.as_ref() else {
            return;
        };
        let needle = name.trim();
        if needle.is_empty() {
            self.lens.harness_focus_applied = true;
            return;
        }
        if let Some(id) = lens_find_node_by_name(graph, needle) {
            self.lens.focus = Some(id);
        }
        self.lens.harness_focus_applied = true;
    }

    fn lens_status_line(&self) -> String {
        match &self.lens.status {
            LensStatus::Idle => "Idle".into(),
            LensStatus::Analyzing => "Analyzing…".into(),
            LensStatus::Ready => {
                if let Some(g) = &self.lens.graph {
                    format!("Ready — {} nodes, {} edges", g.nodes.len(), g.edges.len())
                } else {
                    "Ready".into()
                }
            }
            LensStatus::Error(msg) => format!("Error: {msg}"),
        }
    }

    pub(crate) fn lens_canvas(&mut self, ui: &mut Ui, rect: Rect) {
        let palette = self.palette();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, palette.bg);

        let root = self.doc().lens_root.clone();
        if root.is_none() {
            self.lens_empty_state(ui, rect, &palette);
            return;
        }

        if matches!(self.lens.status, LensStatus::Analyzing) {
            self.lens_analyzing_state(ui, rect, &palette);
            return;
        }

        if let LensStatus::Error(ref msg) = self.lens.status {
            self.lens_error_banner(ui, rect, msg, &palette);
        }

        let Some(layout) = self.lens.layout.clone() else {
            return;
        };

        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let pointer = ui.ctx().pointer_latest_pos();

        // Camera — identical to Grid/Venn.
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 {
                if ui.input(|i| i.modifiers.shift) {
                    let z = self.tab().cam.z;
                    self.tab_mut().cam.offset.x -= scroll / z;
                } else if let Some(p) = pointer {
                    self.zoom_at(p, 1.0 + scroll * 0.0015);
                }
            }
        }
        let wants_kb = ui.ctx().wants_keyboard_input();
        ui.input(|i| {
            if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                self.zoom_at(rect.center(), 1.2);
            }
            if i.key_pressed(egui::Key::Minus) {
                self.zoom_at(rect.center(), 1.0 / 1.2);
            }
            if i.key_pressed(egui::Key::F) && !i.modifiers.ctrl && !wants_kb {
                self.lens_fit_layout(&layout);
            }
        });

        let mut cam_offset_tmp = self.tab().cam.offset;
        let ctx = ui.ctx().clone();
        let turbo_active = self
            .turbo_pan
            .step(&ctx, rect, pointer, &mut cam_offset_tmp);
        if turbo_active {
            let z = self.tab().cam.z;
            let old = self.tab().cam.offset;
            self.tab_mut().cam.offset = old - (cam_offset_tmp - old) / z;
        } else if resp.dragged_by(egui::PointerButton::Primary)
            || resp.dragged_by(egui::PointerButton::Secondary)
        {
            let delta = resp.drag_delta();
            let z = self.tab().cam.z;
            self.tab_mut().cam.offset -= delta / z;
        }

        // Hit-test before painting so hover feedback is visible in this frame.
        let hovered_node = pointer
            .filter(|p| rect.contains(*p))
            .and_then(|p| self.lens_hit_test(&layout, self.screen_to_world(p)));
        self.lens.hover = hovered_node;
        if hovered_node.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }

        let focus_set = self
            .lens
            .focus
            .and_then(|fid| self.lens.graph.as_ref().map(|g| focus_neighborhood(g, fid)));
        let search = self.lens.search.trim().to_lowercase();

        self.paint_dot_grid(&painter, rect, &palette);
        self.lens_paint_wires(&painter, &layout, focus_set.as_ref(), &palette);
        self.lens_paint_nodes(ui, &painter, &layout, focus_set.as_ref(), &search, &palette);

        if resp.clicked() {
            self.lens.focus = hovered_node;
        }
        if resp.double_clicked() {
            if let Some(id) = hovered_node {
                self.lens_handle_double_click(id);
            }
        }

        if let (Some(id), Some(graph)) = (hovered_node, self.lens.graph.as_ref()) {
            self.lens_show_tooltip(ui, graph, id);
        }

        let fit_bounds = rectf_to_rect(layout.bounds);
        self.mini_menu(ui.ctx(), rect, Some(fit_bounds));
    }

    fn lens_empty_state(&mut self, ui: &mut Ui, rect: Rect, palette: &atlas_shell::theme::Palette) {
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() * 0.35);
                ui.label(egui::RichText::new("Lens").size(22.0).color(palette.ink));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "Point this workbook at a Cargo workspace or crate to explore \
                         its dependency graph.",
                    )
                    .color(palette.sub),
                );
                ui.add_space(14.0);
                if ui.button("Choose code root…").clicked() {
                    self.lens_choose_root_dialog();
                }
            });
        });
    }

    fn lens_analyzing_state(&self, ui: &mut Ui, rect: Rect, palette: &atlas_shell::theme::Palette) {
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() * 0.45);
                ui.label(egui::RichText::new("Analyzing codebase…").color(palette.sub));
            });
        });
    }

    fn lens_error_banner(
        &self,
        ui: &mut Ui,
        rect: Rect,
        msg: &str,
        palette: &atlas_shell::theme::Palette,
    ) {
        let banner = Rect::from_min_max(
            Pos2::new(rect.left() + 12.0, rect.top() + 8.0),
            Pos2::new(rect.right() - 12.0, rect.top() + 36.0),
        );
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(banner), |ui| {
            ui.label(egui::RichText::new(msg).small().color(palette.portal));
        });
    }

    fn lens_fit_layout(&mut self, layout: &LensLayout) {
        self.fit_view(rectf_to_rect(layout.bounds));
    }

    fn lens_hit_test(&self, layout: &LensLayout, world: Pos2) -> Option<NodeId> {
        let graph = self.lens.graph.as_ref()?;
        layout.placed.iter().rev().find_map(|pl| {
            let r = rectf_to_rect(pl.rect);
            if pl.collapsed {
                return r.contains(world).then_some(pl.id);
            }
            let header = header_rect(r);
            if header.contains(world) {
                return Some(pl.id);
            }
            if r.contains(world) && !node_has_visible_children(graph, pl.id, &self.lens.expanded) {
                Some(pl.id)
            } else {
                None
            }
        })
    }

    fn lens_handle_double_click(&mut self, id: NodeId) {
        let Some(graph) = self.lens.graph.as_ref() else {
            return;
        };
        let node = graph.node(id);
        if can_expand(node.kind) && !node.children.is_empty() {
            if self.lens.expanded.contains(&id) {
                self.lens.expanded.remove(&id);
            } else {
                self.lens.expanded.insert(id);
            }
            self.lens.layout_dirty = true;
            return;
        }
        if matches!(node.kind, NodeKind::File | NodeKind::Item { .. }) {
            if let Some(root) = &self.doc().lens_root {
                let path = root.join(&node.path);
                Self::open_path(&path);
            }
        }
    }

    fn lens_paint_nodes(
        &self,
        ui: &Ui,
        painter: &egui::Painter,
        layout: &LensLayout,
        focus_set: Option<&HashSet<NodeId>>,
        search: &str,
        palette: &atlas_shell::theme::Palette,
    ) {
        let graph = match self.lens.graph.as_ref() {
            Some(g) => g,
            None => return,
        };
        let overlay = self.lens.overlay.as_ref();
        let z = self.tab().cam.z;

        for pl in &layout.placed {
            let alpha = focus_alpha(focus_set, pl.id);
            let node = graph.node(pl.id);
            let world = rectf_to_rect(pl.rect);
            let screen = self.world_rect_to_screen(world);
            if !screen.intersects(ui.clip_rect()) {
                continue;
            }

            let cluster = overlay.and_then(|ov| match_cluster(ov, graph, pl.id));
            let search_hit = !search.is_empty() && node.name.to_lowercase().contains(search);
            let focused = self.lens.focus == Some(pl.id);
            let hovered = self.lens.hover == Some(pl.id);
            let expandable = can_expand(node.kind) && !node.children.is_empty();
            let expanded = self.lens.expanded.contains(&pl.id);

            if pl.collapsed {
                let style = LensPaintStyle {
                    alpha,
                    search_hit,
                    focused,
                    hovered,
                    expandable,
                    expanded,
                    cluster,
                    palette,
                    z,
                };
                self.lens_paint_chip(painter, screen, node, &style);
            } else {
                let style = LensPaintStyle {
                    alpha,
                    search_hit,
                    focused,
                    hovered,
                    expandable,
                    expanded,
                    cluster,
                    palette,
                    z,
                };
                self.lens_paint_container(painter, screen, node, &style);
            }
        }
    }

    fn lens_paint_container(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        node: &code_lens::LensNode,
        style: &LensPaintStyle<'_>,
    ) {
        let fade = |c: Color32| c.gamma_multiply(style.alpha);
        let radius =
            CornerRadius::same((CONTAINER_RADIUS * style.z.max(0.5)).clamp(4.0, 12.0) as u8);
        painter.rect_filled(rect, radius, fade(style.palette.card));
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0_f32, fade(style.palette.border)),
            StrokeKind::Outside,
        );

        let header_h = (HEADER_STRIP_H * style.z).clamp(18.0, 36.0);
        let header = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.min.y + header_h));
        let header_fill = if let Some(c) = style.cluster.and_then(|cl| cl.color) {
            let accent = Color32::from_rgb(c[0], c[1], c[2]);
            accent.gamma_multiply(0.30 * style.alpha)
        } else {
            fade(style.palette.card_hover)
        };
        painter.rect_filled(
            Rect::from_min_max(header.min, Pos2::new(header.max.x, header.max.y + 1.0)),
            CornerRadius {
                nw: radius.nw,
                ne: radius.ne,
                sw: 0,
                se: 0,
            },
            header_fill,
        );

        let font = FontId::proportional((12.0 * style.z).clamp(10.0, 16.0));
        if style.expandable {
            painter.text(
                header.left_center() + Vec2::new(8.0, 0.0),
                Align2::LEFT_CENTER,
                if style.expanded { "▾" } else { "▸" },
                font.clone(),
                fade(style.palette.accent),
            );
        }
        painter.text(
            header.left_center() + Vec2::new(if style.expandable { 21.0 } else { 8.0 }, 0.0),
            Align2::LEFT_CENTER,
            &node.name,
            font.clone(),
            fade(style.palette.ink),
        );
        painter.text(
            header.right_center() + Vec2::new(-8.0, 0.0),
            Align2::RIGHT_CENTER,
            format!("{} LOC", node.loc),
            font,
            fade(style.palette.sub),
        );

        if let Some(cl) = style.cluster {
            let tag = Rect::from_min_size(
                header.min + Vec2::new(8.0, 2.0),
                Vec2::new(
                    (cl.title.len() as f32 * 5.5 + 12.0).min(header.width() * 0.45),
                    12.0,
                ),
            );
            if tag.max.x < header.max.x - 60.0 {
                painter.rect_filled(tag, CornerRadius::same(3), fade(style.palette.portal));
                painter.text(
                    tag.center(),
                    Align2::CENTER_CENTER,
                    &cl.title,
                    FontId::proportional(9.0),
                    fade(style.palette.ink),
                );
            }
        }

        if let Some(stroke) = interaction_stroke(style) {
            painter.rect_stroke(rect, radius, stroke, StrokeKind::Outside);
        }
    }

    fn lens_paint_chip(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        node: &code_lens::LensNode,
        style: &LensPaintStyle<'_>,
    ) {
        let fade = |c: Color32| c.gamma_multiply(style.alpha);
        let radius = CornerRadius::same((CHIP_RADIUS * style.z.max(0.5)).clamp(3.0, 10.0) as u8);
        let fill = if let Some(c) = style.cluster.and_then(|cl| cl.color) {
            Color32::from_rgb(c[0], c[1], c[2]).gamma_multiply(0.18 * style.alpha)
        } else {
            fade(style.palette.card)
        };
        painter.rect_filled(rect, radius, fill);
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0_f32, fade(style.palette.border)),
            StrokeKind::Outside,
        );

        let glyph = node_glyph(node.kind);
        let font = FontId::proportional((11.0 * style.z).clamp(9.0, 14.0));
        if style.expandable {
            painter.text(
                rect.left_center() + Vec2::new(8.0, 0.0),
                Align2::LEFT_CENTER,
                if style.expanded { "▾" } else { "▸" },
                font.clone(),
                fade(style.palette.accent),
            );
        }
        let glyph_x = if style.expandable { 20.0 } else { 10.0 };
        painter.text(
            rect.left_center() + Vec2::new(glyph_x, 0.0),
            Align2::LEFT_CENTER,
            glyph,
            font.clone(),
            fade(style.palette.accent),
        );
        let name_x = if style.expandable { 32.0 } else { 22.0 };
        painter.text(
            rect.left_center() + Vec2::new(name_x, 0.0),
            Align2::LEFT_CENTER,
            &node.name,
            font,
            fade(style.palette.ink),
        );

        if let Some(stroke) = interaction_stroke(style) {
            painter.rect_stroke(rect, radius, stroke, StrokeKind::Outside);
        }
    }

    fn lens_paint_wires(
        &self,
        painter: &egui::Painter,
        layout: &LensLayout,
        focus_set: Option<&HashSet<NodeId>>,
        palette: &atlas_shell::theme::Palette,
    ) {
        for wire in &layout.wires {
            if !wire_visible(wire, &self.lens) {
                continue;
            }
            let alpha = wire_alpha(focus_set, wire);
            self.lens_paint_wire(painter, wire, alpha, palette);
        }
    }

    fn lens_paint_wire(
        &self,
        painter: &egui::Painter,
        wire: &LensWire,
        alpha: f32,
        palette: &atlas_shell::theme::Palette,
    ) {
        let from = self.world_to_screen(Pos2::new(wire.from_pt.0, wire.from_pt.1));
        let to = self.world_to_screen(Pos2::new(wire.to_pt.0, wire.to_pt.1));
        let (color, base_w, dashed) = wire_kind_style(wire.kind, palette);
        let fade = color.gamma_multiply(alpha);
        let w = (base_w + (wire.weight.max(1) as f32).log2()).clamp(1.0, 4.0)
            * self.tab().cam.z.max(0.4);

        let dx = (to.x - from.x).abs().max(40.0) * 0.45;
        let c1 = Pos2::new(from.x + dx, from.y);
        let c2 = Pos2::new(to.x - dx, to.y);

        if dashed {
            paint_dashed_bezier(painter, from, c1, c2, to, fade, w);
        } else {
            let stroke = Stroke::new(w, fade);
            painter.add(egui::Shape::CubicBezier(
                egui::epaint::CubicBezierShape::from_points_stroke(
                    [from, c1, c2, to],
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ),
            ));
        }

        paint_arrowhead(painter, c2, to, fade, w);
    }

    fn lens_show_tooltip(&self, ui: &Ui, graph: &CodeGraph, id: NodeId) {
        let node = graph.node(id);
        let (in_deg, out_deg) = degree_counts(graph, id);

        let mut text = format!(
            "{}\nKind: {}\nLOC: {}\nIn: {in_deg}  Out: {out_deg}",
            node.path.display(),
            kind_label(node.kind),
            node.loc,
        );
        if let Some(ov) = &self.lens.overlay {
            if let Some(cl) = match_cluster(ov, graph, id) {
                text.push_str(&format!("\n\n{}\n{}", cl.title, cl.summary));
            }
        }
        egui::Area::new(Id::new("lens_node_tip"))
            .order(egui::Order::Tooltip)
            .fixed_pos(ui.ctx().pointer_hover_pos().unwrap_or_default() + Vec2::new(14.0, 14.0))
            .show(ui.ctx(), |ui| {
                ui.label(text);
            });
    }
}

fn default_expanded(graph: &CodeGraph) -> HashSet<NodeId> {
    let mut set = HashSet::new();
    set.insert(graph.root);
    for node in &graph.nodes {
        if matches!(node.kind, NodeKind::Package { .. }) {
            set.insert(node.id);
        }
    }
    set
}

fn focus_neighborhood(graph: &CodeGraph, focus: NodeId) -> HashSet<NodeId> {
    let mut set = HashSet::new();
    set.insert(focus);
    for (nid, _, _) in graph.neighbors(focus) {
        set.insert(nid);
    }
    set
}

fn degree_counts(graph: &CodeGraph, id: NodeId) -> (usize, usize) {
    let in_deg = graph.edges.iter().filter(|edge| edge.to == id).count();
    let out_deg = graph.edges.iter().filter(|edge| edge.from == id).count();
    (in_deg, out_deg)
}

fn edge_stats_by_kind(graph: &CodeGraph, id: NodeId) -> [(&'static str, EdgeStats); 3] {
    [
        (
            "Package dependencies",
            graph.edge_stats(id, EdgeKind::PackageDep),
        ),
        ("Use / import", graph.edge_stats(id, EdgeKind::Use)),
        (
            "Trait implementations",
            graph.edge_stats(id, EdgeKind::ImplTrait),
        ),
    ]
}

fn edge_stats_line(label: &str, stats: EdgeStats) -> String {
    let weighted = stats.incoming_weight != stats.incoming_edges as u32
        || stats.outgoing_weight != stats.outgoing_edges as u32;
    if weighted {
        format!(
            "{label}: {} in / {} refs · {} out / {} refs",
            stats.incoming_edges,
            stats.incoming_weight,
            stats.outgoing_edges,
            stats.outgoing_weight
        )
    } else {
        format!(
            "{label}: {} incoming · {} outgoing",
            stats.incoming_edges, stats.outgoing_edges
        )
    }
}

fn wire_kind_style(kind: EdgeKind, palette: &atlas_shell::theme::Palette) -> (Color32, f32, bool) {
    match kind {
        EdgeKind::PackageDep => (palette.ink, 2.5, false),
        EdgeKind::Use => (palette.accent, 1.0, false),
        EdgeKind::ImplTrait => (palette.portal, 1.0, true),
    }
}

fn paint_wire_legend_swatch(
    painter: &egui::Painter,
    rect: Rect,
    kind: EdgeKind,
    palette: &atlas_shell::theme::Palette,
) {
    let (color, base_w, dashed) = wire_kind_style(kind, palette);
    let y = rect.center().y;
    let from = Pos2::new(rect.left() + 2.0, y);
    let to = Pos2::new(rect.right() - 2.0, y);
    let stroke = Stroke::new(base_w, color);
    if dashed {
        let mut x = from.x;
        while x < to.x {
            let end = (x + 5.0).min(to.x);
            painter.line_segment([Pos2::new(x, y), Pos2::new(end, y)], stroke);
            x += 8.0;
        }
    } else {
        painter.line_segment([from, to], stroke);
    }
    paint_arrowhead(painter, from, to, color, base_w);
}

fn lens_find_node_by_name(graph: &CodeGraph, needle: &str) -> Option<NodeId> {
    let lower = needle.to_lowercase();
    graph
        .nodes
        .iter()
        .find(|node| node.name == needle)
        .map(|node| node.id)
        .or_else(|| {
            graph
                .nodes
                .iter()
                .find(|node| node.name.to_lowercase() == lower)
                .map(|node| node.id)
        })
}

fn interaction_stroke(style: &LensPaintStyle<'_>) -> Option<Stroke> {
    if style.focused {
        Some(Stroke::new(2.5, style.palette.select))
    } else if style.hovered {
        Some(Stroke::new(1.5, style.palette.border_strong))
    } else if style.search_hit {
        Some(Stroke::new(2.0, style.palette.staged))
    } else {
        None
    }
}

fn focus_alpha(focus_set: Option<&HashSet<NodeId>>, id: NodeId) -> f32 {
    match focus_set {
        Some(set) if !set.contains(&id) => 0.25,
        _ => 1.0,
    }
}

fn wire_alpha(focus_set: Option<&HashSet<NodeId>>, wire: &LensWire) -> f32 {
    match focus_set {
        Some(set) if !set.contains(&wire.from) && !set.contains(&wire.to) => 0.25,
        _ => 1.0,
    }
}

fn wire_visible(wire: &LensWire, lens: &LensState) -> bool {
    match wire.kind {
        EdgeKind::PackageDep => lens.filter_package_dep,
        EdgeKind::Use => lens.filter_use,
        EdgeKind::ImplTrait => lens.filter_impl_trait,
    }
}

fn can_expand(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Workspace | NodeKind::Package { .. } | NodeKind::Module
    )
}

fn node_has_visible_children(graph: &CodeGraph, id: NodeId, expanded: &HashSet<NodeId>) -> bool {
    let node = graph.node(id);
    !node.children.is_empty() && expanded.contains(&id)
}

fn header_rect(r: Rect) -> Rect {
    Rect::from_min_max(r.min, Pos2::new(r.max.x, r.min.y + HEADER_STRIP_H))
}

fn rectf_to_rect(r: Rectf) -> Rect {
    Rect::from_min_size(Pos2::new(r.x, r.y), Vec2::new(r.w, r.h))
}

fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Workspace => "Workspace",
        NodeKind::Package { is_app } => {
            if is_app {
                "Package (app)"
            } else {
                "Package"
            }
        }
        NodeKind::Module => "Module",
        NodeKind::File => "File",
        NodeKind::Item { item } => match item {
            ItemKind::Struct => "Struct",
            ItemKind::Enum => "Enum",
            ItemKind::Trait => "Trait",
            ItemKind::Function => "Function",
            ItemKind::Impl => "Impl",
            ItemKind::TypeAlias => "Type alias",
            ItemKind::Const => "Const",
            ItemKind::Static => "Static",
            ItemKind::Macro => "Macro",
        },
    }
}

fn node_glyph(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Module => "M",
        NodeKind::File => "F",
        NodeKind::Item { item } => match item {
            ItemKind::Struct => "S",
            ItemKind::Enum => "E",
            ItemKind::Trait => "T",
            ItemKind::Function => "f",
            ItemKind::Impl => "I",
            ItemKind::TypeAlias => "t",
            ItemKind::Const => "c",
            ItemKind::Static => "s",
            ItemKind::Macro => "m",
        },
        NodeKind::Package { is_app } => {
            if is_app {
                "A"
            } else {
                "P"
            }
        }
        NodeKind::Workspace => "W",
    }
}

fn paint_arrowhead(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32, width: f32) {
    let dir = to - from;
    if dir.length_sq() < 1.0 {
        return;
    }
    let dir = dir.normalized();
    let side = Vec2::new(-dir.y, dir.x);
    let tip = to;
    let base = to - dir * (width * 3.0).max(6.0);
    let p1 = base + side * (width * 1.2);
    let p2 = base - side * (width * 1.2);
    painter.add(egui::Shape::convex_polygon(
        vec![tip, p1, p2],
        color,
        Stroke::NONE,
    ));
}

fn paint_dashed_bezier(
    painter: &egui::Painter,
    p0: Pos2,
    p1: Pos2,
    p2: Pos2,
    p3: Pos2,
    color: Color32,
    width: f32,
) {
    const SEGMENTS: usize = 32;
    let mut pts = Vec::with_capacity(SEGMENTS + 1);
    for i in 0..=SEGMENTS {
        let t = i as f32 / SEGMENTS as f32;
        let u = 1.0 - t;
        let pt = p0.to_vec2() * (u * u * u)
            + p1.to_vec2() * (3.0 * u * u * t)
            + p2.to_vec2() * (3.0 * u * t * t)
            + p3.to_vec2() * (t * t * t);
        pts.push(pt.to_pos2());
    }
    let dash = (width * 4.0).max(6.0);
    let gap = dash * 0.6;
    let stroke = Stroke::new(width, color);
    let mut i = 0;
    while i + 1 < pts.len() {
        let mut acc = 0.0f32;
        let start = pts[i];
        let mut j = i + 1;
        while j < pts.len() {
            let seg = (pts[j] - pts[j - 1]).length();
            if acc + seg >= dash {
                painter.line_segment([start, pts[j]], stroke);
                break;
            }
            acc += seg;
            j += 1;
        }
        i = j + (gap / dash).ceil() as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_lens::{LensEdge, LensNode};

    #[test]
    fn degree_counts_preserve_edge_direction() {
        let node = |id| LensNode {
            id,
            parent: None,
            kind: NodeKind::File,
            name: format!("node-{id}"),
            path: PathBuf::from(format!("node-{id}.rs")),
            loc: 1,
            children: Vec::new(),
        };
        let graph = CodeGraph {
            root: 0,
            nodes: vec![node(0), node(1), node(2)],
            edges: vec![
                LensEdge {
                    from: 0,
                    to: 1,
                    kind: EdgeKind::Use,
                    weight: 1,
                },
                LensEdge {
                    from: 2,
                    to: 1,
                    kind: EdgeKind::Use,
                    weight: 3,
                },
                LensEdge {
                    from: 1,
                    to: 2,
                    kind: EdgeKind::ImplTrait,
                    weight: 1,
                },
            ],
            generated_at: 0,
        };

        assert_eq!(degree_counts(&graph, 1), (2, 1));
    }

    #[test]
    fn lens_find_node_by_name_prefers_exact_match() {
        let node = |id, name: &str| LensNode {
            id,
            parent: None,
            kind: NodeKind::File,
            name: name.into(),
            path: PathBuf::from(format!("{name}.rs")),
            loc: 1,
            children: Vec::new(),
        };
        let graph = CodeGraph {
            root: 0,
            nodes: vec![node(0, "lib"), node(1, "Lib")],
            edges: Vec::new(),
            generated_at: 0,
        };
        assert_eq!(lens_find_node_by_name(&graph, "Lib"), Some(1));
        assert_eq!(lens_find_node_by_name(&graph, "lib"), Some(0));
    }
}
