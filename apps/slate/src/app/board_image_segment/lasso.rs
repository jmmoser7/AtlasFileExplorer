//! Polygonal image selection using the board's MultiPoint collector.
use super::*;
use crate::app::board_path::multi_point::{self, Event, Transition};

impl SlateApp {
    pub(crate) fn start_image_lasso(&mut self, image: NodeId) {
        if !matches!(
            self.doc().scene.node(image).map(|node| &node.kind),
            Some(NodeKind::Image(_))
        ) {
            return;
        }
        let Some(source_key) = self.segment_source_key(image) else {
            return;
        };
        self.transition_image_tool(image, crate::app::board::BoardTool::Select);
        self.image_segments.clear();
        self.image_segments.pinned = None;
        self.image_segments.lasso = Some(LassoDraft {
            tab: self.tab().id,
            host: image,
            source_key,
            points: Vec::new(),
            last_click: None,
            last_click_screen: None,
        });
    }

    pub(super) fn image_lasso_input(&mut self, ui: &egui::Ui, xf: &BoardXf) -> bool {
        let (click, shift, screen, now) = ui.input(|input| {
            (
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.modifiers.shift,
                input.pointer.interact_pos(),
                Instant::now(),
            )
        });
        if let Some(draft) = self.image_segments.lasso.clone() {
            if draft.tab != self.tab().id
                || self.doc().scene.node(draft.host).is_none()
                || self.segment_source_key(draft.host).as_deref() != Some(draft.source_key.as_str())
            {
                self.image_segments.lasso = None;
                return false;
            }
            if click {
                if let Some(screen) = screen {
                    let world = xf.s2w(screen);
                    let host_id = draft.host;
                    let Some(host) = self.doc().scene.node(host_id) else {
                        self.image_segments.lasso = None;
                        return true;
                    };
                    let NodeKind::Image(img) = &host.kind else {
                        self.image_segments.lasso = None;
                        return true;
                    };
                    let (x, y) =
                        slate_doc::image_paint::world_to_host_norm(host, img, world.x, world.y);
                    if !self.point_in_image_paint_window(host_id, world) {
                        return false;
                    }
                    let constrained = self.constrain_lasso_point(host, img, [x, y], shift);
                    let point = [
                        constrained[0].clamp(0.0, 1.0),
                        constrained[1].clamp(0.0, 1.0),
                    ];
                    let double_click = draft.last_click.is_some_and(|last| {
                        now.saturating_duration_since(last) <= LASSO_DOUBLE_CLICK
                            && draft.last_click_screen.is_some_and(|last_pos| {
                                last_pos.distance(screen) <= LASSO_DOUBLE_CLICK_PX
                            })
                    });
                    let close_to_first = draft.points.len() >= 3
                        && distance(draft.points[0], point) <= LASSO_CLOSE_NORM;
                    let Some(draft) = self.image_segments.lasso.as_mut() else {
                        return true;
                    };
                    let result = multi_point::collect(
                        &mut draft.points,
                        Event::Pick {
                            point,
                            close: close_to_first,
                            finish: double_click,
                        },
                        multi_point::LASSO,
                    );
                    if result == Transition::Finish {
                        self.finish_image_lasso(screen);
                    } else if result == Transition::Added {
                        draft.last_click = Some(now);
                        draft.last_click_screen = Some(screen);
                    }
                    return true;
                }
            }
            return ui.input(|input| {
                input.pointer.primary_down()
                    || input.pointer.button_released(egui::PointerButton::Primary)
            });
        }

        false
    }

    pub(super) fn constrain_lasso_point(
        &self,
        host: &Node,
        img: &slate_doc::scene::ImageNode,
        point: [f32; 2],
        shift: bool,
    ) -> [f32; 2] {
        let Some(draft) = &self.image_segments.lasso else {
            return point;
        };
        let Some(&last) = draft.points.last() else {
            return point;
        };
        if !shift {
            return point;
        }
        let basis = slate_doc::image_paint::image_content_rect(host, img);
        let dx = (point[0] - last[0]) * basis.w;
        let dy = (point[1] - last[1]) * basis.h;
        let length = dx.hypot(dy);
        if length <= f32::EPSILON {
            return point;
        }
        let step = std::f32::consts::FRAC_PI_4;
        let angle = (dy.atan2(dx) / step).round() * step;
        [
            last[0] + length * angle.cos() / basis.w.max(f32::EPSILON),
            last[1] + length * angle.sin() / basis.h.max(f32::EPSILON),
        ]
    }

    pub(super) fn finish_image_lasso(&mut self, screen: Pos2) {
        let Some(draft) = self.image_segments.lasso.as_mut() else {
            return;
        };
        if multi_point::collect(&mut draft.points, Event::Finish, multi_point::LASSO)
            != Transition::Finish
        {
            return;
        }
        let Some(draft) = self.image_segments.lasso.take() else {
            return;
        };
        if draft.points.len() < 3 || draft.tab != self.tab().id {
            return;
        }
        let Some(source_key) = self.segment_source_key(draft.host) else {
            return;
        };
        if source_key != draft.source_key {
            return;
        }
        let contours = vec![draft.points];
        let Some(path) = slate_doc::image_paint::region_path(contours.clone()) else {
            return;
        };
        let result = SegmentResult {
            host: draft.host,
            gen: self.scene_gen,
            source_key: source_key.clone(),
            local: region_node(path),
            mask: std::sync::Arc::new(contours.clone()),
            candidates: std::sync::Arc::new(vec![atlas_segment::Candidate {
                quality: 1.0,
                contours,
            }]),
            selected: 0,
        };
        self.image_segments.pinned = Some(PinnedSelection {
            tab: draft.tab,
            host: draft.host,
            source_key,
            result,
            prompts: Vec::new(),
            screen,
            point_norm: [0.5, 0.5],
        });
        self.image_segments.hover = None;
        self.image_segments.result = None;
    }
}
