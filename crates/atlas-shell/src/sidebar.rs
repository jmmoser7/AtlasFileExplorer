//! Left tools rail layout primitives — bordered collapsible sections and
//! aligned control rows. See `SIDEBAR.md` for usage rules.

use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Layout, Margin, Pos2, Rect,
    RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
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
    /// Toggle track (wide capsule) + sliding knob. Tracks stay left-aligned
    /// so a two-column snap grid keeps two vertical pill columns.
    pub const TOGGLE_TRACK_W: f32 = 34.0;
    pub const TOGGLE_TRACK_H: f32 = 18.0;
    pub const ICON_ROW_HEIGHT: f32 = 22.0;
}

/// Knob slide on / off. Shared by stacked rows and icon-strip tertiary pills.
pub const TOGGLE_SLIDE_SECS: f32 = 0.14;

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
        .stroke(Stroke::new(1.0_f32, theme.border))
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

/// Small `+` / `−` expand control — pointing hand, not text (I-beam) cursor.
fn sidebar_expand_toggle(ui: &mut Ui, expanded: bool, theme: SidebarTheme) -> egui::Response {
    let glyph = if expanded { "−" } else { "+" };
    ui.add(
        egui::Label::new(
            RichText::new(glyph)
                .size(SidebarTokens::TOGGLE_SIZE)
                .color(theme.sub),
        )
        .sense(Sense::click()),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
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
        let toggle = sidebar_expand_toggle(ui, *expanded, theme);
        if toggle.clicked() {
            *expanded = !*expanded;
            ui.ctx()
                .data_mut(|d| d.insert_temp(id.with("expanded"), *expanded));
            changed = true;
        }

        let title_resp = ui
            .add(
                egui::Label::new(RichText::new(title).strong().color(theme.ink))
                    .sense(Sense::click()),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
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

/// Stacked-list toggle: wide capsule track, circular knob on the **left**
/// carrying the icon, label in a fixed gutter. `active` lights the track
/// and the knob (on); off is a dim track and a muted knob. The knob does
/// not slide — left alignment is what keeps a two-column snap grid as two
/// clean vertical tracks.
pub fn sidebar_icon_row(
    ui: &mut Ui,
    label: &str,
    hotkey: Option<&str>,
    active: bool,
    theme: SidebarTheme,
    paint_icon: impl FnOnce(&egui::Painter, Rect, Color32),
) -> egui::Response {
    let height = SidebarTokens::ICON_ROW_HEIGHT;
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(
            ui.available_width().max(SidebarTokens::TOGGLE_TRACK_W),
            height,
        ),
        Sense::click(),
    );
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let track = Rect::from_center_size(
        Pos2::new(
            rect.left() + SidebarTokens::TOGGLE_TRACK_W * 0.5,
            rect.center().y,
        ),
        Vec2::new(SidebarTokens::TOGGLE_TRACK_W, SidebarTokens::TOGGLE_TRACK_H),
    );
    let t = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("slide"), active, TOGGLE_SLIDE_SECS);
    paint_sidebar_icon_pill(
        ui.painter(),
        track,
        active,
        resp.hovered(),
        t,
        theme,
        paint_icon,
    );
    let label_x = rect.left() + SidebarTokens::TOGGLE_TRACK_W + 8.0;
    let galley =
        ui.fonts(|f| f.layout_no_wrap(label.to_owned(), FontId::proportional(13.0), theme.ink));
    if let Some(key) = hotkey {
        let kg =
            ui.fonts(|f| f.layout_no_wrap(key.to_owned(), FontId::proportional(11.0), theme.sub));
        let kx = (rect.right() - kg.size().x).max(label_x);
        ui.painter().galley(
            Pos2::new(kx, rect.center().y - kg.size().y * 0.5),
            kg,
            theme.sub,
        );
    }
    ui.painter().galley(
        Pos2::new(label_x, rect.center().y - galley.size().y * 0.5),
        galley,
        theme.ink,
    );
    ui.add_space(SidebarTokens::ROW_GAP);
    resp
}

/// Toggle track + sliding knob. `t` is 0 (off, left) … 1 (on, right).
pub fn paint_sidebar_icon_pill(
    painter: &egui::Painter,
    track: Rect,
    on: bool,
    hovered: bool,
    t: f32,
    theme: SidebarTheme,
    paint_icon: impl FnOnce(&egui::Painter, Rect, Color32),
) {
    let radius = track.height() * 0.5;
    let fill = if on {
        theme.ink.gamma_multiply(if hovered { 0.30 } else { 0.20 })
    } else if hovered {
        theme.border.gamma_multiply(0.48)
    } else {
        theme.border.gamma_multiply(0.26)
    };
    painter.rect_filled(track, radius, fill);
    let inset = (track.height() * 0.12).max(1.5);
    let knob_d = (track.height() - inset * 2.0).max(4.0);
    let min_x = track.left() + inset + knob_d * 0.5;
    let max_x = track.right() - inset - knob_d * 0.5;
    let knob_x = min_x + (max_x - min_x) * t.clamp(0.0, 1.0);
    let knob = Rect::from_center_size(Pos2::new(knob_x, track.center().y), Vec2::splat(knob_d));
    let knob_fill = theme.ink.gamma_multiply(if on { 0.30 } else { 0.12 });
    painter.circle_filled(knob.center(), knob_d * 0.5, knob_fill);
    let icon_color = if on {
        theme.ink
    } else {
        theme.ink.gamma_multiply(0.38)
    };
    paint_icon(painter, knob.shrink((knob_d * 0.18).max(1.0)), icon_color);
}

/// Bordered segmented control. Each cell is label-only or icon-above-label.
/// Returns the clicked index.
pub fn sidebar_segmented(
    ui: &mut Ui,
    label: &str,
    theme: SidebarTheme,
    items: &[SegmentedItem<'_>],
) -> Option<usize> {
    sidebar_subsection_label(ui, label, theme);
    if items.is_empty() {
        return None;
    }
    let n = items.len();
    let cell_w = ((ui.available_width() - 2.0) / n as f32).max(36.0);
    let has_icon = items.iter().any(|it| it.paint_icon.is_some());
    let cell_h = if has_icon { 36.0 } else { 11.0 };
    let (bar, _) = ui.allocate_exact_size(Vec2::new(cell_w * n as f32, cell_h), Sense::hover());
    painter_segmented_frame(ui.painter(), bar, theme);
    let mut clicked = None;
    for (i, item) in items.iter().enumerate() {
        let cell = Rect::from_min_size(
            Pos2::new(bar.left() + i as f32 * cell_w, bar.top()),
            Vec2::new(cell_w, cell_h),
        );
        let resp = ui
            .interact(cell, ui.id().with(("seg", label, i)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if let Some(hint) = item.hint {
            let _ = resp.clone().on_hover_text(hint);
        }
        if item.selected || resp.hovered() {
            let fill = if item.selected {
                theme.ink.gamma_multiply(0.16)
            } else {
                theme.border.gamma_multiply(0.35)
            };
            ui.painter().rect_filled(cell.shrink(1.0), 3.0, fill);
        }
        if i > 0 {
            let x = cell.left();
            let inset = if has_icon { 4.0 } else { 2.0 };
            ui.painter().line_segment(
                [
                    Pos2::new(x, bar.top() + inset),
                    Pos2::new(x, bar.bottom() - inset),
                ],
                Stroke::new(1.0_f32, theme.border.gamma_multiply(0.55)),
            );
        }
        if let Some(paint) = item.paint_icon {
            let icon = Rect::from_center_size(
                Pos2::new(cell.center().x, cell.top() + 11.0),
                Vec2::splat(14.0),
            );
            paint(
                ui.painter(),
                icon,
                if item.selected { theme.ink } else { theme.sub },
            );
        }
        let galley = ui.fonts(|f| {
            f.layout_no_wrap(
                item.label.to_owned(),
                FontId::proportional(if has_icon { 11.0 } else { 9.0 }),
                if item.selected { theme.ink } else { theme.sub },
            )
        });
        let ty = if has_icon {
            cell.bottom() - galley.size().y - 3.0
        } else {
            cell.center().y - galley.size().y * 0.5
        };
        ui.painter().galley(
            Pos2::new(cell.center().x - galley.size().x * 0.5, ty),
            galley,
            if item.selected { theme.ink } else { theme.sub },
        );
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    ui.add_space(SidebarTokens::ROW_GAP);
    clicked
}

fn painter_segmented_frame(painter: &egui::Painter, bar: Rect, theme: SidebarTheme) {
    painter.rect_stroke(
        bar,
        4.0,
        Stroke::new(1.0_f32, theme.border.gamma_multiply(0.7)),
        StrokeKind::Inside,
    );
}

/// One cell in [`sidebar_segmented`].
pub struct SegmentedItem<'a> {
    pub label: &'a str,
    pub selected: bool,
    pub hint: Option<&'a str>,
    pub paint_icon: Option<fn(&egui::Painter, Rect, Color32)>,
}

/// White section title (Object snaps), not a fold and not a muted caption.
pub fn sidebar_heading(ui: &mut Ui, label: &str, theme: SidebarTheme) {
    ui.add_space(SidebarTokens::ROW_GAP);
    ui.label(RichText::new(label).small().strong().color(theme.ink));
    ui.add_space(4.0);
}

/// Horizontal exclusive chips — selected gets a rounded fill, no outer
/// border (the wires row). Use [`sidebar_segmented`] when the group itself
/// is a boxed control (REACH).
pub fn sidebar_choice_chips(
    ui: &mut Ui,
    label: &str,
    theme: SidebarTheme,
    items: &[ChoiceChip<'_>],
) -> Option<usize> {
    sidebar_subsection_label(ui, label, theme);
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, item) in items.iter().enumerate() {
            let color = if item.selected { theme.ink } else { theme.sub };
            let galley = ui.fonts(|f| {
                f.layout_no_wrap(item.label.to_owned(), FontId::proportional(12.0), color)
            });
            let size = Vec2::new(galley.size().x + 16.0, 22.0);
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
            let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
            if let Some(hint) = item.hint {
                let _ = resp.clone().on_hover_text(hint);
            }
            if item.selected {
                ui.painter()
                    .rect_filled(rect, 4.0, theme.ink.gamma_multiply(0.16));
            } else if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, 4.0, theme.border.gamma_multiply(0.38));
            }
            ui.painter().galley(
                Pos2::new(
                    rect.center().x - galley.size().x * 0.5,
                    rect.center().y - galley.size().y * 0.5,
                ),
                galley,
                color,
            );
            if resp.clicked() {
                clicked = Some(i);
            }
        }
    });
    ui.add_space(SidebarTokens::ROW_GAP);
    clicked
}

/// One chip in [`sidebar_choice_chips`].
pub struct ChoiceChip<'a> {
    pub label: &'a str,
    pub selected: bool,
    pub hint: Option<&'a str>,
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

/// Checkbox with a hover hint — used for compact palettes (object snaps).
pub fn sidebar_checkbox_hinted(
    ui: &mut Ui,
    value: &mut bool,
    label: impl Into<egui::WidgetText>,
    hint: &str,
) -> bool {
    ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
    let changed = ui.checkbox(value, label).on_hover_text(hint).changed();
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

/// Independent fold region. Uses Windows-like minimize / maximize glyphs in
/// the upper-right — shared dock/toolbar pattern (see `TOOLBARS.md`).
/// Large toolbars should pass `default_expanded: false`.
pub fn sidebar_fold_region(
    ui: &mut Ui,
    id: Id,
    label: &str,
    default_expanded: bool,
    theme: SidebarTheme,
    add_body: impl FnOnce(&mut Ui),
) {
    let mut expanded = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(id))
        .unwrap_or(default_expanded);

    ui.horizontal(|ui| {
        ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
        let label_color = if expanded { theme.ink } else { theme.sub };
        let label_resp = ui
            .add(
                egui::Label::new(RichText::new(label).small().strong().color(label_color))
                    .sense(Sense::click()),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if windows_min_max_button(ui, expanded, theme) {
                expanded = !expanded;
            }
        });
        if label_resp.clicked() {
            expanded = !expanded;
        }
    });

    if expanded {
        ui.indent(id, add_body);
        ui.add_space(SidebarTokens::ROW_GAP);
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, expanded));
}

/// Windows caption-style minimize (─) / maximize (□) toggle. Returns true on click.
pub fn windows_min_max_button(ui: &mut Ui, expanded: bool, theme: SidebarTheme) -> bool {
    let size = Vec2::splat(14.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = resp
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(if expanded { "Collapse" } else { "Expand" });
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 2.0, theme.border.gamma_multiply(0.35));
    }
    let c = rect.center();
    let s = Stroke::new(1.15_f32, theme.ink);
    if expanded {
        // Minimize: horizontal bar
        painter.line_segment([Pos2::new(c.x - 4.0, c.y), Pos2::new(c.x + 4.0, c.y)], s);
    } else {
        // Maximize: hollow square
        let r = Rect::from_center_size(c, Vec2::splat(7.0));
        painter.rect_stroke(r, 1.0, s, StrokeKind::Middle);
    }
    resp.clicked()
}

/// Accordion row inside a section body — title toggles an indented control block.
///
/// `group_id` scopes mutual exclusivity: at most one region in the group is
/// open. Clicking the active row again collapses it. Top-level section cards
/// (`sidebar_section`) are unaffected.
pub fn sidebar_collapsible_region(
    ui: &mut Ui,
    group_id: Id,
    region_id: Id,
    label: &str,
    theme: SidebarTheme,
    add_body: impl FnOnce(&mut Ui),
) {
    let expanded = ui.ctx().data(|d| d.get_temp::<Option<Id>>(group_id)) == Some(Some(region_id));

    ui.horizontal(|ui| {
        ui.set_min_height(SidebarTokens::CONTROL_ROW_HEIGHT);
        let toggle = sidebar_expand_toggle(ui, expanded, theme);
        let label_color = if expanded { theme.ink } else { theme.sub };
        let label_resp = ui
            .add(
                egui::Label::new(RichText::new(label).small().color(label_color))
                    .sense(Sense::click()),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        if toggle.clicked() || label_resp.clicked() {
            let next = if expanded { None } else { Some(region_id) };
            ui.ctx().data_mut(|d| d.insert_temp(group_id, next));
        }
    });

    if ui.ctx().data(|d| d.get_temp::<Option<Id>>(group_id)) == Some(Some(region_id)) {
        ui.indent(region_id, add_body);
        ui.add_space(SidebarTokens::ROW_GAP);
    }
}

/// Very subtle horizontal rule between sub-regions inside a section card.
pub fn sidebar_subtle_divider(ui: &mut Ui, theme: SidebarTheme) {
    ui.add_space(SidebarTokens::ROW_GAP);
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 8.0), Sense::hover());
    let stroke = Stroke::new(
        1.0_f32,
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
            let toggle = sidebar_expand_toggle(ui, *expanded, theme);
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
