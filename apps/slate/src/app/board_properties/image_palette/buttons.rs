use super::*;

pub(in crate::app::board_properties) fn handles(app: &SlateApp, live: bool) -> bool {
    if !live || app.shape_properties.nodes.len() != 1 {
        return false;
    }
    let Some(node) = app.shape_properties.nodes.first() else {
        return false;
    };
    app.image_controls_eligible(node.id)
}

pub(in crate::app::board_properties) fn attached(app: &SlateApp, live: bool) -> bool {
    handles(app, live) && app.image_blister_eligible(&app.shape_properties.nodes[0])
}

pub(in crate::app::board_properties) fn panels(app: &SlateApp, node: &Node) -> Option<Vec<Panel>> {
    app.image_controls_eligible(node.id).then(|| {
        IMAGE_PANELS
            .iter()
            .copied()
            .filter(|panel| *panel != Panel::Corners || slate_doc::scene::supports_corners(node))
            .collect()
    })
}

pub(in crate::app::board_properties) fn control_count(
    app: &SlateApp,
    node: &Node,
) -> Option<usize> {
    panels(app, node).map(|panels| {
        panels.len() + usize::from(app.image_agent_has_wired_image_and_prompt(node.id))
    })
}

pub(super) fn panel_buttons(
    app: &mut SlateApp,
    ctx: &egui::Context,
    layout: &chrome::ImagePresentationLayout,
    enabled: bool,
    closing_panel: Option<Panel>,
    external_strip: Option<Rect>,
    z: f32,
) -> (bool, Option<Panel>, bool) {
    let theme = app.palette();
    let image = app.shape_properties.nodes[0].id;
    let mut captures = false;
    let mut requested_panel = None;
    let mut close_requested = false;
    let panels = panels(app, &app.shape_properties.nodes[0]).unwrap_or_default();
    let quick_generate = app.image_agent_has_wired_image_and_prompt(image);
    for (index, panel) in panels.iter().copied().enumerate() {
        let button = external_strip.map_or_else(
            || layout.blister.icon_rect(index),
            |strip| chrome::strip_button_rect(strip, index, z),
        );
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
                let active = app.shape_properties.panel == Some(panel)
                    || (app.shape_properties.panel.is_none() && closing_panel == Some(panel));
                let response = if external_strip.is_some() {
                    chrome::button(
                        ui,
                        button,
                        Id::new(("image_external_panel", image.0, index)),
                        panel_label(panel),
                        panel_icon(panel),
                        active,
                        z,
                        theme,
                        1.0,
                        enabled,
                    )
                } else {
                    let response = ui
                        .allocate_rect(
                            button,
                            if enabled {
                                Sense::click()
                            } else {
                                Sense::hover()
                            },
                        )
                        .on_hover_text(panel_label(panel));
                    let color = if active {
                        theme.accent
                    } else if response.hovered() {
                        theme.ink
                    } else {
                        theme.ink.gamma_multiply(0.82)
                    };
                    layout
                        .blister
                        .paint_icon(ui.painter(), index, panel_icon(panel), color, theme);
                    response
                };
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
    if quick_generate {
        let play = external_strip.map_or_else(
            || layout.blister.icon_rect(panels.len()),
            |strip| chrome::strip_button_rect(strip, panels.len(), z),
        );
        let datum = external_strip.unwrap_or(layout.blister.band);
        let count_rect = Rect::from_center_size(
            egui::pos2(play.center().x, datum.top() - 9.0 * z),
            egui::vec2(32.0 * z, 14.0 * z),
        );
        egui::Area::new(Id::new(("image_palette_quick_generate", image.0)))
            .order(chrome::CANVAS_CHROME_ORDER)
            .fixed_pos(play.union(count_rect).min)
            .constrain(false)
            .movable(false)
            .fade_in(false)
            .show(ctx, |ui| {
                ui.set_clip_rect(app.canvas_rect);
                ui.add_enabled_ui(enabled, |ui| {
                    captures |=
                        app.paint_image_agent_quick_generate(ui, image, play, count_rect, z);
                });
            });
        app.shape_properties.chrome_hits.extend([play, count_rect]);
    }
    (captures, requested_panel, close_requested)
}

pub(in crate::app::board_properties) fn panel_kind(panel: Option<Panel>) -> chrome::ImagePanelKind {
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
