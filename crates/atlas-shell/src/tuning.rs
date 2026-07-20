//! Feature-gated live editor for shared UI design tokens.
//!
//! Enable with an app's `ui-tuner` feature. Normal builds retain only the
//! no-op [`show`] function, so the editor and its file-writing path are not
//! present in production behavior.

#[cfg(not(feature = "ui-tuner"))]
#[inline]
pub fn show(_ctx: &eframe::egui::Context) {}

#[cfg(not(feature = "ui-tuner"))]
#[inline]
pub(crate) fn portal_preview_menu() -> Option<usize> {
    None
}

#[cfg(not(feature = "ui-tuner"))]
#[inline]
pub(crate) fn dock_preview_panel() -> Option<&'static str> {
    None
}

#[cfg(feature = "ui-tuner")]
mod enabled {
    use crate::tokens::{
        self, DockThemeTokens, DockTokens, HomeTokens, PortalMenuThemeTokens, PortalMenuTokens,
        TopBarThemeTokens, TopBarTokens, UiTokens,
    };
    use eframe::egui::{self, Color32, RichText, Slider};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};

    static PORTAL_PREVIEW_LOCKED: AtomicBool = AtomicBool::new(false);
    static PORTAL_PREVIEW_MENU: AtomicUsize = AtomicUsize::new(0);
    static DOCK_PREVIEW_LOCKED: AtomicBool = AtomicBool::new(false);
    static DOCK_PREVIEW_PANEL: Mutex<Option<&'static str>> = Mutex::new(None);

    struct TunerState {
        open: bool,
        draft: UiTokens,
        status: String,
    }

    impl Default for TunerState {
        fn default() -> Self {
            Self {
                open: true,
                draft: tokens::current(),
                status: "Live preview active — changes are not saved yet.".to_string(),
            }
        }
    }

    fn state() -> &'static Mutex<TunerState> {
        static STATE: OnceLock<Mutex<TunerState>> = OnceLock::new();
        STATE.get_or_init(|| Mutex::new(TunerState::default()))
    }

    fn token_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui-tokens.toml")
    }

    fn save(tokens: &UiTokens) -> Result<PathBuf, String> {
        let path = token_path();
        let mut stored = tokens.clone();
        stored.topbar.normalize();
        stored.topbar.round_for_storage();
        stored.dock.normalize();
        stored.dock.round_for_storage();
        stored.home.normalize();
        stored.home.round_for_storage();
        let body = toml::to_string_pretty(&stored).map_err(|error| error.to_string())?;
        let header = concat!(
            "# Canonical shared-chrome design tokens.\n",
            "# Edit directly, or run either app with `--features ui-tuner` for live editing.\n",
            "# Saved tuner values are embedded by the next build.\n\n",
        );
        std::fs::write(&path, format!("{header}{body}")).map_err(|error| error.to_string())?;
        Ok(path)
    }

    fn scalar(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
    ) {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(Slider::new(value, range).show_value(true));
        });
    }

    fn integer(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut usize,
        range: std::ops::RangeInclusive<usize>,
    ) {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(Slider::new(value, range).show_value(true));
        });
    }

    fn rgba(ui: &mut egui::Ui, label: &str, value: &mut [u8; 4]) {
        ui.horizontal(|ui| {
            ui.label(label);
            let mut color = Color32::from_rgba_unmultiplied(value[0], value[1], value[2], value[3]);
            if ui.color_edit_button_srgba(&mut color).changed() {
                *value = color.to_array();
            }
            for (index, channel) in ["R", "G", "B", "A"].into_iter().enumerate() {
                ui.label(channel);
                ui.add(
                    egui::DragValue::new(&mut value[index])
                        .range(0..=255)
                        .speed(1),
                );
            }
        });
    }

    fn leak_panel_id(id: &str) -> &'static str {
        Box::leak(id.to_owned().into_boxed_str())
    }

    fn dock_preview_controls(ui: &mut egui::Ui) {
        let mut locked = DOCK_PREVIEW_LOCKED.load(Ordering::Relaxed);
        if ui
            .checkbox(&mut locked, "Lock dock popover open")
            .on_hover_text("Keep the hovered dock panel visible while editing these controls")
            .changed()
        {
            DOCK_PREVIEW_LOCKED.store(locked, Ordering::Relaxed);
        }

        if locked {
            let mut panel = dock_preview_panel()
                .map(str::to_owned)
                .unwrap_or_else(|| "filters".to_owned());
            egui::ComboBox::from_label("Preview panel")
                .selected_text(&panel)
                .show_ui(ui, |ui| {
                    ui.label(RichText::new("File Atlas").strong());
                    ui.selectable_value(&mut panel, "filters".to_string(), "Filters");
                    ui.selectable_value(&mut panel, "display".to_string(), "Display");
                    ui.selectable_value(&mut panel, "workflow".to_string(), "Workflow");
                    ui.selectable_value(&mut panel, "ai".to_string(), "AI");
                    ui.separator();
                    ui.label(RichText::new("Slate · bottom dock").strong());
                    ui.selectable_value(&mut panel, "tool.nav".to_string(), "Nav");
                    ui.selectable_value(&mut panel, "tool.frame".to_string(), "Frame");
                    ui.selectable_value(&mut panel, "tool.shapes".to_string(), "Shapes");
                    ui.selectable_value(&mut panel, "tool.curve".to_string(), "Curve");
                    ui.selectable_value(&mut panel, "board.align".to_string(), "Align");
                    ui.separator();
                    ui.label(RichText::new("Slate · left dock").strong());
                    ui.selectable_value(&mut panel, "tags".to_string(), "Tags");
                    ui.selectable_value(&mut panel, "selection".to_string(), "Selection");
                    ui.selectable_value(&mut panel, "view".to_string(), "View");
                    ui.selectable_value(&mut panel, "lens".to_string(), "Lens");
                });
            *DOCK_PREVIEW_PANEL
                .lock()
                .expect("UI tuner dock preview lock poisoned") = Some(leak_panel_id(&panel));
        }
        ui.separator();
    }

    fn portal_preview_controls(ui: &mut egui::Ui) {
        let mut locked = PORTAL_PREVIEW_LOCKED.load(Ordering::Relaxed);
        if ui
            .checkbox(&mut locked, "Lock portal preview open")
            .on_hover_text("Keep the floating menu visible while editing these controls")
            .changed()
        {
            PORTAL_PREVIEW_LOCKED.store(locked, Ordering::Relaxed);
        }

        if locked {
            let mut menu = PORTAL_PREVIEW_MENU.load(Ordering::Relaxed);
            egui::ComboBox::from_label("Preview submenu")
                .selected_text(match menu {
                    2 => "View",
                    3 => "Preferences",
                    _ => "File",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut menu, 0, "File");
                    ui.selectable_value(&mut menu, 2, "View");
                    ui.selectable_value(&mut menu, 3, "Preferences");
                });
            PORTAL_PREVIEW_MENU.store(menu, Ordering::Relaxed);
        }
        ui.separator();
    }

    fn theme_editor(ui: &mut egui::Ui, name: &str, theme: &mut TopBarThemeTokens) {
        egui::CollapsingHeader::new(name)
            .default_open(false)
            .show(ui, |ui| {
                rgba(ui, "Bar", &mut theme.bar);
                rgba(ui, "Bar top", &mut theme.bar_top);
                rgba(ui, "Inactive tab", &mut theme.inactive);
                rgba(ui, "Inactive hover", &mut theme.inactive_hover);
                scalar(
                    ui,
                    "Active top light mix",
                    &mut theme.active_top_mix,
                    0.0..=0.5,
                );
                scalar(
                    ui,
                    "Divider strength",
                    &mut theme.divider_strength,
                    0.0..=1.0,
                );
                scalar(
                    ui,
                    "Accent → white mix",
                    &mut theme.accent_white_mix,
                    0.0..=1.0,
                );
            });
    }

    fn portal_editor(ui: &mut egui::Ui, portal: &mut PortalMenuTokens) {
        egui::CollapsingHeader::new("Portal menu · Geometry and type")
            .default_open(false)
            .show(ui, |ui| {
                portal_preview_controls(ui);
                scalar(ui, "Panel width", &mut portal.width, 150.0..=420.0);
                scalar(
                    ui,
                    "Submenu width",
                    &mut portal.submenu_width,
                    150.0..=480.0,
                );
                scalar(ui, "Row height", &mut portal.row_height, 20.0..=54.0);
                scalar(ui, "Panel padding", &mut portal.panel_padding, 0.0..=28.0);
                scalar(ui, "Corner radius", &mut portal.corner_radius, 0.0..=28.0);
                scalar(
                    ui,
                    "Horizontal offset",
                    &mut portal.panel_offset_x,
                    -40.0..=80.0,
                );
                scalar(ui, "Top-bar gap", &mut portal.panel_gap, 0.0..=24.0);
                scalar(ui, "Submenu gap", &mut portal.submenu_gap, 0.0..=24.0);
                scalar(ui, "Section gap", &mut portal.separator_gap, 0.0..=24.0);
                scalar(
                    ui,
                    "Header text size",
                    &mut portal.header_text_size,
                    8.0..=24.0,
                );
                scalar(ui, "Row text size", &mut portal.row_text_size, 8.0..=24.0);
                scalar(
                    ui,
                    "Shortcut text size",
                    &mut portal.shortcut_text_size,
                    8.0..=22.0,
                );
                scalar(
                    ui,
                    "Chevron size",
                    &mut portal.chevron_text_size,
                    8.0..=28.0,
                );
                scalar(ui, "Hover close delay", &mut portal.close_delay, 0.0..=1.0);
            });

        egui::CollapsingHeader::new("Portal menu · Shadow")
            .default_open(false)
            .show(ui, |ui| {
                portal_preview_controls(ui);
                scalar(
                    ui,
                    "Shadow X offset",
                    &mut portal.shadow_offset_x,
                    -20.0..=20.0,
                );
                scalar(
                    ui,
                    "Shadow Y offset",
                    &mut portal.shadow_offset_y,
                    -20.0..=30.0,
                );
                scalar(ui, "Shadow blur", &mut portal.shadow_blur, 0.0..=48.0);
                scalar(ui, "Shadow spread", &mut portal.shadow_spread, 0.0..=16.0);
                scalar(ui, "Shadow opacity", &mut portal.shadow_opacity, 0.0..=1.0);
            });
    }

    fn portal_theme_editor(ui: &mut egui::Ui, name: &str, theme: &mut PortalMenuThemeTokens) {
        egui::CollapsingHeader::new(name)
            .default_open(false)
            .show(ui, |ui| {
                portal_preview_controls(ui);
                rgba(ui, "Panel fill", &mut theme.fill);
                rgba(ui, "Border", &mut theme.border);
                rgba(ui, "Hover row", &mut theme.hover);
                rgba(ui, "Text", &mut theme.text);
                rgba(ui, "Muted text", &mut theme.muted_text);
            });
    }

    fn dock_partition_tracer_editor(ui: &mut egui::Ui, dock: &mut DockTokens) {
        egui::CollapsingHeader::new("Dock · Partition & tracers")
            .default_open(true)
            .show(ui, |ui| {
                dock_preview_controls(ui);
                ui.label(RichText::new("Partition line").strong());
                scalar(ui, "Gap from icons", &mut dock.partition_gap, 0.0..=36.0);
                scalar(
                    ui,
                    "Extend past strip",
                    &mut dock.partition_extend,
                    0.0..=120.0,
                );
                scalar(
                    ui,
                    "Max thickness (center)",
                    &mut dock.partition_max_thickness,
                    0.0..=8.0,
                );
                scalar(
                    ui,
                    "Min thickness (ends)",
                    &mut dock.partition_min_thickness,
                    0.0..=4.0,
                );
                scalar(ui, "Opacity", &mut dock.partition_opacity, 0.0..=1.0);
                ui.separator();
                ui.label(RichText::new("Hover tracer (panel border → icon)").strong());
                scalar(ui, "Stroke width", &mut dock.tracer_width, 0.0..=4.0);
                scalar(ui, "Opacity", &mut dock.tracer_opacity, 0.0..=1.0);
                scalar(
                    ui,
                    "Corner radius",
                    &mut dock.tracer_corner_radius,
                    0.0..=24.0,
                );
                scalar(
                    ui,
                    "Border hit band",
                    &mut dock.tracer_border_hit,
                    2.0..=24.0,
                );
                ui.separator();
                scalar(ui, "Panel stack gap", &mut dock.stack_gap, 0.0..=32.0);
                scalar(
                    ui,
                    "Dashboard describe delay",
                    &mut dock.dashboard_describe_delay,
                    0.0..=2.0,
                );
                scalar(
                    ui,
                    "Description fade",
                    &mut dock.describe_fade_duration,
                    0.05..=1.0,
                );
                scalar(
                    ui,
                    "Panel ease-in",
                    &mut dock.panel_open_duration,
                    0.05..=0.8,
                );
                scalar(ui, "Label chip gap", &mut dock.hover_chip_gap, 2.0..=24.0);
            });
    }

    fn dock_editor(ui: &mut egui::Ui, dock: &mut DockTokens) {
        egui::CollapsingHeader::new("Floating docks · Geometry")
            .default_open(false)
            .show(ui, |ui| {
                dock_preview_controls(ui);
                scalar(ui, "Icon size", &mut dock.icon_size, 20.0..=64.0);
                scalar(ui, "Icon gap", &mut dock.icon_gap, 0.0..=28.0);
                scalar(ui, "Icon text size", &mut dock.icon_text_size, 8.0..=24.0);
                scalar(
                    ui,
                    "Squircle exponent",
                    &mut dock.squircle_exponent,
                    2.0..=8.0,
                );
                scalar(ui, "Left margin", &mut dock.left_margin, 0.0..=48.0);
                scalar(ui, "Bottom margin", &mut dock.bottom_margin, 0.0..=64.0);
                scalar(ui, "Popover width", &mut dock.popover_width, 160.0..=520.0);
                scalar(
                    ui,
                    "Popover max height",
                    &mut dock.popover_max_height,
                    160.0..=760.0,
                );
                scalar(ui, "Popover gap", &mut dock.popover_gap, 0.0..=36.0);
                scalar(ui, "Popover padding", &mut dock.popover_padding, 0.0..=28.0);
                scalar(
                    ui,
                    "Popover radius",
                    &mut dock.popover_corner_radius,
                    0.0..=28.0,
                );
                scalar(ui, "Close delay", &mut dock.close_delay, 0.0..=1.0);
            });

        egui::CollapsingHeader::new("Floating docks · Shadow")
            .default_open(false)
            .show(ui, |ui| {
                dock_preview_controls(ui);
                scalar(
                    ui,
                    "Shadow X offset",
                    &mut dock.shadow_offset_x,
                    -20.0..=20.0,
                );
                scalar(
                    ui,
                    "Shadow Y offset",
                    &mut dock.shadow_offset_y,
                    -20.0..=30.0,
                );
                scalar(ui, "Shadow blur", &mut dock.shadow_blur, 0.0..=48.0);
                scalar(ui, "Shadow spread", &mut dock.shadow_spread, 0.0..=16.0);
                scalar(ui, "Shadow opacity", &mut dock.shadow_opacity, 0.0..=1.0);
            });
    }

    fn dock_theme_editor(ui: &mut egui::Ui, name: &str, theme: &mut DockThemeTokens) {
        egui::CollapsingHeader::new(name)
            .default_open(false)
            .show(ui, |ui| {
                dock_preview_controls(ui);
                rgba(ui, "Icon fill", &mut theme.icon_fill);
                rgba(ui, "Icon hover", &mut theme.icon_hover);
                rgba(ui, "Icon active", &mut theme.icon_active);
                rgba(ui, "Popover fill", &mut theme.popover_fill);
                rgba(ui, "Border", &mut theme.border);
                rgba(ui, "Text", &mut theme.text);
                rgba(ui, "Muted text", &mut theme.muted_text);
            });
    }

    fn geometry_editor(ui: &mut egui::Ui, t: &mut TopBarTokens) {
        egui::CollapsingHeader::new("Geometry")
            .default_open(false)
            .show(ui, |ui| {
                scalar(ui, "Bar height", &mut t.height, 20.0..=56.0);
                scalar(ui, "Tab top inset", &mut t.tab_top_inset, 0.0..=16.0);
                scalar(ui, "Top fillet radius", &mut t.tab_top_radius, 0.5..=18.0);
                scalar(
                    ui,
                    "Shoulder fillet radius",
                    &mut t.tab_shoulder_radius,
                    0.5..=24.0,
                );
                scalar(
                    ui,
                    "Tab horizontal padding",
                    &mut t.tab_horizontal_padding,
                    2.0..=32.0,
                );
                scalar(
                    ui,
                    "Close affordance width",
                    &mut t.tab_close_width,
                    8.0..=32.0,
                );
                scalar(ui, "Tab minimum width", &mut t.tab_min_width, 60.0..=260.0);
                scalar(ui, "Tab maximum width", &mut t.tab_max_width, 100.0..=480.0);
                scalar(ui, "Icon portal width", &mut t.icon_zone_width, 20.0..=80.0);
                scalar(ui, "Icon size", &mut t.icon_size, 10.0..=40.0);
                scalar(
                    ui,
                    "Window button width",
                    &mut t.window_button_width,
                    28.0..=64.0,
                );
                scalar(ui, "New-tab hit width", &mut t.plus_hit_width, 16.0..=56.0);
                scalar(ui, "New-tab hover radius", &mut t.plus_radius, 3.0..=20.0);
            });
    }

    fn typography_editor(ui: &mut egui::Ui, t: &mut TopBarTokens) {
        egui::CollapsingHeader::new("Typography and menus")
            .default_open(false)
            .show(ui, |ui| {
                scalar(ui, "Tab text size", &mut t.tab_text_size, 8.0..=24.0);
                integer(ui, "Title character limit", &mut t.tab_title_chars, 8..=80);
                scalar(ui, "New-tab + size", &mut t.plus_text_size, 8.0..=24.0);
            });
    }

    fn effects_editor(ui: &mut egui::Ui, t: &mut TopBarTokens) {
        egui::CollapsingHeader::new("Active-tab glow and emboss")
            .default_open(false)
            .show(ui, |ui| {
                scalar(
                    ui,
                    "Outer falloff width",
                    &mut t.glow_outer_width,
                    0.0..=12.0,
                );
                scalar(
                    ui,
                    "Outer falloff opacity",
                    &mut t.glow_outer_opacity,
                    0.0..=1.0,
                );
                scalar(ui, "Middle glow width", &mut t.glow_middle_width, 0.0..=8.0);
                scalar(
                    ui,
                    "Middle glow opacity",
                    &mut t.glow_middle_opacity,
                    0.0..=1.0,
                );
                scalar(ui, "Core stroke width", &mut t.glow_core_width, 0.0..=4.0);
                scalar(
                    ui,
                    "Core stroke opacity",
                    &mut t.glow_core_opacity,
                    0.0..=1.0,
                );
                scalar(
                    ui,
                    "Inner emboss opacity",
                    &mut t.inner_highlight_opacity,
                    0.0..=1.0,
                );
            });
    }

    fn home_editor(ui: &mut egui::Ui, home: &mut HomeTokens) {
        egui::CollapsingHeader::new("Home · Cover Flow")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(RichText::new("Cards (square)").strong());
                scalar(
                    ui,
                    "Size · canvas fraction",
                    &mut home.cover_frac,
                    0.15..=0.85,
                );
                scalar(ui, "Size · min px", &mut home.cover_min, 60.0..=500.0);
                scalar(ui, "Size · max px", &mut home.cover_max, 120.0..=900.0);
                scalar(ui, "Vertical center", &mut home.center_y_frac, 0.2..=0.75);
                ui.separator();
                ui.label(RichText::new("Spacing & falloff").strong());
                scalar(
                    ui,
                    "Side packing (× card)",
                    &mut home.side_step_frac,
                    0.02..=0.8,
                );
                scalar(
                    ui,
                    "Center gap (× card)",
                    &mut home.center_bulge_frac,
                    0.0..=1.5,
                );
                scalar(
                    ui,
                    "Falloff sharpness (smaller = sharper)",
                    &mut home.bulge_width,
                    0.1..=3.0,
                );
                ui.separator();
                ui.label(RichText::new("Rotation & depth").strong());
                scalar(
                    ui,
                    "Max rotation (°)",
                    &mut home.angle_max_deg,
                    -85.0..=85.0,
                );
                scalar(
                    ui,
                    "Rotation ramp (smaller = flips sooner)",
                    &mut home.angle_width,
                    0.1..=3.0,
                );
                scalar(ui, "Depth push-back", &mut home.depth_max, 0.0..=600.0);
                scalar(ui, "Depth ramp", &mut home.depth_width, 0.1..=4.0);
                scalar(ui, "Focal length", &mut home.focal, 200.0..=4000.0);
                ui.separator();
                ui.label(RichText::new("Card finish").strong());
                scalar(
                    ui,
                    "Corner fillet (× card)",
                    &mut home.corner_bevel_frac,
                    0.0..=0.2,
                );
                scalar(ui, "AO reach (px)", &mut home.ao_size, 0.0..=120.0);
                scalar(ui, "AO strength", &mut home.ao_strength, 0.0..=1.0);
                ui.separator();
                ui.label(RichText::new("Motion feel").strong());
                scalar(ui, "Inertia friction", &mut home.friction, 0.2..=20.0);
                scalar(
                    ui,
                    "Snap stiffness",
                    &mut home.spring_stiffness,
                    4.0..=400.0,
                );
                scalar(ui, "Snap damping", &mut home.spring_damping, 1.0..=60.0);
                scalar(
                    ui,
                    "Snap handover velocity",
                    &mut home.snap_velocity,
                    0.05..=5.0,
                );
                scalar(
                    ui,
                    "Wheel px per album",
                    &mut home.wheel_px_per_album,
                    10.0..=400.0,
                );
            });
    }

    fn normalize(tokens: &mut UiTokens) {
        tokens.topbar.normalize();
    }

    pub fn show(ctx: &egui::Context) {
        let mut state = state().lock().expect("UI tuner lock poisoned");
        let mut open = state.open;

        egui::Window::new("UI Tuner · Shared chrome")
            .open(&mut open)
            .default_width(430.0)
            .default_pos(egui::pos2(540.0, 56.0))
            .resizable(true)
            .vscroll(true)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("DEVELOPMENT TOOL — excluded from normal builds")
                        .strong()
                        .color(Color32::from_rgb(0xe0, 0xa8, 0x3c)),
                );
                ui.label("Adjustments apply live to this app and both themes.");
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui.button("Save as project defaults").clicked() {
                        normalize(&mut state.draft);
                        tokens::replace(state.draft.clone());
                        state.status = match save(&state.draft) {
                            Ok(path) => format!(
                                "Saved {}. Rebuild to embed these defaults.",
                                path.display()
                            ),
                            Err(error) => format!("Save failed: {error}"),
                        };
                    }
                    if ui.button("Revert to build defaults").clicked() {
                        state.draft = tokens::embedded();
                        tokens::replace(state.draft.clone());
                        state.status = "Reverted to values embedded in this build.".to_string();
                    }
                    if ui.button("Factory reset").clicked() {
                        state.draft = UiTokens::default();
                        tokens::replace(state.draft.clone());
                        state.status = "Factory defaults loaded (not saved).".to_string();
                    }
                });

                ui.label(RichText::new(&state.status).small());
                ui.separator();

                // Newest work first for quick access.
                home_editor(ui, &mut state.draft.home);

                dock_partition_tracer_editor(ui, &mut state.draft.dock);

                egui::CollapsingHeader::new("Dock · Geometry, shadow & colors")
                    .default_open(false)
                    .show(ui, |ui| {
                        dock_editor(ui, &mut state.draft.dock);
                        dock_theme_editor(
                            ui,
                            "Floating docks · Light colors",
                            &mut state.draft.dock.light,
                        );
                        dock_theme_editor(
                            ui,
                            "Floating docks · Dark colors",
                            &mut state.draft.dock.dark,
                        );
                    });

                egui::CollapsingHeader::new("Top bar & portal (older)")
                    .default_open(false)
                    .show(ui, |ui| {
                        geometry_editor(ui, &mut state.draft.topbar);
                        typography_editor(ui, &mut state.draft.topbar);
                        effects_editor(ui, &mut state.draft.topbar);
                        portal_editor(ui, &mut state.draft.topbar.portal);
                        portal_theme_editor(
                            ui,
                            "Portal menu · Light colors",
                            &mut state.draft.topbar.portal.light,
                        );
                        portal_theme_editor(
                            ui,
                            "Portal menu · Dark colors",
                            &mut state.draft.topbar.portal.dark,
                        );
                        theme_editor(ui, "Light-mode colors", &mut state.draft.topbar.light);
                        theme_editor(ui, "Dark-mode colors", &mut state.draft.topbar.dark);
                    });

                normalize(&mut state.draft);
                state.draft.dock.normalize();
                state.draft.home.normalize();
                tokens::replace(state.draft.clone());
                ctx.request_repaint();
            });

        state.open = open;
    }

    pub(crate) fn portal_preview_menu() -> Option<usize> {
        PORTAL_PREVIEW_LOCKED
            .load(Ordering::Relaxed)
            .then(|| PORTAL_PREVIEW_MENU.load(Ordering::Relaxed))
    }

    pub(crate) fn dock_preview_panel() -> Option<&'static str> {
        if !DOCK_PREVIEW_LOCKED.load(Ordering::Relaxed) {
            return None;
        }
        let mut slot = DOCK_PREVIEW_PANEL
            .lock()
            .expect("UI tuner dock preview lock poisoned");
        if slot.is_none() {
            *slot = Some(leak_panel_id("filters"));
        }
        *slot
    }
}

#[cfg(feature = "ui-tuner")]
pub(crate) use enabled::dock_preview_panel;
#[cfg(feature = "ui-tuner")]
pub(crate) use enabled::portal_preview_menu;
#[cfg(feature = "ui-tuner")]
pub use enabled::show;
