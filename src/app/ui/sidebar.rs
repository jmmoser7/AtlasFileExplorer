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
    pub const RIGHT_COL_WIDTH: f32 = 60.0;
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
pub fn sidebar_checkbox_row(ui: &mut Ui, value: &mut bool, label: impl Into<egui::WidgetText>) -> bool {
    ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
    let changed = ui.checkbox(value, label).changed();
    ui.add_space(SidebarTokens::ROW_GAP);
    changed
}

/// Label left, custom control in a fixed-width right column.
pub fn sidebar_labeled_row(
    ui: &mut Ui,
    label: &str,
    theme: SidebarTheme,
    add_control: impl FnOnce(&mut Ui),
) {
    ui.horizontal(|ui| {
        ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
        ui.label(RichText::new(label).small().color(theme.sub));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.set_width(SidebarTokens::RIGHT_COL_WIDTH);
            add_control(ui);
        });
    });
    ui.add_space(SidebarTokens::ROW_GAP);
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

/// Checkbox + colored swatch + label in aligned columns.
pub fn sidebar_family_row(
    ui: &mut Ui,
    value: &mut bool,
    swatch_color: Color32,
    label: &str,
) -> bool {
    ui.horizontal(|ui| {
        ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
        let mut changed = false;
        ui.scope(|ui| {
            ui.set_width(16.0);
            if ui.checkbox(value, "").changed() {
                changed = true;
            }
        });
        ui.label(RichText::new("■").color(swatch_color));
        ui.label(label);
        changed
    })
    .inner
}

/// Vertical rhythm wrapper around slider widgets.
pub fn sidebar_slider_block(ui: &mut Ui, add_slider: impl FnOnce(&mut Ui)) {
    ui.add_space(2.0);
    add_slider(ui);
    ui.add_space(SidebarTokens::ROW_GAP);
}
