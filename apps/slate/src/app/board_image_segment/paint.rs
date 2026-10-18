//! Image-local masks and pinned refinement controls.
use super::*;

impl SlateApp {
    pub(super) fn paint_image_segment_hover(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
    ) {
        self.image_segments.action_rect = None;
        self.image_segments.corridor = None;
        let pinned = self.image_segments.pinned.clone();
        let hover = self.image_segments.hover.as_ref();
        let host_id = if let Some(selection) = &pinned {
            if selection.tab != self.tab().id {
                return;
            }
            selection.host
        } else if let Some(hover) = hover {
            if hover.gen != self.scene_gen || hover.tab != self.tab().id {
                return;
            }
            hover.host
        } else if let Some(draft) = self.image_segments.lasso.as_ref() {
            draft.host
        } else {
            return;
        };
        let Some(host) = self.doc().scene.node(host_id) else {
            return;
        };
        let NodeKind::Image(img) = &host.kind else {
            return;
        };
        let basis = slate_doc::image_paint::image_content_rect(host, img);
        let center = host.rect.center();
        let rotation = host.rotation_deg;
        let host_bounds = xf.rect_w2s(slate_doc::node_aabb(host));
        let draft_preview = self
            .image_segments
            .lasso
            .as_ref()
            .filter(|draft| draft.host == host_id && draft.tab == self.tab().id)
            .and_then(|_| ui.ctx().pointer_latest_pos())
            .map(|screen| {
                let world = xf.s2w(screen);
                let (x, y) =
                    slate_doc::image_paint::world_to_host_norm(host, img, world.x, world.y);
                self.constrain_lasso_point(
                    host,
                    img,
                    [x, y],
                    ui.input(|input| input.modifiers.shift),
                )
            });
        let result = pinned
            .as_ref()
            .map(|selection| selection.result.clone())
            .or_else(|| self.image_segments.result.clone());
        if let Some(result) = &result {
            let region = layer_node_to_world(host, img, &result.local);
            if let NodeKind::Shape(shape) = &region.kind {
                if let Some(path) = &shape.path {
                    crate::app::board_path::paint_path_shape(
                        self,
                        painter,
                        xf,
                        &region,
                        shape,
                        path,
                        &|color| color,
                    );
                }
            }
        }
        if let Some(draft) = self
            .image_segments
            .lasso
            .as_ref()
            .filter(|draft| draft.host == host_id && draft.tab == self.tab().id)
        {
            let screen_point = |point: [f32; 2]| {
                let world = slate_doc::geom::orbit_point(
                    center,
                    (basis.x + point[0] * basis.w, basis.y + point[1] * basis.h),
                    rotation,
                );
                xf.w2s(Pos2::new(world.0, world.1))
            };
            let stroke = egui::Stroke::new(canvas_scale::px(1.6, xf.z), self.palette().accent);
            for pair in draft.points.windows(2) {
                painter.line_segment([screen_point(pair[0]), screen_point(pair[1])], stroke);
            }
            for point in &draft.points {
                painter.circle_filled(
                    screen_point(*point),
                    canvas_scale::px(2.2, xf.z),
                    stroke.color,
                );
            }
            if let (Some(last), Some(next)) = (draft.points.last(), draft_preview) {
                painter.line_segment([screen_point(*last), screen_point(next)], stroke);
            }
        }
        // Hover is the silhouette itself, with no capsule over the color datum.
        let Some(selection) = &pinned else {
            return;
        };
        let px = |value: f32| canvas_scale::px(value, xf.z);
        let anchor = Rect::from_center_size(host_bounds.right_top(), egui::Vec2::splat(px(12.0)));
        let badge = format!(
            "{}/{}",
            selection.result.selected + 1,
            selection.result.candidates.len()
        );
        let labels = [
            (
                "+",
                None,
                matches!(self.image_segments.prompt_mode, PromptMode::Include),
                false,
            ),
            (
                "−",
                None,
                matches!(self.image_segments.prompt_mode, PromptMode::Exclude),
                false,
            ),
            (
                "Mask",
                Some(badge.as_str()),
                false,
                selection.result.candidates.len() < 2,
            ),
            ("Clear", None, false, false),
        ];
        let total_h = labels.len() as f32 * selection_tools::CORNER_HEIGHT
            + labels.len().saturating_sub(1) as f32 * selection_tools::STACK_GAP;
        let avoid = self
            .shape_properties
            .chrome_hits
            .iter()
            .fold(host_bounds, |bounds, rect| bounds.union(*rect));
        let panel = selection_tools::place_popup(
            egui::vec2(px(selection_tools::STACK_WIDTH), px(total_h)),
            anchor,
            avoid,
            px(10.0),
            ui.clip_rect(),
        );
        self.image_segments.action_rect = Some(panel);
        self.image_segments.corridor = Some(anchor.union(panel).expand(px(6.0)));
        for (index, (label, badge, selected, disabled)) in labels.into_iter().enumerate() {
            let rect = Rect::from_min_size(
                Pos2::new(
                    panel.left(),
                    panel.top()
                        + index as f32
                            * px(selection_tools::CORNER_HEIGHT + selection_tools::STACK_GAP),
                ),
                egui::vec2(
                    px(selection_tools::STACK_WIDTH),
                    px(selection_tools::CORNER_HEIGHT),
                ),
            );
            self.shape_properties.chrome_hits.push(rect);
            let response = selection_tools::capsule(
                ui,
                Id::new(("image_selection", host_id.0, index)),
                rect,
                &selection_tools::Capsule {
                    label,
                    badge,
                    selected,
                    disabled,
                    ..Default::default()
                },
                xf.z,
                self.palette(),
            );
            if let Some(error) = &self.image_segments.error {
                response.response.clone().on_hover_text(error);
            }
            if label == "Mask" {
                if let Some(quality) = selection
                    .result
                    .candidates
                    .get(selection.result.selected)
                    .map(|candidate| candidate.quality)
                {
                    response
                        .response
                        .clone()
                        .on_hover_text(format!("SAM quality: {quality:.3}"));
                }
            }
            if response.response.clicked() {
                match label {
                    "+" => self.image_segments.prompt_mode = PromptMode::Include,
                    "−" => self.image_segments.prompt_mode = PromptMode::Exclude,
                    "Clear" => self.clear_image_selection(host_id),
                    _ => {}
                }
            }
            if response.badge.is_some_and(|badge| badge.clicked()) {
                self.cycle_image_selection_candidate(true);
            }
        }
    }
}
