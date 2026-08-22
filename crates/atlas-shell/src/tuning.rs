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

#[cfg(not(feature = "ui-tuner"))]
#[inline]
pub(crate) fn dock_advanced_preview() -> bool {
    false
}

#[cfg(not(feature = "ui-tuner"))]
#[inline]
pub fn forcefield_preview_locked() -> bool {
    false
}

#[cfg(feature = "ui-tuner")]
mod enabled {
    use crate::menu::{self, MenuIcon};
    use crate::tokens::{
        self, ActivityHeatmapTokens, BoardForcefieldTokens, BoardPreviewTokens, DockAdvancedTheme,
        DockAdvancedTokens, DockThemeTokens, DockTokens, HomeTokens, MenuThemeTokens, MenuTokens,
        PortalMenuTokens, ReadoutTokens, TopBarThemeTokens, TopBarTokens, UiTokens,
    };
    use eframe::egui::{self, Color32, RichText, Slider};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};

    static PORTAL_PREVIEW_LOCKED: AtomicBool = AtomicBool::new(false);
    static PORTAL_PREVIEW_MENU: AtomicUsize = AtomicUsize::new(0);
    static MENU_PREVIEW_LOCKED: AtomicBool = AtomicBool::new(false);
    static MENU_PREVIEW_KIND: AtomicUsize = AtomicUsize::new(0);
    static DOCK_PREVIEW_LOCKED: AtomicBool = AtomicBool::new(false);
    static DOCK_ADVANCED_PREVIEW: AtomicBool = AtomicBool::new(false);
    static FORCEFIELD_PREVIEW_LOCKED: AtomicBool = AtomicBool::new(false);
    static DOCK_PREVIEW_PANEL: Mutex<Option<&'static str>> = Mutex::new(None);

    /// Which transient menu the tuner holds open.
    const MENU_KIND_PORTAL_FILE: usize = 0;
    const MENU_KIND_PORTAL_VIEW: usize = 1;
    const MENU_KIND_PORTAL_PREFS: usize = 2;
    const MENU_KIND_SAMPLE: usize = 3;

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
        stored.readouts.normalize();
        stored.readouts.round_for_storage();
        stored.activity_heatmap.normalize();
        stored.activity_heatmap.round_for_storage();
        stored.board_preview.normalize();
        stored.board_preview.round_for_storage();
        stored.board_forcefield.normalize();
        stored.board_forcefield.round_for_storage();
        stored.menu.normalize();
        stored.menu.round_for_storage();
        let body = toml::to_string_pretty(&stored).map_err(|error| error.to_string())?;
        let header = concat!(
            "# Canonical shared-chrome design tokens.\n",
            "# Edit directly, or run either app with `--features ui-tuner` for live editing.\n",
            "# Saved tuner values are embedded by the next build.\n\n",
        );
        std::fs::write(&path, format!("{header}{body}")).map_err(|error| error.to_string())?;
        Ok(path)
    }

    /// Returns whether the value changed, for dials that are a view onto some
    /// other token rather than the token itself.
    fn scalar(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
    ) -> bool {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(Slider::new(value, range).show_value(true)).changed()
        })
        .inner
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

    fn dock_advanced_preview_controls(ui: &mut egui::Ui) {
        dock_preview_controls(ui);
        let mut locked = DOCK_ADVANCED_PREVIEW.load(Ordering::Relaxed);
        if ui
            .checkbox(&mut locked, "Lock advanced catalog open")
            .on_hover_text(
                "Keep the Advanced tool canvas visible while these sliders \
                 have the pointer — also locks the parent dock popover.",
            )
            .changed()
        {
            DOCK_ADVANCED_PREVIEW.store(locked, Ordering::Relaxed);
            if locked {
                DOCK_PREVIEW_LOCKED.store(true, Ordering::Relaxed);
            }
        }
        ui.separator();
    }

    /// Everything that dimensions the palette a dock icon opens, in the order
    /// you reach for it: type, icons and their breathing room, the group
    /// frame, the caption, then the dot cluster. Split across `[dock]` and
    /// `[dock.palette]` in the token file, but one thing on screen.
    fn dock_palette_editor(ui: &mut egui::Ui, dock: &mut DockTokens) {
        egui::CollapsingHeader::new("Menu palette · Type, icons, frame & dots")
            .default_open(true)
            .show(ui, |ui| {
                dock_preview_controls(ui);

                ui.label(RichText::new("Placement").strong());
                ui.label(
                    RichText::new(
                        "Gap from the master dock icon to the palettes, and \
                         how far the whole dock sits from the canvas edge.",
                    )
                    .small(),
                );
                scalar(
                    ui,
                    "Palette ↔ master icon gap",
                    &mut dock.popover_gap,
                    0.0..=80.0,
                );
                scalar(
                    ui,
                    "Offset from canvas bottom",
                    &mut dock.bottom_margin,
                    0.0..=96.0,
                );
                scalar(ui, "Offset from canvas left", &mut dock.left_margin, 0.0..=96.0);
                scalar(ui, "Panel stack gap", &mut dock.stack_gap, 0.0..=32.0);

                ui.separator();
                ui.label(RichText::new("Host \u{2194} palette hover").strong());
                ui.label(
                    RichText::new(
                        "Hovering the master icon lights its palettes, and \
                         hovering a palette lights the master icon. Dark mode \
                         lightens the gray; light mode darkens it.",
                    )
                    .small(),
                );
                scalar(
                    ui,
                    "Fill density",
                    &mut dock.palette.associate_fill,
                    0.3..=0.85,
                );
                scalar(
                    ui,
                    "Stroke scale",
                    &mut dock.palette.associate_stroke,
                    1.0..=1.8,
                );
                scalar(
                    ui,
                    "Tint (lighter / darker)",
                    &mut dock.palette.associate_tint,
                    0.0..=0.7,
                );
                ui.separator();
                ui.label(RichText::new("Pinned icon outline").strong());
                ui.label(
                    RichText::new(
                        "A pinned palette's master icon keeps a denser outline \
                         than an undeployed one. Same lighten / darken as hover.",
                    )
                    .small(),
                );
                scalar(
                    ui,
                    "Pinned stroke",
                    &mut dock.palette.pinned_stroke,
                    1.0..=2.4,
                );
                scalar(
                    ui,
                    "Pinned tint",
                    &mut dock.palette.pinned_tint,
                    0.0..=0.7,
                );
                ui.separator();
                ui.label(RichText::new("Primary-dock blister").strong());
                ui.label(
                    RichText::new(
                        "Click beside or below the icon bar to collapse it \
                         into a handle on the readout. Pinned palettes stay.",
                    )
                    .small(),
                );
                scalar(ui, "Blister width", &mut dock.palette.blister_width, 24.0..=160.0);
                scalar(
                    ui,
                    "Blister height",
                    &mut dock.palette.blister_height,
                    4.0..=28.0,
                );
                scalar(ui, "Blister sink", &mut dock.palette.blister_sink, 0.0..=20.0);
                scalar(
                    ui,
                    "Collapse hover zone",
                    &mut dock.palette.collapse_zone,
                    8.0..=48.0,
                );

                ui.separator();
                ui.label(RichText::new("Type").strong());
                ui.label(
                    RichText::new(
                        "Pallet is the name on the box. Category is the name \
                         on the underscore. Icon label is the type on snaps, \
                         grid, and other labeled tools.",
                    )
                    .small(),
                );
                scalar(
                    ui,
                    "Pallet title size",
                    &mut dock.palette.group_label_size,
                    6.0..=24.0,
                );
                scalar(
                    ui,
                    "Pallet title lift",
                    &mut dock.palette.pallet_label_lift,
                    -16.0..=24.0,
                );
                scalar(
                    ui,
                    "Category title size",
                    &mut dock.palette.category_label_size,
                    6.0..=24.0,
                );
                scalar(
                    ui,
                    "Category title lift",
                    &mut dock.palette.group_label_lift,
                    -16.0..=24.0,
                );
                scalar(
                    ui,
                    "Icon label size",
                    &mut dock.palette.labeled_text_size,
                    6.0..=24.0,
                );
                scalar(
                    ui,
                    "Caption title (stacked)",
                    &mut dock.palette.caption_text_size,
                    7.0..=24.0,
                );
                scalar(
                    ui,
                    "Primary icon glyph",
                    &mut dock.icon_text_size,
                    8.0..=24.0,
                );
                scalar(ui, "Icon name chip", &mut dock.label_text_size, 7.0..=20.0);
                scalar(ui, "Hover chip lift", &mut dock.hover_chip_gap, 0.0..=48.0);

                ui.separator();
                ui.label(RichText::new("Icons").strong());
                scalar(ui, "Dock icon size", &mut dock.icon_size, 20.0..=64.0);
                scalar(
                    ui,
                    "Palette icon scale",
                    &mut dock.flyout_icon_scale,
                    0.4..=1.0,
                );
                scalar(ui, "Icon gap", &mut dock.icon_gap, 0.0..=28.0);
                scalar(
                    ui,
                    "Icon buffer (inside frame)",
                    &mut dock.palette.group_pad,
                    0.0..=32.0,
                );
                scalar(
                    ui,
                    "Toggle pair gap",
                    &mut dock.palette.tertiary_stack_gap,
                    0.0..=16.0,
                );

                ui.separator();
                ui.label(RichText::new("Category rule (one underscore)").strong());
                ui.label(
                    RichText::new(
                        "One line under every pallet in this flyout, edge to \
                         edge of those boxes. The label is the category \
                         (Shapes, Document settings) — once, centered. \
                         Pallet names (curves, ink, object snaps) stay on \
                         the boxes.",
                    )
                    .small(),
                );
                scalar(ui, "Rule stroke", &mut dock.palette.rule_stroke, 0.0..=6.0);
                scalar(
                    ui,
                    "Rule offset (below icons)",
                    &mut dock.palette.rule_offset,
                    -16.0..=16.0,
                );
                scalar(
                    ui,
                    "Rule extent (past ends)",
                    &mut dock.palette.rule_extent,
                    -24.0..=32.0,
                );
                scalar(
                    ui,
                    "Title inset from left",
                    &mut dock.palette.group_label_inset,
                    0.0..=32.0,
                );
                scalar(
                    ui,
                    "Gap around title (border & rule)",
                    &mut dock.palette.rule_text_gap,
                    0.0..=16.0,
                );

                ui.separator();
                ui.label(RichText::new("Group frame (icon strip)").strong());
                scalar(
                    ui,
                    "Boundary stroke",
                    &mut dock.palette.group_stroke,
                    0.0..=6.0,
                );
                scalar(
                    ui,
                    "Corner radius",
                    &mut dock.palette.group_radius,
                    0.0..=24.0,
                );
                scalar(
                    ui,
                    "Gap between groups",
                    &mut dock.palette.group_gap,
                    0.0..=48.0,
                );
                scalar(
                    ui,
                    "Reserved dot column",
                    &mut dock.palette.controls_width,
                    8.0..=64.0,
                );

                ui.separator();
                ui.label(RichText::new("Caption & popover (stacked view)").strong());
                scalar(
                    ui,
                    "Caption row height",
                    &mut dock.palette.caption_height,
                    12.0..=48.0,
                );
                scalar(ui, "Popover padding", &mut dock.popover_padding, 0.0..=28.0);
                scalar(
                    ui,
                    "Popover radius",
                    &mut dock.popover_corner_radius,
                    0.0..=28.0,
                );
                scalar(ui, "Popover width", &mut dock.popover_width, 160.0..=520.0);

                ui.separator();
                ui.label(RichText::new("Dot cluster").strong());
                ui.label(
                    RichText::new(
                        "Minimize / Advanced / Drop on every palette; the last \
                         palette along the dock adds the layout toggle.",
                    )
                    .small(),
                );
                scalar(ui, "Dot radius", &mut dock.palette.dot_radius, 0.5..=8.0);
                scalar(ui, "Dot gap", &mut dock.palette.dot_gap, 0.0..=16.0);
                scalar(
                    ui,
                    "Column offset from strip",
                    &mut dock.palette.dot_offset,
                    0.0..=40.0,
                );
                scalar(
                    ui,
                    "Hit slop · across column",
                    &mut dock.palette.dot_hit_x,
                    4.0..=64.0,
                );
                scalar(
                    ui,
                    "Hit slop · across ellipsis",
                    &mut dock.palette.dot_hit_y,
                    4.0..=48.0,
                );
                scalar(
                    ui,
                    "Hit slop · past end dots",
                    &mut dock.palette.dot_hit_end,
                    0.0..=32.0,
                );
                scalar(
                    ui,
                    "Hover chip gap",
                    &mut dock.palette.dot_chip_gap,
                    0.0..=24.0,
                );

                ui.separator();
                ui.label(RichText::new("Colors · dark").strong());
                ui.label(
                    RichText::new(
                        "Pallet text and category text are separate fills. \
                         Rule is only the underscore.",
                    )
                    .small(),
                );
                rgba(ui, "Pallet text", &mut dock.dark.title);
                rgba(ui, "Category text", &mut dock.dark.category);
                rgba(ui, "Rule", &mut dock.dark.rule);
                rgba(ui, "Red", &mut dock.dark.red);
                rgba(ui, "Text", &mut dock.dark.text);
                rgba(ui, "Muted text", &mut dock.dark.muted_text);
                rgba(ui, "Border", &mut dock.dark.border);
                rgba(ui, "Popover fill", &mut dock.dark.popover_fill);

                ui.separator();
                ui.label(RichText::new("Colors · light").strong());
                rgba(ui, "Pallet text", &mut dock.light.title);
                rgba(ui, "Category text", &mut dock.light.category);
                rgba(ui, "Rule", &mut dock.light.rule);
                rgba(ui, "Red", &mut dock.light.red);
                rgba(ui, "Text", &mut dock.light.text);
                rgba(ui, "Muted text", &mut dock.light.muted_text);
                rgba(ui, "Border", &mut dock.light.border);
                rgba(ui, "Popover fill", &mut dock.light.popover_fill);
            });
    }

    fn dock_advanced_editor(ui: &mut egui::Ui, adv: &mut DockAdvancedTokens) {
        egui::CollapsingHeader::new("Dock · Advanced catalog canvas")
            .default_open(true)
            .show(ui, |ui| {
                dock_advanced_preview_controls(ui);
                egui::CollapsingHeader::new("Geometry")
                    .default_open(true)
                    .show(ui, |ui| {
                        scalar(ui, "Width fraction", &mut adv.width_frac, 0.4..=0.95);
                        scalar(ui, "Height fraction", &mut adv.height_frac, 0.4..=0.92);
                        scalar(ui, "Min width", &mut adv.min_width, 320.0..=1600.0);
                        scalar(ui, "Min height", &mut adv.min_height, 240.0..=1200.0);
                        scalar(ui, "Border width", &mut adv.border_width, 1.0..=8.0);
                        scalar(ui, "Frame radius", &mut adv.corner_radius, 0.0..=28.0);
                        scalar(ui, "Card width", &mut adv.card_w, 72.0..=240.0);
                        scalar(ui, "Card height", &mut adv.card_h, 64.0..=200.0);
                        scalar(ui, "Card gap", &mut adv.card_gap, 8.0..=48.0);
                        scalar(ui, "Card radius", &mut adv.card_radius, 0.0..=28.0);
                        scalar(ui, "Portal pad", &mut adv.portal_pad, 8.0..=48.0);
                        scalar(ui, "Portal title", &mut adv.portal_title, 0.0..=40.0);
                        scalar(ui, "Portal gap", &mut adv.portal_gap, 16.0..=96.0);
                        scalar(ui, "Portal radius", &mut adv.portal_radius, 0.0..=28.0);
                        scalar(ui, "Columns", &mut adv.cols, 2.0..=8.0);
                        scalar(ui, "Grid step", &mut adv.grid_step, 12.0..=96.0);
                        scalar(ui, "Zoom min", &mut adv.zoom_min, 0.1..=1.0);
                        scalar(ui, "Zoom max", &mut adv.zoom_max, 0.5..=8.0);
                    });
            });
        dock_advanced_theme_editor(ui, "Advanced catalog · Light colors", &mut adv.light);
        dock_advanced_theme_editor(ui, "Advanced catalog · Dark colors", &mut adv.dark);
    }

    fn dock_advanced_theme_editor(ui: &mut egui::Ui, name: &str, theme: &mut DockAdvancedTheme) {
        egui::CollapsingHeader::new(name)
            .default_open(false)
            .show(ui, |ui| {
                dock_advanced_preview_controls(ui);
                rgba(ui, "Canvas", &mut theme.canvas);
                rgba(ui, "Border", &mut theme.border);
                rgba(ui, "Veil", &mut theme.veil);
                rgba(ui, "Color cast", &mut theme.cast);
                rgba(ui, "Grid", &mut theme.grid);
                rgba(ui, "Portal fill", &mut theme.portal_fill);
                rgba(ui, "Portal border", &mut theme.portal_border);
                rgba(ui, "Card fill", &mut theme.card_fill);
                rgba(ui, "Card border", &mut theme.card_border);
                rgba(ui, "Select", &mut theme.select);
                rgba(ui, "Hover", &mut theme.hover);
                rgba(ui, "On-strip badge", &mut theme.badge);
                rgba(ui, "Text", &mut theme.text);
                rgba(ui, "Muted", &mut theme.muted);
            });
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
        menu_preview_controls(ui);
    }

    fn menu_preview_controls(ui: &mut egui::Ui) {
        let mut locked = MENU_PREVIEW_LOCKED.load(Ordering::Relaxed);
        if ui
            .checkbox(&mut locked, "Lock menu preview open")
            .on_hover_text(
                "Keep a menu visible while the pointer is on these sliders — \
                 same pattern as Lock dock popover open",
            )
            .changed()
        {
            MENU_PREVIEW_LOCKED.store(locked, Ordering::Relaxed);
        }

        if locked {
            let mut kind = MENU_PREVIEW_KIND.load(Ordering::Relaxed);
            egui::ComboBox::from_label("Preview panel")
                .selected_text(menu_kind_label(kind))
                .show_ui(ui, |ui| {
                    ui.label(RichText::new("Icon portal").strong());
                    ui.selectable_value(&mut kind, MENU_KIND_PORTAL_FILE, "Portal · File");
                    ui.selectable_value(&mut kind, MENU_KIND_PORTAL_VIEW, "Portal · View");
                    ui.selectable_value(&mut kind, MENU_KIND_PORTAL_PREFS, "Portal · Preferences");
                    ui.separator();
                    ui.label(RichText::new("Context menu").strong());
                    ui.selectable_value(&mut kind, MENU_KIND_SAMPLE, "Sample right-click");
                });
            MENU_PREVIEW_KIND.store(kind, Ordering::Relaxed);
            sync_portal_lock_from_menu(kind);
        } else {
            PORTAL_PREVIEW_LOCKED.store(false, Ordering::Relaxed);
        }
        ui.separator();
    }

    fn menu_kind_label(kind: usize) -> &'static str {
        match kind {
            MENU_KIND_PORTAL_VIEW => "Portal · View",
            MENU_KIND_PORTAL_PREFS => "Portal · Preferences",
            MENU_KIND_SAMPLE => "Sample right-click",
            _ => "Portal · File",
        }
    }

    fn sync_portal_lock_from_menu(kind: usize) {
        let portal = match kind {
            MENU_KIND_PORTAL_FILE => Some(0),
            MENU_KIND_PORTAL_VIEW => Some(2),
            MENU_KIND_PORTAL_PREFS => Some(3),
            _ => None,
        };
        match portal {
            Some(index) => {
                PORTAL_PREVIEW_LOCKED.store(true, Ordering::Relaxed);
                PORTAL_PREVIEW_MENU.store(index, Ordering::Relaxed);
            }
            None => PORTAL_PREVIEW_LOCKED.store(false, Ordering::Relaxed),
        }
    }

    fn paint_sample_menu(ctx: &egui::Context) {
        if !MENU_PREVIEW_LOCKED.load(Ordering::Relaxed)
            || MENU_PREVIEW_KIND.load(Ordering::Relaxed) != MENU_KIND_SAMPLE
        {
            return;
        }
        let dark = ctx.style().visuals.dark_mode;
        egui::Area::new(egui::Id::new("tuner_menu_sample"))
            .fixed_pos(egui::pos2(48.0, 88.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                menu::frame(dark).show(ui, |ui| {
                    ui.set_min_width(menu::tokens().min_width);
                    menu::heading(ui, "3 object(s)", dark);
                    menu::separator(ui, dark);
                    let _ =
                        menu::item_shortcut(ui, MenuIcon::Duplicate, "Duplicate", "Ctrl+D", dark);
                    let _ = menu::item(ui, MenuIcon::Front, "Bring to front", dark);
                    let _ = menu::submenu(ui, MenuIcon::Settings, "Transform", false, dark);
                    menu::separator(ui, dark);
                    let _ = menu::item_shortcut(ui, MenuIcon::Group, "Group", "Ctrl+G", dark);
                    let _ = menu::item_shortcut(ui, MenuIcon::Lock, "Lock", "Ctrl+L", dark);
                    let _ = menu::toggle(ui, true, "Faint", dark);
                    menu::separator(ui, dark);
                    let _ = menu::row(
                        ui,
                        menu::Row::new(MenuIcon::Trash, "Delete")
                            .shortcut("Del")
                            .danger(),
                        dark,
                    );
                });
            });
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
        egui::CollapsingHeader::new("Portal menu · Placement")
            .default_open(false)
            .show(ui, |ui| {
                portal_preview_controls(ui);
                ui.label(
                    RichText::new(
                        "Look (fill, type, icons, dividers, shadow) lives under Menus. \
                         These knobs only place the icon-portal flyout.",
                    )
                    .small(),
                );
                ui.add_space(4.0);
                scalar(ui, "Panel width", &mut portal.width, 150.0..=420.0);
                scalar(
                    ui,
                    "Submenu width",
                    &mut portal.submenu_width,
                    150.0..=480.0,
                );
                scalar(
                    ui,
                    "Horizontal offset",
                    &mut portal.panel_offset_x,
                    -40.0..=80.0,
                );
                scalar(ui, "Top-bar gap", &mut portal.panel_gap, 0.0..=24.0);
                scalar(ui, "Submenu gap", &mut portal.submenu_gap, 0.0..=24.0);
                scalar(ui, "Hover close delay", &mut portal.close_delay, 0.0..=1.0);
            });
    }

    fn menu_editor(ui: &mut egui::Ui, menu: &mut MenuTokens) {
        egui::CollapsingHeader::new("Menus · Geometry & spacing")
            .default_open(true)
            .show(ui, |ui| {
                menu_preview_controls(ui);
                ui.label(
                    RichText::new(
                        "One language for every dropdown and right-click menu in \
                         File Atlas and Slate — including the icon-portal flyout. \
                         Filleted panel, no border, inset section rules, icon + label \
                         + chevron.",
                    )
                    .small(),
                );
                ui.add_space(4.0);
                ui.label(RichText::new("Panel").strong());
                scalar(ui, "Corner radius", &mut menu.corner_radius, 0.0..=28.0);
                scalar(ui, "Border width", &mut menu.border_width, 0.0..=2.0);
                scalar(ui, "Panel padding", &mut menu.panel_padding, 2.0..=28.0);
                scalar(ui, "Minimum width", &mut menu.min_width, 140.0..=420.0);
                ui.add_space(6.0);
                ui.label(RichText::new("Rows").strong());
                scalar(ui, "Row height", &mut menu.row_height, 20.0..=48.0);
                scalar(ui, "Row pad X", &mut menu.row_pad_x, 4.0..=28.0);
                scalar(ui, "Row gap", &mut menu.row_gap, 0.0..=12.0);
                scalar(ui, "Hover radius", &mut menu.hover_radius, 0.0..=16.0);
                ui.add_space(6.0);
                ui.label(RichText::new("Icon & chevron").strong());
                scalar(ui, "Icon size", &mut menu.icon_size, 8.0..=22.0);
                scalar(ui, "Icon gap", &mut menu.icon_gap, 4.0..=20.0);
                scalar(ui, "Icon stroke", &mut menu.icon_stroke, 0.6..=2.4);
                scalar(ui, "Chevron size", &mut menu.chevron_size, 6.0..=20.0);
                ui.add_space(6.0);
                ui.label(RichText::new("Dividers").strong());
                scalar(ui, "Divider inset", &mut menu.divider_inset, 0.0..=32.0);
                scalar(
                    ui,
                    "Divider thickness",
                    &mut menu.divider_thickness,
                    0.5..=2.0,
                );
                scalar(ui, "Divider gap", &mut menu.divider_gap, 0.0..=16.0);
            });

        egui::CollapsingHeader::new("Menus · Typography")
            .default_open(true)
            .show(ui, |ui| {
                menu_preview_controls(ui);
                scalar(ui, "Text size", &mut menu.text_size, 9.0..=20.0);
                scalar(ui, "Letter spacing", &mut menu.letter_spacing, 0.0..=1.5);
                scalar(
                    ui,
                    "Shortcut text size",
                    &mut menu.shortcut_text_size,
                    8.0..=16.0,
                );
            });

        egui::CollapsingHeader::new("Menus · Shadow")
            .default_open(false)
            .show(ui, |ui| {
                menu_preview_controls(ui);
                scalar(
                    ui,
                    "Shadow X offset",
                    &mut menu.shadow_offset_x,
                    -20.0..=20.0,
                );
                scalar(
                    ui,
                    "Shadow Y offset",
                    &mut menu.shadow_offset_y,
                    -8.0..=32.0,
                );
                scalar(ui, "Shadow blur", &mut menu.shadow_blur, 0.0..=48.0);
                scalar(ui, "Shadow spread", &mut menu.shadow_spread, 0.0..=12.0);
                scalar(ui, "Shadow opacity", &mut menu.shadow_opacity, 0.0..=0.6);
            });

        menu_theme_editor(ui, "Menus · Light colors", &mut menu.light);
        menu_theme_editor(ui, "Menus · Dark colors", &mut menu.dark);
    }

    fn menu_theme_editor(ui: &mut egui::Ui, name: &str, theme: &mut MenuThemeTokens) {
        egui::CollapsingHeader::new(name)
            .default_open(false)
            .show(ui, |ui| {
                menu_preview_controls(ui);
                rgba(ui, "Panel fill", &mut theme.fill);
                rgba(ui, "Hover row", &mut theme.hover);
                rgba(ui, "Text", &mut theme.text);
                rgba(ui, "Muted / shortcut", &mut theme.muted);
                rgba(ui, "Icon", &mut theme.icon);
                rgba(ui, "Divider", &mut theme.divider);
                rgba(ui, "Danger", &mut theme.danger);
                rgba(ui, "Border (if width > 0)", &mut theme.border);
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
                scalar(ui, "Label chip gap", &mut dock.hover_chip_gap, 0.0..=48.0);
            });
    }

    fn dock_editor(ui: &mut egui::Ui, dock: &mut DockTokens) {
        egui::CollapsingHeader::new("Floating docks · Geometry")
            .default_open(false)
            .show(ui, |ui| {
                dock_preview_controls(ui);
                scalar(ui, "Icon size", &mut dock.icon_size, 20.0..=64.0);
                scalar(
                    ui,
                    "Flyout icon scale",
                    &mut dock.flyout_icon_scale,
                    0.4..=1.0,
                );
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
                rgba(ui, "Pallet text", &mut theme.title);
                rgba(ui, "Category text", &mut theme.category);
                rgba(ui, "Rule", &mut theme.rule);
                rgba(ui, "Red", &mut theme.red);
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

    fn board_preview_editor(ui: &mut egui::Ui, preview: &mut BoardPreviewTokens) {
        egui::CollapsingHeader::new("Slate board · Selection & hover preview")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Applies to every board node. Selected chrome is immediate. \
                         Body hover eases in and falls off after the pointer leaves. \
                         Edge hover is cursor-only — it does not paint this outline.",
                    )
                    .small(),
                );
                ui.add_space(4.0);
                ui.label(RichText::new("Selected object").strong());
                scalar(
                    ui,
                    "Line weight (px)",
                    &mut preview.select_line_weight,
                    0.5..=6.0,
                );
                scalar(ui, "Opacity", &mut preview.select_opacity, 0.05..=1.0);
                ui.add_space(6.0);
                ui.label(RichText::new("Hover highlight").strong());
                scalar(
                    ui,
                    "Line weight (px)",
                    &mut preview.hover_line_weight,
                    0.5..=6.0,
                );
                scalar(ui, "Opacity", &mut preview.hover_opacity, 0.05..=1.0);
                scalar(ui, "Highlight in (s)", &mut preview.highlight_in, 0.0..=1.5);
                scalar(ui, "Falloff (s)", &mut preview.highlight_out, 0.0..=1.5);
            });
    }

    fn board_forcefield_editor(ui: &mut egui::Ui, field: &mut BoardForcefieldTokens) {
        egui::CollapsingHeader::new("Slate board · Forcefield snap guides")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Alignment snap fires a short ribbon from the impact: it grows \
                         off the canvas in both directions, thickest and brightest at \
                         the center, then fades. Drag a node to see it, or lock a \
                         preview pulse on the board.",
                    )
                    .small(),
                );
                ui.add_space(4.0);
                let mut locked = FORCEFIELD_PREVIEW_LOCKED.load(Ordering::Relaxed);
                if ui
                    .checkbox(&mut locked, "Lock forcefield preview on the board")
                    .changed()
                {
                    FORCEFIELD_PREVIEW_LOCKED.store(locked, Ordering::Relaxed);
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Timing").strong());
                scalar(ui, "Expand (s)", &mut field.expand_secs, 0.02..=0.80);
                scalar(ui, "Fade (s)", &mut field.fade_secs, 0.02..=0.80);
                ui.add_space(6.0);
                ui.label(RichText::new("Stroke").strong());
                scalar(
                    ui,
                    "Center weight (px)",
                    &mut field.center_weight,
                    0.05..=4.0,
                );
                scalar(ui, "Edge weight (px)", &mut field.edge_weight, 0.0..=4.0);
                scalar(ui, "Center opacity", &mut field.center_opacity, 0.0..=1.0);
                scalar(ui, "Edge opacity", &mut field.edge_opacity, 0.0..=1.0);
            });
    }

    fn readouts_editor(ui: &mut egui::Ui, readouts: &mut ReadoutTokens) {
        egui::CollapsingHeader::new("Readout bar (bottom)")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "The strip carrying the gear menu, live counts, root path, and \
                         the activity timeline. Several readouts share these few vertical \
                         pixels, so pack them by eye.",
                    )
                    .small(),
                );
                ui.add_space(4.0);
                scalar(ui, "Text size (pt)", &mut readouts.text_size, 7.0..=20.0);
                scalar(ui, "Pad above (px)", &mut readouts.pad_top, 0.0..=24.0);
                scalar(ui, "Pad below (px)", &mut readouts.pad_bottom, 0.0..=24.0);
                scalar(
                    ui,
                    "Metrics row → timeline (px)",
                    &mut readouts.row_gap,
                    0.0..=24.0,
                );
                scalar(ui, "Item spacing (px)", &mut readouts.item_gap, 0.0..=24.0);
                scalar(
                    ui,
                    "Min row height, 0 = auto (px)",
                    &mut readouts.row_height,
                    0.0..=48.0,
                );
                ui.checkbox(&mut readouts.separators, "Separator rules");
                ui.add_space(6.0);
                ui.label(RichText::new("Collapse chevron (lower-left)").strong());
                scalar(ui, "Chevron size", &mut readouts.chevron_size, 2.0..=16.0);
                scalar(ui, "Hit size", &mut readouts.chevron_hit, 6.0..=28.0);
                scalar(ui, "Inset X", &mut readouts.chevron_inset_x, 0.0..=24.0);
                scalar(ui, "Inset Y", &mut readouts.chevron_inset_y, 0.0..=16.0);
                scalar(ui, "Stroke", &mut readouts.chevron_stroke, 0.4..=2.4);
                scalar(
                    ui,
                    "Idle opacity",
                    &mut readouts.chevron_idle_opacity,
                    0.08..=1.0,
                );
                scalar(
                    ui,
                    "Hover opacity",
                    &mut readouts.chevron_hover_opacity,
                    0.2..=1.0,
                );
                scalar(
                    ui,
                    "Hover fill",
                    &mut readouts.chevron_hover_fill,
                    0.0..=0.4,
                );
                scalar(ui, "Emboss", &mut readouts.chevron_emboss, 0.0..=0.6);
            });
    }

    fn activity_heatmap_editor(ui: &mut egui::Ui, heat: &mut ActivityHeatmapTokens) {
        egui::CollapsingHeader::new("Activity timeline · Selection appearance")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "One axis for the contribution graph and the range handles: \
                         in-selection buckets stay fully saturated, the rest are muted. \
                         Prefer mute over a heavy border.",
                    )
                    .small(),
                );
                ui.add_space(4.0);
                ui.label(RichText::new("Out-of-selection mute").strong());
                scalar(ui, "Opacity", &mut heat.out_of_range_opacity, 0.05..=1.0);
                scalar(
                    ui,
                    "Saturation",
                    &mut heat.out_of_range_saturation,
                    0.0..=1.0,
                );
                ui.separator();
                ui.label(RichText::new("Optional in-selection stroke (usually off)").strong());
                scalar(
                    ui,
                    "Stroke width (px)",
                    &mut heat.selected_stroke_width,
                    0.0..=3.0,
                );
                scalar(
                    ui,
                    "Stroke opacity",
                    &mut heat.selected_stroke_opacity,
                    0.0..=1.0,
                );
            });

        egui::CollapsingHeader::new("Activity timeline · Geometry & semantic zoom")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "The two morph curves are spans in days visible, wide → narrow. \
                         Stagger shears week columns into per-day slots; expansion then \
                         flattens the weekday staircase into a full-height bucket strip. \
                         Large monitors can hold the grid legible longer, so these are \
                         dials rather than constants.",
                    )
                    .small(),
                );
                ui.add_space(4.0);
                ui.label(RichText::new("Grid").strong());
                scalar(ui, "Cell (px)", &mut heat.cell, 4.0..=28.0);
                scalar(ui, "Cell gap (px)", &mut heat.cell_gap, 0.0..=8.0);
                ui.separator();
                ui.label(RichText::new("Rail & scale").strong());
                scalar(ui, "Rail height (px)", &mut heat.rail_height, 10.0..=48.0);
                scalar(
                    ui,
                    "Scale height, min (px)",
                    &mut heat.scale_height,
                    12.0..=48.0,
                );
                ui.separator();
                ui.label(RichText::new("Stagger (days visible)").strong());
                scalar(ui, "Begins at", &mut heat.stagger_begin_days, 8.0..=400.0);
                scalar(ui, "Complete at", &mut heat.stagger_full_days, 1.0..=60.0);
                ui.label(RichText::new("Expansion (days visible)").strong());
                scalar(ui, "Begins at", &mut heat.expand_begin_days, 1.0..=60.0);
                scalar(ui, "Complete at", &mut heat.expand_full_days, 0.02..=7.0);
                ui.separator();
                ui.label(RichText::new("Detail").strong());
                scalar(ui, "Min bucket (px)", &mut heat.min_bucket_px, 1.0..=40.0);
                scalar(
                    ui,
                    "File dashes below (days)",
                    &mut heat.file_tick_days,
                    0.01..=14.0,
                );
                scalar(ui, "Dash width (px)", &mut heat.file_tick_width, 0.5..=4.0);
                ui.separator();
                ui.label(RichText::new("Wheel").strong());
                scalar(
                    ui,
                    "Deepest zoom (s visible)",
                    &mut heat.min_view_secs,
                    5.0..=86_400.0,
                );
                scalar(ui, "Zoom per notch", &mut heat.zoom_per_notch, 1.02..=2.0);
                scalar(ui, "Pan per notch", &mut heat.pan_per_notch, 0.01..=1.0);
                scalar(ui, "Zoom ease (s)", &mut heat.zoom_ease, 0.0..=0.6);
                ui.checkbox(&mut heat.pan_invert, "Invert wheel pan direction");
            });

        egui::CollapsingHeader::new("Activity timeline · Padding & positions")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Where the parts sit, top to bottom: info row, graph, handle rail, \
                         tick scale. The left inset is the weekday gutter above; the right \
                         inset stops the axis short of the panel edge.",
                    )
                    .small(),
                );
                ui.add_space(4.0);
                ui.label(RichText::new("Vertical (px)").strong());
                // The seven-row structure is fixed, so this is `cell` solved
                // from the other end — the end you actually care about when
                // budgeting the readout bar's height.
                let mut graph_h = heat.grid_height();
                if scalar(ui, "Graph height", &mut graph_h, 40.0..=208.0) {
                    heat.set_grid_height(graph_h);
                }
                scalar(ui, "Above info row", &mut heat.pad_top, 0.0..=24.0);
                scalar(ui, "Info row → graph", &mut heat.row_gap, 0.0..=24.0);
                scalar(ui, "Below scale", &mut heat.pad_bottom, 0.0..=24.0);
                ui.separator();
                ui.label(RichText::new("Horizontal insets (px)").strong());
                scalar(
                    ui,
                    "Left · weekday gutter",
                    &mut heat.day_label_width,
                    0.0..=48.0,
                );
                scalar(ui, "Right", &mut heat.pad_right, 0.0..=48.0);
                ui.separator();
                ui.label(RichText::new("Info row").strong());
                scalar(ui, "Text size (pt)", &mut heat.info_text, 7.0..=18.0);
                scalar(ui, "Item spacing (px)", &mut heat.info_gap, 0.0..=24.0);
                scalar(
                    ui,
                    "Button padding · x (px)",
                    &mut heat.info_button_pad_x,
                    0.0..=12.0,
                );
                scalar(
                    ui,
                    "Button padding · y (px)",
                    &mut heat.info_button_pad_y,
                    0.0..=12.0,
                );
                scalar(
                    ui,
                    "Min row height, 0 = auto (px)",
                    &mut heat.info_row_height,
                    0.0..=48.0,
                );
                scalar(ui, "Legend swatch (px)", &mut heat.legend_cell, 4.0..=20.0);
                scalar(ui, "Legend gap (px)", &mut heat.legend_gap, 0.0..=8.0);
                ui.separator();
                ui.label(RichText::new("Labels").strong());
                scalar(ui, "Font size (pt)", &mut heat.label_font, 6.0..=16.0);
                scalar(
                    ui,
                    "Weekday inset (px)",
                    &mut heat.weekday_label_dx,
                    0.0..=16.0,
                );
                ui.separator();
                ui.label(RichText::new("Handle rail").strong());
                scalar(ui, "Band inset (px)", &mut heat.rail_inset, 0.0..=12.0);
                scalar(ui, "Handle radius (px)", &mut heat.handle_radius, 0.0..=6.0);
                scalar(ui, "Handle hit size (px)", &mut heat.handle_hit, 6.0..=32.0);
                scalar(
                    ui,
                    "Min grip width (px)",
                    &mut heat.grip_min_width,
                    8.0..=80.0,
                );
                ui.separator();
                ui.label(RichText::new("Tick scale").strong());
                ui.label(
                    RichText::new(
                        "The band grows to fit the label, so these push the dates \
                         around without ever clipping them.",
                    )
                    .small(),
                );
                scalar(ui, "Top gap (px)", &mut heat.scale_top_gap, 0.0..=16.0);
                scalar(ui, "Tick length (px)", &mut heat.scale_tick_len, 0.0..=16.0);
                scalar(
                    ui,
                    "Tick → label (px)",
                    &mut heat.scale_label_gap,
                    0.0..=16.0,
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
                dock_palette_editor(ui, &mut state.draft.dock);

                dock_advanced_editor(ui, &mut state.draft.dock.advanced);

                menu_editor(ui, &mut state.draft.menu);

                board_preview_editor(ui, &mut state.draft.board_preview);

                board_forcefield_editor(ui, &mut state.draft.board_forcefield);

                readouts_editor(ui, &mut state.draft.readouts);

                activity_heatmap_editor(ui, &mut state.draft.activity_heatmap);

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
                        theme_editor(ui, "Light-mode colors", &mut state.draft.topbar.light);
                        theme_editor(ui, "Dark-mode colors", &mut state.draft.topbar.dark);
                    });

                normalize(&mut state.draft);
                state.draft.dock.normalize();
                state.draft.home.normalize();
                state.draft.activity_heatmap.normalize();
                state.draft.board_preview.normalize();
                state.draft.board_forcefield.normalize();
                state.draft.menu.normalize();
                tokens::replace(state.draft.clone());
                ctx.request_repaint();
            });

        state.open = open;
        paint_sample_menu(ctx);
    }

    pub(crate) fn portal_preview_menu() -> Option<usize> {
        PORTAL_PREVIEW_LOCKED
            .load(Ordering::Relaxed)
            .then(|| PORTAL_PREVIEW_MENU.load(Ordering::Relaxed))
    }

    pub(crate) fn dock_advanced_preview() -> bool {
        DOCK_ADVANCED_PREVIEW.load(Ordering::Relaxed)
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

    pub fn forcefield_preview_locked() -> bool {
        FORCEFIELD_PREVIEW_LOCKED.load(Ordering::Relaxed)
    }
}

#[cfg(feature = "ui-tuner")]
pub(crate) use enabled::dock_advanced_preview;
#[cfg(feature = "ui-tuner")]
pub(crate) use enabled::dock_preview_panel;
#[cfg(feature = "ui-tuner")]
pub use enabled::forcefield_preview_locked;
#[cfg(feature = "ui-tuner")]
pub(crate) use enabled::portal_preview_menu;
#[cfg(feature = "ui-tuner")]
pub use enabled::show;
