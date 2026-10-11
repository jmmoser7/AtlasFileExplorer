//! Selected-image palette adapter. Geometry and paint belong to atlas-shell;
//! the app routes its gestures through existing property and tool owners.

use super::{BoardXf, Panel, SlateApp};
use atlas_commands::CommandId;
use atlas_shell::{
    icons::{self, Icon},
    selection_tools as chrome,
};
use eframe::egui::{self, Id, Rect, Sense};
use slate_doc::{NodeId, scene::NodeKind};

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

impl SlateApp {
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
    let NodeKind::Image(_) = host.kind else {
        return false;
    };
    let image_rect = xf.rect_w2s(host.rect.rotated_bounds(host.rotation_deg));
    let z = xf.z;
    let theme = app.palette();
    let panel = app.shape_properties.panel;
    let kind = if panel == Some(Panel::Agent) {
        chrome::ImagePanelKind::Custom {
            main_height: app.image_agent_body_height(host.id),
        }
    } else {
        panel_kind(panel)
    };
    let mut layout = chrome::image_presentation_layout(image_rect, z, kind);
    let id = host.id;
    let ctx = ui.ctx().clone();
    let canvas = app.canvas_rect;
    let mut captures = false;
    let mut requested_panel = None;
    let mut close_requested = false;
    let mut sample = None;
    let expansion = chrome::strip_fade(
        &ctx,
        Id::new(("image_palette_expansion", id.0)),
        panel.is_some(),
    );
    if panel.is_some() {
        let full_height = layout.main.height();
        layout.main.max.y = layout.main.top() + full_height * expansion;
        layout.overall.max.y = layout.main.bottom();
    } else {
        layout.main.max.y = layout.main.top();
        layout.overall.max.y = layout.technical.bottom();
    }

    egui::Area::new(Id::new(("image_selection_palette", id.0)))
        .order(chrome::CANVAS_CHROME_ORDER)
        .fixed_pos(layout.overall.min)
        .constrain(false)
        .movable(false)
        .fade_in(false)
        .show(&ctx, |ui| {
            ui.set_clip_rect(canvas.intersect(layout.overall));
            ui.set_min_size(layout.overall.size());
            chrome::paint_image_blister(ui.painter(), &layout, z, theme);
            for (index, panel) in IMAGE_PANELS.iter().copied().enumerate() {
                let button = layout.technical_controls.panel_buttons[index];
                let response = ui
                    .interact(
                        button,
                        Id::new(("image_palette_panel", id.0, index)),
                        Sense::click(),
                    )
                    .on_hover_text(panel_label(panel));
                let active = app.shape_properties.panel == Some(panel);
                let color = if active {
                    theme.select
                } else if response.hovered() {
                    theme.ink
                } else {
                    theme.sub
                };
                icons::paint(ui.painter(), button, panel_icon(panel), color);
                if response.clicked() && enabled {
                    requested_panel = Some(panel);
                }
                captures |= response.hovered();
                app.shape_properties.chrome_hits.push(button);
            }
            app.shape_properties.chrome_hits.push(layout.tab);
            let tab_response = ui.interact(
                layout.tab,
                Id::new(("image_palette_tab", id.0)),
                Sense::click(),
            );
            chrome::paint_image_tab(ui.painter(), &layout, z, theme, tab_response.hovered());
            captures |= tab_response.hovered();
            if tab_response.clicked() && enabled {
                if panel.is_some() {
                    close_requested = true;
                } else {
                    requested_panel = Some(Panel::Fill);
                }
            }
            if panel.is_none() {
                captures |= ctx
                    .pointer_latest_pos()
                    .is_some_and(|p| canvas.contains(p) && layout.technical.contains(p));
                app.shape_properties.chrome_hits.push(layout.technical);
                return;
            }
            if let Some(selected_panel) = panel {
                if selected_panel == Panel::Brush {
                    captures |= brush_body(ui, layout.main, id, z, theme, enabled, app);
                } else if selected_panel == Panel::Layers {
                    let hits = app.image_layer_palette_body(ui, &layout, id, z, theme);
                    captures |= ctx
                        .pointer_latest_pos()
                        .is_some_and(|p| hits.iter().any(|rect| rect.contains(p)));
                    app.shape_properties.chrome_hits.extend(hits);
                } else {
                    sample =
                        app.shape_property_body(ui, layout.main, selected_panel, z, Some(&layout));
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
            }
        });

    if close_requested {
        app.apply_shape_preview(&ctx, true);
        app.shape_properties.panel = None;
    }
    if let Some(panel) = requested_panel {
        app.apply_shape_preview(&ctx, true);
        app.shape_properties.panel = Some(panel);
    }
    if let Some(gesture) = sample {
        app.start_property_desktop_sample(panel.unwrap_or(Panel::Fill), gesture);
    }
    if panel.is_some() && !captures {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.apply_shape_preview(&ctx, false);
            app.shape_properties.panel = None;
        } else if app.property_dismiss_press(&ctx).is_some_and(|press| !press) {
            app.apply_shape_preview(&ctx, true);
            app.shape_properties.panel = None;
        }
    }
    if captures && ctx.input(|i| i.pointer.any_pressed()) {
        app.board_align_eat_press = true;
    }
    captures
}

fn panel_kind(panel: Option<Panel>) -> chrome::ImagePanelKind {
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
        Panel::Filter => Icon::Filters,
        Panel::Brush => Icon::Brush,
        Panel::Layers => Icon::Pages,
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

fn brush_body(
    ui: &mut egui::Ui,
    rect: Rect,
    image: NodeId,
    zoom: f32,
    theme: atlas_shell::theme::Palette,
    enabled: bool,
    app: &mut SlateApp,
) -> bool {
    let size = chrome::canvas_scale::px(28.0, zoom);
    let gap = chrome::canvas_scale::px(18.0, zoom);
    let center = rect.center();
    let buttons = [
        (
            Rect::from_center_size(
                egui::pos2(center.x - (size + gap) * 0.5, center.y),
                egui::vec2(size, size),
            ),
            Icon::Brush,
            "Paint on this image",
            "board.tool.brush",
        ),
        (
            Rect::from_center_size(
                egui::pos2(center.x + (size + gap) * 0.5, center.y),
                egui::vec2(size, size),
            ),
            Icon::Select,
            "Lasso selection",
            "board.image.selection.lasso",
        ),
    ];
    let mut captures = false;
    for (index, (button, glyph, label, command)) in buttons.into_iter().enumerate() {
        let response = ui
            .interact(
                button,
                Id::new(("image_brush_action", image.0, index)),
                Sense::click(),
            )
            .on_hover_text(label);
        icons::paint(
            ui.painter(),
            button,
            glyph,
            if response.hovered() {
                theme.select
            } else {
                theme.ink
            },
        );
        if enabled && response.clicked() {
            app.dispatch(
                ui.ctx(),
                CommandId(command),
                (command == "board.image.selection.lasso").then(|| image.0.to_string()),
            );
        }
        captures |= response.hovered();
    }
    captures
}
