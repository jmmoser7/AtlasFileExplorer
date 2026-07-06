//! Dynamic Selection inspector — the sidebar panel that reshapes itself
//! around whatever is selected on the board (shapes, images, text, frames).
//!
//! Every edit funnels through `SlateApp::patch_nodes`, so inspector changes
//! are invertible commands: continuous slider scrubs coalesce into single
//! undo steps, and the same command surface will later back the MCP agent.

use super::super::board::to_rgba;
use super::super::chrome::ToolPanel;
use super::super::SlateApp;
use atlas_shell::sidebar::{
    sidebar_region, sidebar_section, sidebar_slider_block, sidebar_subtle_divider, SidebarTheme,
};
use atlas_shell::widgets::thin_sidebar_slider;
use eframe::egui::{self, Color32, Id, RichText};
use slate_doc::scene::{Corner, Dash, FontChoice, Node, NodeKind, Rgba, TextAlign};
use slate_doc::{NodeId, ViewKind};

fn rgba32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.0[0], c.0[1], c.0[2], c.0[3])
}

pub fn selection_panel(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let n = app.board_sel.len();
    let subtitle = match n {
        0 => None,
        1 => Some("1 object"),
        _ => Some("multiple"),
    };
    let mut expanded = app.tab().chrome.tool_expanded(ToolPanel::Selection);
    if sidebar_section(
        ui,
        Id::new("slate_selection"),
        "Selection",
        subtitle,
        &mut expanded,
        theme,
        |ui| body(app, ui, theme),
    ) {
        app.tab_mut()
            .chrome
            .set_tool_expanded(ToolPanel::Selection, expanded);
    }
}

fn body(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    if app.doc().view.active_view != ViewKind::Board {
        ui.label(
            RichText::new("Switch to the Board view to edit objects.")
                .small()
                .color(theme.sub),
        );
        return;
    }
    // Selection in z-order, primary = first.
    let ids: Vec<NodeId> = app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| app.board_sel.contains(&n.id))
        .map(|n| n.id)
        .collect();
    if ids.is_empty() {
        ui.label(
            RichText::new("Nothing selected — click objects on the board.")
                .small()
                .color(theme.sub),
        );
        return;
    }
    let primary = app.doc().scene.node(ids[0]).cloned().unwrap();

    // ----- common: opacity + actions -----
    sidebar_slider_block(ui, |ui| {
        let mut pct = (primary.opacity * 100.0).round() as usize;
        if thin_sidebar_slider(
            ui,
            &mut pct,
            0..=100,
            "Opacity",
            "%",
            "Whole-object opacity (CSS `opacity`)",
            theme.sub,
        ) {
            let v = pct as f32 / 100.0;
            app.patch_nodes(&ids, |n| n.opacity = v);
        }
    });
    ui.horizontal(|ui| {
        if ui.button(RichText::new("Duplicate").small()).clicked() {
            app.duplicate_board_nodes(&ids, 24.0, 24.0);
        }
        if ui.button(RichText::new("Delete").small()).clicked() {
            app.delete_board_nodes(&ids);
        }
    });

    // Homogeneous-kind sections. Mixed selections keep just the common block.
    let kinds: Vec<&'static str> = ids
        .iter()
        .filter_map(|id| app.doc().scene.node(*id).map(|n| n.kind.kind_name()))
        .collect();
    let uniform = kinds.windows(2).all(|w| w[0] == w[1]);
    if !uniform {
        sidebar_subtle_divider(ui, theme);
        ui.label(
            RichText::new("Mixed selection — opacity and actions only.")
                .small()
                .color(theme.sub),
        );
        return;
    }

    match &primary.kind {
        NodeKind::Shape(s) => {
            let is_line = s.shape == slate_doc::scene::ShapeKind::Line;
            if !is_line {
                sidebar_subtle_divider(ui, theme);
                fill_section(app, ui, theme, &ids, &primary);
            }
            sidebar_subtle_divider(ui, theme);
            stroke_section(app, ui, theme, &ids, &primary);
            if s.shape == slate_doc::scene::ShapeKind::Rect {
                sidebar_subtle_divider(ui, theme);
                corner_section(app, ui, theme, &ids, &primary);
            }
        }
        NodeKind::Image(img) => {
            let kind = app
                .doc()
                .item(img.item)
                .map(|it| slate_doc::media_kind(&it.path))
                .unwrap_or(slate_doc::MediaKind::Other);
            sidebar_subtle_divider(ui, theme);
            stroke_section(app, ui, theme, &ids, &primary);
            sidebar_subtle_divider(ui, theme);
            corner_section(app, ui, theme, &ids, &primary);
            if kind == slate_doc::MediaKind::Model {
                // 3D viewports: the camera pose is the framing — crop and
                // pixel adjustments don't apply (in either renderer).
                sidebar_subtle_divider(ui, theme);
                model_section(app, ui, theme, &primary);
            } else {
                sidebar_subtle_divider(ui, theme);
                crop_section(app, ui, theme, &ids, &primary);
                sidebar_subtle_divider(ui, theme);
                adjust_section(app, ui, theme, &ids, &primary);
                if kind == slate_doc::MediaKind::Video {
                    sidebar_subtle_divider(ui, theme);
                    video_section(app, ui, theme, &ids, &primary);
                }
            }
        }
        NodeKind::Text(_) => {
            sidebar_subtle_divider(ui, theme);
            text_section(app, ui, theme, &ids, &primary);
        }
        NodeKind::Frame(_) => {
            sidebar_subtle_divider(ui, theme);
            frame_section(app, ui, theme, &ids, &primary);
        }
    }
}

// ---------- shared style sections ----------

fn stroke_of(node: &Node) -> Option<slate_doc::scene::Stroke> {
    match &node.kind {
        NodeKind::Shape(s) => Some(s.stroke),
        NodeKind::Image(i) => Some(i.stroke),
        _ => None,
    }
}

fn set_stroke(node: &mut Node, stroke: slate_doc::scene::Stroke) {
    match &mut node.kind {
        NodeKind::Shape(s) => s.stroke = stroke,
        NodeKind::Image(i) => i.stroke = stroke,
        _ => {}
    }
}

fn corner_of(node: &Node) -> Option<Corner> {
    match &node.kind {
        NodeKind::Shape(s) => Some(s.corner),
        NodeKind::Image(i) => Some(i.corner),
        _ => None,
    }
}

fn set_corner(node: &mut Node, corner: Corner) {
    match &mut node.kind {
        NodeKind::Shape(s) => s.corner = corner,
        NodeKind::Image(i) => i.corner = corner,
        _ => {}
    }
}

fn fill_section(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    theme: SidebarTheme,
    ids: &[NodeId],
    primary: &Node,
) {
    let NodeKind::Shape(s) = &primary.kind else {
        return;
    };
    let mut fill = s.fill;
    sidebar_region(ui, "Fill", theme, |ui| {
        ui.horizontal(|ui| {
            let mut on = fill.is_some();
            if ui
                .checkbox(&mut on, RichText::new("Filled").small())
                .changed()
            {
                fill = if on {
                    Some(Rgba([120, 144, 156, 200]))
                } else {
                    None
                };
                let f = fill;
                app.patch_nodes(ids, move |n| {
                    if let NodeKind::Shape(s) = &mut n.kind {
                        s.fill = f;
                    }
                });
            }
            if let Some(c) = fill {
                let mut col = rgba32(c);
                if ui.color_edit_button_srgba(&mut col).changed() {
                    let f = Some(to_rgba(col));
                    app.patch_nodes(ids, move |n| {
                        if let NodeKind::Shape(s) = &mut n.kind {
                            s.fill = f;
                        }
                    });
                }
            }
        });
    });
}

fn stroke_section(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    theme: SidebarTheme,
    ids: &[NodeId],
    primary: &Node,
) {
    let Some(stroke) = stroke_of(primary) else {
        return;
    };
    let title = if matches!(primary.kind, NodeKind::Image(_)) {
        "Outline"
    } else {
        "Stroke"
    };
    sidebar_region(ui, title, theme, |ui| {
        sidebar_slider_block(ui, |ui| {
            let mut w = stroke.width.round() as usize;
            if thin_sidebar_slider(
                ui,
                &mut w,
                0..=24,
                "Width",
                "px",
                "0 = no stroke",
                theme.sub,
            ) {
                let mut s = stroke;
                s.width = w as f32;
                app.patch_nodes(ids, move |n| set_stroke(n, s));
            }
        });
        ui.horizontal(|ui| {
            let mut col = rgba32(stroke.color);
            if ui.color_edit_button_srgba(&mut col).changed() {
                let mut s = stroke;
                s.color = to_rgba(col);
                app.patch_nodes(ids, move |n| set_stroke(n, s));
            }
            let mut dash = stroke.dash;
            egui::ComboBox::from_id_salt(("stroke_dash", ids[0].0))
                .selected_text(match dash {
                    Dash::Solid => "Solid",
                    Dash::Dashed => "Dashed",
                    Dash::Dotted => "Dotted",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut dash, Dash::Solid, "Solid");
                    ui.selectable_value(&mut dash, Dash::Dashed, "Dashed");
                    ui.selectable_value(&mut dash, Dash::Dotted, "Dotted");
                });
            if dash != stroke.dash {
                let mut s = stroke;
                s.dash = dash;
                app.patch_nodes(ids, move |n| set_stroke(n, s));
            }
        });
    });
}

fn corner_section(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    theme: SidebarTheme,
    ids: &[NodeId],
    primary: &Node,
) {
    let Some(corner) = corner_of(primary) else {
        return;
    };
    sidebar_region(ui, "Corners", theme, |ui| {
        let (mode, amount) = match corner {
            Corner::Square => (0usize, 0.0),
            Corner::Rounded { radius } => (1, radius),
            Corner::Chamfer { cut } => (2, cut),
        };
        let mut new_mode = mode;
        ui.horizontal(|ui| {
            for (i, label, hint) in [
                (0usize, "Square", "Plain corners"),
                (1, "Round", "border-radius"),
                (2, "Chamfer", "Cut (jammed) corners"),
            ] {
                if ui
                    .selectable_label(new_mode == i, RichText::new(label).small())
                    .on_hover_text(hint)
                    .clicked()
                {
                    new_mode = i;
                }
            }
        });
        let mut new_amount = amount;
        if new_mode != 0 {
            sidebar_slider_block(ui, |ui| {
                let mut a = amount.round() as usize;
                if thin_sidebar_slider(
                    ui,
                    &mut a,
                    0..=120,
                    if new_mode == 1 { "Radius" } else { "Cut" },
                    "px",
                    "Corner size in world units",
                    theme.sub,
                ) {
                    new_amount = a as f32;
                }
            });
        }
        if new_mode != mode || new_amount != amount {
            let c = match new_mode {
                1 => Corner::Rounded {
                    radius: if new_mode != mode && new_amount == 0.0 {
                        12.0
                    } else {
                        new_amount
                    },
                },
                2 => Corner::Chamfer {
                    cut: if new_mode != mode && new_amount == 0.0 {
                        12.0
                    } else {
                        new_amount
                    },
                },
                _ => Corner::Square,
            };
            app.patch_nodes(ids, move |n| set_corner(n, c));
        }
    });
}

// ---------- image sections ----------

fn crop_section(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    theme: SidebarTheme,
    ids: &[NodeId],
    primary: &Node,
) {
    let NodeKind::Image(img) = &primary.kind else {
        return;
    };
    let crop = img.crop;
    sidebar_region(ui, "Crop", theme, |ui| {
        let mut c = crop;
        let mut changed = false;
        sidebar_slider_block(ui, |ui| {
            let mut w = (c.w * 100.0).round() as usize;
            if thin_sidebar_slider(
                ui,
                &mut w,
                5..=100,
                "Width",
                "%",
                "Visible width of the source",
                theme.sub,
            ) {
                c.w = w as f32 / 100.0;
                changed = true;
            }
            let mut h = (c.h * 100.0).round() as usize;
            if thin_sidebar_slider(
                ui,
                &mut h,
                5..=100,
                "Height",
                "%",
                "Visible height of the source",
                theme.sub,
            ) {
                c.h = h as f32 / 100.0;
                changed = true;
            }
            let mut x = (c.x * 100.0).round() as usize;
            if thin_sidebar_slider(
                ui,
                &mut x,
                0..=95,
                "Pan X",
                "%",
                "Crop window offset",
                theme.sub,
            ) {
                c.x = x as f32 / 100.0;
                changed = true;
            }
            let mut y = (c.y * 100.0).round() as usize;
            if thin_sidebar_slider(
                ui,
                &mut y,
                0..=95,
                "Pan Y",
                "%",
                "Crop window offset",
                theme.sub,
            ) {
                c.y = y as f32 / 100.0;
                changed = true;
            }
        });
        if !crop.is_full() && ui.button(RichText::new("Reset crop").small()).clicked() {
            c = slate_doc::scene::Crop::full();
            changed = true;
        }
        if changed {
            let c = c.clamped();
            app.patch_nodes(ids, move |n| {
                if let NodeKind::Image(i) = &mut n.kind {
                    i.crop = c;
                }
            });
        }
    });
}

fn adjust_section(
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
    sidebar_region(ui, "Adjust", theme, |ui| {
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
        // Hue can be negative; use a plain slider for it.
        ui.horizontal(|ui| {
            ui.label(RichText::new("Hue").small().color(theme.sub));
            let mut hue = a.hue_deg;
            if ui
                .add(egui::Slider::new(&mut hue, -180.0..=180.0).suffix("°"))
                .changed()
            {
                a.hue_deg = hue;
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
    });
}

// ---------- 3D model ----------

/// Viewport controls for a placed 3D model. The saved camera pose is
/// document state (journaled on lock); the board shows a live render while
/// unlocked and the frozen-camera poster while locked — the artifact
/// exports that same poster.
fn model_section(app: &mut SlateApp, ui: &mut egui::Ui, theme: SidebarTheme, primary: &Node) {
    let id = primary.id;
    sidebar_region(ui, "3D viewport", theme, |ui| {
        let live = app.model3d.live.contains_key(&id);
        let status = if live {
            "Live — drag to orbit, Shift+drag to pan, scroll to zoom. \
             Auto-locks after 30 s idle."
        } else {
            "Locked — showing the saved camera angle as a still. \
             Unlock to orbit the model."
        };
        ui.label(RichText::new(status).small().color(theme.sub));
        ui.horizontal(|ui| {
            if live {
                if ui
                    .button(RichText::new("🔒 Lock viewport").small())
                    .on_hover_text("Freeze this camera angle as the node's image")
                    .clicked()
                {
                    app.lock_model(id);
                }
            } else if ui
                .button(RichText::new("🔓 Unlock viewport").small())
                .on_hover_text("Load the model and orbit it in place")
                .clicked()
            {
                app.unlock_model(id);
            }
            if ui
                .button(RichText::new("Reset view").small())
                .on_hover_text("Back to the auto-fit three-quarter view")
                .clicked()
            {
                app.reset_model_camera(id);
            }
        });
        if let NodeKind::Image(img) = &primary.kind {
            let cam = img.model;
            let deg = |r: f32| r.to_degrees();
            ui.label(
                RichText::new(if cam.distance > 0.0 {
                    format!(
                        "Saved pose · yaw {:.0}° · pitch {:.0}° · distance {:.1}",
                        deg(cam.yaw),
                        deg(cam.pitch),
                        cam.distance
                    )
                } else {
                    format!(
                        "Saved pose · yaw {:.0}° · pitch {:.0}° · auto-fit",
                        deg(cam.yaw),
                        deg(cam.pitch)
                    )
                })
                .small()
                .color(theme.sub),
            );
        }
        ui.label(
            RichText::new(
                "Duplicate this node (Ctrl+D) and orbit each copy to keep \
                 several perspectives of one model across slides.",
            )
            .small()
            .color(theme.sub),
        );
    });
}

// ---------- video ----------

/// Playback + time-trim controls. Spatial cropping is the shared Crop
/// section above; everything here maps to `<video>` attributes and a
/// media-fragment trim window in the artifact. The board itself shows the
/// poster frame (with a ▶ badge) — playback happens in the exported HTML.
fn video_section(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    theme: SidebarTheme,
    ids: &[NodeId],
    primary: &Node,
) {
    let NodeKind::Image(img) = &primary.kind else {
        return;
    };
    let v = img.video;
    sidebar_region(ui, "Video", theme, |ui| {
        let mut nv = v;
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label(RichText::new("Trim start").small().color(theme.sub));
            let mut start = nv.start;
            if ui
                .add(
                    egui::DragValue::new(&mut start)
                        .range(0.0..=86_400.0)
                        .speed(0.1)
                        .suffix(" s"),
                )
                .changed()
            {
                nv.start = start;
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            let mut has_end = nv.end.is_some();
            if ui
                .checkbox(&mut has_end, RichText::new("Trim end").small())
                .on_hover_text("Unchecked = play to the end of the file")
                .changed()
            {
                nv.end = if has_end { Some(nv.start + 5.0) } else { None };
                changed = true;
            }
            if let Some(end) = nv.end {
                let mut e = end;
                if ui
                    .add(
                        egui::DragValue::new(&mut e)
                            .range(0.0..=86_400.0)
                            .speed(0.1)
                            .suffix(" s"),
                    )
                    .changed()
                {
                    nv.end = Some(e);
                    changed = true;
                }
            }
        });

        ui.horizontal_wrapped(|ui| {
            for (label, value, hint) in [
                ("Autoplay", &mut nv.autoplay, "Start when the slide shows"),
                ("Loop", &mut nv.looped, "Repeat the trimmed window"),
                ("Muted", &mut nv.muted, "Required for browser autoplay"),
                ("Controls", &mut nv.controls, "Show the player bar"),
            ] {
                if ui
                    .checkbox(value, RichText::new(label).small())
                    .on_hover_text(hint)
                    .changed()
                {
                    changed = true;
                }
            }
        });
        if nv.autoplay && !nv.muted {
            ui.label(
                RichText::new("Browsers block unmuted autoplay — keep Muted on.")
                    .small()
                    .color(theme.sub),
            );
        }

        if changed {
            let nv = nv.clamped();
            app.patch_nodes(ids, move |n| {
                if let NodeKind::Image(i) = &mut n.kind {
                    i.video = nv;
                }
            });
        }
        ui.label(
            RichText::new("Playback happens in the exported artifact; the board shows the poster.")
                .small()
                .color(theme.sub),
        );
    });
}

// ---------- text ----------

fn text_section(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    theme: SidebarTheme,
    ids: &[NodeId],
    primary: &Node,
) {
    let NodeKind::Text(t) = &primary.kind else {
        return;
    };
    sidebar_region(ui, "Text", theme, |ui| {
        ui.horizontal(|ui| {
            let mut family = t.family;
            egui::ComboBox::from_id_salt(("text_family", ids[0].0))
                .selected_text(family.label())
                .show_ui(ui, |ui| {
                    for f in FontChoice::ALL {
                        ui.selectable_value(&mut family, f, f.label());
                    }
                });
            if family != t.family {
                app.patch_nodes(ids, move |n| {
                    if let NodeKind::Text(t) = &mut n.kind {
                        t.family = family;
                    }
                });
            }
            let mut col = rgba32(t.color);
            if ui.color_edit_button_srgba(&mut col).changed() {
                let c = to_rgba(col);
                app.patch_nodes(ids, move |n| {
                    if let NodeKind::Text(t) = &mut n.kind {
                        t.color = c;
                    }
                });
            }
        });
        sidebar_slider_block(ui, |ui| {
            let mut size = t.size.round() as usize;
            if thin_sidebar_slider(
                ui,
                &mut size,
                8..=144,
                "Size",
                "pt",
                "Font size in world units",
                theme.sub,
            ) {
                let s = size as f32;
                app.patch_nodes(ids, move |n| {
                    if let NodeKind::Text(t) = &mut n.kind {
                        t.size = s;
                    }
                });
            }
        });
        ui.horizontal(|ui| {
            for (align, label) in [
                (TextAlign::Left, "⯇ left"),
                (TextAlign::Center, "◆ center"),
                (TextAlign::Right, "right ⯈"),
            ] {
                if ui
                    .selectable_label(t.align == align, RichText::new(label).small())
                    .clicked()
                {
                    app.patch_nodes(ids, move |n| {
                        if let NodeKind::Text(t) = &mut n.kind {
                            t.align = align;
                        }
                    });
                }
            }
        });
        ui.label(
            RichText::new("Double-click the text on the board to edit it.")
                .small()
                .color(theme.sub),
        );
    });
}

// ---------- frame ----------

fn frame_section(
    app: &mut SlateApp,
    ui: &mut egui::Ui,
    theme: SidebarTheme,
    ids: &[NodeId],
    primary: &Node,
) {
    let NodeKind::Frame(f) = &primary.kind else {
        return;
    };
    sidebar_region(ui, "Frame", theme, |ui| {
        let mut title = f.title.clone();
        if ui
            .add(egui::TextEdit::singleline(&mut title).desired_width(ui.available_width()))
            .changed()
        {
            let t = title.clone();
            app.patch_nodes(ids, move |n| {
                if let NodeKind::Frame(f) = &mut n.kind {
                    f.title = t.clone();
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Background").small().color(theme.sub));
            let mut col = rgba32(f.fill);
            if ui.color_edit_button_srgba(&mut col).changed() {
                let c = to_rgba(col);
                app.patch_nodes(ids, move |n| {
                    if let NodeKind::Frame(f) = &mut n.kind {
                        f.fill = c;
                    }
                });
            }
        });
        sidebar_subtle_divider(ui, theme);
        ui.label(
            RichText::new("Frame tags (inherited by dropped images)")
                .small()
                .color(theme.sub),
        );
        app.frame_tags_menu(ui, ids[0]);
        sidebar_subtle_divider(ui, theme);
        ui.horizontal(|ui| {
            if ui
                .button(RichText::new("▶ Present from here").small())
                .clicked()
            {
                app.start_present(Some(ids[0]));
            }
        });
    });
}
