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
    /// Adapt the one shell silhouette to the node's actual model and port sites.
    /// Image-backed media and rectangular portals inherit the same notch policy.
    pub(crate) fn media_host_surface(
        &self,
        ctx: &egui::Context,
        xf: &BoardXf,
        node: &Node,
    ) -> Option<Arc<chrome::MediaHostGeometry>> {
        if self.slate_nesting() || self.portal_capture_owns_viewport(node.id) {
            return None;
        }
        let ports = if self.flow_ports_visible(node, ctx, xf) {
            let host = self.wire_input_host(node);
            host.ports()
                .into_iter()
                .map(|site| {
                    let outward = host.outward(site.side, site.t);
                    chrome::MediaPortNotch {
                        center: xf.w2s(egui::Pos2::new(site.point[0], site.point[1])),
                        inward: egui::vec2(-outward[0], -outward[1]),
                    }
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let tab = self
            .shape_properties
            .last_chrome
            .as_ref()
            .and_then(|state| state.media)
            .filter(|prepared| prepared.tab == self.tab().id && prepared.host == node.id)
            .and_then(|prepared| {
                let bounds = xf.rect_w2s(node.rect.rotated_bounds(node.rotation_deg));
                chrome::blister_layout(bounds, xf.z, prepared.count, prepared.presentation).cutout
            });
        if ports.is_empty() && tab.is_none() {
            return None;
        }
        let outline = self.node_screen_outline(ctx, xf, node);
        Some(chrome::media_host_geometry(
            ctx,
            Id::new(("media_host_surface", self.tab().id, node.id.0)),
            &outline,
            &ports,
            tab,
            xf.z,
        ))
    }

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
        let surface = self.media_host_surface(ctx, xf, shown);
        let fallback;
        let outline = match surface.as_ref().and_then(|surface| surface.outline.first()) {
            Some(outline) => outline.as_slice(),
            None => {
                fallback = self.node_screen_outline(ctx, xf, shown);
                fallback.as_slice()
            }
        };
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
            outline,
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
        // Capture can start after the selection palette painted this frame.
        if self.portal_capture_owns_viewport(image) {
            return None;
        }
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
