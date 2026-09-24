//! Shared command-reference plumbing and canvas navigation helpers.
//!
//! **Rule:** every user-facing shortcut or input command in an Atlas app must
//! be registered in that app's `ENTRIES` table (a `&[CommandEntry]`) and will
//! appear automatically in Advanced → Commands & shortcuts via
//! [`shortcuts_reference_ui`]. See each app's `COMMANDS.md`.

use eframe::egui::{self, Pos2, Rect, Ui, Vec2};

/// One row in the Advanced settings command reference.
pub struct CommandEntry {
    pub category: &'static str,
    pub name: &'static str,
    pub binding: &'static str,
}

/// Speed multiplier for turbo pan: screen-space pull distance → px/frame.
pub const TURBO_PAN_GAIN: f32 = 0.12;
/// Minimum pull before turbo pan engages (distinguishes from a right-click tap).
pub const TURBO_PAN_ENGAGE_PX: f32 = 4.0;
/// Minimum movement before the pan axis locks to horizontal or vertical.
pub const TURBO_PAN_AXIS_LOCK_PX: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PanAxis {
    Horizontal,
    Vertical,
}

/// Ctrl + right-drag turbo pan state (per canvas interaction).
#[derive(Default, Clone)]
pub struct TurboPanState {
    anchor: Option<Pos2>,
    axis: Option<PanAxis>,
    /// Pull exceeded [`TURBO_PAN_ENGAGE_PX`]; suppresses the file context menu.
    engaged: bool,
    /// Set on release after an engaged turbo pan; cleared once the menu decision runs.
    suppress_menu: bool,
}

impl TurboPanState {
    pub fn should_suppress_context_menu(&self) -> bool {
        self.suppress_menu
    }

    pub fn acknowledge_context_menu(&mut self) {
        self.suppress_menu = false;
    }

    /// Apply turbo pan while Ctrl and the secondary button are held. Returns `true`
    /// when panning is active this frame (left-drag pan should be skipped).
    pub fn step(
        &mut self,
        ctx: &egui::Context,
        canvas_rect: Rect,
        pointer: Option<Pos2>,
        cam_offset: &mut Vec2,
    ) -> bool {
        let (secondary_down, secondary_released, ctrl) = ctx.input(|i| {
            (
                i.pointer.button_down(egui::PointerButton::Secondary),
                i.pointer.button_released(egui::PointerButton::Secondary),
                i.modifiers.ctrl,
            )
        });

        if secondary_released {
            let was_engaged = self.engaged;
            self.reset();
            if was_engaged {
                self.suppress_menu = true;
            }
            return was_engaged;
        }

        if self.anchor.is_some() && !ctrl {
            self.reset();
            return false;
        }

        let Some(p) = pointer else {
            return self.engaged;
        };
        if !canvas_rect.contains(p) {
            return self.engaged;
        }

        if secondary_down && ctrl && self.anchor.is_none() {
            self.anchor = Some(p);
            self.axis = None;
            self.engaged = false;
        }

        let Some(anchor) = self.anchor else {
            return false;
        };

        if !secondary_down || !ctrl {
            return false;
        }

        let delta = p - anchor;
        if !self.engaged && delta.length() >= TURBO_PAN_ENGAGE_PX {
            self.engaged = true;
        }

        if self.axis.is_none() && delta.length() >= TURBO_PAN_AXIS_LOCK_PX {
            self.axis = Some(if delta.x.abs() >= delta.y.abs() {
                PanAxis::Horizontal
            } else {
                PanAxis::Vertical
            });
        }

        if self.engaged {
            let (dx, dy) = match self.axis {
                Some(PanAxis::Horizontal) => (delta.x, 0.0),
                Some(PanAxis::Vertical) => (0.0, delta.y),
                None => (0.0, 0.0),
            };
            cam_offset.x += dx * TURBO_PAN_GAIN;
            cam_offset.y += dy * TURBO_PAN_GAIN;
            ctx.request_repaint();
        }

        self.engaged
    }

    fn reset(&mut self) {
        self.anchor = None;
        self.axis = None;
        self.engaged = false;
    }
}

/// Screen-space pan when a right-drag, middle-drag, or (when `primary_pans`)
/// primary drag starts over chrome that egui did not give to the canvas.
///
/// `allow` is false while another gesture owns the camera (turbo pan, a
/// focused page). `over_chrome` is this frame's palette hit. The drag stays
/// latched after the pointer leaves the palette until the button comes up.
/// Clicks stay on the palette: this does not synthesize a primary click.
pub fn chrome_pass_pan_delta(
    ctx: &egui::Context,
    canvas: &egui::Response,
    canvas_rect: Rect,
    allow: bool,
    over_chrome: bool,
    primary_pans: bool,
) -> Option<Vec2> {
    let delta = ctx.input(|i| i.pointer.delta());
    let (secondary, middle, primary) = ctx.input(|i| {
        (
            i.pointer.button_down(egui::PointerButton::Secondary),
            i.pointer.button_down(egui::PointerButton::Middle),
            i.pointer.button_down(egui::PointerButton::Primary),
        )
    });
    let down = secondary || middle || (primary_pans && primary);
    let id = egui::Id::new("atlas.chrome.pan_latch");
    let latch = ctx.data(|d| d.get_temp(id).unwrap_or(false));
    let latch = step_pan_latch(latch, allow, over_chrome, down);
    ctx.data_mut(|d| d.insert_temp(id, latch));
    let pointer_in = ctx
        .pointer_latest_pos()
        .is_some_and(|p| canvas_rect.contains(p));
    pan_delta_through_chrome(
        latch,
        pointer_in,
        secondary,
        middle,
        primary,
        primary_pans,
        canvas.dragged_by(egui::PointerButton::Secondary),
        canvas.dragged_by(egui::PointerButton::Middle),
        canvas.dragged_by(egui::PointerButton::Primary),
        delta,
    )
}

pub(crate) fn step_pan_latch(
    latch: bool,
    allow: bool,
    over_chrome: bool,
    button_down: bool,
) -> bool {
    if !allow || !button_down {
        false
    } else if over_chrome {
        true
    } else {
        latch
    }
}

pub(crate) fn pan_delta_through_chrome(
    pass: bool,
    pointer_in_canvas: bool,
    secondary_down: bool,
    middle_down: bool,
    primary_down: bool,
    primary_pans: bool,
    canvas_secondary: bool,
    canvas_middle: bool,
    canvas_primary: bool,
    delta: Vec2,
) -> Option<Vec2> {
    if !pass || !pointer_in_canvas || delta == Vec2::ZERO {
        return None;
    }
    let want = (secondary_down && !canvas_secondary)
        || (middle_down && !canvas_middle)
        || (primary_pans && primary_down && !canvas_primary);
    want.then_some(delta)
}

/// Reference table for Advanced settings. `source_hint` names the file where
/// the app's `ENTRIES` table lives so contributors keep it complete.
pub fn shortcuts_reference_ui(ui: &mut Ui, entries: &[CommandEntry], source_hint: &str) {
    let visuals = ui.visuals();
    let hint = visuals.weak_text_color();
    let name_color = visuals.text_color();
    let binding_color = visuals.weak_text_color();

    ui.label(egui::RichText::new("Commands & shortcuts").small().strong());
    ui.label(
        egui::RichText::new(format!(
            "All bindings below are defined in {source_hint} — add new \
             commands there so this list stays complete."
        ))
        .small()
        .color(hint),
    );
    ui.add_space(6.0);

    let mut last_category = "";
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .id_salt("commands_reference")
        .show(ui, |ui| {
            for entry in entries {
                if entry.category != last_category {
                    if !last_category.is_empty() {
                        ui.add_space(6.0);
                    }
                    ui.label(egui::RichText::new(entry.category).small().strong());
                    last_category = entry.category;
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(entry.name).small().color(name_color));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(entry.binding)
                                .small()
                                .color(binding_color),
                        );
                    });
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::{pan_delta_through_chrome, step_pan_latch};
    use eframe::egui::Vec2;

    #[test]
    fn palette_hover_pans_when_the_canvas_did_not_receive_the_drag() {
        let delta = Vec2::new(4.0, -2.0);
        assert_eq!(
            pan_delta_through_chrome(
                true, true, true, false, false, false, false, false, false, delta
            ),
            Some(delta)
        );
        assert_eq!(
            pan_delta_through_chrome(
                true, true, false, true, false, false, false, false, false, delta
            ),
            Some(delta)
        );
    }

    #[test]
    fn palette_clicks_and_canvas_owned_drags_do_not_pan_twice() {
        let delta = Vec2::new(3.0, 1.0);
        assert_eq!(
            pan_delta_through_chrome(
                true, true, false, false, true, false, false, false, false, delta
            ),
            None,
            "primary click on a palette icon"
        );
        assert_eq!(
            pan_delta_through_chrome(
                true, true, true, false, false, false, true, false, false, delta
            ),
            None,
            "canvas already has the right-drag"
        );
        assert_eq!(
            pan_delta_through_chrome(
                false, true, true, false, false, false, false, false, false, delta
            ),
            None
        );
        assert_eq!(
            pan_delta_through_chrome(
                true, true, false, false, true, true, false, false, false, delta
            ),
            Some(delta),
            "space or hand tool primary-drags through the palette"
        );
    }

    #[test]
    fn a_pan_that_starts_on_a_palette_continues_after_the_pointer_leaves() {
        assert!(step_pan_latch(false, true, true, true));
        assert!(step_pan_latch(true, true, false, true));
        assert!(!step_pan_latch(true, true, false, false));
        assert!(!step_pan_latch(true, false, true, true));
    }
}
