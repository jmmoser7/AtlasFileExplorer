use super::super::*;

#[derive(Clone)]
pub(crate) struct ImagePaintFocusSnapshot {
    image: NodeId,
    result: ImageResultId,
    layer_id: Option<PaintLayerId>,
    layer_index: usize,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct LayerMenuOrigin {
    tab: u64,
    image: NodeId,
    result: ImageResultId,
    layer: PaintLayerId,
}

impl LayerMenuOrigin {
    fn is_current(&self, app: &SlateApp) -> bool {
        app.tab().id == self.tab && app.image_result_id(self.image).as_ref() == Some(&self.result)
    }

    fn draft_id(&self) -> egui::Id {
        egui::Id::new(("image-layer-name-draft", self))
    }
}

#[derive(Clone)]
struct ActiveLayerMenu {
    origin: LayerMenuOrigin,
    at: egui::Pos2,
    edit: Option<egui::Id>,
    pass: u64,
}

fn active_layer_menu_id() -> egui::Id {
    egui::Id::new("active-image-layer-context-menu")
}

impl SlateApp {
    /// The key router runs before the popup paints. Discard the layer draft.
    pub(crate) fn cancel_image_layer_name_draft(&mut self, ctx: &egui::Context) -> bool {
        let active = ctx.data(|data| data.get_temp::<ActiveLayerMenu>(active_layer_menu_id()));
        let Some(active) = active.filter(|menu| {
            menu.origin.is_current(self) && ctx.cumulative_pass_nr().saturating_sub(menu.pass) <= 1
        }) else {
            return false;
        };
        ctx.data_mut(|data| {
            data.remove::<ActiveLayerMenu>(active_layer_menu_id());
            data.remove::<String>(active.origin.draft_id());
        });
        if let Some(edit) = active.edit {
            ctx.memory_mut(|memory| memory.surrender_focus(edit));
        }
        true
    }

    pub(crate) fn delete_paint_layer_by_id(&mut self, image: NodeId, layer: PaintLayerId) -> bool {
        let Some(result) = self.image_result_id(image) else {
            return false;
        };
        let Some(node) = self.doc().scene.node(image) else {
            return false;
        };
        let NodeKind::Image(img) = &node.kind else {
            return false;
        };
        let Some(index) = (0..Self::selected_layers(img, &result).len()).find(|index| {
            Self::session_layer_raw_index(img, &result, *index)
                .and_then(|raw| Self::selected_layers(img, &result).get(raw))
                .is_some_and(|candidate| candidate.id == layer)
        }) else {
            return false;
        };
        let depth = self.tab().journal.undo_depth();
        self.delete_paint_layer(image, index);
        self.tab().journal.undo_depth() != depth
    }

    pub(crate) fn toggle_paint_layer_visibility(
        &mut self,
        image: NodeId,
        layer_id: PaintLayerId,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(result) = self.image_result_id(image) else {
            return false;
        };
        let Some(before) = self.doc().scene.node(image).cloned() else {
            return false;
        };
        let NodeKind::Image(old_img) = &before.kind else {
            return false;
        };
        if !Self::selected_layers(old_img, &result)
            .iter()
            .any(|layer| layer.id == layer_id)
        {
            return false;
        }
        let mut after = before.clone();
        let NodeKind::Image(img) = &mut after.kind else {
            return false;
        };
        Self::prepare_result_stack(img, result.clone());
        let Some(layer) = Self::selected_layers_mut(img, &result)
            .iter_mut()
            .find(|layer| layer.id == layer_id)
        else {
            return false;
        };
        layer.visible = !layer.visible;
        self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }])
    }

    /// A name edit belongs to its layer identity. Blur and menu dismissal
    /// commit the draft; Escape cancels it before closing the context menu.
    pub(super) fn image_layer_context_menu(
        &mut self,
        response: &egui::Response,
        image: NodeId,
        result: &ImageResultId,
        layer: &PaintLayer,
        default_name: &str,
    ) {
        let origin = LayerMenuOrigin {
            tab: self.tab().id,
            image,
            result: result.clone(),
            layer: layer.id,
        };
        let ctx = &response.ctx;
        let pass = ctx.cumulative_pass_nr();
        let to_global = ctx
            .layer_transform_to_global(response.layer_id)
            .unwrap_or_default();
        let opened_at = response
            .interact_pointer_pos()
            .filter(|_| response.secondary_clicked());
        let stored = ctx.data(|data| data.get_temp::<ActiveLayerMenu>(active_layer_menu_id()));
        let draft_id = origin.draft_id();
        let Some(mut menu) = opened_at
            .map(|at| ActiveLayerMenu {
                origin: origin.clone(),
                at,
                edit: None,
                pass,
            })
            .or_else(|| {
                stored.filter(|menu| menu.origin == origin && pass.saturating_sub(menu.pass) <= 1)
            })
        else {
            // A workbook or result switch can leave a draft behind after its
            // popup stops painting. Apply it only when its authoring layer returns.
            if origin.is_current(self) {
                if let Some(name) = ctx.data_mut(|data| data.remove_temp::<String>(draft_id)) {
                    self.commit_image_layer_name(ctx, &origin, default_name, name);
                }
            }
            return;
        };
        let mut name = ctx
            .data(|data| data.get_temp::<String>(draft_id))
            .unwrap_or_else(|| default_name.to_owned());
        let id = response.id.with(("image-layer-menu", &origin));
        let order = atlas_shell::selection_tools::POPUP_ORDER;
        ctx.move_to_top(egui::LayerId::new(order, id));
        let mut rename = false;
        let mut close = false;
        let mut action = None;
        let metrics = crate::app::board::board_menu::layout().metrics;
        let drawn = atlas_shell::selection_tools::popup_area(ctx, id, response.layer_id)
            .fixed_pos(menu.at)
            .default_width(metrics.menu_width)
            .movable(false)
            .fade_in(false)
            .show(ctx, |ui| {
                metrics.frame(self.dark_mode).show(ui, |ui| {
                    metrics.prepare(ui, self.dark_mode);
                    let edit = ui.add_sized(
                        [ui.available_width(), metrics.row_height],
                        egui::TextEdit::singleline(&mut name)
                            .frame(false)
                            .font(egui::FontId::proportional(14.0)),
                    );
                    menu.edit = Some(edit.id);
                    let enter = (edit.has_focus() || edit.lost_focus())
                        && ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                        });
                    rename = enter || edit.lost_focus();
                    close = enter;
                    for entry in crate::app::board::board_menu::entries("image-layer") {
                        use atlas_shell::menu::{MenuIcon, Row};
                        let row = match entry {
                            "menu.layer_name" => continue,
                            "board.image.layer.visibility" => {
                                Row::new(MenuIcon::View, "Visible").checked(layer.visible)
                            }
                            "board.image.layer.add_image" => {
                                Row::new(MenuIcon::Image, "Add image as layer")
                            }
                            "board.image.layer.duplicate" => {
                                Row::new(MenuIcon::Duplicate, "Duplicate layer")
                            }
                            "board.image.layer.delete" => {
                                Row::new(MenuIcon::Trash, "Delete layer").danger()
                            }
                            _ if entry.starts_with("sep:") => {
                                metrics.separator(ui, self.dark_mode);
                                continue;
                            }
                            _ => continue,
                        };
                        if metrics.row(ui, row, self.dark_mode).clicked() {
                            action = Some(entry);
                            close = true;
                        }
                    }
                })
            });
        // The parent Layers panel uses screen-space chrome hits for
        // dismissal. Keep its popup alive through a row's press and release.
        let bounds = to_global * drawn.response.rect;
        self.shape_properties.chrome_hits.push(bounds);
        let cancel = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let outside = opened_at.is_none()
            && ctx.input(|input| {
                input.pointer.any_pressed()
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|point| !bounds.contains(point))
            });
        let commit_name = !cancel && (rename || close || outside);
        ctx.data_mut(|data| {
            if cancel || commit_name || name == default_name {
                data.remove::<String>(draft_id);
            } else {
                data.insert_temp(draft_id, name.clone());
            }
        });
        if cancel || close || outside {
            ctx.data_mut(|data| data.remove::<ActiveLayerMenu>(active_layer_menu_id()));
            if let Some(edit) = menu.edit {
                ctx.memory_mut(|memory| memory.surrender_focus(edit));
            }
        } else {
            menu.pass = pass;
            ctx.data_mut(|data| data.insert_temp(active_layer_menu_id(), menu.clone()));
        }
        if cancel || !origin.is_current(self) {
            return;
        }
        if commit_name {
            self.commit_image_layer_name(ctx, &origin, default_name, name);
        }
        if !origin.is_current(self) {
            return;
        }
        if let Some(id) = action {
            self.dispatch(
                ctx,
                atlas_commands::CommandId(id),
                Some(serde_json::json!({ "image": image.0, "layer": layer.id.0 }).to_string()),
            );
        }
    }

    fn commit_image_layer_name(
        &mut self,
        ctx: &egui::Context,
        origin: &LayerMenuOrigin,
        current: &str,
        name: String,
    ) {
        if name != current {
            self.dispatch(
                ctx,
                atlas_commands::CommandId("board.image.layer.rename"),
                Some(
                    serde_json::json!({
                        "image": origin.image.0, "layer": origin.layer.0, "name": name,
                    })
                    .to_string(),
                ),
            );
        }
    }

    pub(crate) fn capture_image_paint_focus(&self) -> Option<ImagePaintFocusSnapshot> {
        let session = self.image_paint.as_ref()?;
        let layer_id = self
            .doc()
            .scene
            .node(session.image)
            .and_then(|node| match &node.kind {
                NodeKind::Image(img) => {
                    Self::session_layer_raw_index(img, &session.result, session.layer_index)
                        .and_then(|raw| Self::selected_layers(img, &session.result).get(raw))
                        .map(|layer| layer.id)
                }
                _ => None,
            });
        Some(ImagePaintFocusSnapshot {
            image: session.image,
            result: session.result.clone(),
            layer_id,
            layer_index: session.layer_index,
        })
    }

    pub(crate) fn restore_image_paint_focus(&mut self, snapshot: Option<ImagePaintFocusSnapshot>) {
        let Some(snapshot) = snapshot else { return };
        let Some(img) = self
            .doc()
            .scene
            .node(snapshot.image)
            .and_then(|node| match &node.kind {
                NodeKind::Image(img) => Some(img),
                _ => None,
            })
        else {
            return;
        };
        let layers = Self::selected_layers(img, &snapshot.result);
        let layer_index = if layers.is_empty() {
            0
        } else {
            snapshot
                .layer_id
                .and_then(|id| {
                    (0..layers.len()).find(|index| {
                        Self::session_layer_raw_index(img, &snapshot.result, *index)
                            .and_then(|raw| layers.get(raw))
                            .is_some_and(|layer| layer.id == id)
                    })
                })
                .unwrap_or_else(|| snapshot.layer_index.min(layers.len() - 1))
        };
        let focus = if layers.is_empty() {
            ImageStripFocus::Filter
        } else {
            ImageStripFocus::Layer(layer_index)
        };
        let Some(session) = self
            .image_paint
            .as_mut()
            .filter(|session| session.image == snapshot.image && session.result == snapshot.result)
        else {
            return;
        };
        session.layer_index = layer_index;
        session.focus = focus;
    }
}
