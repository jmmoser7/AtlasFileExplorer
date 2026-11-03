//! Text's source sheet and flex review section; neither changes document geometry.
use super::*;
use egui::{FontId, Stroke as EStroke};
use std::sync::Arc;
#[cfg(test)]
mod tests;
mod wheel;

// Actual DOM receipt TEXT-review-answer: border 1 + top padding 10 +
// anonymous parent line box 21.6875 + paragraph margin 5. The inline
// 10px label's own 15.5px line height does not size that parent line box.
const REVIEW_COPY_TOP: f32 = 37.6875;
const REVIEW_ACTION_HEIGHT: f32 = 23.4;

#[derive(Clone, Copy)]
struct ReviewHits {
    ancestor: Rect,
    source: Rect,
    section: Rect,
    source_layer: egui::LayerId,
    section_layer: egui::LayerId,
}

#[derive(Clone, Copy)]
struct WheelOwner {
    frame: u64,
    pointer: Pos2,
    layer: Option<egui::LayerId>,
    owner: Option<NodeId>,
}

fn scroll_id(id: NodeId, review: bool) -> Id {
    Id::new(("text-sheet-offset", id.0, review))
}

/// Logical scroll offsets survive zoom, edit entry, and closing Agent chrome.
fn source_offset(ctx: &egui::Context, id: NodeId, zoom: f32) -> f32 {
    ctx.data(|d| d.get_temp::<f32>(scroll_id(id, false)).unwrap_or(0.0)) * zoom
}

fn store_source_offset(ctx: &egui::Context, id: NodeId, offset: f32, zoom: f32) {
    ctx.data_mut(|d| d.insert_temp(scroll_id(id, false), offset / zoom.max(1.0e-4)));
}

fn review_galley(
    ctx: &egui::Context,
    text: &str,
    width: f32,
    zoom: f32,
    color: Color32,
) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(
        text.to_owned(),
        FontId::proportional(12.0 * zoom),
        color,
        width.max(1.0),
    );
    job.sections[0].format.line_height = Some(17.4 * zoom);
    ctx.fonts(|fonts| fonts.layout_job(job))
}

/// CSS's non-shrinking review follows the editable source, inside the host.
fn split_sheet(body: Rect, review_height: f32) -> (Rect, Rect) {
    let top = (body.bottom() - review_height).max(body.top());
    (
        Rect::from_min_max(body.min, Pos2::new(body.right(), top)),
        Rect::from_min_max(Pos2::new(body.left(), top), body.max),
    )
}

/// Below the Studio's supported size domain, preserve the authored viewport.
/// The review alone extends transiently; scene bounds and wire anchors stay put.
fn sheet_geometry(body: Rect, review_height: f32, line_height: f32, zoom: f32) -> (Rect, Rect) {
    if body.height() < review_height + (60.0 + line_height) * zoom {
        let review = Rect::from_min_size(
            body.left_bottom(),
            egui::vec2(body.width().max(420.0 * zoom), review_height),
        );
        (body, review)
    } else {
        let (sheet, review) = split_sheet(body, review_height);
        let source = Rect::from_min_max(
            sheet.min + egui::vec2(16.0, 16.0) * zoom,
            sheet.max - egui::vec2(16.0, 44.0) * zoom,
        );
        (source, review)
    }
}

impl SlateApp {
    pub(crate) fn text_source_editor_clip(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        area: Rect,
    ) -> Rect {
        ctx.data(|d| d.get_temp::<ReviewHits>(Id::new(("text-review-visible-hits", id.0))))
            .map_or(Rect::NOTHING, |h| area.intersect(h.ancestor))
    }
    pub(crate) fn text_source_editor_hits(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        source: Rect,
        layer: egui::LayerId,
    ) {
        let key = Id::new(("text-review-visible-hits", id.0));
        ctx.data_mut(|d| {
            if let Some(mut hits) = d.get_temp::<ReviewHits>(key) {
                hits.source = source.intersect(hits.ancestor);
                hits.source_layer = layer;
                d.insert_temp(key, hits);
            }
        });
    }
    pub(crate) fn text_review_visible(&self, id: NodeId) -> bool {
        self.text_review_view(id)
            .is_some_and(|r| r.edit || r.selection.is_none())
    }
    pub(crate) fn text_source_offset(&self, ctx: &egui::Context, id: NodeId, zoom: f32) -> f32 {
        source_offset(ctx, id, zoom)
    }
    pub(crate) fn set_text_source_offset(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        offset: f32,
        zoom: f32,
    ) {
        store_source_offset(ctx, id, offset, zoom);
    }
    fn text_sheet_geometry(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        body: Rect,
        zoom: f32,
    ) -> (Rect, Option<Rect>) {
        let line = self.doc().scene.node(id).map_or(14.0, |n| match &n.kind {
            NodeKind::Text(t) => t.size * 1.6,
            _ => 14.0 * 1.6,
        });
        let Some(review) = self
            .text_review_view(id)
            .filter(|r| r.edit || r.selection.is_none())
        else {
            return (body, None);
        };
        let galley = review_galley(
            ctx,
            &review.text,
            body.width() - 32.0 * zoom,
            zoom,
            self.palette().ink,
        );
        let height =
            (REVIEW_COPY_TOP + galley.size().y / zoom + 10.0 + REVIEW_ACTION_HEIGHT + 12.0)
                .min(165.0)
                * zoom;
        let (source, mut section) = self
            .doc()
            .scene
            .node(id)
            .filter(|n| n.rotation_deg.abs() > 0.01 || n.clip.is_some())
            .map_or_else(
                || sheet_geometry(body, height, line, zoom),
                |n| {
                    let rotation = egui::emath::Rot2::from_angle(n.rotation_deg.to_radians());
                    let corners = [
                        body.left_top(),
                        body.right_top(),
                        body.left_bottom(),
                        body.right_bottom(),
                    ]
                    .map(|p| body.center() + rotation * (p - body.center()));
                    let bounds = Rect::from_points(&corners);
                    (
                        body,
                        Rect::from_min_size(
                            bounds.left_bottom(),
                            egui::vec2(bounds.width().max(420.0 * zoom), height),
                        ),
                    )
                },
            );
        if section.width() != body.width() {
            let copy = review_galley(
                ctx,
                &review.text,
                section.width() - 32.0 * zoom,
                zoom,
                self.palette().ink,
            );
            section.max.y = section.top()
                + ((REVIEW_COPY_TOP + 10.0 + REVIEW_ACTION_HEIGHT + 12.0) * zoom + copy.size().y)
                    .min(165.0 * zoom);
        }
        (source, Some(section))
    }
    pub(crate) fn text_source_viewport(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        body: Rect,
        zoom: f32,
    ) -> Rect {
        self.text_sheet_geometry(ctx, id, body, zoom).0
    }
    pub(crate) fn text_review_visual_body(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        body: Rect,
        zoom: f32,
    ) -> Rect {
        self.text_sheet_geometry(ctx, id, body, zoom)
            .1
            .map_or(body, |r| body.union(r))
    }

    /// The same section is painted for authored Text and linked text documents,
    /// including while the native editor owns the source sheet above it.
    pub(crate) fn paint_text_review(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        id: NodeId,
        body: Rect,
        zoom: f32,
        interactive: bool,
    ) -> Rect {
        let (source, section) = self.text_sheet_geometry(ui.ctx(), id, body, zoom);
        let Some(review) = self
            .text_review_view(id)
            .filter(|r| r.edit || r.selection.is_none())
        else {
            return source;
        };
        let Some(section) = section else {
            return source;
        };
        if !section.is_positive() {
            return source;
        }
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                Id::new(("text-review-visible-hits", id.0)),
                ReviewHits {
                    ancestor: painter.clip_rect(),
                    source: source.intersect(painter.clip_rect()),
                    section: section.intersect(painter.clip_rect()),
                    source_layer: painter.layer_id(),
                    section_layer: painter.layer_id(),
                },
            )
        });
        let palette = self.palette();
        let p = painter.with_clip_rect(section.intersect(painter.clip_rect()));
        p.rect_filled(section, 0.0, palette.card);
        p.rect_filled(section, 0.0, palette.link.gamma_multiply(0.03));
        p.line_segment(
            [section.left_top(), section.right_top()],
            EStroke::new(zoom, palette.sub.gamma_multiply(0.18)),
        );
        let copy = review_galley(
            ui.ctx(),
            &review.text,
            section.width() - 32.0 * zoom,
            zoom,
            palette.ink,
        );
        let content_height =
            (REVIEW_COPY_TOP + 10.0 + REVIEW_ACTION_HEIGHT + 12.0) * zoom + copy.size().y;
        let offset =
            self.text_sheet_scroll(ui, id, (section, content_height), zoom, true, interactive);
        let origin = section.min + egui::vec2(16.0 * zoom, 11.0 * zoom - offset);
        canvas_text::text(
            &p,
            origin + egui::vec2(0.0, 6.0 * zoom),
            Align2::LEFT_TOP,
            if review.edit {
                "Proposed revision"
            } else {
                "Passage note"
            },
            FontId::proportional(10.0 * zoom),
            palette.sub,
        );
        let at = section.min + egui::vec2(16.0 * zoom, REVIEW_COPY_TOP * zoom - offset);
        p.galley(at, copy.clone(), palette.ink);
        let mut x = origin.x;
        let y = at.y + copy.size().y + 10.0 * zoom;
        let mut action = None;
        for (apply, label) in [
            (
                true,
                if review.selection.is_some() {
                    "Replace highlighted text"
                } else {
                    "Replace document"
                },
            ),
            (false, "Dismiss"),
        ] {
            if apply && !review.edit {
                continue;
            }
            let text = p.layout_no_wrap(
                label.to_owned(),
                FontId::proportional(11.0 * zoom),
                palette.ink,
            );
            let rect = Rect::from_min_size(
                Pos2::new(x, y),
                egui::vec2(text.size().x + 18.0 * zoom, REVIEW_ACTION_HEIGHT * zoom),
            );
            let hit = rect.intersect(section).intersect(painter.clip_rect());
            let pointer = ui.ctx().pointer_hover_pos();
            let pointer_blocked = pointer
                .is_some_and(|p| hit.contains(p) && !self.text_review_wheel_owned(ui.ctx(), id, p));
            let response = ui.interact(
                hit,
                Id::new(("text-review-action", id.0, apply)),
                if interactive && hit.is_positive() && !pointer_blocked {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            p.rect_filled(
                rect,
                4.0 * zoom,
                palette
                    .sub
                    .gamma_multiply(if response.hovered() { 0.16 } else { 0.08 }),
            );
            p.galley(
                rect.min + egui::vec2(9.0 * zoom, 4.0 * zoom),
                text,
                palette.ink,
            );
            if response.clicked() {
                action = Some(if apply {
                    "portal.agent.text_review.apply"
                } else {
                    "portal.agent.text_review.dismiss"
                });
            }
            x = rect.right() + 8.0 * zoom;
        }
        if let Some(command) = action {
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId(command),
                Some(serde_json::json!({"node": id}).to_string()),
            );
        }
        source
    }

    pub(crate) fn text_sheet_scroll(
        &self,
        ui: &egui::Ui,
        id: NodeId,
        sheet: (Rect, f32),
        zoom: f32,
        review: bool,
        interactive: bool,
    ) -> f32 {
        let (viewport, content_height) = sheet;
        let key = scroll_id(id, review);
        let max = (content_height - viewport.height()).max(0.0);
        let mut offset = ui.ctx().data(|d| d.get_temp::<f32>(key).unwrap_or(0.0)) * zoom;
        let pointer = ui.input(|i| {
            if i.modifiers.ctrl {
                None
            } else {
                i.pointer.hover_pos()
            }
        });
        let owns = interactive
            && pointer.is_some_and(|p| {
                viewport.contains(p)
                    && ui.ctx().layer_id_at(p) == Some(ui.layer_id())
                    && self.text_review_wheel_owned(ui.ctx(), id, p)
            });
        if owns && self.claim_text_wheel(ui.ctx(), id, review) {
            offset -= ui.input(|i| i.smooth_scroll_delta.y);
        }
        offset = offset.clamp(0.0, max);
        ui.ctx()
            .data_mut(|d| d.insert_temp(key, offset / zoom.max(1.0e-4)));
        atlas_shell::selection_tools::generator::scroll::paint(
            ui,
            key,
            viewport,
            content_height,
            offset,
            zoom,
            self.palette(),
        );
        offset
    }
}
