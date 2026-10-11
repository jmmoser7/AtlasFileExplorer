//! Pinned image masks, prompt refinement, and polygonal lasso input.

use super::*;

impl SlateApp {
    /// Lasso owns its editing keys before the application command router can
    /// interpret Delete, Enter, or Escape as board-wide actions.
    pub(crate) fn image_selection_keys(&mut self, ctx: &egui::Context) -> bool {
        let Some(draft) = self.image_segments.lasso.clone() else {
            return false;
        };
        if ctx.wants_keyboard_input()
            || self.text_compose_active()
            || self.sheet_edit.is_some()
            || self.web.focused.is_some()
            || self.agents.focused.is_some()
        {
            return false;
        }
        let key = ctx.input(|input| {
            if input.key_pressed(egui::Key::Escape) {
                Some(egui::Key::Escape)
            } else if input.key_pressed(egui::Key::Delete) {
                Some(egui::Key::Delete)
            } else if input.key_pressed(egui::Key::Enter)
                && !input.modifiers.ctrl
                && !input.modifiers.shift
                && !input.modifiers.alt
            {
                Some(egui::Key::Enter)
            } else {
                None
            }
        });
        let Some(key) = key else {
            return false;
        };
        ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, key);
        });
        match key {
            egui::Key::Escape => self.image_segments.lasso = None,
            egui::Key::Delete => {
                if let Some(draft) = self.image_segments.lasso.as_mut() {
                    draft.points.pop();
                }
            }
            egui::Key::Enter => {
                self.finish_image_lasso(draft.last_click_screen.unwrap_or(Pos2::ZERO));
            }
            _ => unreachable!("only lasso-editing keys are routed here"),
        }
        true
    }

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

    /// Capture selection clicks before the board can turn them into node drags.
    pub(crate) fn image_selection_input(&mut self, ui: &egui::Ui, xf: &BoardXf) -> bool {
        let (click, shift, alt, screen, now) = ui.input(|input| {
            (
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.modifiers.shift,
                input.modifiers.alt,
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
                    if double_click || close_to_first {
                        self.finish_image_lasso(screen);
                    } else {
                        if let Some(draft) = self.image_segments.lasso.as_mut() {
                            draft.points.push(point);
                            draft.last_click = Some(now);
                            draft.last_click_screen = Some(screen);
                        }
                    }
                    return true;
                }
            }
            return false;
        }

        let Some(selection) = self.image_segments.pinned.as_ref() else {
            return false;
        };
        if selection.tab != self.tab().id
            || self.segment_source_key(selection.host).as_deref()
                != Some(selection.source_key.as_str())
        {
            return false;
        }
        if self.board_tool != super::board::BoardTool::Select {
            return false;
        }
        let host_id = selection.host;
        if !click {
            return false;
        }
        let Some(screen) = screen else {
            return false;
        };
        // The popup is ordinary UI. Let its controls receive clicks instead
        // of turning them into image prompts.
        if self
            .image_segments
            .action_rect
            .is_some_and(|rect| rect.contains(screen))
        {
            return false;
        }
        let world = xf.s2w(screen);
        let Some(host) = self.doc().scene.node(host_id) else {
            return false;
        };
        let NodeKind::Image(img) = &host.kind else {
            return false;
        };
        if !self.point_in_image_paint_window(host_id, world) {
            return false;
        }
        let (x, y) = slate_doc::image_paint::world_to_host_norm(host, img, world.x, world.y);
        let point = [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)];
        self.add_image_selection_prompt(point, screen, alt, ui.ctx().clone());
        true
    }

    fn constrain_lasso_point(
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

    fn add_image_selection_prompt(
        &mut self,
        point: [f32; 2],
        screen: Pos2,
        alt: bool,
        ctx: egui::Context,
    ) {
        let Some(selection) = self.image_segments.pinned.as_ref() else {
            return;
        };
        let prompt = PointPrompt {
            point,
            positive: !alt && matches!(self.image_segments.prompt_mode, PromptMode::Include),
        };
        let host = selection.host;
        let mut prompts = selection.prompts.clone();
        prompts.push(prompt);
        let tab = selection.tab;
        let source_key = selection.source_key.clone();
        let Some(request) = self.segment_request(host, &prompts) else {
            self.image_segments.error = Some("Image preview pixels are not resident yet.".into());
            return;
        };
        if let Some(selection) = self.image_segments.pinned.as_mut() {
            selection.prompts = prompts.clone();
            selection.point_norm = point;
            selection.screen = screen;
        }
        self.image_segments.error = None;
        if self.image_segments.worker.is_none() {
            self.image_segments.start_worker(ctx);
        }
        let serial = self.image_segments.serial.wrapping_add(1);
        self.image_segments.serial = serial;
        self.image_segments.hover = Some(Hover {
            tab,
            host,
            gen: self.scene_gen,
            source_key,
            point_norm: point,
            prompts,
            screen,
            since: Instant::now(),
        });
        self.image_segments.submit(serial, request);
    }

    pub(crate) fn image_selection_clip(&self, image: NodeId) -> Option<Vec<Vec<[f32; 2]>>> {
        let selection = self.image_segments.pinned.as_ref()?;
        (selection.host == image
            && selection.tab == self.tab().id
            && self.segment_source_key(image).as_deref() == Some(selection.source_key.as_str()))
        .then(|| (*selection.result.mask).clone())
    }

    pub(super) fn clear_image_selection(&mut self, image: NodeId) {
        if self
            .image_segments
            .pinned
            .as_ref()
            .is_some_and(|selection| selection.host == image)
        {
            self.image_segments.pinned = None;
            self.image_segments.hover = None;
            self.image_segments.result = None;
            self.image_segments.pending = None;
            self.image_segments.queued = None;
            self.image_segments.serial = self.image_segments.serial.wrapping_add(1);
            self.image_segments.action_rect = None;
            self.image_segments.corridor = None;
        }
    }

    pub(crate) fn create_image_layer_from_selection(&mut self, image: NodeId) -> bool {
        let Some(mask) = self.image_selection_clip(image) else {
            return false;
        };
        let Some(mut child) = self.doc().scene.node(image).cloned() else {
            return false;
        };
        let NodeKind::Image(img) = &mut child.kind else {
            return false;
        };
        img.paint_layers.clear();
        img.result_layers.clear();
        child.clip = slate_doc::image_paint::clip_path_for_layer_node(&child, &mask);
        if child.clip.is_none() {
            return false;
        }
        child.id = self
            .doc_mut()
            .scene
            .build_node(child.rect, child.kind.clone())
            .id;
        let id = child.id;
        let index = self.doc().scene.nodes.len();
        if !self.commit_scene(vec![SceneCmd::Add { index, node: child }]) {
            return false;
        }
        self.push_history(
            atlas_commands::CommandId("board.image.selection.layer"),
            Some("layer".into()),
        );
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.clear_image_selection(image);
        true
    }

    pub(crate) fn paint_image_selection(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
    ) {
        self.paint_image_segment_hover(ui, painter, xf);
    }

    /// The board's existing node menu hosts these actions; return true only
    /// when one was activated so its owner can close the menu.
    pub(crate) fn image_selection_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        image: NodeId,
    ) -> bool {
        if self.image_selection_clip(image).is_none() {
            return false;
        }
        if ui.button("Create layer from selection").clicked() {
            return self.create_image_layer_from_selection(image);
        }
        if ui.button("Clear selection").clicked() {
            self.clear_image_selection(image);
            return true;
        }
        false
    }

    pub(super) fn pin_hover_selection(&mut self) {
        let (Some(result), Some(hover)) = (
            self.image_segments.result.clone(),
            self.image_segments.hover.as_ref(),
        ) else {
            return;
        };
        if result.host != hover.host || result.source_key != hover.source_key {
            return;
        }
        self.image_segments.prompt_mode = PromptMode::Include;
        self.image_segments.pinned = Some(PinnedSelection {
            tab: hover.tab,
            host: hover.host,
            source_key: hover.source_key.clone(),
            result,
            prompts: hover.prompts.clone(),
            screen: hover.screen,
            point_norm: hover.point_norm,
        });
        self.image_segments.hover = None;
        self.image_segments.result = None;
    }

    pub(super) fn cycle_image_selection_candidate(&mut self, forward: bool) {
        let Some(selection) = self.image_segments.pinned.as_mut() else {
            return;
        };
        let len = selection.result.candidates.len();
        if len < 2 {
            return;
        }
        let index = if forward {
            (selection.result.selected + 1) % len
        } else {
            (selection.result.selected + len - 1) % len
        };
        let Some(candidate) = selection.result.candidates.get(index) else {
            return;
        };
        let Some(path) = slate_doc::image_paint::region_path(candidate.contours.clone()) else {
            return;
        };
        selection.result.selected = index;
        selection.result.mask = std::sync::Arc::new(candidate.contours.clone());
        selection.result.local = region_node(path);
    }

    fn finish_image_lasso(&mut self, screen: Pos2) {
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

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
