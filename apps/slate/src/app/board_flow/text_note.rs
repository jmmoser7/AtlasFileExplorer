//! Text-media presentation adapts the shared selection controls.
use super::*;
use atlas_core::display::SLATE_BOARD;
mod answers;
mod review;
impl SlateApp {
    /// Text stays its authored content surface. The explicitly opened Agent
    /// dialogue owns the only composer and Send control.
    pub(crate) fn paint_agent_note(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &slate_doc::scene::Node,
        body: Rect,
    ) {
        let id = node.id;
        let z = xf.z;
        self.paint_text_result_strip(ui, id, body, z);
        // Ordinary Text keeps its authored sheet. Shared Agent chrome owns
        // sending and activity; the response is reviewed below the sheet.
        if self.text_response_only(id) {
            return;
        }
        let px = |v: f32| canvas_scale::px(v, z);
        let font = canvas_scale::font(BLOCK_TEXT_PX, z);
        if !canvas_text::legible(font.size) {
            return;
        }
        let running = self.agent_is_running(id);
        let reply = self.displayed_text_output(id);
        let own = self.agent_text_window_owned(id);
        if let Some(reply) = &reply {
            if !own && !running {
                let laid = canvas_text::layout(
                    painter,
                    reply.clone(),
                    canvas_scale::font(BLOCK_TEXT_PX, z),
                    Color32::WHITE,
                    body.width(),
                );
                self.fit_agent_note(id, reply, laid.size().y / z.max(SLATE_BOARD.min));
            }
        }
        let ink = self.palette().overlay();
        let failure = self
            .agent_failure_reason(id)
            .map(str::to_string)
            .filter(|_| !running);
        if let Some(reason) = failure {
            let laid = canvas_text::layout(
                painter,
                reason,
                canvas_scale::font(crate::app::board_agent::GENERATOR_CHIP_PX, z),
                ink.warn,
                body.width() - px(24.0),
            );
            let at = Pos2::new(body.left() + px(12.0), body.top() + px(12.0));
            paint_overlay_pill(
                painter,
                Rect::from_min_size(at, laid.size()).expand(px(5.0)),
                px(6.0),
                ink.fill,
                &ink,
                z,
            );
            laid.paint(painter, at, ink.warn);
        }
        self.paint_run_progress(ui, painter, id, body, z, "Writing", false);
    }
}
