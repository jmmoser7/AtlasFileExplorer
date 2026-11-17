//! Paint the editor body in the shared image palette layout.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_panel_body(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    layout: &chrome::ImagePresentationLayout,
    id: NodeId,
    panel: Panel,
    z: f32,
    theme: atlas_shell::theme::Palette,
    enabled: bool,
) -> (bool, Option<chrome::SampleGesture>, std::ops::Range<usize>) {
    match panel {
        Panel::Brush => {
            let response = brush::body(ui, layout, id, z, theme, enabled, app);
            (response.captures, response.sample, response.popup_hits)
        }
        Panel::Layers => {
            let popup_start = app.shape_properties.chrome_hits.len();
            let hits = app.image_layer_palette_body(ui, layout, id, z, theme);
            let popup_hits = popup_start..app.shape_properties.chrome_hits.len();
            let captures = ui.ctx().pointer_latest_pos().is_some_and(|p| {
                hits.iter()
                    .chain(&app.shape_properties.chrome_hits[popup_hits.clone()])
                    .any(|rect| rect.contains(p))
            });
            app.shape_properties.chrome_hits.extend(hits);
            (captures, None, popup_hits)
        }
        _ => {
            let start = app.shape_properties.chrome_hits.len();
            let sample = app.shape_property_body(ui, layout.editor_rect(z), panel, z, Some(layout));
            (false, sample, start..app.shape_properties.chrome_hits.len())
        }
    }
}
