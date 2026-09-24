//! Ctrl+U image-adjust popover. Selection properties live on the per-node
//! strips; this file only keeps the journaled adjust controls that popover uses.

use super::super::board::to_rgba;
use super::super::SlateApp;
use atlas_shell::sidebar::{sidebar_slider_block, SidebarTheme};
use atlas_shell::widgets::{thin_sidebar_slider, thin_sidebar_slider_i32};
use eframe::egui::{self, Color32, RichText};
use slate_doc::scene::{Node, NodeKind, Rgba};
use slate_doc::{NodeId, ViewKind};

fn rgba32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.0[0], c.0[1], c.0[2], c.0[3])
}

impl SlateApp {
    /// Ctrl+U: the ImageAdjust controls as a popover anchored beside the
    /// selected image(s) — the same widgets the inspector's Adjust section
    /// renders, targeting the same journaled `patch_nodes` path.
    pub(crate) fn adjust_popover_frame(&mut self, ctx: &egui::Context) {
        if !self.adjust_popover_open {
            return;
        }
        if self.doc().view.active_view != ViewKind::Board {
            self.adjust_popover_open = false;
            return;
        }
        let images = self.selected_image_nodes();
        let Some(primary) = images
            .first()
            .and_then(|id| self.doc().scene.node(*id).cloned())
        else {
            self.adjust_popover_open = false;
            return;
        };
        let srect = self.board_xf().rect_w2s(primary.rect);
        let canvas = self.canvas_rect;
        let pos = egui::Pos2::new(
            (srect.right() + 12.0).clamp(canvas.left(), canvas.right() - 240.0),
            srect.top().clamp(canvas.top(), canvas.bottom() - 320.0),
        );
        let theme = self.palette().sidebar_theme();
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("slate_adjust_popover"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Adjust — {} image(s)", images.len()))
                                .small()
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("✕")
                                .on_hover_text("Close (Ctrl+U)")
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });
                    adjust_controls(self, ui, theme, &images, &primary);
                });
            });
        // Click-away closes (presses outside the popover panel).
        let clicked_outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !area.response.rect.expand(4.0).contains(p))
        });
        if close || clicked_outside {
            self.adjust_popover_open = false;
        }
    }
}

/// Image adjustment sliders (CSS-filter math). Shared by the inspector's
/// Adjust section and the Ctrl+U popover ([`SlateApp::adjust_popover_frame`]).
pub(crate) fn adjust_controls(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    theme: SidebarTheme,
    ids: &[NodeId],
    primary: &Node,
) {
    let NodeKind::Image(img) = &primary.kind else {
        return;
    };
    let adj = img.adjust;
    let mut a = adj;
    let mut changed = false;
    sidebar_slider_block(ui, |ui| {
        for (label, hint, value, identity) in [
            ("Exposure", "brightness()", &mut a.brightness, 1.0f32),
            ("Contrast", "contrast()", &mut a.contrast, 1.0),
            ("Saturation", "saturate()", &mut a.saturate, 1.0),
        ] {
            let mut v = (*value * 100.0).round() as usize;
            if thin_sidebar_slider(ui, &mut v, 0..=200, label, "%", hint, theme.sub) {
                *value = v as f32 / 100.0;
                changed = true;
            }
            let _ = identity;
        }
        for (label, hint, value) in [
            ("B&W", "grayscale()", &mut a.grayscale),
            ("Sepia", "sepia()", &mut a.sepia),
        ] {
            let mut v = (*value * 100.0).round() as usize;
            if thin_sidebar_slider(ui, &mut v, 0..=100, label, "%", hint, theme.sub) {
                *value = v as f32 / 100.0;
                changed = true;
            }
        }
    });
    // Hue can be negative; use the signed thin slider for it.
    sidebar_slider_block(ui, |ui| {
        let mut hue = a.hue_deg.round() as i32;
        if thin_sidebar_slider_i32(
            ui,
            &mut hue,
            -180..=180,
            "Hue",
            "°",
            "hue-rotate()",
            theme.sub,
        ) {
            a.hue_deg = hue as f32;
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        let mut inv = a.invert != 0.0;
        if ui
            .checkbox(&mut inv, RichText::new("Invert").small())
            .on_hover_text("CSS invert(1) — also on Ctrl+I")
            .changed()
        {
            a.invert = if inv { 1.0 } else { 0.0 };
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        let mut on = a.overlay.is_some();
        if ui
            .checkbox(&mut on, RichText::new("Color overlay").small())
            .changed()
        {
            a.overlay = if on {
                Some(Rgba([230, 90, 60, 90]))
            } else {
                None
            };
            changed = true;
        }
        if let Some(ov) = a.overlay {
            let mut col = rgba32(ov);
            if ui.color_edit_button_srgba(&mut col).changed() {
                a.overlay = Some(to_rgba(col));
                changed = true;
            }
        }
    });
    if !adj.is_identity()
        && ui
            .button(RichText::new("Reset adjustments").small())
            .clicked()
    {
        a = slate_doc::scene::ImageAdjust::default();
        changed = true;
    }
    if changed {
        app.patch_nodes(ids, move |n| {
            if let NodeKind::Image(i) = &mut n.kind {
                i.adjust = a;
            }
        });
    }
}
