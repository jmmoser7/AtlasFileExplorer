use super::*;

impl SlateApp {
    pub(super) fn grasshopper_control_row(
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
    pub(super) fn grasshopper_color_editor(
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
}
