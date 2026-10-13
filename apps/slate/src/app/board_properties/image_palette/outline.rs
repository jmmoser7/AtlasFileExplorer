//! Model/style adapter for the one shared image-and-blister frame.
use super::*;
use std::sync::Arc;

#[derive(Clone)]
struct ActiveFrame {
    tab: u64,
    image: NodeId,
    geometry: Arc<chrome::ImageFrameGeometry>,
    layer: egui::LayerId,
}

fn frame_id() -> Id {
    Id::new("active_image_palette_frame")
}

pub(super) fn layer(image: NodeId) -> egui::LayerId {
    egui::LayerId::new(
        chrome::CANVAS_CHROME_ORDER,
        Id::new(("image_selection_palette", image.0)),
    )
}

impl SlateApp {
    pub(crate) fn clear_image_palette_frame(&self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.remove::<ActiveFrame>(frame_id()));
    }

    pub(super) fn update_image_palette_frame(
        &self,
        ctx: &egui::Context,
        xf: &BoardXf,
        host: &Node,
        layout: &chrome::ImagePresentationLayout,
        expanded: bool,
    ) {
        let shown = self
            .shape_properties
            .preview
            .iter()
            .find(|node| node.id == host.id)
            .unwrap_or(host);
        let outline = self.node_screen_outline(ctx, xf, shown);
        let stroke = match &shown.kind {
            NodeKind::Image(image) if shown.clip.is_none() && !image.stroke.is_none() => {
                image.stroke.width * xf.z
            }
            _ => 0.0,
        };
        let width = atlas_shell::tokens::current()
            .board_preview
            .select_line_weight
            * xf.z;
        let geometry = chrome::image_frame_geometry(
            ctx,
            Id::new(("image_palette_frame_geometry", self.tab().id, host.id.0)),
            &outline,
            expanded.then_some(layout.overall),
            host.rotation_deg.abs() < 0.001,
            xf.z,
            stroke,
            width,
        );
        ctx.data_mut(|data| {
            data.insert_temp(
                frame_id(),
                ActiveFrame {
                    tab: self.tab().id,
                    image: host.id,
                    geometry,
                    layer: layer(host.id),
                },
            )
        });
    }

    pub(crate) fn image_palette_frame(
        &self,
        ctx: &egui::Context,
        image: NodeId,
    ) -> Option<(Arc<chrome::ImageFrameGeometry>, egui::LayerId)> {
        ctx.data(|data| data.get_temp::<ActiveFrame>(frame_id()))
            .filter(|frame| frame.tab == self.tab().id && frame.image == image)
            .map(|frame| (frame.geometry, frame.layer))
    }

    pub(crate) fn paint_image_palette_stroke(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        image: &slate_doc::scene::ImageNode,
    ) -> bool {
        if node.clip.is_some() {
            return false;
        }
        let Some((frame, layer)) = self.image_palette_frame(painter.ctx(), node.id) else {
            return false;
        };
        let painter = painter.clone().with_layer_id(layer);
        for outline in &frame.body {
            crate::app::board::stroke_outline(&painter, outline, &image.stroke, xf.z);
        }
        true
    }
}
