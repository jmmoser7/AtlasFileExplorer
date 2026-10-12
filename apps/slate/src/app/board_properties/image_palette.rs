//! Selected-image palette adapter. Geometry and paint belong to atlas-shell;
//! the app routes its gestures through existing property and tool owners.

use super::{BoardXf, Panel, SlateApp};
use atlas_shell::{
    icons::{self, Icon},
    selection_tools as chrome,
};
use eframe::egui::{self, Id, Rect, Sense};
use slate_doc::{
    scene::{Node, NodeKind},
    NodeId,
};

mod brush;

const IMAGE_PANELS: [Panel; 7] = [
    Panel::Fill,
    Panel::Stroke,
    Panel::Corners,
    Panel::Filter,
    Panel::Brush,
    Panel::Layers,
    Panel::Agent,
];

pub(super) fn handles(app: &SlateApp, live: bool) -> bool {
    if !live || app.shape_properties.nodes.len() != 1 {
        return false;
    }
    let Some(node) = app.shape_properties.nodes.first() else {
        return false;
    };
    app.image_palette_eligible(node.id)
}

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
    pub(crate) fn paint_image_interaction_shadow(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        chrome: bool,
    ) {
        if !chrome || self.slate_nesting() || !self.image_palette_eligible(node.id) {
            return;
        }
        let selected = self.board_sel.len() == 1 && self.board_sel.contains(&node.id);
        let fade = chrome::strip_fade(
            painter.ctx(),
            Id::new(("image_active_shadow", node.id.0)),
            selected,
        );
        if fade <= 0.0 {
            return;
        }
        let mut shadow = painter.clone();
        shadow.multiply_opacity(fade);
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
}

pub(super) fn show(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    xf: &BoardXf,
    _selection_bounds: Rect,
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
        app.shape_properties
            .remembered_image_panels
            .insert((app.tab().id, host.id), panel);
    }
    if panel == Some(Panel::Layers) {
        app.sync_image_paint_for_tool();
    }
    let theme = app.palette();
    let requested_kind = if panel == Some(Panel::Agent) {
        chrome::ImagePanelKind::Custom {
            main_height: app.image_agent_body_height(ui.painter(), host.id),
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
    let mut layout = chrome::image_presentation_layout(image_rect, z, kind);
    let count = IMAGE_PANELS.len() - usize::from(!slate_doc::scene::supports_corners(&host));
    layout.tab = chrome::blister_tab_rect(image_rect, z, count);
    let mut captures = false;
    let mut sample = None;
    if showing_panel {
        let full_overall_height = layout.overall.height();
        let full_height = layout.main.height();
        layout.main.max.y = layout.main.top() + full_height * expansion;
        layout.overall.max.y = layout.overall.top() + full_overall_height * expansion;
    } else {
        layout.main.max.y = layout.main.top();
        layout.overall.max.y = layout.technical.bottom();
    }
    let area_rect = if clear_tab {
        layout.overall
    } else if showing_panel {
        layout.overall.union(layout.tab)
    } else {
        layout.tab
    };

    egui::Area::new(Id::new(("image_selection_palette", id.0)))
        .order(chrome::CANVAS_CHROME_ORDER)
        .fixed_pos(area_rect.min)
        .constrain(false)
        .movable(false)
        .sense(Sense::hover())
        .fade_in(false)
        .show(&ctx, |ui| {
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
            let tab_hover = !clear_tab
                && ctx
                    .pointer_latest_pos()
                    .is_some_and(|p| layout.tab.contains(p));
            if !showing_panel {
                chrome::paint_image_tab(ui.painter(), &layout, z, theme, tab_hover, tab_opacity);
            }
            captures |= tab_hover;
            if !clear_tab {
                app.shape_properties.chrome_hits.push(layout.tab);
            }
            if panel.is_none() {
                if showing_panel {
                    if let Some(last_panel) = closing_panel {
                        ui.add_enabled_ui(false, |ui| match last_panel {
                            Panel::Brush => {
                                let _ = brush::body(ui, &layout, id, z, theme, false, app);
                            }
                            Panel::Layers => {
                                let _ = app.image_layer_palette_body(ui, &layout, id, z, theme);
                            }
                            _ => {
                                let _ = app.shape_property_body(
                                    ui,
                                    layout.editor_rect(z),
                                    last_panel,
                                    z,
                                    Some(&layout),
                                );
                            }
                        });
                    }
                    captures |= ctx
                        .pointer_latest_pos()
                        .is_some_and(|p| canvas.contains(p) && layout.overall.contains(p));
                    app.shape_properties.chrome_hits.push(layout.overall);
                }
                return;
            }
            if let Some(selected_panel) = panel {
                ui.add_enabled_ui(enabled, |ui| {
                    if selected_panel == Panel::Brush {
                        let brush = brush::body(ui, &layout, id, z, theme, enabled, app);
                        captures |= brush.captures;
                        sample = brush.sample;
                    } else if selected_panel == Panel::Layers {
                        let hits = app.image_layer_palette_body(ui, &layout, id, z, theme);
                        captures |= ctx
                            .pointer_latest_pos()
                            .is_some_and(|p| hits.iter().any(|rect| rect.contains(p)));
                        app.shape_properties.chrome_hits.extend(hits);
                    } else {
                        sample = app.shape_property_body(
                            ui,
                            layout.editor_rect(z),
                            selected_panel,
                            z,
                            Some(&layout),
                        );
                    }
                });
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
            }
        });

    let (icon_captures, requested_panel, close_requested) = panel_buttons(
        app,
        &ctx,
        &layout,
        enabled && (panel.is_some() || expansion <= 0.0),
        closing_panel,
    );
    captures |= icon_captures;
    if close_requested {
        app.finish_image_layer_opacity(&ctx);
        brush::finish_setting_edit(&ctx, id, app);
        app.apply_shape_preview(&ctx, true);
        app.shape_properties.panel = None;
        app.shape_properties
            .remembered_image_panels
            .remove(&(app.tab().id, id));
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
    if !captures {
        if panel.is_some()
            && panel != Some(Panel::Brush)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            app.apply_shape_preview(&ctx, false);
            app.shape_properties.panel = None;
            app.shape_properties
                .remembered_image_panels
                .remove(&(app.tab().id, id));
        } else if app.property_dismiss_press(&ctx).is_some_and(|press| !press) && on_image {
            if !app.shape_properties.edits.is_empty() {
                app.apply_shape_preview(&ctx, false);
            }
        } else if app.property_dismiss_press(&ctx).is_some_and(|press| !press) && !on_image {
            app.finish_image_layer_opacity(&ctx);
            if panel == Some(Panel::Brush) {
                brush::finish_setting_edit(&ctx, id, app);
            }
            app.apply_shape_preview(&ctx, true);
            app.disarm_image_tool(id);
            app.shape_properties.panel = None;
            app.board_sel.remove(&id);
            app.clear_image_paint_session();
            app.dismiss_image_segment();
        }
    }
    if captures && ctx.input(|i| i.pointer.any_pressed()) {
        app.board_align_eat_press = true;
    }
    captures
}

// Separate icon Areas leave the transparent spaces between them in the canvas
// hit-test world. An image-spanning Area would still block the brush there.
fn panel_buttons(
    app: &mut SlateApp,
    ctx: &egui::Context,
    layout: &chrome::ImagePresentationLayout,
    enabled: bool,
    closing_panel: Option<Panel>,
) -> (bool, Option<Panel>, bool) {
    let theme = app.palette();
    let image = app.shape_properties.nodes[0].id;
    let mut captures = false;
    let mut requested_panel = None;
    let mut close_requested = false;
    let panels: Vec<_> = IMAGE_PANELS
        .iter()
        .copied()
        .filter(|panel| {
            *panel != Panel::Corners
                || slate_doc::scene::supports_corners(&app.shape_properties.nodes[0])
        })
        .collect();
    for (index, panel) in panels.iter().copied().enumerate() {
        let button = chrome::blister_icon_rect(layout.tab, index, panels.len());
        egui::Area::new(Id::new(("image_palette_icon_area", image.0, index)))
            .order(chrome::CANVAS_CHROME_ORDER)
            .fixed_pos(button.min)
            .constrain(false)
            .movable(false)
            .sense(Sense::hover())
            .fade_in(false)
            .show(ctx, |ui| {
                ctx.move_to_top(ui.layer_id());
                ui.set_clip_rect(app.canvas_rect);
                let response = ui
                    .allocate_rect(button, Sense::click())
                    .on_hover_text(panel_label(panel));
                let active = app.shape_properties.panel == Some(panel)
                    || (app.shape_properties.panel.is_none() && closing_panel == Some(panel));
                let color = if active {
                    theme.accent
                } else if response.hovered() {
                    theme.ink
                } else {
                    theme.ink.gamma_multiply(0.82)
                };
                icons::paint(ui.painter(), button, panel_icon(panel), color);
                if response.clicked() && enabled {
                    if app.shape_properties.panel == Some(panel) {
                        close_requested = true;
                    } else {
                        requested_panel = Some(panel);
                    }
                }
                captures |= response.hovered();
            });
        app.shape_properties.chrome_hits.push(button);
    }
    (captures, requested_panel, close_requested)
}

pub(super) fn panel_kind(panel: Option<Panel>) -> chrome::ImagePanelKind {
    match panel {
        Some(Panel::Stroke) => chrome::ImagePanelKind::Color { stroke: true },
        Some(Panel::Fill) => chrome::ImagePanelKind::Color { stroke: false },
        Some(Panel::Corners) => chrome::ImagePanelKind::Corners,
        Some(Panel::Filter) => chrome::ImagePanelKind::Filters,
        Some(Panel::Brush) => chrome::ImagePanelKind::Brush,
        Some(Panel::Layers) => chrome::ImagePanelKind::Layers,
        Some(Panel::Agent) => chrome::ImagePanelKind::Custom {
            main_height: chrome::AGENT_HEIGHT,
        },
        _ => chrome::ImagePanelKind::Custom { main_height: 0.0 },
    }
}

fn panel_icon(panel: Panel) -> Icon {
    match panel {
        Panel::Fill => Icon::Fill,
        Panel::Stroke => Icon::Ellipse,
        Panel::Corners => Icon::Corners,
        Panel::Filter => Icon::Sliders,
        Panel::Brush => Icon::Brush,
        Panel::Layers => Icon::Media,
        Panel::Agent => Icon::Agent,
        _ => Icon::Image,
    }
}

fn panel_label(panel: Panel) -> &'static str {
    match panel {
        Panel::Fill => "Fill color",
        Panel::Stroke => "Stroke color and width",
        Panel::Corners => "Corners and crop",
        Panel::Filter => "Photo filters",
        Panel::Brush => "Brush and lasso selection",
        Panel::Layers => "Image results and paint layers",
        Panel::Agent => "Generate or edit this image",
        _ => "Image controls",
    }
}
