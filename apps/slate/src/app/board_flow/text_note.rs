//! Text-media presentation adapts the shared selection controls.
use super::*;
use atlas_core::display::SLATE_BOARD;
impl SlateApp {
    /// A note an agent writes: its newest reply shows in the note itself (the
    /// board paints it), and the prompt docks along the bottom with Run. An
    /// empty note is a prompt field until its first reply.
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
        let px = |v: f32| canvas_scale::px(v, z);
        let font = canvas_scale::font(BLOCK_TEXT_PX, z);
        if !canvas_text::legible(font.size) {
            return;
        }
        let running = self.agent_is_running(id);
        let waiting = self.generations_waiting(id);
        let reply = self.agent_reply(id);
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
        let hovered = ui
            .ctx()
            .pointer_latest_pos()
            .is_some_and(|p| body.expand(px(8.0)).contains(p));
        let reveal = hovered || self.board_sel.contains(&id) || self.flow_capsule_open(id);
        let empty = reply.is_none() && !own;
        let property_controls = self.agent_property_controls_own(id);
        let ink = self.palette().overlay();
        if empty && !property_controls && !(running || waiting > 0) {
            let wired = self
                .generator_view(id)
                .inputs
                .iter()
                .any(|i| i.role == InputRole::Prompt);
            let hint = if wired {
                "Add to the wired prompt, or press Run"
            } else {
                "Wire a note to Prompt, or type here"
            };
            let field = Rect::from_center_size(
                body.center(),
                egui::vec2(body.width() - px(32.0), px(64.0)),
            );
            paint_overlay_pill(painter, field.expand(px(8.0)), px(10.0), ink.scrim, &ink, z);
            self.paint_generator_prompt(ui, id, field, z, hint);
        }
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
        if !reveal || property_controls {
            return;
        }
        let at = Pos2::new(body.left() + px(12.0), body.bottom() - px(40.0));
        let (run, rect) = Self::generator_button(
            ui,
            painter,
            at,
            if running { "Stop" } else { "Run" },
            Id::new(("text-block-run", id.0)),
            &ink,
            z,
        );
        if run.clicked() {
            self.board_sel = std::iter::once(id).collect();
            let command = if running {
                "portal.agent.stop"
            } else {
                "portal.agent.send"
            };
            self.dispatch(ui.ctx(), atlas_commands::CommandId(command), None);
        }
        if !empty {
            let capsule = Rect::from_min_max(
                Pos2::new(rect.right() + px(6.0), rect.top()),
                Pos2::new(body.right() - px(12.0), rect.bottom()),
            );
            self.paint_prompt_capsule(ui, painter, id, capsule, body, z);
        }
    }
}
