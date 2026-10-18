//! Reference tag-create composition. The dock retains its capsule and primary.
use super::*;

const WIDTH: f32 = 420.0;
const BODY_HEIGHT: f32 = 162.0;
const POINTER_HEIGHT: f32 = 12.0;
const PRESETS: [[u8; 3]; 8] = [
    [32, 32, 32],
    [83, 87, 90],
    [244, 244, 244],
    [171, 216, 235],
    [25, 130, 128],
    [254, 87, 90],
    [123, 60, 248],
    [252, 190, 38],
];

#[derive(Clone)]
pub struct TagEditorState {
    pub name: String,
    pub color: [u8; 3],
    colors: Vec<[u8; 3]>,
    picker: ColorState,
    focus: bool,
}

impl Default for TagEditorState {
    fn default() -> Self {
        let color: Color32 = egui::ecolor::Hsva::new(0.5, 0.905, 0.73, 1.0).into();
        Self {
            name: String::new(),
            color: [color.r(), color.g(), color.b()],
            colors: PRESETS.to_vec(),
            picker: ColorState::default(),
            focus: true,
        }
    }
}

impl TagEditorState {
    /// Start another name draft while retaining the session's chosen colors.
    pub fn begin(&mut self) {
        self.name.clear();
        self.focus = true;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TagEditorAction {
    None,
    Cancel,
    Add,
}

fn secondary_ink(theme: Palette) -> Color32 {
    if theme.dark_mode {
        theme.sub
    } else {
        Color32::from_rgb(116, 120, 125)
    }
}

/// Only the inline body: no duplicate capsule frame or tag icon.
pub fn tag_add_button(
    ui: &mut egui::Ui,
    empty: bool,
    diameter: f32,
    theme: Palette,
) -> egui::Response {
    let gap = diameter * 0.27;
    let text = empty.then(|| {
        ui.painter().layout_no_wrap(
            "No tags in this workbook".into(),
            egui::FontId::proportional(15.0),
            secondary_ink(theme),
        )
    });
    let width = diameter + text.as_ref().map_or(0.0, |t| gap + t.size().x + gap * 0.5);
    let (slot, _) = ui.allocate_exact_size(Vec2::new(width, diameter), Sense::hover());
    let circle = Rect::from_min_size(slot.min, Vec2::splat(diameter));
    let response = ui
        .interact(circle, ui.id().with("add-tag"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let (ink, _) = tokens::current()
        .dock
        .inline
        .primary_colors(theme.dark_mode, false);
    ui.painter().circle(
        circle.center(),
        diameter * 0.5 - 0.5,
        if response.hovered() {
            theme.card_hover
        } else {
            Color32::TRANSPARENT
        },
        Stroke::new(0.9_f32, ink),
    );
    icons::paint_plus(
        ui.painter(),
        circle.center(),
        diameter * 0.19,
        Stroke::new(1.3_f32, ink),
    );
    if let Some(text) = text {
        ui.painter().galley(
            Pos2::new(
                circle.right() + gap,
                circle.center().y - text.size().y * 0.5,
            ),
            text,
            secondary_ink(theme),
        );
    }
    response
}

/// The downward callout is aligned to the plus, about a quarter across the panel.
pub fn tag_editor_placement(anchor: Rect, viewport: Rect) -> Rect {
    let width = WIDTH.min(viewport.width() - POPUP_MARGIN * 2.0);
    let x = (anchor.center().x - width * 0.255).clamp(
        viewport.left() + POPUP_MARGIN,
        (viewport.right() - POPUP_MARGIN - width).max(viewport.left() + POPUP_MARGIN),
    );
    Rect::from_min_size(
        Pos2::new(x, anchor.top() - 6.0 - BODY_HEIGHT - POINTER_HEIGHT),
        Vec2::new(width, BODY_HEIGHT + POINTER_HEIGHT),
    )
}

/// The same anchored popup host for the application and native review fixture.
pub fn tag_editor_popup(
    ui: &egui::Ui,
    id: Id,
    anchor: Rect,
    state: &mut TagEditorState,
    theme: Palette,
) -> TagEditorAction {
    let ctx = ui.ctx();
    let rect = tag_editor_placement(anchor, ctx.screen_rect());
    crate::menu_wheel::claim(ctx, rect);
    let response = popup_area(ctx, id, ui.layer_id())
        .fixed_pos(rect.min)
        .default_size(rect.size())
        .movable(false)
        .fade_in(false)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_clip_rect(ctx.screen_rect());
            ui.allocate_exact_size(rect.size(), Sense::hover());
            tag_editor(ui, rect, anchor.center().x, state, theme)
        });
    if ctx.input(|i| i.pointer.any_pressed())
        && ctx
            .pointer_latest_pos()
            .is_some_and(|p| !rect.contains(p) && !anchor.contains(p))
    {
        TagEditorAction::Cancel
    } else {
        response.inner
    }
}

pub fn tag_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    anchor_x: f32,
    state: &mut TagEditorState,
    theme: Palette,
) -> TagEditorAction {
    let body = Rect::from_min_size(rect.min, Vec2::new(rect.width(), BODY_HEIGHT));
    surface(ui, body, anchor_x, theme);
    let inset = 9.0;
    let field = Rect::from_min_size(
        body.min + Vec2::splat(inset),
        Vec2::new(body.width() - inset * 2.0, 59.5),
    );
    color_field::sync(&mut state.picker, state.color);
    let (_, mut changed) = color_field::sv_field(ui, field, &mut state.picker, 1.3);
    let hue = Rect::from_min_size(
        Pos2::new(field.left(), field.bottom() + 7.0),
        Vec2::new(field.width(), 9.0),
    );
    color_field::hue_spectrum(ui, hue, &mut state.picker, 4.5);
    let center = Pos2::new(
        egui::lerp(hue.x_range(), state.picker.hsv.h),
        hue.center().y,
    );
    let hit = crate::widgets::rail_interaction(
        ui,
        ui.id().with("tag-hue"),
        hue.expand2(Vec2::new(0.0, 3.0)),
        hue.x_range(),
        &mut state.picker.hsv.h,
        Rect::from_center_size(center, Vec2::splat(14.0)),
        false,
        1.0,
    );
    changed |= hit.response.changed();
    let center = Pos2::new(
        egui::lerp(hue.x_range(), state.picker.hsv.h),
        hue.center().y,
    );
    let (ink, _) = tokens::current()
        .dock
        .inline
        .primary_colors(theme.dark_mode, false);
    ui.painter()
        .circle(center, 6.2, theme.panel, Stroke::new(0.9_f32, ink));
    let selected: Color32 = egui::ecolor::Hsva::new(state.picker.hsv.h, 1.0, 1.0, 1.0).into();
    ui.painter().circle_filled(center, 4.3, selected);
    if changed {
        state.color = color_field::rgb(&mut state.picker);
    }
    let y = hue.bottom() + 20.0;
    let pitch = 33.3_f32.min((field.width() - 20.0) / state.colors.len() as f32);
    for (index, color) in state.colors.iter().copied().enumerate() {
        let center = Pos2::new(field.left() + 12.0 + pitch * index as f32, y);
        let hit = Rect::from_center_size(center, Vec2::splat(20.0));
        let response = ui.interact(hit, ui.id().with(("tag-preset", index)), Sense::click());
        ui.painter().circle(
            center,
            10.0,
            Color32::from_rgb(color[0], color[1], color[2]),
            Stroke::new(if index == 2 { 0.6_f32 } else { 0.0_f32 }, theme.border),
        );
        if response.clicked() {
            state.color = color;
        }
    }
    let center = Pos2::new(field.left() + 12.0 + pitch * state.colors.len() as f32, y);
    let add_preset = ui.interact(
        Rect::from_center_size(center, Vec2::splat(20.0)),
        ui.id().with("tag-save-color"),
        Sense::click(),
    );
    let (fill, ink) = if theme.dark_mode {
        (theme.card_hover, theme.sub)
    } else {
        (
            Color32::from_rgb(233, 234, 235),
            Color32::from_rgb(128, 135, 141),
        )
    };
    ui.painter().circle_filled(center, 10.0, fill);
    icons::paint_plus(ui.painter(), center, 4.2, Stroke::new(1.0_f32, ink));
    if add_preset.clicked() && !state.colors.contains(&state.color) {
        state.colors.push(state.color);
    }

    let row = Rect::from_min_size(
        Pos2::new(field.left(), body.top() + 128.5),
        Vec2::new(field.width(), 24.5),
    );
    let add = Rect::from_min_size(
        Pos2::new(row.right() - 69.0, row.top()),
        Vec2::new(69.0, row.height()),
    );
    let cancel = Rect::from_min_size(
        Pos2::new(add.left() - 6.5 - 64.0, row.top()),
        Vec2::new(64.0, row.height()),
    );
    let name = Rect::from_min_max(row.min, Pos2::new(cancel.left() - 6.5, row.bottom()));
    ui.painter().rect(
        name,
        3.8,
        theme.panel,
        Stroke::new(0.6_f32, theme.border),
        egui::StrokeKind::Inside,
    );
    let input = ui.put(
        name.shrink2(Vec2::new(8.0, 3.0)),
        egui::TextEdit::singleline(&mut state.name)
            .id(ui.id().with("tag-name"))
            .hint_text(egui::RichText::new("Tag name").color(secondary_ink(theme)))
            .font(egui::FontId::proportional(13.0))
            .frame(false)
            .text_color(theme.ink)
            .margin(Vec2::ZERO),
    );
    if state.focus {
        input.request_focus();
        state.focus = false;
    }
    let cancel_clicked = text_button(ui, cancel, "Cancel", false, theme);
    let add_clicked = text_button(ui, add, "Add tag", true, theme);
    if cancel_clicked || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        TagEditorAction::Cancel
    } else if !state.name.trim().is_empty()
        && (add_clicked
            || ((input.has_focus() || input.lost_focus())
                && ui.input(|i| i.key_pressed(egui::Key::Enter))))
    {
        TagEditorAction::Add
    } else {
        TagEditorAction::None
    }
}

fn text_button(ui: &mut egui::Ui, rect: Rect, text: &str, primary: bool, theme: Palette) -> bool {
    let response = ui.interact(rect, ui.id().with(text), Sense::click());
    let (fill, ink) = tokens::current()
        .dock
        .inline
        .primary_colors(theme.dark_mode, false);
    ui.painter().rect(
        rect,
        3.8,
        if primary { fill } else { theme.panel },
        Stroke::new(0.6_f32, if primary { fill } else { theme.border }),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(13.0),
        if primary { ink } else { secondary_ink(theme) },
    );
    response.clicked()
}

fn surface(ui: &egui::Ui, body: Rect, anchor_x: f32, theme: Palette) {
    let x = anchor_x.clamp(body.left() + 18.0, body.right() - 18.0);
    let tip = Pos2::new(x, body.bottom() + POINTER_HEIGHT);
    let left = Pos2::new(x - 13.0, body.bottom() - 0.6);
    let right = Pos2::new(x + 13.0, body.bottom() - 0.6);
    let shadow = egui::epaint::Shadow {
        offset: [0, 2],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(if theme.dark_mode { 35 } else { 18 }),
    };
    ui.painter().add(shadow.as_shape(body, 5.0));
    ui.painter().rect(
        body,
        5.0,
        theme.panel,
        Stroke::new(0.6_f32, theme.border),
        egui::StrokeKind::Inside,
    );
    ui.painter().add(egui::Shape::convex_polygon(
        vec![left, tip, right],
        theme.panel,
        Stroke::NONE,
    ));
    ui.painter()
        .line_segment([left, tip], Stroke::new(0.6_f32, theme.border));
    ui.painter()
        .line_segment([tip, right], Stroke::new(0.6_f32, theme.border));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        state: &mut TagEditorState,
        events: Vec<egui::Event>,
    ) -> TagEditorAction {
        let mut action = TagEditorAction::None;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 500.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                action = tag_editor(
                    ui,
                    Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(420.0, 174.0)),
                    127.0,
                    state,
                    Palette::light(),
                );
            });
        });
        action
    }

    #[test]
    fn name_entry_accepts_enter_and_escape_cancels_the_draft() {
        let ctx = egui::Context::default();
        let mut state = TagEditorState::default();
        frame(&ctx, &mut state, vec![]);
        frame(
            &ctx,
            &mut state,
            vec![egui::Event::Text("Reference".into())],
        );
        assert_eq!(state.name, "Reference");
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        assert!(frame(&ctx, &mut state, vec![key(egui::Key::Enter)]) == TagEditorAction::Add);
        assert!(frame(&ctx, &mut state, vec![key(egui::Key::Escape)]) == TagEditorAction::Cancel);
    }

    #[test]
    fn a_quick_color_field_click_updates_the_selected_color() {
        let ctx = egui::Context::default();
        let mut state = TagEditorState::default();
        let pos = Pos2::new(80.0, 70.0);
        frame(&ctx, &mut state, vec![]);
        frame(&ctx, &mut state, vec![egui::Event::PointerMoved(pos)]);
        let before = state.color;
        let events = [true, false]
            .into_iter()
            .map(|pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            })
            .collect();
        frame(&ctx, &mut state, events);
        assert_ne!(state.color, before);
    }

    #[test]
    fn preset_selection_and_blank_name_do_not_commit() {
        let ctx = egui::Context::default();
        let mut state = TagEditorState::default();
        frame(&ctx, &mut state, vec![]);
        let pos = Pos2::new(41.0, 124.5);
        for pressed in [true, false] {
            let action = frame(
                &ctx,
                &mut state,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
            assert!(action == TagEditorAction::None);
        }
        assert_eq!(state.color, PRESETS[0]);
        assert!(state.name.is_empty());
    }
}
