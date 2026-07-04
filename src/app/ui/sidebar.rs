//! Left tools rail layout primitives — bordered collapsible sections and
//! aligned control rows. See `SIDEBAR.md` for usage rules.

use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Id, Layout, Margin, RichText, Sense, Stroke, Ui,
};

/// Theme colors for sidebar sections (sourced from `AtlasApp::palette()`).
#[derive(Clone, Copy)]
pub struct SidebarTheme {
    pub card: Color32,
    pub border: Color32,
    pub ink: Color32,
    pub sub: Color32,
}

pub struct SidebarTokens;

impl SidebarTokens {
    pub const CORNER_RADIUS: u8 = 6;
    pub const INNER_MARGIN_X: f32 = 8.0;
    pub const INNER_MARGIN_Y: f32 = 6.0;
    pub const SECTION_GAP: f32 = 6.0;
    pub const HEADER_HEIGHT: f32 = 18.0;
    pub const CONTROL_ROW_HEIGHT: f32 = 20.0;
    pub const TOOLBAR_ROW_HEIGHT: f32 = 22.0;
    pub const TOGGLE_SIZE: f32 = 8.0;
    pub const TOGGLE_HIT: f32 = 14.0;
    pub const ROW_GAP: f32 = 4.0;
    pub const TOOLBAR_GAP: f32 = 4.0;
    pub const OPTION_GAP: f32 = 4.0;
}

/// Bordered card with a collapsible header row and optional body.
pub fn sidebar_section(
    ui: &mut Ui,
    id: Id,
    title: &str,
    subtitle: Option<&str>,
    expanded: &mut bool,
    theme: SidebarTheme,
    add_body: impl FnOnce(&mut Ui),
) -> bool {
    let mut changed = false;
    ui.add_space(SidebarTokens::SECTION_GAP);

    Frame::new()
        .fill(theme.card)
        .corner_radius(CornerRadius::same(SidebarTokens::CORNER_RADIUS))
        .stroke(Stroke::new(1.0, theme.border))
        .inner_margin(Margin::symmetric(
            SidebarTokens::INNER_MARGIN_X as i8,
            SidebarTokens::INNER_MARGIN_Y as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            changed |= section_header(ui, id, title, subtitle, expanded, theme);
            if *expanded {
                ui.add_space(SidebarTokens::ROW_GAP);
                add_body(ui);
            }
        });

    changed
}

fn section_header(
    ui: &mut Ui,
    id: Id,
    title: &str,
    subtitle: Option<&str>,
    expanded: &mut bool,
    theme: SidebarTheme,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_min_height(SidebarTokens::HEADER_HEIGHT);
        let toggle_label = if *expanded { "−" } else { "+" };
        let toggle_color = theme.sub;
        let toggle = ui.add(
            egui::Label::new(
                RichText::new(toggle_label)
                    .size(SidebarTokens::TOGGLE_SIZE)
                    .color(toggle_color),
            )
            .sense(Sense::click()),
        );
        if toggle.clicked() {
            *expanded = !*expanded;
            ui.ctx()
                .data_mut(|d| d.insert_temp(id.with("expanded"), *expanded));
            changed = true;
        }

        let title_resp = ui.add(
            egui::Label::new(RichText::new(title).strong().color(theme.ink)).sense(Sense::click()),
        );
        if title_resp.clicked() {
            *expanded = !*expanded;
            ui.ctx()
                .data_mut(|d| d.insert_temp(id.with("expanded"), *expanded));
            changed = true;
        }

        if let Some(sub) = subtitle {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(sub).small().color(theme.sub));
            });
        }
    });
    changed
}

/// Primary action buttons in a uniform-height row.
pub fn sidebar_toolbar_row(ui: &mut Ui, add_controls: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SidebarTokens::TOOLBAR_GAP;
        ui.set_min_height(SidebarTokens::TOOLBAR_ROW_HEIGHT);
        add_controls(ui);
    });
    ui.add_space(SidebarTokens::ROW_GAP);
}

/// Full-width checkbox with inline label.
pub fn sidebar_checkbox_row(
    ui: &mut Ui,
    value: &mut bool,
    label: impl Into<egui::WidgetText>,
) -> bool {
    ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
    let changed = ui.checkbox(value, label).changed();
    ui.add_space(SidebarTokens::ROW_GAP);
    changed
}

/// Muted subsection label with top spacing.
pub fn sidebar_subsection_label(ui: &mut Ui, label: &str, theme: SidebarTheme) {
    ui.add_space(SidebarTokens::ROW_GAP);
    ui.label(RichText::new(label).small().color(theme.sub));
    ui.add_space(2.0);
}

/// Muted label followed by a horizontal row of option toggles.
pub fn sidebar_option_group(
    ui: &mut Ui,
    label: &str,
    theme: SidebarTheme,
    add_options: impl FnOnce(&mut Ui),
) {
    sidebar_subsection_label(ui, label, theme);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SidebarTokens::OPTION_GAP;
        ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
        add_options(ui);
    });
    ui.add_space(SidebarTokens::ROW_GAP);
}

/// Groups related controls under a muted region label inside a section body.
pub fn sidebar_region(
    ui: &mut Ui,
    label: &str,
    theme: SidebarTheme,
    add_body: impl FnOnce(&mut Ui),
) {
    sidebar_subsection_label(ui, label, theme);
    add_body(ui);
}

/// Very subtle horizontal rule between sub-regions inside a section card.
pub fn sidebar_subtle_divider(ui: &mut Ui, theme: SidebarTheme) {
    ui.add_space(SidebarTokens::ROW_GAP);
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 8.0), Sense::hover());
    let stroke = Stroke::new(
        1.0,
        theme
            .border
            .gamma_multiply(if ui.visuals().dark_mode { 0.55 } else { 0.85 }),
    );
    ui.painter().hline(rect.x_range(), rect.center().y, stroke);
}

/// Selectable mode pill with a brief inline description and optional hover detail.
pub fn sidebar_mode_row(
    ui: &mut Ui,
    selected: bool,
    mode_label: &str,
    brief: &str,
    hover_detail: &str,
    theme: SidebarTheme,
) -> egui::Response {
    let area = ui.horizontal(|ui| {
        ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
        let mode = ui.selectable_label(selected, mode_label);
        ui.label(RichText::new(brief).small().color(theme.sub));
        mode
    });
    area.response.on_hover_text(hover_detail);
    ui.add_space(SidebarTokens::ROW_GAP);
    area.inner
}

/// Family master row with a layer-style expand toggle for stacked sub-type rows.
pub fn sidebar_family_master_row(
    ui: &mut Ui,
    expanded: &mut bool,
    has_subtypes: bool,
    value: &mut bool,
    swatch_color: Color32,
    label: &str,
    theme: SidebarTheme,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
        if has_subtypes {
            let glyph = if *expanded { "−" } else { "+" };
            let toggle = ui.add(
                egui::Label::new(
                    RichText::new(glyph)
                        .size(SidebarTokens::TOGGLE_SIZE)
                        .color(theme.sub),
                )
                .sense(Sense::click()),
            );
            if toggle.clicked() {
                *expanded = !*expanded;
            }
        } else {
            ui.add_space(SidebarTokens::TOGGLE_HIT);
        }
        ui.scope(|ui| {
            ui.set_width(16.0);
            if ui.checkbox(value, "").changed() {
                changed = true;
            }
        });
        ui.label(RichText::new("■").color(swatch_color));
        ui.label(label);
    });
    changed
}

/// Compact checkbox row for nested sub-type lists (no extra trailing gap).
pub fn sidebar_nested_checkbox_row(
    ui: &mut Ui,
    value: &mut bool,
    label: impl Into<egui::WidgetText>,
) -> bool {
    ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
    ui.checkbox(value, label).changed()
}

/// Vertical rhythm wrapper around slider widgets.
pub fn sidebar_slider_block(ui: &mut Ui, add_slider: impl FnOnce(&mut Ui)) {
    ui.add_space(2.0);
    add_slider(ui);
    ui.add_space(SidebarTokens::ROW_GAP);
}
