//! Bottom readout sub-dashboard capsules — bordered, resizable cards that
//! sit in the strip above the metrics ticker. See `READOUTS.md`.

use super::sidebar::SidebarTokens;
use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, Frame, Id, Margin, Rect, RichText, Sense, Stroke, Ui,
    Vec2,
};

/// Theme colors for readout sub-dashboards (sourced from `AtlasApp::palette()`).
#[derive(Clone, Copy)]
pub struct ReadoutDashboardTheme {
    pub card: Color32,
    pub border: Color32,
    pub ink: Color32,
    pub sub: Color32,
}

pub struct ReadoutDashboardTokens;

impl ReadoutDashboardTokens {
    pub const CORNER_RADIUS: u8 = SidebarTokens::CORNER_RADIUS;
    pub const INNER_MARGIN_X: f32 = SidebarTokens::INNER_MARGIN_X;
    pub const INNER_MARGIN_Y: f32 = SidebarTokens::INNER_MARGIN_Y;
    pub const HEADER_HEIGHT: f32 = SidebarTokens::HEADER_HEIGHT;
    pub const STRIP_GAP: f32 = SidebarTokens::SECTION_GAP;
    pub const TOGGLE_SIZE: f32 = SidebarTokens::TOGGLE_SIZE;
    pub const TOGGLE_HIT: f32 = SidebarTokens::TOGGLE_HIT;
    pub const ROW_GAP: f32 = SidebarTokens::ROW_GAP;
    pub const EDGE_HANDLE: f32 = 6.0;
    pub const MIN_WIDTH_FRAC: f32 = 0.25;
    pub const MAX_WIDTH_FRAC: f32 = 1.0;
    pub const DEFAULT_WIDTH_FRAC: f32 = 0.62;
    pub const COLLAPSED_WIDTH_FRAC: f32 = 0.28;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResizeEdge {
    Left,
    Right,
}

/// Capsule card for a bottom-bar sub-dashboard: rounded border, header toggle,
/// optional body, and left/right edge drag to resize width along the bar.
pub fn readout_dashboard_capsule(
    ui: &mut Ui,
    id: Id,
    title: &str,
    expanded: &mut bool,
    width_frac: &mut f32,
    theme: ReadoutDashboardTheme,
    add_body: impl FnOnce(&mut Ui),
) -> bool {
    let mut changed = false;
    let bar_width = ui.available_width().max(1.0);
    *width_frac = width_frac.clamp(
        ReadoutDashboardTokens::MIN_WIDTH_FRAC,
        ReadoutDashboardTokens::MAX_WIDTH_FRAC,
    );
    let capsule_w = (bar_width * *width_frac).max(120.0);

    let outer = ui.allocate_ui_with_layout(
        Vec2::new(capsule_w, 0.0),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            ui.set_width(capsule_w);
            Frame::new()
                .fill(theme.card)
                .corner_radius(CornerRadius::same(ReadoutDashboardTokens::CORNER_RADIUS))
                .stroke(Stroke::new(1.0, theme.border))
                .inner_margin(Margin::symmetric(
                    ReadoutDashboardTokens::INNER_MARGIN_X as i8,
                    ReadoutDashboardTokens::INNER_MARGIN_Y as i8,
                ))
                .show(ui, |ui| {
                    changed |= dashboard_header(ui, id, title, expanded, width_frac, theme);
                    if *expanded {
                        ui.add_space(ReadoutDashboardTokens::ROW_GAP);
                        add_body(ui);
                    }
                });
        },
    );

    if *expanded {
        let frame_rect = outer.response.rect;
        changed |= handle_edge_resize(
            ui,
            id.with("resize_left"),
            ResizeEdge::Left,
            frame_rect,
            bar_width,
            width_frac,
        );
        changed |= handle_edge_resize(
            ui,
            id.with("resize_right"),
            ResizeEdge::Right,
            frame_rect,
            bar_width,
            width_frac,
        );
    }

    changed
}

fn dashboard_header(
    ui: &mut Ui,
    id: Id,
    title: &str,
    expanded: &mut bool,
    width_frac: &mut f32,
    theme: ReadoutDashboardTheme,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_min_height(ReadoutDashboardTokens::HEADER_HEIGHT);
        let toggle_label = if *expanded { "−" } else { "+" };
        let toggle = ui.add(
            egui::Label::new(
                RichText::new(toggle_label)
                    .size(ReadoutDashboardTokens::TOGGLE_SIZE)
                    .color(theme.sub),
            )
            .sense(Sense::click()),
        );
        let title_resp = ui.add(
            egui::Label::new(RichText::new(title).strong().color(theme.ink)).sense(Sense::click()),
        );

        if toggle.clicked() || title_resp.clicked() {
            if *expanded {
                readout_dashboard_set_fully_contracted(expanded, width_frac);
            } else {
                readout_dashboard_set_fully_expanded(expanded, width_frac);
            }
            ui.ctx()
                .data_mut(|d| d.insert_temp(id.with("expanded"), *expanded));
            changed = true;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if *expanded {
                ui.label(
                    RichText::new("drag edges")
                        .small()
                        .color(theme.sub.gamma_multiply(0.85)),
                )
                .on_hover_text("Drag the left or right capsule edge to resize width");
            }
        });
    });
    changed
}

fn handle_edge_resize(
    ui: &mut Ui,
    id: Id,
    edge: ResizeEdge,
    frame_rect: Rect,
    bar_width: f32,
    width_frac: &mut f32,
) -> bool {
    let handle_rect = match edge {
        ResizeEdge::Left => Rect::from_min_max(
            frame_rect.left_top(),
            egui::pos2(
                frame_rect.left() + ReadoutDashboardTokens::EDGE_HANDLE,
                frame_rect.bottom(),
            ),
        ),
        ResizeEdge::Right => Rect::from_min_max(
            egui::pos2(
                frame_rect.right() - ReadoutDashboardTokens::EDGE_HANDLE,
                frame_rect.top(),
            ),
            frame_rect.right_bottom(),
        ),
    };

    let resp = ui.interact(handle_rect, id, Sense::drag());
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    if resp.dragged() {
        let delta = ui.input(|i| i.pointer.delta().x);
        let adjust = match edge {
            ResizeEdge::Left => -delta / bar_width,
            ResizeEdge::Right => delta / bar_width,
        };
        *width_frac = (*width_frac + adjust).clamp(
            ReadoutDashboardTokens::MIN_WIDTH_FRAC,
            ReadoutDashboardTokens::MAX_WIDTH_FRAC,
        );
        return true;
    }
    false
}

/// Toggle fully expanded (body + full bar width) or fully contracted (header only).
pub fn readout_dashboard_set_fully_expanded(expanded: &mut bool, width_frac: &mut f32) {
    *expanded = true;
    *width_frac = ReadoutDashboardTokens::MAX_WIDTH_FRAC;
}

pub fn readout_dashboard_set_fully_contracted(expanded: &mut bool, width_frac: &mut f32) {
    *expanded = false;
    *width_frac = ReadoutDashboardTokens::COLLAPSED_WIDTH_FRAC;
}
