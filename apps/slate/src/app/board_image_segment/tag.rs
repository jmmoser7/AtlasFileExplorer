//! Highlight tag and the layer squircle above an image.

use super::SlateApp;
use crate::app::board::BoardTool;
use crate::app::board_image_layers::{ImagePaintSession, ImageStripFocus};
use atlas_shell::{canvas_scale, selection_tools};
use eframe::egui::{self, Id, Pos2, Rect};
use slate_doc::{scene::NodeKind, NodeId};

pub(super) const CREATE_LAYER_LABEL: &str = "Create layer from highlight";
const ADD_LAYER_LABEL: &str = "Add layer";

impl SlateApp {
    /// A click on the silhouette opens the tag and is consumed, so it never
    /// selects the photo underneath. A click elsewhere is not ours.
    pub(crate) fn segment_click_opens_tag(&mut self, world: Pos2) -> bool {
        if !self.segment_mask_hit(world) {
            return false;
        }
        let screen = self.board_xf().w2s(world);
        self.open_segment_tag(screen);
        true
    }

    pub(super) fn paint_segment_tag(&mut self, ui: &egui::Ui, xf: &super::super::board::BoardXf) {
        let Some(anchor) = self.image_segments.tag_at else {
            self.image_segments.tag_rects.clear();
            return;
        };
        if self.image_segments.result.is_none() {
            self.image_segments.tag_at = None;
            return;
        }
        let z = xf.z;
        let rect = tag_rect(anchor, z);
        self.image_segments.tag_rects = vec![rect];
        self.shape_properties.chrome_hits.push(rect);
        atlas_shell::menu_wheel::claim(ui.ctx(), rect);
        let response = selection_tools::capsule(
            ui,
            Id::new(("segment_tag", self.tab().id)),
            rect,
            &selection_tools::Capsule {
                label: CREATE_LAYER_LABEL,
                ..Default::default()
            },
            z,
            self.palette(),
        );
        let fresh = std::mem::take(&mut self.image_segments.tag_fresh);
        if response.response.clicked() {
            self.commit_hover_segment(None);
            return;
        }
        if !fresh && clicked_outside(ui, &[rect]) {
            self.image_segments.tag_at = None;
            self.image_segments.tag_rects.clear();
        }
    }

    pub(super) fn paint_layer_squircle(
        &mut self,
        ui: &egui::Ui,
        xf: &super::super::board::BoardXf,
    ) {
        let Some(host_id) = self.image_segments.layers_menu else {
            self.image_segments.layer_rects.clear();
            return;
        };
        let Some(host) = self.doc().scene.node(host_id).cloned() else {
            self.image_segments.layers_menu = None;
            return;
        };
        let NodeKind::Image(img) = &host.kind else {
            self.image_segments.layers_menu = None;
            return;
        };
        let labels = layer_rows(img);
        let active = self
            .image_paint_session()
            .filter(|s| s.image == host_id)
            .map(|s| s.layer_index);
        let z = xf.z;
        let bounds = xf.rect_w2s(host.rect.rotated_bounds(host.rotation_deg));
        let h = canvas_scale::px(selection_tools::CORNER_HEIGHT, z);
        let w = canvas_scale::px(selection_tools::STACK_WIDTH, z);
        let gap = canvas_scale::px(selection_tools::STACK_GAP, z);
        let top = bounds.top() + canvas_scale::px(6.0, z);
        let mut rects = Vec::with_capacity(labels.len());
        let mut picked = None;
        for (i, label) in labels.iter().enumerate() {
            let rect = Rect::from_min_size(
                Pos2::new(bounds.center().x - w * 0.5, top + i as f32 * (h + gap)),
                egui::vec2(w, h),
            );
            rects.push(rect);
            self.shape_properties.chrome_hits.push(rect);
            let add = i == img.paint_layers.len();
            let response = selection_tools::capsule(
                ui,
                Id::new(("image_layers", host_id.0, i)),
                rect,
                &selection_tools::Capsule {
                    label,
                    selected: active == Some(i),
                    spawned: img.paint_layers.get(i).is_some_and(|l| l.visible),
                    dim: add,
                    ..Default::default()
                },
                z,
                self.palette(),
            );
            if response.response.clicked() {
                picked = Some(i);
            }
        }
        if let Some(union) = rects.iter().copied().reduce(Rect::union) {
            atlas_shell::menu_wheel::claim(ui.ctx(), union);
        }
        self.image_segments.layer_rects = rects.clone();
        let fresh = std::mem::take(&mut self.image_segments.layers_fresh);
        if let Some(i) = picked {
            self.paint_on_image_layer(host_id, i);
        } else if !fresh && clicked_outside(ui, &rects) {
            self.image_segments.layers_menu = None;
            self.image_segments.layer_rects.clear();
        }
    }

    /// Reach a layer through the ordinary paint session: select the picture,
    /// arm the brush, and make layer `index` active. The row past the last
    /// layer adds one first (journaled).
    fn paint_on_image_layer(&mut self, image: NodeId, index: usize) {
        let index = if self.image_layer_count(image) <= index {
            match self.add_paint_layer_on_image(image) {
                Some(i) => i,
                None => return,
            }
        } else {
            index
        };
        self.image_segments.layers_menu = None;
        self.image_segments.layer_rects.clear();
        self.board_sel = [image].into();
        if self.board_tool != BoardTool::Brush {
            self.set_board_tool(BoardTool::Brush);
        }
        self.sync_image_paint_for_tool();
        if self.paint_layer_host() == Some(image) {
            self.image_paint = Some(ImagePaintSession {
                image,
                layer_index: index,
                focus: ImageStripFocus::Layer(index),
            });
        }
    }

    fn image_layer_count(&self, image: NodeId) -> usize {
        match self.doc().scene.node(image).map(|n| &n.kind) {
            Some(NodeKind::Image(img)) => img.paint_layers.len(),
            _ => 0,
        }
    }
}

fn clicked_outside(ui: &egui::Ui, rects: &[Rect]) -> bool {
    ui.input(|i| {
        i.pointer
            .button_clicked(egui::PointerButton::Primary)
            .then(|| i.pointer.interact_pos())
            .flatten()
    })
    .is_some_and(|p| rects.iter().all(|r| !r.contains(p)))
}

fn tag_rect(anchor: Pos2, zoom: f32) -> Rect {
    let w = canvas_scale::px(selection_tools::STACK_WIDTH, zoom);
    let h = canvas_scale::px(selection_tools::CORNER_HEIGHT, zoom);
    let gap = canvas_scale::px(8.0, zoom);
    Rect::from_min_size(
        Pos2::new(anchor.x - w * 0.5, anchor.y - gap - h),
        egui::vec2(w, h),
    )
}

/// One row per paint layer, then the row that adds one.
pub(super) fn layer_rows(img: &slate_doc::scene::ImageNode) -> Vec<String> {
    let mut rows: Vec<String> = (1..=img.paint_layers.len())
        .map(|n| format!("Layer {n}"))
        .collect();
    rows.push(ADD_LAYER_LABEL.to_string());
    rows
}
