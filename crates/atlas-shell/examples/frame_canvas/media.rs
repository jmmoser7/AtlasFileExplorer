//! Deterministic media inputs for the presentation policy specimen.
use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Override {
    #[default]
    Inherit,
    Off,
    On,
}

impl Override {
    fn resolve(self, inherited: bool) -> bool {
        match self {
            Self::Inherit => inherited,
            Self::Off => false,
            Self::On => true,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Policy {
    start: Override,
    repeat: Override,
    last_view: bool,
    saved: Option<u8>,
    interactive: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            start: Override::Inherit,
            repeat: Override::Inherit,
            last_view: false,
            saved: None,
            interactive: true,
        }
    }
}

#[derive(Default)]
pub(super) struct Media {
    policies: HashMap<NodeId, Policy>,
    last: HashMap<NodeId, u8>,
    pub current: Option<Playback>,
}

pub(super) struct Playback {
    pub id: NodeId,
    pub playing: bool,
    pub repeat: bool,
    pub view: u8,
    pub interactive: bool,
    pub yaw: i32,
}

impl Media {
    pub fn leave(&mut self) {
        if let Some(media) = self.current.take() {
            self.last.insert(media.id, media.view);
        }
    }

    pub fn enter(&mut self, id: NodeId) {
        if self.current.as_ref().is_some_and(|media| media.id == id) {
            return;
        }
        self.leave();
        let p = self.policies.get(&id).copied().unwrap_or_default();
        let saved = p.saved.unwrap_or(1);
        self.current = Some(Playback {
            id,
            playing: p.start.resolve(true),
            repeat: p.repeat.resolve(true),
            view: if p.last_view {
                self.last.get(&id).copied().unwrap_or(saved)
            } else {
                saved
            },
            interactive: p.interactive,
            yaw: 0,
        });
    }

    pub fn panel(&mut self, ui: &mut Ui, id: NodeId, rect: Rect, z: f32, theme: Palette) {
        let last = self.last.get(&id).copied().unwrap_or(1);
        let policy = self.policies.entry(id).or_default();
        let col = (rect.width() / z - 114.0) / 2.0;
        let widget = |row, col_index| {
            Rect::from_min_size(
                rect.min + vec2(96.0 + col_index * (col + 5.0), 12.0 + row * 39.0) * z,
                vec2(col, 29.0) * z,
            )
        };
        for (row, title) in ["Video", "Webpages", "3D models"].into_iter().enumerate() {
            canvas_text::text(
                ui.painter(),
                pos2(rect.left() + 12.0 * z, widget(row as f32, 0.0).center().y),
                Align2::LEFT_CENTER,
                title,
                canvas_scale::font(12.0, z),
                theme.ink,
            );
        }
        ui.scope(|ui| {
            ui.style_mut().override_font_id = Some(canvas_scale::font(12.0, z));
            let mut dropdown = |key: &str, row, col, selected: &str| {
                let r = widget(row, col);
                let mut response = None;
                ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
                    let menu = egui::ComboBox::from_id_salt((id.0, key))
                        .width(r.width())
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            let options = match key {
                                "start" => &[
                                    "Start · Media setting",
                                    "Start · On click",
                                    "Start · Automatically",
                                ][..],
                                "repeat" => {
                                    &["Loop · Media setting", "Loop · Off", "Loop · On"][..]
                                }
                                _ => &["Saved view", "Last view"][..],
                            };
                            for (i, opt) in options.iter().enumerate() {
                                if ui.selectable_label(*opt == selected, *opt).clicked() {
                                    response = Some(i);
                                }
                            }
                        });
                    let _ = menu;
                });
                response
            };
            let start = [
                "Start · Media setting",
                "Start · On click",
                "Start · Automatically",
            ];
            let repeat = ["Loop · Media setting", "Loop · Off", "Loop · On"];
            let choices = [Override::Inherit, Override::Off, Override::On];
            if let Some(i) = dropdown("start", 0.0, 0.0, start[policy.start as usize]) {
                policy.start = choices[i];
            }
            if let Some(i) = dropdown("repeat", 0.0, 1.0, repeat[policy.repeat as usize]) {
                policy.repeat = choices[i];
            }
            if let Some(i) = dropdown(
                "view",
                1.0,
                0.0,
                if policy.last_view {
                    "Last view"
                } else {
                    "Saved view"
                },
            ) {
                policy.last_view = i == 1;
            }
            if frame::control(
                ui,
                widget(1.0, 1.0),
                Id::new(("save-view", id.0)),
                "Save current view",
                z,
                theme,
                false,
            )
            .clicked()
            {
                policy.saved = Some(last);
            }
            ui.put(
                widget(2.0, 0.0),
                egui::Checkbox::new(&mut policy.interactive, "Interactive"),
            );
        });
    }

    pub fn playback(&mut self, ui: &mut Ui, theme: Palette) {
        let Some(media) = &mut self.current else {
            return;
        };
        let r = ui.max_rect();
        let panel = Rect::from_min_size(
            r.left_bottom() + vec2(12.0, -35.0),
            vec2(r.width() - 24.0, 27.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(panel), |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(theme.sub, "Media fixture");
                if ui
                    .button(if media.playing {
                        "Pause video"
                    } else {
                        "Play video"
                    })
                    .clicked()
                {
                    media.playing = !media.playing;
                }
                ui.label(if media.repeat { "Loop on" } else { "Loop off" });
                if ui.button(format!("Web view {}", media.view)).clicked() {
                    media.view = media.view % 3 + 1;
                }
                if ui
                    .add_enabled(
                        media.interactive,
                        egui::Button::new(format!("Rotate model · {}°", media.yaw)),
                    )
                    .clicked()
                {
                    media.yaw = (media.yaw + 15) % 360;
                }
            });
        });
    }
}
