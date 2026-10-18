//! Selected-image palette adapter. Geometry and paint belong to atlas-shell;
//! the app routes its gestures through existing property and tool owners.

use super::{BoardXf, Panel, SlateApp};
use atlas_shell::{icons::Icon, selection_tools as chrome};
use eframe::egui::{self, Id, Rect, Sense};
use slate_doc::{
    scene::{Node, NodeKind},
    NodeId,
};

mod brush;
mod buttons;
use buttons::panel_buttons;
pub(super) use buttons::{attached, control_count, handles, panel_kind, panels};
mod outline;

const IMAGE_PANELS: [Panel; 7] = [
    Panel::Fill,
    Panel::Stroke,
    Panel::Corners,
    Panel::Filter,
    Panel::Brush,
    Panel::Layers,
    Panel::Agent,
];

pub(super) fn editing(app: &SlateApp) -> bool {
    let mut selected = app.board_sel.iter();
    let Some(&image) = selected.next() else {
        return false;
    };
    selected.next().is_none()
        && app.image_palette_eligible(image)
        && app.agent_picture_draw_mode(image)
}

pub(super) fn dimensions_visible(ctx: &egui::Context, app: &SlateApp) -> bool {
    let Some(node) = app.shape_properties.nodes.first() else {
        return true;
    };
    chrome::strip_fade(
        ctx,
        Id::new(("image_palette_expansion", node.id.0)),
        app.shape_properties.panel.is_some(),
    ) <= 0.0
        && app.shape_properties.panel.is_none()
}

pub(super) fn dismiss_if_inactive(ctx: &egui::Context, app: &mut SlateApp, live: bool) {
    brush::dismiss_if_inactive(ctx, live, app);
}

pub(super) fn cancel_brush_edit(ctx: &egui::Context, image: NodeId, app: &mut SlateApp) {
    brush::cancel_setting_edit(ctx, image, app);
}

impl SlateApp {
    /// The one owner of interaction-shadow eligibility and its dismissal fade.
    pub(crate) fn media_interaction_shadow_fade(
        &self,
        painter: &egui::Painter,
        node: &Node,
    ) -> f32 {
        if self.slate_nesting()
            || !(matches!(node.kind, NodeKind::Image(_) | NodeKind::Portal(_))
                || matches!(node.kind, NodeKind::Text(_)) && self.is_text_block(node.id))
            || self.agent_picture_draw_mode(node.id)
        {
            return 0.0;
        }
        let selected = self.board_sel.len() == 1 && self.board_sel.contains(&node.id);
        chrome::strip_fade(
            painter.ctx(),
            Id::new(("image_active_shadow", node.id.0)),
            selected,
        )
    }

    pub(crate) fn paint_image_interaction_shadow(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        chrome: bool,
    ) {
        if !chrome {
            return;
        }
        let fade = self.media_interaction_shadow_fade(painter, node);
        if fade <= 0.0 {
            return;
        }
        let mut shadow = painter.clone();
        shadow.multiply_opacity(fade);
        if let Some(surface) = self.media_host_surface(painter.ctx(), xf, node) {
            if let Some(outline) = surface.outline.first() {
                let world = outline.iter().map(|p| xf.s2w(*p)).collect::<Vec<_>>();
                chrome::paint_outline_interaction_shadow(
                    &shadow,
                    Id::new(("media_host_shadow", self.tab().id, node.id.0)),
                    &world,
                    egui::emath::TSTransform::new(xf.center.to_vec2() - xf.offset * xf.z, xf.z),
                    self.palette(),
                );
                return;
            }
        }
        chrome::paint_image_interaction_shadow(
            &shadow,
            Id::new(("image_active_shadow_mesh", node.id.0)),
            xf.rect_w2s(node.rect),
            slate_doc::scene::resolved_corner_radius(node, None) * xf.z,
            node.rotation_deg,
            xf.z,
            self.palette(),
        );
    }

    pub(crate) fn image_palette_eligible(&self, id: NodeId) -> bool {
        let Some(node) = self.doc().scene.node(id) else {
            return false;
        };
        let NodeKind::Image(image) = &node.kind else {
            return false;
        };
        self.doc()
            .item(image.item)
            .is_some_and(|item| slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Image)
            || slate_doc::agent_chat::agent(node)
                .is_some_and(|agent| agent.view == atlas_agent::PortalView::Images)
    }

    pub(crate) fn image_blister_eligible(&self, node: &Node) -> bool {
        control_count(self, node)
            .is_some_and(|count| super::property_strip::media_blister_fits(node, count))
    }

    pub(crate) fn image_controls_eligible(&self, id: NodeId) -> bool {
        !self.portal_capture_owns_viewport(id) && self.image_palette_eligible(id)
    }
}

pub(super) fn show(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    xf: &BoardXf,
    external_strip: Option<Rect>,
    enabled: bool,
) -> bool {
    let Some(host) = app.shape_properties.nodes.first().cloned() else {
        return false;
    };
    let host = app.doc().scene.node(host.id).cloned().unwrap_or(host);
    let NodeKind::Image(_) = host.kind else {
        return false;
    };
    let image_rect = xf.rect_w2s(host.rect.rotated_bounds(host.rotation_deg));
    let z = xf.z;
    let panel = app.shape_properties.panel;
    if let Some(panel) = panel {
        app.remember_object_panel_open(host.id, panel);
    }
    if panel == Some(Panel::Layers) {
        app.sync_image_paint_for_tool();
    }
    let theme = app.palette();
    let requested_kind = if panel == Some(Panel::Agent) {
        chrome::ImagePanelKind::Custom {
            main_height: app.image_agent_body_height_for_width(
                ui.painter(),
                host.id,
                if external_strip.is_some() {
                    chrome::EDITOR_WIDTH
                } else {
                    image_rect.width() / z
                },
            ),
        }
    } else {
        panel_kind(panel)
    };
    let id = host.id;
    let ctx = ui.ctx().clone();
    let canvas = app.canvas_rect;
    brush::prepare_image(&ctx, id, panel == Some(Panel::Brush), app);
    let expansion = chrome::strip_fade(
        &ctx,
        Id::new(("image_palette_expansion", id.0)),
        panel.is_some(),
    );
    let clear_tab =
        panel.is_some() || app.agent_picture_draw_mode(id) || app.image_lasso_active(id);
    let tab_opacity = 1.0 - chrome::strip_fade(&ctx, Id::new(("image_tab_clear", id.0)), clear_tab);
    let kind_key = Id::new(("image_palette_last_kind", id.0));
    let panel_key = Id::new(("image_palette_last_panel", id.0));
    let closing_panel = if panel.is_none() && expansion > 0.0 {
        ctx.data(|data| data.get_temp::<Panel>(panel_key))
    } else {
        None
    };
    let kind = if panel.is_some() {
        ctx.data_mut(|data| {
            data.insert_temp(kind_key, requested_kind);
            data.insert_temp(panel_key, panel.unwrap());
        });
        requested_kind
    } else if expansion > 0.0 {
        ctx.data(|data| data.get_temp::<chrome::ImagePanelKind>(kind_key))
            .unwrap_or(chrome::ImagePanelKind::Custom { main_height: 0.0 })
    } else {
        chrome::ImagePanelKind::Custom { main_height: 0.0 }
    };
    let showing_panel = panel.is_some() || expansion > 0.0;
    let presentation = if showing_panel {
        chrome::BlisterPresentation::Overlay
    } else {
        chrome::BlisterPresentation::Collapsed
    };
    let mut layout = external_strip.map_or_else(
        || chrome::image_presentation_layout(image_rect, z, kind),
        |strip| chrome::image_editor_presentation_layout(image_rect, strip, z, kind, canvas),
    );
    let count = control_count(app, &host).unwrap_or_default();
    if external_strip.is_none() {
        layout.blister = chrome::blister_layout(image_rect, z, count, presentation);
    }
    let mut captures = false;
    let mut sample = None;
    let mut popup_hits = 0..0;
    if showing_panel {
        layout.overall.max.y = layout.overall.top() + layout.overall.height() * expansion;
    } else {
        layout.main.max.y = layout.main.top();
        layout.overall.max.y = layout.technical.bottom();
    }
    let area_rect = if external_strip.is_some() || clear_tab {
        layout.overall
    } else if showing_panel {
        layout.overall.union(layout.blister.band)
    } else {
        layout.blister.band
    };
    let owner = Some((app.tab().id, id));
    if showing_panel {
        app.occupy_result_chrome(owner, layout.overall);
    }
    app.occupy_result_chrome(owner, external_strip.unwrap_or(layout.blister.band));
    let mut paint = |ui: &mut egui::Ui| {
        if external_strip.is_none() {
            ui.set_clip_rect(canvas.intersect(image_rect.union(layout.overall).expand(16.0 * z)));
            ui.set_min_size(area_rect.size());
            if showing_panel {
                if host.rotation_deg.abs() < 0.001 {
                    let (chamfer, radius) =
                        slate_doc::scene::resolved_corner_effective(&host, None);
                    chrome::paint_blister_corner_backing(
                        ui.painter(),
                        Id::new(("image_blister_corners", id.0)),
                        image_rect,
                        radius * z,
                        chamfer,
                        theme,
                    );
                }
                chrome::paint_image_blister(ui.painter(), &layout, z, theme, tab_opacity);
            }
            if !clear_tab {
                captures |= ctx
                    .pointer_latest_pos()
                    .is_some_and(|p| layout.blister.band.contains(p));
                app.shape_properties.chrome_hits.push(layout.blister.band);
            }
        }
        ui.set_clip_rect(canvas.intersect(layout.overall));
        let Some(body_panel) = panel.or(closing_panel).filter(|_| showing_panel) else {
            return;
        };
        let body_enabled = enabled && panel.is_some();
        let (body_captures, gesture, body_popups) = ui
            .add_enabled_ui(body_enabled, |ui| {
                paint_panel_body(app, ui, &layout, id, body_panel, z, theme, body_enabled)
            })
            .inner;
        popup_hits = body_popups;
        if panel.is_some() {
            captures |= body_captures;
            sample = gesture;
        }
        captures |= ctx.pointer_latest_pos().is_some_and(|p| {
            canvas.contains(p)
                && (layout.overall.contains(p)
                    || app
                        .shape_properties
                        .chrome_hits
                        .iter()
                        .any(|hit| hit.contains(p)))
        });
        app.shape_properties.chrome_hits.push(layout.overall);
    };
    let editor_area = if let Some(strip) = external_strip {
        showing_panel
            .then(|| {
                super::EditorHost {
                    node: Some(&host),
                    bounds: image_rect,
                    strip,
                    zoom: z,
                    blister: false,
                    canvas,
                    theme,
                }
                .show_image(
                    &ctx,
                    outline::layer(id).id,
                    ui.layer_id(),
                    &layout,
                    true,
                    &mut paint,
                )
            })
            .flatten()
            .map(|editor| editor.area)
    } else {
        egui::Area::new(outline::layer(id).id)
            .order(chrome::CANVAS_CHROME_ORDER)
            .fixed_pos(area_rect.min)
            .constrain(false)
            .movable(false)
            .sense(Sense::hover())
            .fade_in(false)
            .show(&ctx, &mut paint);
        None
    };
    drop(paint);
    if let Some(area) = editor_area {
        app.occupy_result_chrome(owner, area);
    }
    if showing_panel && expansion < 1.0 {
        let visible = canvas.intersect(layout.overall);
        let on_popup = ctx.pointer_latest_pos().is_some_and(|p| {
            canvas.contains(p)
                && app.shape_properties.chrome_hits[popup_hits.clone()]
                    .iter()
                    .any(|hit| hit.contains(p))
        });
        for (index, hit) in app.shape_properties.chrome_hits.iter_mut().enumerate() {
            // Popups paint on their own Area above the reveal clip. Their
            // visible extension must keep input ownership during that reveal.
            *hit = hit.intersect(if popup_hits.contains(&index) {
                canvas
            } else {
                visible
            });
        }
        app.shape_properties
            .chrome_hits
            .retain(|hit| hit.is_positive());
        captures = on_popup
            || captures
                && ctx
                    .pointer_latest_pos()
                    .is_some_and(|p| visible.contains(p));
    }
    if external_strip.is_none() && layout.blister.band.intersect(canvas).is_positive() {
        app.prepare_media_blister(count, presentation);
    } else {
        app.clear_prepared_media_blister();
    }
    if external_strip.is_none() {
        app.update_image_palette_frame(&ctx, xf, &host, &layout, showing_panel);
    }
    let (icon_captures, requested_panel, close_requested) = panel_buttons(
        app,
        &ctx,
        &layout,
        enabled && (panel.is_some() || expansion <= 0.0),
        closing_panel,
        external_strip,
        z,
    );
    captures |= icon_captures;
    if close_requested {
        app.finish_image_layer_opacity(&ctx);
        brush::finish_setting_edit(&ctx, id, app);
        app.apply_shape_preview(&ctx, true);
        app.shape_properties.panel = None;
        app.forget_object_panel(id);
    }
    if let Some(panel) = requested_panel {
        app.finish_image_layer_opacity(&ctx);
        brush::finish_setting_edit(&ctx, id, app);
        app.apply_shape_preview(&ctx, true);
        app.shape_properties.panel = Some(panel);
    }
    if let Some(gesture) = sample {
        if panel == Some(Panel::Brush) {
            brush::start_brush_sample(app, gesture);
        } else {
            app.start_property_desktop_sample(panel.unwrap_or(Panel::Fill), gesture);
        }
    }
    let on_image = ctx.pointer_latest_pos().is_some_and(|pointer| {
        let point = xf.s2w(pointer);
        slate_doc::geom::point_in_node_outline(&host, point.x, point.y, 0.0)
    });
    let on_results = app.result_strip_contains(&ctx, id, xf.rect_w2s(host.rect), z);
    if !captures {
        if panel.is_some()
            && panel != Some(Panel::Brush)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            app.apply_shape_preview(&ctx, false);
            app.shape_properties.panel = None;
            app.forget_object_panel(id);
        } else if app.property_dismiss_press(&ctx).is_some_and(|press| !press) && on_image {
            if !app.shape_properties.edits.is_empty() {
                app.apply_shape_preview(&ctx, false);
            }
        } else if app.property_dismiss_press(&ctx).is_some_and(|press| !press)
            && !on_image
            && !on_results
        {
            if let Some(panel) = panel {
                app.dismiss_object_panel(id, panel, std::time::Instant::now());
            }
            app.finish_image_layer_opacity(&ctx);
            if panel == Some(Panel::Brush) {
                brush::finish_setting_edit(&ctx, id, app);
            }
            app.apply_shape_preview(&ctx, true);
            app.disarm_image_tool(id);
            app.shape_properties.panel = None;
            app.clear_image_paint_session();
            app.dismiss_image_segment();
            let primary_canvas = ctx.pointer_latest_pos().filter(|p| {
                canvas.contains(*p) && ctx.input(|input| input.pointer.primary_pressed())
            });
            // The dismissal owns this press; the board still owns selection.
            if let Some(pointer) = primary_canvas {
                let world = xf.s2w(pointer);
                if !app.try_dock_embed_click(&ctx, world) {
                    app.board_click(world, ctx.input(|input| input.modifiers));
                }
                captures = true;
            } else {
                app.board_sel.remove(&id);
            }
        }
    }
    if captures && ctx.input(|i| i.pointer.any_pressed()) {
        app.board_align_eat_press = true;
    }
    captures
}

// Separate icon Areas leave the transparent spaces between them in the canvas
// hit-test world. An image-spanning Area would still block the brush there.

#[allow(clippy::too_many_arguments)]
fn paint_panel_body(
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
            let hits = app.image_layer_palette_body(ui, layout, id, z, theme);
            let captures = ui
                .ctx()
                .pointer_latest_pos()
                .is_some_and(|p| hits.iter().any(|rect| rect.contains(p)));
            app.shape_properties.chrome_hits.extend(hits);
            (captures, None, 0..0)
        }
        _ => {
            let start = app.shape_properties.chrome_hits.len();
            let sample = app.shape_property_body(ui, layout.editor_rect(z), panel, z, Some(layout));
            (false, sample, start..app.shape_properties.chrome_hits.len())
        }
    }
}
