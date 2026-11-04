use super::*;
use slate_doc::scene::SceneCmd;

fn popup_id(tab: u64, id: NodeId) -> Id {
    Id::new(("frame-media-popup", tab, id.0))
}

pub(super) fn popup_at(ctx: &egui::Context, tab: u64, id: NodeId, p: Pos2) -> bool {
    ctx.data(|d| d.get_temp::<egui::LayerId>(popup_id(tab, id)))
        .is_some_and(|layer| ctx.layer_id_at(p) == Some(layer))
}

impl SlateApp {
    pub(crate) fn commit_frame_settings(&mut self) {
        let Some(settings) = self.frame_presentation.media_draft.take() else {
            return;
        };
        let Some(id) = self.frame_presentation.host else {
            return;
        };
        if self.frame_presentation.tab != Some(self.tab().id) || self.tab().read_only {
            return;
        }
        let Some(before) = self.doc().scene.node(id).filter(|n| !n.locked).cloned() else {
            return;
        };
        let mut after = before.clone();
        let NodeKind::Frame(f) = &mut after.kind else {
            return;
        };
        f.presentation_media = settings;
        if before != after {
            self.commit_scene(vec![SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            }]);
        }
    }

    pub(super) fn frame_presentation_settings_ui(
        &mut self,
        ui: &mut Ui,
        id: NodeId,
        rect: Rect,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) {
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return;
        };
        let NodeKind::Frame(frame) = &before.kind else {
            return;
        };
        let mut draft = self
            .frame_presentation
            .media_draft
            .clone()
            .unwrap_or_else(|| frame.presentation_media.clone());
        let cfg = &mut draft;
        let editable = !self.tab().read_only && !before.locked;
        let width = ((rect.width() / z - 116.0) * 0.5).max(90.0);
        let cell = |row, col| {
            Rect::from_min_size(
                rect.min + vec2(96.0 + col * (width + 5.0), 12.0 + row * 39.0) * z,
                vec2(width, 28.0) * z,
            )
        };
        for (row, label) in ["Video", "Webpages", "3D models"].into_iter().enumerate() {
            canvas_text::text(
                ui.painter(),
                pos2(rect.left() + 12.0 * z, cell(row as f32, 0.0).center().y),
                Align2::LEFT_CENTER,
                label,
                canvas_scale::font(12.0, z),
                theme.ink,
            );
        }
        let start_popup = setting(
            ui,
            id,
            "start",
            cell(0.0, 0.0),
            z,
            editable,
            &mut cfg.video_start,
            &[
                (None, "Start · Media setting"),
                (Some(false), "Start · On click"),
                (Some(true), "Start · Automatically"),
            ],
        );
        let loop_popup = setting(
            ui,
            id,
            "loop",
            cell(0.0, 1.0),
            z,
            editable,
            &mut cfg.video_loop,
            &[
                (None, "Loop · Media setting"),
                (Some(true), "Loop · On"),
                (Some(false), "Loop · Off"),
            ],
        );
        let view_popup = setting(
            ui,
            id,
            "view",
            cell(1.0, 0.0),
            z,
            editable,
            &mut cfg.web_last_view,
            &[(false, "Saved view"), (true, "Last view")],
        );
        // A combo can extend outside the attached palette. Its own egui
        // layer keeps that press from becoming a click on blank canvas.
        let popup = popup_id(self.tab().id, id);
        ui.ctx().data_mut(|d| {
            if let Some(layer) = start_popup.or(loop_popup).or(view_popup) {
                d.insert_temp(popup, layer);
            } else {
                d.remove::<egui::LayerId>(popup);
            }
        });
        let members = self.doc().scene.members_of(id);
        let bookmarks = members
            .into_iter()
            .filter_map(|id| {
                let NodeKind::Portal(portal) = &self.doc().scene.node(id)?.kind else {
                    return None;
                };
                if portal.kind != slate_doc::scene::PortalKind::Web {
                    return None;
                }
                let url = url::Url::parse(&self.web_display_url(id)?).ok()?;
                (matches!(url.scheme(), "https" | "http")
                    && url.username().is_empty()
                    && url.password().is_none())
                .then(|| (id, url.to_string()))
            })
            .collect::<Vec<_>>();
        let save = ui
            .add_enabled_ui(editable && !bookmarks.is_empty(), |ui| {
                chrome::control(
                    ui,
                    cell(1.0, 1.0),
                    Id::new(("frame-save-web", id.0)),
                    "Save current view",
                    z,
                    theme,
                    false,
                )
            })
            .inner;
        if save.clicked() {
            cfg.web_bookmarks.extend(bookmarks);
        }
        ui.add_enabled_ui(editable, |ui| {
            ui.put(
                cell(2.0, 0.0),
                egui::Checkbox::new(
                    &mut cfg.interactive_models,
                    egui::RichText::new("Interactive").size(12.0 * z),
                ),
            );
        });
        if editable {
            self.frame_presentation.media_draft = Some(draft);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn setting<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: NodeId,
    key: &str,
    r: Rect,
    z: f32,
    enabled: bool,
    active: &mut T,
    items: &[(T, &str)],
) -> Option<egui::LayerId> {
    let label = items
        .iter()
        .find_map(|(v, label)| (*v == *active).then_some(*label))
        .unwrap_or("");
    ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
        ui.style_mut().override_font_id = Some(canvas_scale::font(12.0, z));
        ui.add_enabled_ui(enabled, |ui| {
            egui::ComboBox::from_id_salt(("frame-media", id.0, key))
                .width(r.width())
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for &(value, label) in items {
                        ui.selectable_value(active, value, label);
                    }
                    ui.layer_id()
                })
                .inner
        })
        .inner
    })
    .inner
}
