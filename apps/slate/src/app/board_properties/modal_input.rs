//! Raw property click-away input follows egui modal ownership.
use super::*;

impl SlateApp {
    /// This frame's press that commits an open panel's preview (D11), if
    /// any: `Some(true)` for a primary press on a crop handle, a cropping
    /// image, or a painted curve grip, which is that gesture rather than a
    /// click-away. Crop keeps the Corners panel up (D09) and the selection
    /// whole; a grip press is the grip's drag (P1.curve.grips). Neither is
    /// eaten. Right-drag and middle-drag are the
    /// canvas pan, so only a secondary or middle click without a drag counts.
    pub(super) fn property_dismiss_press(&self, ctx: &egui::Context) -> Option<bool> {
        if ctx.memory(|memory| memory.top_modal_layer().is_some()) {
            return None;
        }
        let (primary, click) = ctx.input(|i| {
            (
                i.pointer
                    .button_pressed(egui::PointerButton::Primary)
                    .then(|| i.pointer.press_origin()),
                i.pointer.button_clicked(egui::PointerButton::Secondary)
                    || i.pointer.button_clicked(egui::PointerButton::Middle),
            )
        });
        match primary {
            Some(origin) => Some(origin.is_some_and(|p| {
                self.crop_owns_pointer(p)
                    || self.property_grip_under(p)
                    || (self.board_tool == BoardTool::Select
                        && self.wire_grip_at(p, &self.board_xf()).is_some())
            })),
            None => click.then_some(false),
        }
    }
}
