//! Paint the local selection preview and pointer-attached controls.

use super::*;

impl SlateApp {
    pub(super) fn paint_image_segment_hover(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
    ) {
        let pinned = self.image_segments.pinned.clone();
        let hover = self.image_segments.hover.as_ref();
        let (host_id, point) = if let Some(pinned) = &pinned {
            if pinned.tab != self.tab().id {
                return;
            }
            (pinned.host, pinned.point_norm)
        } else if let Some(hover) = hover {
            if hover.gen != self.scene_gen || hover.tab != self.tab().id {
                return;
            }
            (hover.host, hover.point_norm)
        } else if let Some(draft) = self.image_segments.lasso.as_ref() {
            (draft.host, [0.5, 0.5])
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
        let world = slate_doc::geom::orbit_point(
            host.rect.center(),
            (basis.x + point[0] * basis.w, basis.y + point[1] * basis.h),
            host.rotation_deg,
        );
        let screen = xf.w2s(Pos2::new(world.0, world.1));
        let host_bounds = xf.rect_w2s(slate_doc::node_aabb(host));
        let result = pinned
            .as_ref()
            .map(|selection| selection.result.clone())
            .or_else(|| self.image_segments.result.clone());
        if let Some(result) = &result {
            let region = layer_node_to_world(host, img, &result.local);
            if let NodeKind::Shape(shape) = &region.kind {
                if let Some(path) = &shape.path {
                    super::board_path::paint_path_shape(
                        self,
                        painter,
                        xf,
                        &region,
                        shape,
                        path,
                        &|c| c,
                    );
                }
            }
        }
        if let Some(draft) = self
            .image_segments
            .lasso
            .as_ref()
            .filter(|draft| draft.host == host_id)
        {
            let points: Vec<Pos2> = draft
                .points
                .iter()
                .map(|p| {
                    let world = slate_doc::geom::orbit_point(
                        host.rect.center(),
                        (basis.x + p[0] * basis.w, basis.y + p[1] * basis.h),
                        host.rotation_deg,
                    );
                    xf.w2s(Pos2::new(world.0, world.1))
                })
                .collect();
            if points.len() >= 2 {
                painter.add(egui::Shape::line(
                    points,
                    egui::Stroke::new(canvas_scale::px(1.6, xf.z), self.palette().accent),
                ));
            }
        }
        if !self.image_segments.offered() {
            return;
        }
        let px = |v: f32| canvas_scale::px(v, xf.z);
        let anchor = Rect::from_center_size(screen, egui::Vec2::splat(px(12.0)));
        let pinned_active = pinned.is_some();
        let candidate_badge = pinned.as_ref().map(|selection| {
            format!(
                "{}/{}",
                selection.result.selected + 1,
                selection.result.candidates.len()
            )
        });
        let labels: Vec<(&str, Option<&str>, bool, bool)> = if pinned_active {
            let count = pinned.as_ref().unwrap().result.candidates.len();
            vec![
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
                ("Mask", candidate_badge.as_deref(), false, count < 2),
                ("Clear", None, false, false),
            ]
        } else {
            vec![(
                result
                    .as_ref()
                    .map_or("Starting local model / encoding…", |_| "Pin selection"),
                None,
                false,
                result.is_none(),
            )]
        };
        let total_h = labels.len() as f32 * selection_tools::CORNER_HEIGHT
            + labels.len().saturating_sub(1) as f32 * selection_tools::STACK_GAP;
        let panel = selection_tools::place_popup(
            egui::vec2(px(selection_tools::STACK_WIDTH), px(total_h)),
            anchor,
            host_bounds,
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
                if let Some(quality) = pinned.as_ref().and_then(|selection| {
                    selection
                        .result
                        .candidates
                        .get(selection.result.selected)
                        .map(|candidate| candidate.quality)
                }) {
                    response
                        .response
                        .clone()
                        .on_hover_text(format!("SAM quality: {quality:.3}"));
                }
            }
            if response.response.clicked() {
                match label {
                    "Pin selection" => self.pin_hover_selection(),
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
