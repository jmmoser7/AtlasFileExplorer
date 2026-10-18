//! One layout, surface, clipping and input owner for attached property editors.
use super::*;

pub(crate) struct EditorHost<'a> {
    pub node: Option<&'a Node>,
    pub bounds: Rect,
    pub strip: Rect,
    pub zoom: f32,
    pub blister: bool,
    pub canvas: Rect,
    pub theme: atlas_shell::theme::Palette,
}

pub(crate) struct EditorResponse<T> {
    pub inner: T,
    pub area: Rect,
}

impl EditorHost<'_> {
    /// The canvas clips both the painted apron and its egui Area. The child
    /// keeps the complete layout, so clipping never moves or resizes controls.
    pub(crate) fn show<T>(
        &self,
        ctx: &egui::Context,
        id: Id,
        layer: egui::LayerId,
        panel: (chrome::ImagePanelKind, f32),
        enabled: bool,
        paint: impl FnOnce(&mut egui::Ui, Rect, Option<&chrome::ImagePresentationLayout>) -> T,
    ) -> Option<EditorResponse<T>> {
        let (kind, height) = panel;
        if canvas_scale::too_small(12.0 * self.zoom) {
            return None;
        }
        let layout = self
            .blister
            .then(|| chrome::image_presentation_layout(self.bounds, self.zoom, kind));
        let rect = layout.map_or_else(
            || chrome::editor_placement(self.strip, self.bounds, height, self.zoom, self.canvas),
            |layout| layout.editor_rect(self.zoom),
        );
        let area = layout.map_or(rect, |layout| layout.overall);
        self.show_area(ctx, id, layer, area, |content| {
            if let Some(layout) = &layout {
                chrome::paint_blister_surface(
                    content.painter(),
                    layout.overall,
                    self.strip,
                    self.zoom,
                    self.theme,
                    0.0,
                );
                if let Some(host) = self.node.filter(|host| host.rotation_deg.abs() < 0.001) {
                    let (chamfer, radius) = scene::resolved_corner_effective(host, None);
                    chrome::paint_blister_corner_backing(
                        content.painter(),
                        Id::new(("portal_blister_corners", host.id.0)),
                        self.bounds,
                        radius * self.zoom,
                        chamfer,
                        self.theme,
                    );
                }
            }
            content
                .add_enabled_ui(enabled, |ui| paint(ui, rect, layout.as_ref()))
                .inner
        })
    }

    pub(crate) fn show_image<T>(
        &self,
        ctx: &egui::Context,
        id: Id,
        layer: egui::LayerId,
        layout: &chrome::ImagePresentationLayout,
        enabled: bool,
        paint: impl FnOnce(&mut egui::Ui) -> T,
    ) -> Option<EditorResponse<T>> {
        self.show_area(ctx, id, layer, layout.overall, |ui| {
            chrome::paint_blister_surface(
                ui.painter(),
                layout.overall,
                self.strip,
                self.zoom,
                self.theme,
                0.0,
            );
            ui.add_enabled_ui(enabled, paint).inner
        })
    }

    fn show_area<T>(
        &self,
        ctx: &egui::Context,
        id: Id,
        layer: egui::LayerId,
        area: Rect,
        paint: impl FnOnce(&mut egui::Ui) -> T,
    ) -> Option<EditorResponse<T>> {
        if canvas_scale::too_small(12.0 * self.zoom) {
            return None;
        }
        let hit = area.intersect(self.canvas);
        if !hit.is_positive() {
            return None;
        }
        let shown = chrome::popup_area(ctx, id, layer)
            .fixed_pos(hit.min)
            .default_size(hit.size())
            .constrain(false)
            .movable(false)
            .fade_in(false)
            .sense(egui::Sense::hover())
            .show(ctx, |ui| {
                ui.set_min_size(hit.size());
                ui.set_clip_rect(self.canvas);
                let mut content = ui.new_child(egui::UiBuilder::new().max_rect(area));
                paint(&mut content)
            });
        Some(EditorResponse {
            inner: shown.inner,
            area: hit,
        })
    }
}

impl SlateApp {
    pub(super) fn paint_image_properties(
        &mut self,
        ui: &mut egui::Ui,
        xf: &BoardXf,
        state: &LastChrome,
        live: bool,
        enabled: bool,
    ) -> Option<bool> {
        if !image_palette::handles(self, live) {
            return None;
        }
        let ctx = ui.ctx().clone();
        let dimensions = if image_palette::dimensions_visible(&ctx, self) {
            self.paint_property_dimensions(ui, xf, live, enabled)
        } else {
            self.shape_properties.number = None;
            false
        };
        let external = (!state.image).then(|| {
            chrome::strip_rect_below(
                xf.w2s(state.bottom),
                state.count,
                xf.z,
                chrome::strip_fade(&ctx, Id::new("selection_property_strip"), live),
            )
        });
        Some(image_palette::show(self, ui, xf, external, enabled) || dimensions)
    }

    pub(crate) fn property_editor_kind(
        &self,
        painter: &egui::Painter,
        panel: Panel,
        width_units: f32,
    ) -> chrome::ImagePanelKind {
        match panel {
            Panel::Fill | Panel::Stroke | Panel::Corners | Panel::Filter => {
                image_palette::panel_kind(Some(panel))
            }
            _ => chrome::ImagePanelKind::Custom {
                main_height: self.property_editor_height(painter, panel, width_units),
            },
        }
    }

    pub(crate) fn property_editor_height(
        &self,
        painter: &egui::Painter,
        panel: Panel,
        width_units: f32,
    ) -> f32 {
        match panel {
            Panel::Fill => chrome::FILL_HEIGHT,
            Panel::Stroke => chrome::STROKE_HEIGHT,
            Panel::Corners | Panel::Bumper => chrome::CORNER_HEIGHT,
            Panel::Wire => chrome::WIRE_HEIGHT,
            Panel::Filter | Panel::Brush | Panel::Layers => chrome::FILTER_HEIGHT,
            Panel::Pages => 0.0,
            Panel::AtlasFormat => chrome::ATLAS_FORMAT_HEIGHT,
            Panel::Text => chrome::TEXT_HEIGHT,
            Panel::Agent => self
                .shape_properties
                .ids
                .first()
                .copied()
                .filter(|id| {
                    self.doc()
                        .scene
                        .node(*id)
                        .and_then(scene::artifact)
                        .is_some()
                })
                .map_or(chrome::AGENT_HEIGHT, |id| {
                    self.artifact_editor_height(painter, id, width_units)
                }),
            Panel::ModelDisplay => self.model_display_editor_height(),
            Panel::Capture => chrome::capture::launch_height(
                self.shape_properties
                    .ids
                    .first()
                    .is_some_and(|id| self.portal_capture_can_video(*id)),
                width_units,
            ),
        }
    }
}
