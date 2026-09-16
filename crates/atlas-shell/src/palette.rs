//! Shared canvas command palette.
//!
//! A small floating popup opened at a canvas point (e.g. double-click on
//! empty board): a focused query field over a short result list. The palette
//! is a pure chrome surface (Constitution Art. X): apps supply plain
//! [`PaletteRow`]s — typically adapted from their command registry — and
//! react to the returned [`PaletteAction`]. Filtering/ranking is the app's
//! job; on [`PaletteAction::QueryChanged`] it re-queries and passes fresh
//! rows next frame.
//!
//! The palette carries the invocation's **world point** (`PaletteState::world`)
//! so placeable commands dispatched from it can place at that point.
//!
//! Zero cost while closed: [`palette_ui`] returns immediately when
//! `state.open` is `false` (Constitution Art. II).
//!
//! Geometry lives under `[palette]` in `ui-tokens.toml`.

use crate::theme::Palette;
use crate::tokens;
use eframe::egui::{self, Color32, CornerRadius, Pos2, RichText, Stroke, Vec2};

/// One result row: command label plus an optional dim binding hint.
pub struct PaletteRow {
    pub label: String,
    /// Binding hint shown right-aligned when it fits, and in the row tooltip.
    pub hint: String,
}

/// Retained palette state. Keep one per canvas and pass it back every frame.
pub struct PaletteState {
    pub open: bool,
    /// Current query text (the app filters rows against this).
    pub query: String,
    /// Index into the rows currently displayed.
    pub selected: usize,
    /// Screen anchor: where the palette pops up (clamped on-screen).
    pub anchor: Pos2,
    /// World point of the invocation — placeables place here.
    pub world: Pos2,
    /// Focus the query field on the next frame (set by [`Self::open_at`]).
    needs_focus: bool,
}

impl Default for PaletteState {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            selected: 0,
            anchor: Pos2::ZERO,
            world: Pos2::ZERO,
            needs_focus: false,
        }
    }
}

impl PaletteState {
    /// Open (or re-anchor) the palette at a screen point, remembering the
    /// world point under it. Resets the query and selection.
    pub fn open_at(&mut self, screen: Pos2, world: Pos2) {
        self.open_with_query(screen, world, String::new());
    }

    /// Open the palette pre-seeded with a query (type-to-command entry).
    pub fn open_with_query(&mut self, screen: Pos2, world: Pos2, query: String) {
        self.open = true;
        self.anchor = screen;
        self.world = world;
        self.query = query;
        self.selected = 0;
        self.needs_focus = true;
    }
}

/// What the user did to the palette this frame.
pub enum PaletteAction {
    None,
    /// The query text changed — re-run the app's fuzzy query and pass the new
    /// rows next frame.
    QueryChanged,
    /// Execute the row at this index (Enter or click). The palette closes.
    Execute(usize),
    /// Esc or click-away. The palette closes.
    Dismiss,
}

/// Show the palette if open. `rows` are the app's current query results;
/// only the first `[palette] max_rows` are displayed.
pub fn palette_ui(
    ctx: &egui::Context,
    state: &mut PaletteState,
    rows: &[PaletteRow],
) -> PaletteAction {
    if !state.open {
        return PaletteAction::None;
    }

    let t = tokens::current();
    let pt = &t.palette;
    let dark = ctx.style().visuals.dark_mode;
    let palette = Palette::for_mode(dark);
    let th = if dark { &t.dock.dark } else { &t.dock.light };

    let shown = rows.len().min(pt.max_rows);
    state.selected = state.selected.min(shown.saturating_sub(1));

    let mut action = PaletteAction::None;

    // Keyboard first, so the focused TextEdit doesn't swallow the keys.
    ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) && shown > 0 {
            state.selected = (state.selected + 1).min(shown - 1);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            state.selected = state.selected.saturating_sub(1);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) && shown > 0 {
            action = PaletteAction::Execute(state.selected);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
            action = PaletteAction::Dismiss;
        }
    });

    // Clamp the anchor so the whole popup stays on-screen.
    let est_height = 40.0 + shown as f32 * pt.row_height;
    let screen = ctx.screen_rect();
    let pos = Pos2::new(
        state.anchor.x.clamp(
            screen.left(),
            (screen.right() - pt.width).max(screen.left()),
        ),
        state.anchor.y.clamp(
            screen.top(),
            (screen.bottom() - est_height).max(screen.top()),
        ),
    );

    let area = egui::Area::new(egui::Id::new("atlas_shell_canvas_palette"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(th.popover_fill_color())
                .stroke(Stroke::new(1.0_f32, th.border_color()))
                .corner_radius(CornerRadius::same(pt.corner_radius.clamp(0.0, 255.0) as u8))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 14,
                    spread: 1,
                    color: Color32::from_black_alpha(70),
                })
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(pt.width);

                    let edit = egui::TextEdit::singleline(&mut state.query)
                        .hint_text("Type a command…")
                        .font(egui::FontId::proportional(pt.text_size))
                        .desired_width(f32::INFINITY);
                    let resp = ui.add(edit);
                    if state.needs_focus {
                        resp.request_focus();
                        state.needs_focus = false;
                    }
                    if resp.changed() {
                        state.selected = 0;
                        if matches!(action, PaletteAction::None) {
                            action = PaletteAction::QueryChanged;
                        }
                    }

                    if shown > 0 {
                        ui.add_space(4.0);
                    }
                    for (i, row) in rows.iter().take(shown).enumerate() {
                        let (rect, row_resp) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), pt.row_height),
                            egui::Sense::click(),
                        );
                        if row_resp.hovered() && ctx.input(|i| i.pointer.delta() != Vec2::ZERO) {
                            state.selected = i;
                        }
                        if row_resp.clicked() {
                            action = PaletteAction::Execute(i);
                        }
                        if i == state.selected {
                            ui.painter().rect_filled(rect, 6.0, th.icon_hover_color());
                        }
                        let text_y = rect.center().y;
                        // Command names get the row width. Some registry hints are
                        // descriptive sentences, so only inline a hint that fits
                        // beside the name; retain the full wording on hover.
                        let mut label_job = egui::text::LayoutJob::simple_singleline(
                            row.label.clone(),
                            egui::FontId::proportional(pt.text_size),
                            th.text_color(),
                        );
                        label_job.wrap = egui::text::TextWrapping {
                            max_width: (rect.width() - 16.0).max(0.0),
                            max_rows: 1,
                            break_anywhere: true,
                            ..Default::default()
                        };
                        let label = ui.fonts(|f| f.layout_job(label_job));
                        if !row.hint.is_empty() {
                            let hint = ui.painter().layout_no_wrap(
                                row.hint.clone(),
                                egui::FontId::proportional(pt.text_size - 1.5),
                                th.muted_text_color(),
                            );
                            if label.size().x + hint.size().x + 12.0 <= rect.width() - 16.0 {
                                ui.painter().galley(
                                    Pos2::new(
                                        rect.right() - 8.0 - hint.size().x,
                                        text_y - hint.size().y / 2.0,
                                    ),
                                    hint,
                                    th.muted_text_color(),
                                );
                            }
                            row_resp.on_hover_text(format!("{}\n{}", row.label, row.hint));
                        } else if label.elided {
                            row_resp.on_hover_text(&row.label);
                        }
                        ui.painter().galley(
                            Pos2::new(rect.left() + 8.0, text_y - label.size().y / 2.0),
                            label,
                            th.text_color(),
                        );
                    }
                    if shown == 0 && !state.query.is_empty() {
                        ui.label(
                            RichText::new("No matching commands")
                                .size(pt.text_size - 1.0)
                                .color(palette.sub),
                        );
                    }
                });
        });

    // Click-away dismisses (presses outside the popup panel).
    if matches!(action, PaletteAction::None) {
        let clicked_outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !area.response.rect.contains(p))
        });
        if clicked_outside {
            action = PaletteAction::Dismiss;
        }
    }

    if matches!(action, PaletteAction::Execute(_) | PaletteAction::Dismiss) {
        state.open = false;
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        state: &mut PaletteState,
        rows: &[PaletteRow],
        time: &mut f64,
        events: Vec<egui::Event>,
    ) -> (PaletteAction, egui::FullOutput) {
        *time += 1.0 / 60.0;
        let mut action = PaletteAction::None;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(1000.0, 800.0),
                )),
                time: Some(*time),
                events,
                ..Default::default()
            },
            |ctx| action = palette_ui(ctx, state, rows),
        );
        (action, output)
    }

    fn key(key: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn label_center(output: &egui::FullOutput, label: &str) -> Pos2 {
        fn find(shape: &egui::Shape, label: &str) -> Option<Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|shape| find(&shape.shape, label))
            .unwrap_or_else(|| panic!("palette did not paint command label {label:?}"))
    }

    #[test]
    fn stationary_pointer_preserves_arrow_navigation_and_enter_while_motion_selects_rows() {
        let ctx = egui::Context::default();
        let mut state = PaletteState::default();
        state.open_at(Pos2::new(100.0, 100.0), Pos2::ZERO);
        let rows = ["First command", "Second command", "Third command"].map(|label| PaletteRow {
            label: label.into(),
            hint: String::new(),
        });
        let mut time = 0.0;

        // Let the Area finish sizing and register its hit rectangles, then
        // derive pointer targets from actual painted labels, not row offsets.
        frame(&ctx, &mut state, &rows, &mut time, vec![]);
        let (_, output) = frame(&ctx, &mut state, &rows, &mut time, vec![]);
        let first = label_center(&output, &rows[0].label);
        let third = label_center(&output, &rows[2].label);
        frame(
            &ctx,
            &mut state,
            &rows,
            &mut time,
            vec![egui::Event::PointerMoved(first)],
        );
        frame(
            &ctx,
            &mut state,
            &rows,
            &mut time,
            vec![egui::Event::PointerMoved(third)],
        );
        assert_eq!(state.selected, 2, "moving the mouse must select its row");
        frame(
            &ctx,
            &mut state,
            &rows,
            &mut time,
            vec![egui::Event::PointerMoved(first)],
        );
        assert_eq!(state.selected, 0);

        // The mouse now rests over row zero. Keyboard events must move the
        // selection and keep it there without requiring the mouse to leave.
        let (action, _) = frame(
            &ctx,
            &mut state,
            &rows,
            &mut time,
            vec![key(egui::Key::ArrowDown, true)],
        );
        assert!(matches!(action, PaletteAction::None));
        assert_eq!(
            state.selected, 1,
            "stationary hover must not undo ArrowDown"
        );
        frame(
            &ctx,
            &mut state,
            &rows,
            &mut time,
            vec![key(egui::Key::ArrowDown, false)],
        );
        assert_eq!(
            state.selected, 1,
            "idle hover must preserve keyboard selection"
        );
        let (action, _) = frame(
            &ctx,
            &mut state,
            &rows,
            &mut time,
            vec![key(egui::Key::Enter, true)],
        );
        assert!(matches!(action, PaletteAction::Execute(1)));
        assert!(!state.open);
    }
}
