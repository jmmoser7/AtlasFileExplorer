//! Collapsed output strip; interactions share the generator result controller.
use super::*;

impl SlateApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_agent_album_strip(
        &mut self,
        ui: &egui::Ui,
        id: NodeId,
        body: Rect,
        images: &[atlas_agent::ImageOutput],
        shown: Option<usize>,
        flowing: bool,
        reveal: bool,
        z: f32,
    ) {
        let base = self.tab().path.clone();
        // Squares under the card: every result, grouped by run. While it
        // flows they follow the flow.
        if images.len() > 1 {
            let focus = if flowing {
                self.agents.cover_focus.get(&id).copied()
            } else {
                shown
            }
            .unwrap_or(0);
            let squares: Vec<AlbumImage> = images
                .iter()
                .enumerate()
                .map(|(i, image)| {
                    let near = i.abs_diff(focus) <= 12;
                    let texture = near
                        .then(|| {
                            self.linked_image_texture(
                                resolve_source(base.as_deref(), &image.source),
                                &image.id,
                                canvas_scale::px(44.0, z),
                            )
                        })
                        .flatten();
                    AlbumImage {
                        texture: texture.as_ref().map(|t| t.id()),
                        size: texture
                            .as_ref()
                            .map(|t| t.size_vec2())
                            .unwrap_or(egui::vec2(1.0, 1.0)),
                        enabled: true,
                    }
                })
                .collect();
            let runs: Vec<&str> = images.iter().map(|i| i.request.as_str()).collect();
            let selection = self.result_selection(id);
            let selected: Vec<usize> = images
                .iter()
                .enumerate()
                .filter_map(|(i, image)| selection.contains(&image.id).then_some(i))
                .collect();
            let interaction = atlas_shell::home::album_result_strip(
                ui,
                Id::new(("agent-picture", id.0)),
                body,
                &squares,
                &runs,
                focus,
                reveal,
                z,
                self.palette(),
                2,
                None,
                &selected,
            );
            self.remember_result_strip(
                id,
                body,
                interaction
                    .painted_bounds
                    .filter(|_| reveal)
                    .unwrap_or(Rect::NOTHING),
                z,
            );
            let strip = atlas_shell::home::album_strip_rect(body, images.len(), z);
            self.handle_result_interaction(ui, id, interaction, strip);
        } else {
            self.remember_result_strip(id, body, Rect::NOTHING, z);
        }
    }
}
