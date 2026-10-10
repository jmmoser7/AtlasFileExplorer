//! Highlight tag and the layer squircle above an image.

use super::SlateApp;
use atlas_shell::{canvas_scale, selection_tools};
use eframe::egui::{self, Id, Pos2, Rect};
use slate_doc::scene::NodeKind;

pub(super) const CREATE_LAYER_LABEL: &str = "Create layer from highlight";
const LAYERS_LABEL: &str = "Layers";

impl SlateApp {
    /// A click on the silhouette opens the tag. A click elsewhere does not.
    pub(crate) fn segment_click_opens_tag(&mut self, world: Pos2) -> bool {
        if self.image_segments.tag_at.is_some() || !self.segment_mask_hit(world) {
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
        let fresh = self.image_segments.tag_fresh;
        self.image_segments.tag_fresh = false;
        if response.response.clicked() {
            self.commit_hover_segment(None);
            return;
        }
        if fresh {
            return;
        }
        let click = ui.input(|i| {
            i.pointer
                .button_clicked(egui::PointerButton::Primary)
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        if click.is_some_and(|p| !rect.contains(p)) {
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
        let z = xf.z;
        let bounds = xf.rect_w2s(host.rect);
        let h = canvas_scale::px(selection_tools::CORNER_HEIGHT, z);
        let w = canvas_scale::px(selection_tools::STACK_WIDTH, z);
        let gap = canvas_scale::px(selection_tools::STACK_GAP, z);
        let top = bounds.top() + canvas_scale::px(6.0, z);
        let mut rects = Vec::with_capacity(labels.len());
        for (i, label) in labels.iter().enumerate() {
            let rect = Rect::from_min_size(
                Pos2::new(bounds.center().x - w * 0.5, top + i as f32 * (h + gap)),
                egui::vec2(w, h),
            );
            rects.push(rect);
            self.shape_properties.chrome_hits.push(rect);
            let _ = selection_tools::capsule(
                ui,
                Id::new(("image_layers", host_id.0, i)),
                rect,
                &selection_tools::Capsule {
                    label,
                    selected: i == 0,
                    ..Default::default()
                },
                z,
                self.palette(),
            );
        }
        if let Some(union) = rects.first().copied() {
            let union = rects.iter().fold(union, |a, r| a.union(*r));
            atlas_shell::menu_wheel::claim(ui.ctx(), union);
        }
        self.image_segments.layer_rects = rects.clone();
        let fresh = self.image_segments.layers_fresh;
        self.image_segments.layers_fresh = false;
        if fresh {
            return;
        }
        let click = ui.input(|i| {
            i.pointer
                .button_clicked(egui::PointerButton::Primary)
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        if click.is_some_and(|p| rects.iter().all(|r| !r.contains(p))) {
            self.image_segments.layers_menu = None;
            self.image_segments.layer_rects.clear();
        }
    }
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

/// "Layers" plus one row per paint layer, so a highlight layer is listed.
pub(super) fn layer_rows(img: &slate_doc::scene::ImageNode) -> Vec<String> {
    let mut rows = vec![LAYERS_LABEL.to_string()];
    for (i, _) in img.paint_layers.iter().enumerate() {
        rows.push(format!("Layer {}", i + 1));
    }
    rows
}
