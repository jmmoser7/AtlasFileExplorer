//! Board adapter for shared video controls and playhead chrome.
use super::*;
use atlas_shell::{canvas_scale, canvas_text};
use eframe::egui::{Color32, FontId, Stroke, Vec2};

const PLAYHEAD_PX: f32 = 2.0;
const TIMECODE_PX: f32 = 12.0;

impl SlateApp {
    pub(crate) fn paint_video_chrome(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &slate_doc::scene::Node,
    ) {
        let id = node.id;
        atlas_shell::video_controls::paint(
            painter,
            xf.rect_w2s(node.rect.normalized()),
            xf.z,
            self.video.play.as_ref().is_some_and(|play| play.id == id),
            self.palette(),
        );
        let control = atlas_shell::video_controls::rect(xf.rect_w2s(node.rect.normalized()), xf.z);
        if painter
            .ctx()
            .input(|input| input.pointer.hover_pos())
            .is_some_and(|at| control.contains(at))
        {
            egui::show_tooltip_at_pointer(
                painter.ctx(),
                painter.layer_id(),
                egui::Id::new(("video-control", id.0)),
                |ui| {
                    ui.add(egui::Label::new(self.video_control_hint(id)).extend());
                },
            );
        }
        let Some(time) = self.video_shown(id) else {
            return;
        };
        let Some((start, end)) = self.video_window(id) else {
            return;
        };
        let u = self
            .video
            .hover
            .and_then(|(hid, u)| (hid == id).then_some(u))
            .unwrap_or_else(|| atlas_core::video::fraction_at(time, start, end));
        let rect = node.rect.normalized();
        let x = rect.x + rect.w * u;
        let a = rect.rotate_point([x, rect.y], node.rotation_deg);
        let b = rect.rotate_point([x, rect.y + rect.h], node.rotation_deg);
        let a = xf.w2s(Pos2::new(a[0], a[1]));
        let b = xf.w2s(Pos2::new(b[0], b[1]));
        let weight = canvas_scale::px(PLAYHEAD_PX, xf.z);
        if !canvas_scale::too_small(weight) {
            painter.line_segment(
                [a, b],
                Stroke::new(
                    weight + canvas_scale::px(1.0, xf.z),
                    Color32::from_black_alpha(170),
                ),
            );
            painter.line_segment([a, b], Stroke::new(weight, Color32::WHITE));
        }
        let size = canvas_scale::px(TIMECODE_PX, xf.z);
        if !canvas_text::legible(size) {
            return;
        }
        let label = atlas_core::video::format_timecode(time);
        let font = FontId::proportional(size);
        let laid = canvas_text::layout_no_wrap(painter, label, font, Color32::WHITE);
        let pad = Vec2::new(canvas_scale::px(5.0, xf.z), canvas_scale::px(2.0, xf.z));
        let mid = rect.rotate_point([rect.x + rect.w * 0.5, rect.y + rect.h], node.rotation_deg);
        let anchor =
            xf.w2s(Pos2::new(mid[0], mid[1])) - Vec2::new(0.0, canvas_scale::px(6.0, xf.z));
        let origin = anchor - Vec2::new(laid.size().x * 0.5 + pad.x, laid.size().y + pad.y * 2.0);
        let bg = egui::Rect::from_min_size(origin, laid.size() + pad * 2.0);
        let srect = xf.rect_w2s(rect);
        if bg.width() > srect.width() || bg.height() > srect.height() {
            return;
        }
        painter.rect_filled(
            bg,
            canvas_scale::px(3.0, xf.z),
            Color32::from_black_alpha(150),
        );
        laid.paint(painter, origin + pad, Color32::WHITE);
    }

    fn video_control_hint(&self, id: NodeId) -> &'static str {
        if self.video.failed_play == Some(id)
            || self.video.strips.get(&id).is_some_and(|strip| strip.failed)
        {
            "Video unavailable. Click Play to retry."
        } else if self
            .video
            .pool
            .as_ref()
            .is_some_and(|pool| pool.health().status == atlas_core::video::VideoStatus::Unavailable)
        {
            "Video decoder unavailable. Click Play to retry."
        } else if self.video.play.as_ref().is_some_and(|play| play.id == id) {
            "Pause"
        } else {
            "Play"
        }
    }
}
