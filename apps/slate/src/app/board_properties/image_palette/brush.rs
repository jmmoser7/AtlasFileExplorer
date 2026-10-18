//! Brush parameter and color controls embedded in the selected-image blister.

use crate::app::{
    board::BoardTool,
    board_color::{BrushSettingUndo, DesktopDestination},
    SlateApp,
};
use atlas_commands::CommandId;
use atlas_shell::selection_tools as chrome;
use eframe::egui::{self, Id};
use slate_doc::{scene::Rgba, NodeId};

#[derive(Clone, Copy, Default)]
pub(super) struct BrushResponse {
    pub captures: bool,
    pub sample: Option<chrome::SampleGesture>,
}

#[derive(Clone, Copy)]
struct EditSession {
    before: BrushSettingUndo,
    remember_color: bool,
}

pub(super) fn body(
    ui: &mut egui::Ui,
    layout: &chrome::ImagePresentationLayout,
    image: NodeId,
    zoom: f32,
    theme: atlas_shell::theme::Palette,
    enabled: bool,
    app: &mut SlateApp,
) -> BrushResponse {
    let (_, _, opacity) = brush_tip(app);
    let mut captures = false;
    if let Some(header) = layout.brush_header {
        let key = Id::new(("image_brush_header", image.0));
        let mut layer_open = ui
            .ctx()
            .data(|data| data.get_temp::<bool>(key.with("layer_open")))
            .unwrap_or(false);
        let choices = app.image_paint_layer_choices(image);
        let layer_labels: Vec<&str> = choices.iter().map(|(_, name)| name.as_str()).collect();
        let active_layer = app
            .image_paint
            .as_ref()
            .filter(|session| session.image == image)
            .map_or(0, |session| session.layer_index);
        let response = chrome::image_brush_header(
            ui,
            header,
            chrome::ImageBrushEdit {
                id: key,
                enabled,
                brush_active: app.board_tool == BoardTool::Brush,
                lasso_active: app.image_lasso_active(image),
                layers: &layer_labels,
                active_layer,
                layer_open: &mut layer_open,
            },
            zoom,
            theme,
        );
        captures |= response.captures;
        app.shape_properties
            .chrome_hits
            .extend(chrome::image_brush_header_groups(header, zoom));
        app.occupy_property_popups(&response.popups);
        app.shape_properties
            .chrome_hits
            .extend(response.popups.iter().copied());
        ui.ctx()
            .data_mut(|data| data.insert_temp(key.with("layer_open"), layer_open));
        if enabled {
            if let Some(index) = response.layer {
                app.select_image_paint_layer(image, index);
            }
            if response.brush || response.lasso {
                app.board_sel.clear();
                app.board_sel.insert(image);
                if (response.brush && app.board_tool == BoardTool::Brush)
                    || (response.lasso && app.image_lasso_active(image))
                {
                    app.disarm_image_tool(image);
                } else {
                    app.dispatch(
                        ui.ctx(),
                        CommandId(if response.brush {
                            "board.tool.brush"
                        } else {
                            "board.image.selection.lasso"
                        }),
                        None,
                    );
                }
            }
        }
    }
    let recent = app.doc().view.recent_color_swatches();
    let foreground = app.board_colors.fg.0;
    let edit = chrome::image_color_editor(
        ui,
        layout,
        [
            foreground[0],
            foreground[1],
            foreground[2],
            (opacity * 255.0).round() as u8,
        ],
        None,
        false,
        recent.as_slice(),
        &mut app.shape_properties.color,
        zoom,
        theme,
    );

    let mut sample = edit.sample;
    if enabled && (edit.rgb.is_some() || edit.alpha.is_some() || edit.width.is_some()) {
        let key = edit_key(image);
        let mut session = ui.ctx().data(|data| data.get_temp::<EditSession>(key));
        if session.is_none() {
            session = Some(EditSession {
                before: app.brush_setting_snapshot(),
                remember_color: false,
            });
        }
        let mut session = session.expect("edit session initialized");
        if let Some(rgb) = edit.rgb {
            app.board_colors.fg.0[..3].copy_from_slice(&rgb);
            session.remember_color = true;
        }
        let (width, softness, mut opacity) = brush_tip(app);
        if let Some(value) = edit.alpha {
            opacity = value as f32 / 255.0;
        }
        set_brush_tip(app, width, softness, opacity);
        ui.ctx().data_mut(|data| data.insert_temp(key, session));
    }
    if !ui.input(|input| input.pointer.primary_down()) {
        finish_setting_edit(ui.ctx(), image, app);
    }

    if sample.is_some() && !enabled {
        sample = None;
    }
    BrushResponse { captures, sample }
}

pub(super) fn start_brush_sample(app: &mut SlateApp, gesture: chrome::SampleGesture) {
    let mode = match gesture {
        chrome::SampleGesture::Click => atlas_shell::desktop_color::PickMode::Click,
        chrome::SampleGesture::Drag => atlas_shell::desktop_color::PickMode::Drag,
    };
    app.begin_desktop_sample(
        DesktopDestination::Tool {
            background: false,
            alt_selects_background: false,
            brush_before: Some(app.brush_setting_snapshot()),
        },
        mode,
    );
}

pub(super) fn finish_setting_edit(ctx: &egui::Context, image: NodeId, app: &mut SlateApp) {
    let key = edit_key(image);
    let session = ctx.data(|data| data.get_temp::<EditSession>(key));
    ctx.data_mut(|data| data.remove::<EditSession>(key));
    let Some(session) = session else {
        return;
    };
    app.push_brush_setting_undo(session.before);
    app.settings.save();
    app.save_board_colors();
    if session.remember_color {
        let color = app.board_colors.fg.0;
        app.remember_document_colors(vec![[color[0], color[1], color[2]]]);
    }
}

pub(super) fn cancel_setting_edit(ctx: &egui::Context, image: NodeId, app: &mut SlateApp) {
    let key = edit_key(image);
    let session = ctx.data(|data| data.get_temp::<EditSession>(key));
    ctx.data_mut(|data| data.remove::<EditSession>(key));
    let Some(session) = session else {
        return;
    };
    restore_brush_setting(app, session.before);
}

pub(super) fn prepare_image(
    ctx: &egui::Context,
    image: NodeId,
    brush_open: bool,
    app: &mut SlateApp,
) {
    let menu_key = Id::new(("image_brush_header", image.0));
    if !brush_open {
        ctx.data_mut(|data| {
            data.insert_temp(menu_key.with("layer_open"), false);
        });
    }
    let image_key = Id::new("active_image_brush_edit");
    let open_key = Id::new("active_image_brush_panel");
    let previous_image = ctx.data(|data| data.get_temp::<NodeId>(image_key));
    let was_open = ctx
        .data(|data| data.get_temp::<bool>(open_key))
        .unwrap_or(false);
    if let Some(previous) = previous_image {
        if previous != image {
            finish_setting_edit(ctx, previous, app);
        } else if was_open && !brush_open {
            if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                cancel_setting_edit(ctx, image, app);
            } else {
                finish_setting_edit(ctx, image, app);
            }
        }
    }
    ctx.data_mut(|data| {
        data.insert_temp(image_key, image);
        data.insert_temp(open_key, brush_open);
    });
}

pub(super) fn dismiss_if_inactive(ctx: &egui::Context, live: bool, app: &mut SlateApp) {
    let image_key = Id::new("active_image_brush_edit");
    let open_key = Id::new("active_image_brush_panel");
    let previous_image = ctx.data(|data| data.get_temp::<NodeId>(image_key));
    let was_open = ctx
        .data(|data| data.get_temp::<bool>(open_key))
        .unwrap_or(false);
    let current_image = app.shape_properties.nodes.first().map(|node| node.id);
    let still_active = live
        && app.shape_properties.panel == Some(super::super::Panel::Brush)
        && current_image == previous_image;
    if was_open && !still_active {
        if let Some(image) = previous_image {
            if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                cancel_setting_edit(ctx, image, app);
            } else {
                finish_setting_edit(ctx, image, app);
            }
        }
        ctx.data_mut(|data| data.insert_temp(open_key, false));
    }
}

fn brush_tip(app: &SlateApp) -> (f32, f32, f32) {
    if app.board_tool == BoardTool::Brush {
        app.active_tip()
    } else {
        (app.brush_width, app.brush_softness, app.brush_opacity)
    }
}

fn set_brush_tip(app: &mut SlateApp, width: f32, softness: f32, opacity: f32) {
    if app.board_tool == BoardTool::Brush {
        app.set_active_tip(width, softness, opacity);
    } else {
        app.brush_width = width;
        app.brush_softness = softness;
        app.brush_opacity = opacity;
        app.settings.brush_width = width;
        app.settings.brush_softness = softness;
        app.settings.brush_opacity = opacity;
    }
}

fn restore_brush_setting(app: &mut SlateApp, before: BrushSettingUndo) {
    app.brush_width = before.width;
    app.brush_softness = before.softness;
    app.brush_opacity = before.opacity;
    app.eraser_width = before.eraser_width;
    app.eraser_softness = before.eraser_softness;
    app.eraser_opacity = before.eraser_opacity;
    app.brush_texture = before.brush_texture;
    app.eraser_texture = before.eraser_texture;
    app.board_colors.fg = Rgba(before.fg);
    app.settings.brush_width = before.width;
    app.settings.brush_softness = before.softness;
    app.settings.brush_opacity = before.opacity;
    app.settings.eraser_width = before.eraser_width;
    app.settings.eraser_softness = before.eraser_softness;
    app.settings.eraser_opacity = before.eraser_opacity;
    app.settings.board_fg = Some(before.fg);
}

fn edit_key(image: NodeId) -> Id {
    Id::new(("image_brush_setting_edit", image.0))
}
