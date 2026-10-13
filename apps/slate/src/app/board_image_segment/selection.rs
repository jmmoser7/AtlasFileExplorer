//! Pinned image masks, prompt refinement, and polygonal lasso input.

use super::*;
use crate::app::board_path::multi_point::{self, Event};

impl SlateApp {
    pub(crate) fn image_lasso_active(&self, image: NodeId) -> bool {
        self.image_segments
            .lasso
            .as_ref()
            .is_some_and(|draft| draft.tab == self.tab().id && draft.host == image)
    }

    pub(crate) fn arm_image_brush(&mut self, image: NodeId) {
        self.transition_image_tool(image, crate::app::board::BoardTool::Brush);
    }

    pub(crate) fn disarm_image_tool(&mut self, image: NodeId) {
        self.transition_image_tool(image, crate::app::board::BoardTool::Select);
    }

    pub(super) fn transition_image_tool(
        &mut self,
        image: NodeId,
        tool: crate::app::board::BoardTool,
    ) {
        let panel = self.shape_properties.panel;
        self.image_segments.cancel_tool_gestures();
        self.set_board_tool(tool);
        self.board_sel.clear();
        self.board_sel.insert(image);
        self.sync_shape_properties();
        self.shape_properties.panel = panel;
    }

    /// Lasso owns its editing keys before the application command router can
    /// interpret Delete, Enter, or Escape as board-wide actions.
    pub(crate) fn image_selection_keys(&mut self, ctx: &egui::Context) -> bool {
        if self.doc().view.active_view != slate_doc::ViewKind::Board
            || ctx.wants_keyboard_input()
            || self.text_compose_active()
            || self.sheet_edit.is_some()
            || self.web.focused.is_some()
            || self.agents.focused.is_some()
        {
            return false;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            if self.image_segments.drag.take().is_some() {
                ctx.input_mut(|input| {
                    input.consume_key(input.modifiers, egui::Key::Escape);
                });
                return true;
            }
            if self.image_segments.lasso.is_none()
                && self.board_tool == crate::app::board::BoardTool::Brush
                && self.board_drag.is_none()
            {
                if let Some(image) = self.selected_image_nodes().first().copied() {
                    ctx.input_mut(|input| {
                        input.consume_key(input.modifiers, egui::Key::Escape);
                    });
                    if self.brush_hud.is_some() {
                        self.cancel_brush_hud();
                    } else {
                        self.disarm_image_tool(image);
                    }
                    return true;
                }
            }
            if self.image_segments.lasso.is_none()
                && self.board_tool == crate::app::board::BoardTool::Select
                && self.dismiss_image_segment()
            {
                ctx.input_mut(|input| {
                    input.consume_key(input.modifiers, egui::Key::Escape);
                });
                return true;
            }
        }
        let Some(draft) = self
            .image_segments
            .lasso
            .clone()
            .filter(|draft| draft.tab == self.tab().id)
        else {
            return false;
        };
        let key = ctx.input(|input| {
            if input.key_pressed(egui::Key::Escape) {
                Some((egui::Key::Escape, input.modifiers))
            } else if input.key_pressed(egui::Key::Delete) {
                Some((egui::Key::Delete, input.modifiers))
            } else if input.key_pressed(egui::Key::Enter)
                && !input.modifiers.ctrl
                && !input.modifiers.shift
                && !input.modifiers.alt
            {
                Some((egui::Key::Enter, input.modifiers))
            } else {
                None
            }
        });
        let Some((key, modifiers)) = key else {
            return false;
        };
        ctx.input_mut(|input| {
            input.consume_key(modifiers, key);
        });
        match key {
            egui::Key::Escape => {
                if let Some(draft) = self.image_segments.lasso.as_mut() {
                    multi_point::collect(&mut draft.points, Event::Cancel, multi_point::LASSO);
                }
                self.image_segments.lasso = None;
            }
            egui::Key::Delete => {
                if let Some(draft) = self.image_segments.lasso.as_mut() {
                    multi_point::collect(&mut draft.points, Event::Remove, multi_point::LASSO);
                }
            }
            egui::Key::Enter => {
                self.finish_image_lasso(draft.last_click_screen.unwrap_or(Pos2::ZERO));
            }
            _ => unreachable!("only lasso-editing keys are routed here"),
        }
        true
    }

    pub(super) fn add_image_selection_prompt(
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

    pub(crate) fn clear_image_selection(&mut self, image: NodeId) {
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
        let Some(item) = self.image_selection_item(image) else {
            return false;
        };
        if self
            .add_selection_cutout_layer(image, item, &mask)
            .is_none()
        {
            return false;
        }
        self.board_sel.clear();
        self.board_sel.insert(image);
        self.clear_image_selection(image);
        true
    }

    pub(crate) fn image_selection_item(&mut self, image: NodeId) -> Option<slate_doc::ItemId> {
        let NodeKind::Image(img) = &self.doc().scene.node(image)?.kind else {
            return None;
        };
        let item = img.item;
        if img.agent.is_some() {
            self.agent_shown_path(image)
                .and_then(|path| self.item_for_path(&path))
                .or_else(|| (!item.is_none()).then_some(item))
        } else {
            (!item.is_none()).then_some(item)
        }
    }

    pub(super) fn pin_hover_selection(&mut self) {
        let (Some(result), Some(hover)) = (
            self.image_segments.result.clone(),
            self.image_segments.hover.as_ref(),
        ) else {
            return;
        };
        if result.host != hover.host
            || result.source_key != hover.source_key
            || result.gen != self.scene_gen
            || hover.gen != self.scene_gen
            || hover.tab != self.tab().id
            || self.segment_source_key(hover.host).as_deref() != Some(hover.source_key.as_str())
        {
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
}
