//! Grasshopper inputs in the native model viewer's property apron.

use super::*;
use crate::app::grasshopper::{is_grasshopper_plugin, EditPhase};
use scene::{GrasshopperControl, GrasshopperControlId, GrasshopperControlKind, GrasshopperValue};
use std::path::PathBuf;

const ROW: f32 = chrome::CORNER_HEIGHT;
const GAP: f32 = chrome::STACK_GAP;
const PANEL_INSET: f32 = 8.0;
const MAX_ROWS: usize = 7;

pub(super) struct Draft {
    node: NodeId,
    control: GrasshopperControlId,
    value: GrasshopperValue,
}

impl SlateApp {
    pub(crate) fn consume_grasshopper_plugin_drop(&mut self, files: &mut Vec<PathBuf>, at: Pos2) {
        let plugins: Vec<_> = files
            .iter()
            .filter(|path| is_grasshopper_plugin(path))
            .cloned()
            .collect();
        if plugins.is_empty() {
            return;
        }
        if let Some(id) = self.grasshopper_plugin_drop_target(at) {
            if self.grasshopper_drop_plugins(id, plugins) {
                files.retain(|path| !is_grasshopper_plugin(path));
            }
        }
    }

    fn grasshopper_plugin_drop_target(&self, at: Pos2) -> Option<NodeId> {
        if self.at_home
            || self.doc().view.active_view != slate_doc::ViewKind::Board
            || !self.canvas_rect.contains(at)
        {
            return None;
        }
        let props = &self.shape_properties;
        let apron = (props.tab == self.tab().id && props.panel == Some(Panel::Grasshopper))
            .then_some(props.grasshopper_drop_area)
            .flatten()
            .filter(|(id, area)| props.ids.as_slice() == [*id] && area.contains(at));
        let id = if let Some((id, _)) = apron {
            id
        } else if let Some(maximized) = self.portal_chrome.maximized {
            let (host, _) = self.portal_capture_maximized_host(maximized, self.canvas_rect)?;
            host.contains(at).then_some(maximized)?
        } else {
            let world = self.board_xf().s2w(at);
            let id = self.board_pick_node_with_locked(world.x, world.y, true)?;
            let node = self.doc().scene.node(id)?;
            node.rect
                .contains_rotated(world.x, world.y, node.rotation_deg)
                .then_some(id)?
        };
        let node = self.doc().scene.node(id)?;
        (!node.hidden && self.grasshopper_accepts_plugin_drop(id)).then_some(id)
    }

    fn grasshopper_expanded_control(&self) -> Option<&GrasshopperControl> {
        let node = self.shape_properties.ids.first().copied()?;
        let id = self.shape_properties.grasshopper_expanded.as_ref()?;
        self.grasshopper_control(node, id)
    }

    pub(super) fn grasshopper_color_heading_height(&self) -> f32 {
        if self
            .grasshopper_expanded_control()
            .is_some_and(|control| control.kind == GrasshopperControlKind::Color)
        {
            PANEL_INSET + ROW + GAP
        } else {
            0.0
        }
    }

    pub(super) fn grasshopper_editor_height(&self) -> f32 {
        let Some(node) = self.shape_properties.ids.first().copied() else {
            return ROW + 2.0 * PANEL_INSET;
        };
        let expanded = self.grasshopper_expanded_control();
        if expanded.is_some_and(|control| control.kind == GrasshopperControlKind::Color) {
            return self.grasshopper_color_heading_height() + chrome::FILL_HEIGHT;
        }
        let options = expanded.map_or(0, |control| control.options.len());
        let rows = (self.grasshopper_control_count(node) + options).min(MAX_ROWS);
        ROW + rows as f32 * (ROW + GAP) + 2.0 * PANEL_INSET
    }

    pub(super) fn grasshopper_editor(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        image_layout: Option<&chrome::ImagePresentationLayout>,
        zoom: f32,
        theme: atlas_shell::theme::Palette,
    ) {
        let Some(node) = self.shape_properties.ids.first().copied() else {
            return;
        };
        let state = self.grasshopper_panel(node);
        let content = rect.shrink(PANEL_INSET * zoom);
        let top = Rect::from_min_size(content.min, Vec2::new(content.width(), ROW * zoom));
        let body_top = top.bottom() + GAP * zoom;
        if let Some(control) = state.as_ref().and_then(|state| {
            state.controls.iter().find(|control| {
                control.kind == GrasshopperControlKind::Color
                    && self.shape_properties.grasshopper_expanded.as_ref() == Some(&control.id)
            })
        }) {
            let body = Rect::from_min_max(Pos2::new(rect.left(), body_top), rect.max);
            self.grasshopper_color_editor(ui, node, control, top, body, image_layout, zoom, theme);
            return;
        }

        let body = Rect::from_min_max(Pos2::new(content.left(), body_top), content.max);
        self.grasshopper_actions(ui, node, top, state.as_ref(), zoom, theme);
        let Some(state) = state.filter(|state| !state.controls.is_empty()) else {
            return;
        };
        if !body.is_positive() {
            return;
        }
        let scroll_id = ui.id().with(("grasshopper-inputs", node.0));
        let mut rows = ui.new_child(egui::UiBuilder::new().id_salt(scroll_id).max_rect(body));
        rows.set_clip_rect(body.intersect(ui.clip_rect()));
        rows.spacing_mut().item_spacing.y = GAP * zoom;
        let scroll = egui::ScrollArea::vertical()
            .id_salt(scroll_id)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .min_scrolled_height(0.0)
            .max_height(body.height())
            .auto_shrink([false, false])
            .show(&mut rows, |ui| {
                ui.set_width(body.width());
                for control in &state.controls {
                    ui.push_id(control.id.as_str(), |ui| {
                        self.grasshopper_control_row(ui, node, control, zoom, theme);
                    });
                }
            });
        chrome::generator::scroll::paint(
            ui,
            scroll_id,
            body,
            scroll.content_size.y,
            scroll.state.offset.y,
            zoom,
            theme,
        );
        atlas_shell::menu_wheel::claim_on_layer(
            ui.ctx(),
            body.intersect(ui.clip_rect()),
            ui.layer_id(),
        );
    }

    fn grasshopper_actions(
        &mut self,
        ui: &egui::Ui,
        node: NodeId,
        top: Rect,
        state: Option<&crate::app::grasshopper::PanelState>,
        zoom: f32,
        theme: atlas_shell::theme::Palette,
    ) {
        let drop = self.grasshopper_accepts_plugin_drop(node);
        let retry = state.is_some_and(|state| state.can_retry);
        let bake = state.is_some_and(|state| state.can_bake)
            && self.shape_properties.grasshopper_draft.is_none();
        let status = state.and_then(|state| state.status.as_deref());
        let action = if retry {
            Some("Retry")
        } else if bake
            || state.is_some_and(|state| !state.controls.is_empty() || state.status.is_none())
        {
            Some("Bake")
        } else {
            None
        };
        let width = (72.0 * zoom).min(top.width() * 0.32);
        let action_rect = Rect::from_min_max(Pos2::new(top.right() - width, top.top()), top.max);
        if let Some(action) = action {
            if capsule(
                ui,
                action_rect,
                action,
                None,
                false,
                !(retry || bake),
                zoom,
                theme,
            )
            .clicked()
            {
                if retry {
                    self.grasshopper_retry(node);
                } else {
                    self.dispatch_grasshopper_bake(Some(&node.0.to_string()));
                }
                ui.ctx().request_repaint();
            }
        }
        let label = if drop {
            Some("Drop plug-ins")
        } else {
            status
                .or_else(|| {
                    state
                        .filter(|state| state.controls.is_empty() && !bake)
                        .map(|_| "No named inputs")
                })
                .or_else(|| state.is_none().then_some("Loading Grasshopper…"))
        };
        if let Some(label) = label {
            let right = if action.is_some() {
                action_rect.left() - GAP * zoom
            } else {
                top.right()
            };
            let info = Rect::from_min_max(top.min, Pos2::new(right, top.bottom()));
            let response = capsule(ui, info, label, None, false, true, zoom, theme);
            if drop {
                response.on_hover_text(format!(
                    "Drop .gha or .yak files on the viewer or palette.\n{}",
                    status.unwrap_or_default()
                ));
            } else {
                response.on_hover_text(label);
            }
        }
    }

    fn grasshopper_control_row(
        &mut self,
        ui: &mut egui::Ui,
        node: NodeId,
        control: &GrasshopperControl,
        zoom: f32,
        theme: atlas_shell::theme::Palette,
    ) {
        let rect = allocate_row(ui, zoom);
        let editing = self
            .shape_properties
            .grasshopper_draft
            .as_ref()
            .filter(|draft| draft.node == node && draft.control == control.id);
        let active_draft = editing.is_some();
        let value = editing
            .map(|draft| &draft.value)
            .or(control.value.as_ref())
            .cloned();
        let expanded = self.shape_properties.grasshopper_expanded.as_ref() == Some(&control.id);
        let change = match (control.kind, value) {
            (GrasshopperControlKind::Slider, Some(GrasshopperValue::Number(value))) => {
                if let Some((min, max)) = control
                    .min
                    .zip(control.max)
                    .filter(|(min, max)| min.is_finite() && max.is_finite() && min < max)
                {
                    let track = labeled_control(ui, rect, &control.name, zoom, theme);
                    let edit = chrome::scalar_capsule(
                        ui,
                        track,
                        value,
                        min..=max,
                        control.step,
                        control.precision,
                        zoom,
                        theme,
                    );
                    match (edit.value, edit.scrubbing, active_draft) {
                        (Some(value), true, _) => {
                            Some((GrasshopperValue::Number(value), EditPhase::Preview))
                        }
                        (Some(value), false, _) => {
                            Some((GrasshopperValue::Number(value), EditPhase::Commit))
                        }
                        (None, false, true) => {
                            Some((GrasshopperValue::Number(value), EditPhase::Commit))
                        }
                        _ => None,
                    }
                } else {
                    let value = value.to_string();
                    capsule(
                        ui,
                        rect,
                        &control.name,
                        Some(&value),
                        false,
                        true,
                        zoom,
                        theme,
                    );
                    None
                }
            }
            (GrasshopperControlKind::Toggle, Some(GrasshopperValue::Bool(enabled))) => {
                let track = labeled_control(ui, rect, &control.name, zoom, theme);
                let toggle = Rect::from_min_max(
                    Pos2::new(
                        track.right() - (98.0 * zoom).min(track.width()),
                        track.top(),
                    ),
                    track.max,
                );
                let next = chrome::segments(
                    ui,
                    toggle,
                    ui.id().with("toggle"),
                    ["Off", "On"],
                    usize::from(enabled),
                    zoom,
                    theme,
                ) == 1;
                (next != enabled).then_some((GrasshopperValue::Bool(next), EditPhase::Commit))
            }
            (GrasshopperControlKind::Text, Some(GrasshopperValue::Text(mut text))) => {
                let track = labeled_control(ui, rect, &control.name, zoom, theme);
                let mut field =
                    ui.new_child(egui::UiBuilder::new().id_salt("text").max_rect(track));
                field.set_clip_rect(track.intersect(ui.clip_rect()));
                let response = field.add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(track.width())
                        .frame(false)
                        .margin(egui::Margin::ZERO)
                        .font(egui::FontId::proportional(10.0 * zoom)),
                );
                let enter =
                    response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if (response.lost_focus() || enter) && (response.changed() || active_draft) {
                    response.surrender_focus();
                    Some((GrasshopperValue::Text(text), EditPhase::Commit))
                } else {
                    if response.changed() {
                        self.shape_properties.grasshopper_draft = Some(Draft {
                            node,
                            control: control.id.clone(),
                            value: GrasshopperValue::Text(text),
                        });
                    }
                    None
                }
            }
            (GrasshopperControlKind::List, Some(GrasshopperValue::Choice(mut selected))) => {
                let title = choice_title(control, &selected);
                let chip: String = title.chars().take(28).collect();
                if capsule(
                    ui,
                    rect,
                    &control.name,
                    Some(&chip),
                    expanded,
                    false,
                    zoom,
                    theme,
                )
                .on_hover_text(format!("{} · {title}", control.name))
                .clicked()
                {
                    self.toggle_grasshopper_control(control, expanded);
                }
                if self.shape_properties.grasshopper_expanded.as_ref() == Some(&control.id) {
                    self.grasshopper_choices(ui, control, &mut selected, zoom, theme)
                } else {
                    None
                }
            }
            (GrasshopperControlKind::Color, Some(GrasshopperValue::Color(rgba))) => {
                if color_capsule(ui, rect, &control.name, rgba, zoom, theme).clicked() {
                    self.toggle_grasshopper_control(control, expanded);
                }
                None
            }
            _ => {
                capsule(
                    ui,
                    rect,
                    &control.name,
                    Some("Read only"),
                    false,
                    true,
                    zoom,
                    theme,
                )
                .on_hover_text(
                    control
                        .status
                        .as_deref()
                        .unwrap_or("This input is read-only."),
                );
                None
            }
        };
        if let Some((value, phase)) = change {
            self.apply_grasshopper_control_edit(node, control, value, phase);
        }
    }

    fn toggle_grasshopper_control(&mut self, control: &GrasshopperControl, expanded: bool) {
        self.close_grasshopper_draft();
        self.shape_properties.grasshopper_expanded = (!expanded).then(|| control.id.clone());
    }

    fn grasshopper_choices(
        &mut self,
        ui: &mut egui::Ui,
        control: &GrasshopperControl,
        selected: &mut Vec<String>,
        zoom: f32,
        theme: atlas_shell::theme::Palette,
    ) -> Option<(GrasshopperValue, EditPhase)> {
        for option in &control.options {
            let mut rect = allocate_row(ui, zoom);
            rect.min.x += 12.0 * zoom;
            let index = selected.iter().position(|selected| *selected == option.id);
            let clicked = ui
                .push_id(&option.id, |ui| {
                    capsule(
                        ui,
                        rect,
                        &option.label,
                        None,
                        index.is_some(),
                        false,
                        zoom,
                        theme,
                    )
                    .on_hover_text(&option.label)
                    .clicked()
                })
                .inner;
            if clicked {
                if !control.multiple {
                    selected.clear();
                    selected.push(option.id.clone());
                    self.shape_properties.grasshopper_expanded = None;
                } else if let Some(index) = index {
                    selected.remove(index);
                } else {
                    selected.push(option.id.clone());
                }
                return Some((
                    GrasshopperValue::Choice(std::mem::take(selected)),
                    EditPhase::Commit,
                ));
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn grasshopper_color_editor(
        &mut self,
        ui: &mut egui::Ui,
        node: NodeId,
        control: &GrasshopperControl,
        top: Rect,
        body: Rect,
        image_layout: Option<&chrome::ImagePresentationLayout>,
        zoom: f32,
        theme: atlas_shell::theme::Palette,
    ) {
        let draft = self
            .shape_properties
            .grasshopper_draft
            .as_ref()
            .filter(|draft| draft.node == node && draft.control == control.id);
        let active_draft = draft.is_some();
        let Some(GrasshopperValue::Color(mut rgba)) = draft
            .map(|draft| &draft.value)
            .or(control.value.as_ref())
            .cloned()
        else {
            return;
        };
        if color_capsule(ui, top, &format!("‹  {}", control.name), rgba, zoom, theme)
            .on_hover_text("Back to Grasshopper inputs")
            .clicked()
        {
            self.toggle_grasshopper_control(control, true);
            return;
        }
        let recent = self.doc().view.recent_color_swatches();
        let edit = ui
            .push_id(control.id.as_str(), |ui| {
                if let Some(layout) = image_layout {
                    chrome::image_color_editor(
                        ui,
                        layout,
                        rgba,
                        None,
                        false,
                        recent.as_slice(),
                        &mut self.shape_properties.color,
                        zoom,
                        theme,
                    )
                } else {
                    chrome::color_editor(
                        ui,
                        body,
                        rgba,
                        None,
                        false,
                        recent.as_slice(),
                        &mut self.shape_properties.color,
                        zoom,
                        theme,
                    )
                }
            })
            .inner;
        if let Some(gesture) = edit.sample {
            self.start_grasshopper_desktop_sample(node, control.id.clone(), rgba[3], gesture);
        }
        let changed = edit.rgb.is_some() || edit.alpha.is_some();
        if let Some(rgb) = edit.rgb {
            rgba[..3].copy_from_slice(&rgb);
        }
        if let Some(alpha) = edit.alpha {
            rgba[3] = alpha;
        }
        let holding = ui.input(|input| input.pointer.primary_down());
        if changed || (active_draft && !holding) {
            let phase = if holding {
                EditPhase::Preview
            } else {
                EditPhase::Commit
            };
            self.apply_grasshopper_control_edit(
                node,
                control,
                GrasshopperValue::Color(rgba),
                phase,
            );
            if matches!(phase, EditPhase::Commit) {
                self.remember_document_colors(vec![[rgba[0], rgba[1], rgba[2]]]);
            }
        }
        atlas_shell::menu_wheel::claim_on_layer(
            ui.ctx(),
            body.intersect(ui.clip_rect()),
            ui.layer_id(),
        );
    }

    fn apply_grasshopper_control_edit(
        &mut self,
        node: NodeId,
        control: &GrasshopperControl,
        value: GrasshopperValue,
        phase: EditPhase,
    ) {
        self.shape_properties.grasshopper_draft =
            matches!(phase, EditPhase::Preview).then(|| Draft {
                node,
                control: control.id.clone(),
                value: value.clone(),
            });
        self.grasshopper_edit(node, control.id.clone(), value, phase);
    }

    pub(super) fn finish_grasshopper_draft(&mut self, commit: bool) {
        if let Some(draft) = self.shape_properties.grasshopper_draft.take() {
            if self.shape_properties.tab == self.tab().id {
                let phase = if commit {
                    EditPhase::Commit
                } else {
                    EditPhase::Cancel
                };
                self.grasshopper_edit(draft.node, draft.control, draft.value, phase);
            }
        }
    }

    pub(super) fn close_grasshopper_draft(&mut self) {
        let commit_text = self
            .shape_properties
            .grasshopper_draft
            .as_ref()
            .is_some_and(|draft| matches!(draft.value, GrasshopperValue::Text(_)));
        self.finish_grasshopper_draft(commit_text);
    }
}

fn allocate_row(ui: &mut egui::Ui, zoom: f32) -> Rect {
    ui.allocate_exact_size(
        Vec2::new(ui.available_width(), ROW * zoom),
        egui::Sense::hover(),
    )
    .0
}

#[allow(clippy::too_many_arguments)]
fn capsule(
    ui: &egui::Ui,
    rect: Rect,
    label: &str,
    chip: Option<&str>,
    selected: bool,
    dim: bool,
    zoom: f32,
    theme: atlas_shell::theme::Palette,
) -> egui::Response {
    let style = chrome::Capsule {
        label,
        chip,
        selected,
        dim,
        ..Default::default()
    };
    if dim {
        chrome::paint_capsule_row(
            &chrome::object_painter(ui),
            rect,
            &style,
            false,
            false,
            zoom,
            theme,
        );
        ui.interact(rect, ui.id().with(label), egui::Sense::hover())
    } else {
        chrome::capsule(ui, ui.id().with(label), rect, &style, zoom, theme).response
    }
}

fn labeled_control(
    ui: &egui::Ui,
    rect: Rect,
    label: &str,
    zoom: f32,
    theme: atlas_shell::theme::Palette,
) -> Rect {
    let painter = chrome::object_painter(ui);
    chrome::paint_capsule_row(
        &painter,
        rect,
        &Default::default(),
        false,
        false,
        zoom,
        theme,
    );
    let track_left = rect.left() + rect.width() * 0.36;
    let label_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 16.0 * zoom, rect.top()),
        Pos2::new(track_left - GAP * zoom, rect.bottom()),
    );
    atlas_shell::canvas_text::text(
        &painter.with_clip_rect(label_rect.intersect(painter.clip_rect())),
        label_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(9.0 * zoom),
        theme.ink,
    );
    ui.interact(label_rect, ui.id().with("label"), egui::Sense::hover())
        .on_hover_text(label);
    Rect::from_min_max(
        Pos2::new(track_left, rect.top() + 4.0 * zoom),
        rect.max - Vec2::new(8.0, 4.0) * zoom,
    )
}

fn color_capsule(
    ui: &egui::Ui,
    rect: Rect,
    label: &str,
    [r, g, b, a]: [u8; 4],
    zoom: f32,
    theme: atlas_shell::theme::Palette,
) -> egui::Response {
    let chip = format!("#{r:02X}{g:02X}{b:02X}");
    let response = capsule(ui, rect, label, Some(&chip), false, false, zoom, theme);
    let painter = chrome::object_painter(ui);
    let center = rect.left_center() + Vec2::new(9.0 * zoom, 0.0);
    painter.circle(
        center,
        3.4 * zoom,
        egui::Color32::from_rgba_unmultiplied(r, g, b, a),
        egui::Stroke::new(0.6 * zoom, theme.border_strong),
    );
    response
}

fn choice_title(control: &GrasshopperControl, selected: &[String]) -> String {
    match selected {
        [] => "Select…".into(),
        [id] => control
            .options
            .iter()
            .find(|option| option.id == *id)
            .map_or_else(|| "Select…".into(), |option| option.label.clone()),
        many => format!("{} selected", many.len()),
    }
}
