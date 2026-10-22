//! File Atlas keyboard dispatch and its existing cancel/repeat stack.
use super::{commands, AtlasApp, CommandId};
use atlas_commands::{cancel_target, CancelLayer};
use eframe::egui::{self, Vec2};
use std::time::{Duration, Instant};

impl AtlasApp {
    /// Keyboard dispatch: every chord resolves through `commands::REGISTRY`
    /// (one table for dispatch, reference UI, and repeat — Art. VII), then
    /// runs the same handler bodies as before the migration. Escape goes
    /// through the formal cancel stack; Space/Enter drive repeat-last;
    /// arrows pan per-frame.
    pub(super) fn hotkeys(&mut self, ctx: &egui::Context) {
        // Standard menus own keys before their draw pass consumes Escape.
        if ctx.is_context_menu_open() {
            self.space_press = None;
            return;
        }
        let wants_kb = ctx.wants_keyboard_input();

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.handle_escape(ctx);
        }
        self.repeat_keys(ctx, wants_kb);
        self.arrow_pan(ctx, wants_kb);

        // --- registry chord dispatch ---
        let mut avail = atlas_commands::Availability::ATLAS | atlas_commands::Availability::GLOBAL;
        if !self.selection.is_empty() {
            avail |= atlas_commands::Availability::NEEDS_SELECTION;
        }
        let mut fired: Vec<CommandId> = Vec::new();
        ctx.input_mut(|i| {
            i.events.retain(|e| {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    repeat,
                    ..
                } = e
                else {
                    return true;
                };
                let Some(k) = commands::map_key(*key) else {
                    return true;
                };
                let mut chord = atlas_commands::Chord {
                    key: k,
                    ctrl: modifiers.command,
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                };
                // "+" often arrives as Shift+= — zoom chords ignore Shift.
                if matches!(k, atlas_commands::Key::Plus | atlas_commands::Key::Minus) {
                    chord.shift = false;
                }
                // Ctrl+Shift+Z is the documented redo alias of Ctrl+Y.
                if chord.key == atlas_commands::Key::Z && chord.ctrl && chord.shift && !chord.alt {
                    chord = atlas_commands::Chord::ctrl(atlas_commands::Key::Y);
                }
                let Some(spec) = commands::REGISTRY.by_chord(chord, avail) else {
                    return true;
                };
                // Typing gate: bare keys and clipboard/selection chords stay
                // with the focused text field (same gates as before the
                // migration; undo/redo/open/F11 keep firing while typing).
                if wants_kb && !command_allowed_while_typing(spec.id.0) {
                    return true;
                }
                // Held-key auto-repeat must not flutter the toggles.
                if *repeat && matches!(spec.id.0, "canvas.minimap" | "canvas.tool.zoom") {
                    return false;
                }
                fired.push(spec.id);
                false
            });
        });
        for id in fired {
            self.dispatch_command(ctx, id);
        }
    }

    /// One Esc pops exactly one layer. The search field and the hand-rolled
    /// context menu swallow Esc before the stack (the menu is today's first
    /// cascade step); everything else resolves through
    /// `atlas_commands::cancel_target` with Atlas's layer mapping.
    pub(super) fn handle_escape(&mut self, ctx: &egui::Context) {
        // Search field focused: return focus to the canvas, keep the query.
        let dock_field = egui::Id::new("atlas_filters_search");
        let popup_field = egui::Id::new("atlas_search_popup_field");
        let search_focused = ctx.memory(|m| m.has_focus(dock_field) || m.has_focus(popup_field));
        if search_focused {
            ctx.memory_mut(|m| {
                m.surrender_focus(dock_field);
                m.surrender_focus(popup_field);
            });
            self.search_popup_open = false;
            return;
        }
        if self.search_popup_open {
            self.search_popup_open = false;
            return;
        }
        if self.menu_at.is_some() {
            self.menu_at = None;
            return;
        }
        let mut live: Vec<CancelLayer> = Vec::new();
        if self.zoom_marquee.is_some() {
            live.push(CancelLayer::ActiveOperation);
        }
        if self.edit_open || self.detail.is_some() {
            live.push(CancelLayer::Draft);
        }
        if self.zoom_armed {
            live.push(CancelLayer::Mode);
        }
        if !self.selection.is_empty() {
            live.push(CancelLayer::Selection);
        }
        if self.time_selection_active() {
            live.push(CancelLayer::Readout);
        }
        match cancel_target(&live) {
            Some(CancelLayer::ActiveOperation) => self.zoom_marquee = None,
            Some(CancelLayer::Draft) => {
                // Today's order within the draft layer: edit panel, then detail.
                if self.edit_open {
                    self.edit_open = false;
                } else {
                    self.detail = None;
                }
            }
            Some(CancelLayer::Mode) => self.zoom_armed = false,
            Some(CancelLayer::Selection) => self.selection.clear(),
            // The activity timeline back to the whole span.
            Some(CancelLayer::Readout) => self.reset_time_selection(),
            Some(CancelLayer::Chrome) | None => {}
        }
    }

    /// Space TAP (release < ~250 ms, no pointer press while held, not typing)
    /// and Enter (idle, not typing, no draft) dispatch the last repeatable
    /// history entry.
    fn repeat_keys(&mut self, ctx: &egui::Context, wants_kb: bool) {
        let (space_pressed, space_released, pointer_down, enter) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.key_released(egui::Key::Space),
                i.pointer.any_down(),
                i.key_pressed(egui::Key::Enter),
            )
        });
        if space_pressed && !wants_kb && self.space_press.is_none() {
            self.space_press = Some((Instant::now(), false));
        }
        if let Some((_, dragged)) = &mut self.space_press {
            if pointer_down {
                *dragged = true;
            }
        }
        if space_released {
            if let Some((t0, dragged)) = self.space_press.take() {
                if !dragged && !wants_kb && t0.elapsed() < Duration::from_millis(250) {
                    self.repeat_last(ctx);
                }
            }
        }
        if enter && !wants_kb && !self.edit_open && self.menu_at.is_none() {
            self.repeat_last(ctx);
        }
    }

    pub(super) fn repeat_last(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.cmd_history.last_repeatable(&commands::REGISTRY) {
            self.dispatch_command(ctx, id);
        }
    }

    /// Arrows always pan in Atlas (no nudge semantics); Shift = ×4 speed.
    fn arrow_pan(&mut self, ctx: &egui::Context, wants_kb: bool) {
        if wants_kb || self.root.is_none() {
            return;
        }
        let (l, r, u, d, shift, dt) = ctx.input(|i| {
            (
                i.key_down(egui::Key::ArrowLeft),
                i.key_down(egui::Key::ArrowRight),
                i.key_down(egui::Key::ArrowUp),
                i.key_down(egui::Key::ArrowDown),
                i.modifiers.shift,
                i.stable_dt,
            )
        });
        if !(l || r || u || d) {
            return;
        }
        let step = 900.0 * dt.clamp(0.0, 0.05) * if shift { 4.0 } else { 1.0 };
        let mut delta = Vec2::ZERO;
        if l {
            delta.x += step;
        }
        if r {
            delta.x -= step;
        }
        if u {
            delta.y += step;
        }
        if d {
            delta.y -= step;
        }
        self.anim = None;
        self.cam.offset += delta;
        self.grid_fade_armed = true;
        ctx.request_repaint();
    }
}

/// Chords that keep firing while a text field has focus — matching the
/// pre-migration gates (undo/redo/open/F11 were never typing-gated) plus the
/// global window commands. Everything else (bare keys, Ctrl+A, Ctrl+C) stays
/// with the focused widget.
fn command_allowed_while_typing(id: &str) -> bool {
    matches!(
        id,
        "app.undo"
            | "app.redo"
            | "app.open"
            | "app.fullscreen"
            | "app.new_tab"
            | "app.help"
            | "app.preferences"
            | "canvas.search"
    )
}
