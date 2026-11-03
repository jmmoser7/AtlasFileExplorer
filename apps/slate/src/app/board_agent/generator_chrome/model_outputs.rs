//! Model histories adapt immutable connector output references to shared preview chrome.
use super::*;
use atlas_shell::selection_tools::generator::{GeneratorHistory, GeneratorHistoryItem};
pub(super) struct ModelHistoryEntry {
    pub id: String,
    pub label: String,
    pub thumbnail: Option<egui::TextureId>,
    pub current: bool,
}
pub(super) struct ModelHistory {
    pub entries: Vec<ModelHistoryEntry>,
    pub selected: atlas_shell::result_actions::ResultSelection,
}
impl GeneratorHistory for ModelHistory {
    fn len(&self) -> usize {
        self.entries.len()
    }
    fn get(&self, index: usize) -> Option<GeneratorHistoryItem<'_>> {
        let entry = self.entries.get(index)?;
        Some(GeneratorHistoryItem {
            label: std::borrow::Cow::Borrowed(&entry.label),
            thumb: entry.thumbnail,
        })
    }
    fn image_actions(&self) -> bool {
        true
    }
    fn is_selected(&self, index: usize) -> bool {
        self.entries
            .get(index)
            .is_some_and(|entry| entry.current || self.selected.contains(&entry.id))
    }
}
impl SlateApp {
    pub(super) fn model_output_history(
        &mut self,
        ctx: &egui::Context,
        host: NodeId,
        range: std::ops::Range<usize>,
    ) -> ModelHistory {
        let outputs = self.model_generator_outputs(host);
        let entries = outputs
            .into_iter()
            .enumerate()
            .map(|(index, output)| {
                let thumbnail = range
                    .contains(&index)
                    .then(|| self.model_generator_preview_info(host, &output.id))
                    .flatten()
                    .and_then(|info| self.model_poster_texture(ctx, &info, &Default::default()))
                    .map(|texture| texture.id());
                ModelHistoryEntry {
                    id: output.id,
                    label: output.source.to_string_lossy().into_owned(),
                    thumbnail,
                    current: output.current,
                }
            })
            .collect();
        ModelHistory {
            entries,
            selected: self.result_selection(host),
        }
    }
    pub(crate) fn select_model_output(&mut self, detail: Option<&str>) -> bool {
        #[derive(serde::Deserialize)]
        struct Choice {
            node: NodeId,
            output: String,
        }
        let Some(choice) = detail.and_then(|value| serde_json::from_str::<Choice>(value).ok())
        else {
            return false;
        };
        match self.select_model_generator_output(choice.node, &choice.output) {
            Ok(()) => true,
            Err(error) => {
                self.toast(error);
                false
            }
        }
    }
    pub(crate) fn choose_model_output(
        &mut self,
        ctx: &egui::Context,
        host: NodeId,
        identity: &str,
    ) {
        self.dispatch(
            ctx,
            atlas_commands::CommandId("portal.agent.model_output"),
            Some(serde_json::json!({"node":host,"output":identity}).to_string()),
        );
    }
    pub(crate) fn paint_model_result_strip(
        &mut self,
        ui: &egui::Ui,
        host: NodeId,
        body: egui::Rect,
        zoom: f32,
    ) {
        let outputs = self.model_generator_outputs(host);
        if outputs.is_empty() {
            return;
        }
        let circles = self.shape_properties.ids == [host]
            && self.shape_properties.panel
                == Some(super::super::super::board_properties::Panel::Agent);
        if circles {
            return;
        }
        let focus = outputs
            .iter()
            .position(|output| output.current)
            .unwrap_or(outputs.len() - 1);
        let selection = self.result_selection(host);
        let shown = !selection.is_empty()
            || self.board_sel.contains(&host)
            || ui
                .ctx()
                .pointer_latest_pos()
                .is_some_and(|point| body.contains(point))
            || self.result_strip_hovered(ui.ctx(), host, body, zoom);
        let mut images = vec![
            atlas_shell::home::AlbumImage {
                texture: None,
                size: egui::Vec2::splat(96.0),
                enabled: true
            };
            outputs.len()
        ];
        for (index, _) in
            atlas_shell::home::generator_result_rects(body, outputs.len(), focus, zoom)
        {
            if let Some(texture) = self
                .model_generator_preview_info(host, &outputs[index].id)
                .and_then(|info| self.model_poster_texture(ui.ctx(), &info, &Default::default()))
            {
                images[index].size = texture.size_vec2();
                images[index].texture = Some(texture.id());
            }
        }
        let identities = outputs
            .iter()
            .map(|output| output.id.clone())
            .collect::<Vec<_>>();
        let selected = identities
            .iter()
            .enumerate()
            .filter_map(|(index, id)| selection.contains(id).then_some(index))
            .collect::<Vec<_>>();
        let interaction = atlas_shell::home::album_result_strip(
            ui,
            Id::new(("model-results", host.0)),
            body,
            &images,
            &[],
            focus,
            shown,
            zoom,
            self.palette(),
            1,
            None,
            &selected,
        );
        if let Some(bounds) = interaction.painted_bounds {
            self.remember_result_strip(host, body, bounds, zoom);
        }
        if let Some(index) = interaction.clicked {
            self.select_generator_result(host, &identities, index, interaction.shift);
            self.choose_model_output(ui.ctx(), host, &identities[index]);
            self.select_square_result(host);
        }
        self.open_result_context(ui, host, interaction);
    }
}
