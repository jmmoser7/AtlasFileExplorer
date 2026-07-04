//! Left tools rail layout primitives — collapsible sections and
//! type-grouped controls. See `SIDEBAR.md` for usage rules.

use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Id, Layout, Margin, RichText, Sense, Stroke, Ui,
    Vec2,
};

/// Theme colors for sidebar sections (sourced from `AtlasApp::palette()`).
#[derive(Clone, Copy)]
pub struct SidebarTheme {
    pub card: Color32,
    pub border: Color32,
    pub ink: Color32,
    pub sub: Color32,
    pub line: Color32,
}

pub struct SidebarTokens;

impl SidebarTokens {
    pub const CORNER_RADIUS: u8 = 6;
    pub const INNER_MARGIN_X: f32 = 6.0;
    pub const INNER_MARGIN_Y: f32 = 3.5;
    pub const SECTION_GAP: f32 = 4.0;
    pub const RAIL_TOP_LIFT: f32 = 6.5;
    pub const HEADER_HEIGHT: f32 = 14.0;
    pub const CONTROL_ROW_HEIGHT: f32 = 18.0;
    pub const TOGGLE_SIZE: f32 = 8.0;
    pub const ROW_GAP: f32 = 2.0;
    pub const OPTION_GAP: f32 = 3.0;
    pub const RIGHT_COL_WIDTH: f32 = 60.0;
    pub const GROUP_DIVIDER_OPACITY: f32 = 0.22;
    pub const GROUP_DIVIDER_PAD: f32 = 3.0;
    pub const ACTION_ITEM_PAD: f32 = 8.0;
}

/// Shared rail/handle dimensions — matches the date timeline in basic filters.
pub struct SidebarSliderStyle;

impl SidebarSliderStyle {
    pub const RAIL_HEIGHT: f32 = 4.0;
    pub const RAIL_STROKE: f32 = 1.5;
    pub const HANDLE_RADIUS: f32 = 4.5;
    pub const INTERACT_HEIGHT: f32 = Self::HANDLE_RADIUS * 2.5;
    pub const LABEL_GAP: f32 = 0.4;
    pub const BETWEEN: f32 = 3.0;
}

pub fn apply_sidebar_slider_style(ui: &mut Ui) {
    ui.spacing_mut().slider_width = ui.available_width();
    ui.spacing_mut().slider_rail_height = SidebarSliderStyle::RAIL_HEIGHT;
    ui.spacing_mut().interact_size.y = SidebarSliderStyle::INTERACT_HEIGHT;
    ui.spacing_mut().item_spacing.y = 0.0;
}

pub fn sidebar_slider_rail_stroke(theme: SidebarTheme) -> Stroke {
    Stroke::new(
        SidebarSliderStyle::RAIL_STROKE,
        theme.border.gamma_multiply(0.9),
    )
}

/// Collapsible capsule — fill only, no outer border.
pub fn sidebar_section(
    ui: &mut Ui,
    id: Id,
    title: &str,
    subtitle: Option<&str>,
    expanded: &mut bool,
    theme: SidebarTheme,
    first: bool,
    add_body: impl FnOnce(&mut Ui),
) -> bool {
    let mut changed = false;
    let top_gap = if first {
        (SidebarTokens::SECTION_GAP - SidebarTokens::RAIL_TOP_LIFT).max(0.0)
    } else {
        SidebarTokens::SECTION_GAP
    };
    ui.add_space(top_gap);

    Frame::new()
        .fill(theme.card)
        .corner_radius(CornerRadius::same(SidebarTokens::CORNER_RADIUS))
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
        ui.spacing_mut().item_spacing.x = 3.0;
        ui.set_min_height(SidebarTokens::HEADER_HEIGHT);
        let toggle_label = if *expanded { "−" } else { "+" };
        let toggle = ui.add(
            egui::Label::new(
                RichText::new(toggle_label)
                    .size(SidebarTokens::TOGGLE_SIZE)
                    .color(theme.sub),
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

pub fn sidebar_group_divider(ui: &mut Ui, theme: SidebarTheme) {
    ui.add_space(SidebarTokens::GROUP_DIVIDER_PAD);
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
    let color = theme
        .line
        .gamma_multiply(SidebarTokens::GROUP_DIVIDER_OPACITY);
    ui.painter().rect_filled(rect, CornerRadius::ZERO, color);
    ui.add_space(SidebarTokens::GROUP_DIVIDER_PAD);
}

pub fn sidebar_control_group(
    ui: &mut Ui,
    theme: SidebarTheme,
    divider_before: bool,
    add_content: impl FnOnce(&mut Ui),
) {
    if divider_before {
        sidebar_group_divider(ui, theme);
    }
    add_content(ui);
}

pub fn sidebar_subtle_divider(ui: &mut Ui, theme: SidebarTheme) {
    sidebar_group_divider(ui, theme);
}

pub fn sidebar_actions_column(ui: &mut Ui, add_actions: impl FnOnce(&mut Ui)) {
    ui.with_layout(Layout::top_down(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = SidebarTokens::ACTION_ITEM_PAD;
        add_actions(ui);
    });
}

pub fn sidebar_action_block(
    ui: &mut Ui,
    theme: SidebarTheme,
    description: &str,
    add_control: impl FnOnce(&mut Ui),
) {
    add_control(ui);
    ui.label(RichText::new(description).small().color(theme.sub));
}

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

pub fn sidebar_subsection_label(ui: &mut Ui, label: &str, theme: SidebarTheme) {
    ui.add_space(SidebarTokens::ROW_GAP);
    ui.label(RichText::new(label).small().color(theme.sub));
    ui.add_space(2.0);
}

pub fn sidebar_option_group(
    ui: &mut Ui,
    label: &str,
    theme: SidebarTheme,
    add_options: impl FnOnce(&mut Ui),
) {
    ui.label(RichText::new(label).small().color(theme.sub));
    ui.add_space(1.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SidebarTokens::OPTION_GAP;
        ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
        add_options(ui);
    });
    ui.add_space(SidebarTokens::ROW_GAP);
}

pub fn sidebar_region(
    ui: &mut Ui,
    label: &str,
    theme: SidebarTheme,
    add_body: impl FnOnce(&mut Ui),
) {
    sidebar_subsection_label(ui, label, theme);
    add_body(ui);
}

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

pub fn sidebar_family_row(
    ui: &mut Ui,
    value: &mut bool,
    swatch_color: Color32,
    label: &str,
) -> bool {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
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

pub fn sidebar_sliders_group(ui: &mut Ui, add_sliders: impl FnOnce(&mut Ui)) {
    ui.with_layout(Layout::top_down(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = SidebarSliderStyle::BETWEEN;
        add_sliders(ui);
    });
}
