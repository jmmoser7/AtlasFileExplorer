//! Selected native mesh presentation controls over the shared shell widgets.

use super::*;

fn model_display_glyph(mode: scene::ModelDisplay) -> ([u8; 3], Option<[u8; 3]>) {
    match mode {
        scene::ModelDisplay::Shaded => ([108, 118, 132], None),
        scene::ModelDisplay::Arctic => ([238, 238, 234], None),
        scene::ModelDisplay::HiddenLine => ([238, 238, 234], Some([48, 52, 58])),
        scene::ModelDisplay::Material => ([196, 88, 72], Some([72, 132, 188])),
        scene::ModelDisplay::Depth => ([32, 32, 32], Some([228, 228, 228])),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{board, model_node};
    use super::*;
    use crate::app::tests::Harness;

    fn camera(h: &Harness, id: NodeId) -> scene::ModelCamera {
        h.app.model_node_info(id).unwrap().cam
    }

    #[test]
    fn materials_toggle_preserves_camera_and_display_and_undoes_once() {
        let mut h = board();
        let id = model_node(
            &mut h,
            "colored.3dm",
            WorldRect::new(0.0, 0.0, 240.0, 180.0),
        );
        let before = camera(&h, id);
        assert!(before.materials);
        assert!(h.app.dispatch(
            &h.ctx,
            CommandId("board.model_materials"),
            Some(format!("{}:off", id.0))
        ));
        assert_eq!(
            camera(&h, id),
            scene::ModelCamera {
                materials: false,
                ..before
            }
        );
        assert!(
            !h.app.dispatch(
                &h.ctx,
                CommandId("board.model_materials"),
                Some(format!("{}:off", id.0))
            ),
            "re-picking Off is not an edit"
        );
        h.app.board_undo();
        assert_eq!(camera(&h, id), before);
        h.app.board_redo();
        assert!(!camera(&h, id).materials);
        h.app.board_sel = [id].into_iter().collect();
        assert!(h
            .app
            .dispatch(&h.ctx, CommandId("board.model_materials"), None));
        assert_eq!(camera(&h, id), before);
    }

    #[test]
    fn material_mask_stays_distinct_and_invalid_commands_do_not_edit() {
        let mut h = board();
        let id = model_node(
            &mut h,
            "colored.3dm",
            WorldRect::new(0.0, 0.0, 240.0, 180.0),
        );
        h.app.board_sel = [id].into_iter().collect();
        assert!(h.app.dispatch(
            &h.ctx,
            CommandId("board.model_display"),
            Some(format!("{}:material", id.0))
        ));
        assert!(h.app.dispatch(
            &h.ctx,
            CommandId("board.model_materials"),
            Some(format!("{}:false", id.0))
        ));
        assert_eq!(camera(&h, id).display, scene::ModelDisplay::Material);
        let before = camera(&h, id);
        for detail in [
            "unexpected".to_owned(),
            format!("{}:maybe", id.0),
            "missing:on".to_owned(),
        ] {
            assert!(!h
                .app
                .dispatch(&h.ctx, CommandId("board.model_materials"), Some(detail)));
            assert_eq!(camera(&h, id), before);
        }
        assert!(!h.app.dispatch(
            &h.ctx,
            CommandId("board.model_display"),
            Some("unexpected".into())
        ));
        assert_eq!(camera(&h, id), before);
    }
}

fn model_display_radios(thumbs: [Option<egui::TextureId>; 5]) -> [chrome::FilterRadio; 5] {
    std::array::from_fn(|i| {
        let mode = scene::ModelDisplay::ALL[i];
        let (fill, fill_b) = model_display_glyph(mode);
        chrome::FilterRadio {
            label: mode.label(),
            fill,
            fill_b,
            thumb: thumbs[i],
        }
    })
}

impl SlateApp {
    pub(super) fn model_display_editor_height(&self) -> f32 {
        let materials = self
            .shape_properties
            .ids
            .first()
            .and_then(|id| self.model_display_of(*id))
            .is_some_and(scene::ModelDisplay::uses_materials);
        chrome::FILTER_CHIPS_HEIGHT
            + if materials {
                chrome::STACK_GAP + chrome::CORNER_HEIGHT
            } else {
                0.0
            }
    }

    pub(super) fn model_display_editor(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) {
        let Some(id) = self.shape_properties.ids.first().copied() else {
            return;
        };
        let Some(display) = self.model_display_of(id) else {
            return;
        };
        let current = scene::ModelDisplay::ALL
            .iter()
            .position(|mode| *mode == display)
            .unwrap_or(0);
        self.ensure_model_display_swatches(id);
        let radios = model_display_radios(self.model_display_swatch_ids(ui.ctx(), id));
        let chip_height = chrome::FILTER_CHIPS_HEIGHT * z;
        let chip_w = chrome::filter_chips_width(radios.len(), chip_height, z);
        let row = Rect::from_center_size(
            Pos2::new(
                rect.center().x,
                rect.top() + chrome::FILTER_CHIPS_HEIGHT * z * 0.5,
            ),
            Vec2::new(chip_w, chip_height),
        );
        let edit = chrome::filter_capsule(
            ui,
            row,
            &radios,
            Some(current),
            None,
            z,
            theme,
            chrome::FilterCapsuleStyle::ChipsOnly,
        );
        if let Some(picked) = edit.clicked.filter(|picked| *picked != current) {
            self.dispatch(
                &ui.ctx().clone(),
                CommandId("board.model_display"),
                Some(format!(
                    "{}:{}",
                    id.0,
                    scene::ModelDisplay::ALL[picked].key()
                )),
            );
        }
        if display.uses_materials() {
            let Some(enabled) = self.model_materials_of(id) else {
                return;
            };
            let materials = Rect::from_center_size(
                Pos2::new(
                    rect.center().x,
                    row.bottom() + (chrome::STACK_GAP + chrome::CORNER_HEIGHT * 0.5) * z,
                ),
                Vec2::new(chrome::STACK_WIDTH, chrome::CORNER_HEIGHT) * z,
            );
            let picked = chrome::segments(
                ui,
                materials,
                ui.id().with(("model_materials", id.0)),
                ["Materials off", "Materials on"],
                enabled as usize,
                z,
                theme,
            ) == 1;
            ui.interact(materials, ui.id().with(("model_materials_help", id.0)), egui::Sense::hover())
                .on_hover_text("Use imported surface colors in Shaded and Hidden line. Arctic keeps white clay; Material mask and Depth are unaffected.");
            if picked != enabled {
                self.dispatch(
                    &ui.ctx().clone(),
                    CommandId("board.model_materials"),
                    Some(format!("{}:{}", id.0, if picked { "on" } else { "off" })),
                );
            }
        }
    }
}
