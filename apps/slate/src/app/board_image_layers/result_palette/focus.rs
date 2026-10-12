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
}

#[derive(Clone)]
struct LayerNameDraft {
    origin: LayerMenuOrigin,
    name: String,
}

#[derive(Clone)]
struct ActiveLayerMenu {
    origin: LayerMenuOrigin,
    response: egui::Response,
    draft: egui::Id,
    edit: egui::Id,
}

fn active_layer_menu_id() -> egui::Id {
    egui::Id::new("active-image-layer-context-menu")
}

impl SlateApp {
    /// The key router runs before the menu paints. Cancel its draft there,
    /// using egui's existing context-menu owner to dismiss the open menu.
    pub(crate) fn cancel_image_layer_name_draft(&mut self, ctx: &egui::Context) -> bool {
        let active = ctx.data(|data| data.get_temp::<ActiveLayerMenu>(active_layer_menu_id()));
        let Some(active) = active
            .filter(|menu| menu.origin.is_current(self) && menu.response.context_menu_opened())
        else {
            return false;
        };
        ctx.data_mut(|data| {
            data.remove::<LayerNameDraft>(active.draft);
            data.remove::<ActiveLayerMenu>(active_layer_menu_id());
        });
        ctx.memory_mut(|memory| memory.surrender_focus(active.edit));
        active.response.context_menu(|ui| ui.close_menu());
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
        let rename_id = response.id.with(("paint-layer-name", &origin));
        let mut rename = None;
        let mut action = None;
        response.context_menu(|ui| {
            let mut name = ui
                .data(|data| data.get_temp::<LayerNameDraft>(rename_id))
                .filter(|draft| draft.origin == origin)
                .map(|draft| draft.name)
                .unwrap_or_else(|| default_name.to_owned());
            let edit = ui.text_edit_singleline(&mut name);
            ui.data_mut(|data| {
                data.insert_temp(
                    active_layer_menu_id(),
                    ActiveLayerMenu {
                        origin: origin.clone(),
                        response: response.clone(),
                        draft: rename_id,
                        edit: edit.id,
                    },
                );
            });
            let draft = LayerNameDraft {
                origin: origin.clone(),
                name,
            };
            ui.data_mut(|data| data.insert_temp(rename_id, draft.clone()));
            let cancel = ui.input(|input| input.key_pressed(egui::Key::Escape));
            let enter = (edit.has_focus() || edit.lost_focus())
                && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            if cancel {
                ui.data_mut(|data| data.remove::<LayerNameDraft>(rename_id));
                ui.close_menu();
            } else if enter || edit.lost_focus() {
                rename = Some(draft);
                ui.data_mut(|data| data.remove::<LayerNameDraft>(rename_id));
                if enter {
                    ui.close_menu();
                }
            }
            if ui.button("Duplicate layer").clicked() {
                action = Some(0);
                ui.close_menu();
            }
            let mut visible = layer.visible;
            if ui.checkbox(&mut visible, "Visible").changed() {
                action = Some(1);
                ui.close_menu();
            }
            if ui.button("Delete layer").clicked() {
                action = Some(2);
                ui.close_menu();
            }
        });
        if response
            .ctx
            .input(|input| input.key_pressed(egui::Key::Escape))
        {
            rename = None;
            response
                .ctx
                .data_mut(|data| data.remove::<LayerNameDraft>(rename_id));
        } else if !response.context_menu_opened() {
            if let Some(draft) = response.ctx.data_mut(|data| {
                let draft = data.get_temp::<LayerNameDraft>(rename_id);
                data.remove::<LayerNameDraft>(rename_id);
                draft
            }) {
                rename = Some(draft);
            }
        }
        if !response.context_menu_opened() {
            response.ctx.data_mut(|data| {
                if data
                    .get_temp::<ActiveLayerMenu>(active_layer_menu_id())
                    .is_some_and(|menu| menu.response.id == response.id)
                {
                    data.remove::<ActiveLayerMenu>(active_layer_menu_id());
                }
            });
        }
        if !origin.is_current(self) {
            response
                .ctx
                .data_mut(|data| data.remove::<LayerNameDraft>(rename_id));
            return;
        }
        let command = |id, detail| (atlas_commands::CommandId(id), Some(detail));
        if let Some(draft) = rename.filter(|draft| draft.origin == origin) {
            let (id, detail) = command(
                "board.image.layer.rename",
                serde_json::json!({
                    "image": image.0, "layer": layer.id.0, "name": draft.name,
                })
                .to_string(),
            );
            self.dispatch(&response.ctx, id, detail);
        }
        if !origin.is_current(self) {
            return;
        }
        if let Some(id) = match action {
            Some(0) => Some("board.image.layer.duplicate"),
            Some(1) => Some("board.image.layer.visibility"),
            Some(2) => Some("board.image.layer.delete"),
            _ => None,
        } {
            let (id, detail) = command(
                id,
                serde_json::json!({ "image": image.0, "layer": layer.id.0 }).to_string(),
            );
            self.dispatch(&response.ctx, id, detail);
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
