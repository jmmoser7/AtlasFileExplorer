//! Scheduled messages on a chat card: the dialog, the command, and the clock
//! in the card's top strip. Task registration and date parsing are
//! `atlas_ai::schedule`; this file only asks and shows.

use atlas_ai::schedule::{self as plan, AgentSchedule, Repeat};
use eframe::egui::{self, Id, Pos2, Rect, Sense};
use slate_doc::scene::NodeId;

use super::super::SlateApp;

/// What the person is typing into the schedule dialog.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScheduleDialog {
    pub portal: NodeId,
    pub date: String,
    pub time: String,
    pub repeat: Repeat,
    pub prompt: String,
    pub error: Option<String>,
    /// The card already has a schedule; the dialog offers Stop.
    pub existing: bool,
}

impl SlateApp {
    /// This card's conversation schedule, read once per session.
    pub(crate) fn agent_schedule(&mut self, id: NodeId) -> Option<AgentSchedule> {
        let (session, _) = self.agent_session_for(id)?;
        if let Some(known) = self.agents.schedules.get(&session) {
            return known.clone();
        }
        let ws = self.ai.config.workspace_dir.clone()?;
        let found = self
            .agent_link_dir(id, &ws)
            .and_then(|dir| plan::load(&dir));
        self.agents.schedules.insert(session, found.clone());
        found
    }

    /// The composer's text, else the last message sent on this card.
    fn schedule_prompt(&mut self, id: NodeId) -> String {
        let typed = self.agents.prompt_mut(id).trim().to_string();
        if !typed.is_empty() {
            return typed;
        }
        self.visible_agent_turns(id)
            .iter()
            .rev()
            .find(|t| t.role == "user")
            .map(|t| atlas_ai::agent::display_prompt(&t.text).trim().to_string())
            .unwrap_or_default()
    }

    /// `portal.agent.schedule`. Detail: `{"open":true}` opens the dialog;
    /// `{"stop":true}` removes the schedule; `{"start":"2026-10-01T04:55",
    /// "repeat":"daily","prompt":"…"}` registers one.
    pub(crate) fn agent_set_schedule(&mut self, detail: Option<&str>) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some((session, provider)) = self.agent_session_for(id) else {
            return false;
        };
        if !atlas_ai::runtime::linear_provider(&provider) {
            return false;
        }
        let value = detail
            .and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok())
            .unwrap_or_else(|| serde_json::json!({"open": true}));
        if value.get("open").and_then(|v| v.as_bool()) == Some(true) {
            self.open_schedule_dialog(id);
            return true;
        }
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.toast("Set an AI workspace before scheduling a message.");
            return false;
        };
        let Some(dir) = self.agent_link_dir(id, &ws) else {
            return false;
        };
        if value.get("stop").and_then(|v| v.as_bool()) == Some(true) {
            return match plan::unregister(&dir) {
                Ok(()) => {
                    self.agents.schedules.insert(session, None);
                    self.toast("This conversation is no longer scheduled.");
                    true
                }
                Err(e) => {
                    self.toast(format!("Could not remove the schedule: {e}"));
                    false
                }
            };
        }
        let Some(start) = value.get("start").and_then(|v| v.as_str()) else {
            return false;
        };
        let repeat = value
            .get("repeat")
            .and_then(|v| serde_json::from_value::<Repeat>(v.clone()).ok())
            .unwrap_or_default();
        let prompt = value
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| self.schedule_prompt(id));
        let schedule = AgentSchedule {
            start: start.to_string(),
            repeat,
            prompt,
            provider,
            cwd: self
                .agent_folder_for(id)
                .unwrap_or_else(|| ws.clone())
                .to_string_lossy()
                .into_owned(),
            ai_workspace: ws.to_string_lossy().into_owned(),
            model: self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .and_then(|a| a.model.clone()),
        };
        let Ok(exe) = std::env::current_exe() else {
            self.toast("Could not find Slate to schedule it.");
            return false;
        };
        match plan::register(&dir, &schedule, &exe) {
            Ok(()) => {
                self.toast(format!(
                    "{}. It runs while Slate is closed; this computer must be on.",
                    schedule.describe()
                ));
                self.agents.schedules.insert(session, Some(schedule));
                true
            }
            Err(e) => {
                if let Some(dialog) = self.agents.schedule_dialog.as_mut() {
                    dialog.error = Some(e.clone());
                } else {
                    self.toast(e);
                }
                false
            }
        }
    }

    fn open_schedule_dialog(&mut self, id: NodeId) {
        let existing = self.agent_schedule(id);
        let (date, time, repeat, prompt) = match &existing {
            Some(s) => {
                let t = s.start_time().unwrap_or_else(plan::now_local);
                (
                    t.format("%Y-%m-%d").to_string(),
                    t.format("%-I:%M %p").to_string(),
                    s.repeat,
                    s.prompt.clone(),
                )
            }
            None => (
                "tonight".into(),
                "1:00 AM".into(),
                Repeat::Once,
                self.schedule_prompt(id),
            ),
        };
        self.agents.schedule_dialog = Some(ScheduleDialog {
            portal: id,
            date,
            time,
            repeat,
            prompt,
            error: None,
            existing: existing.is_some(),
        });
    }

    /// The dialog, centered like other Slate confirmations.
    pub(crate) fn paint_schedule_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.agents.schedule_dialog.clone() else {
            return;
        };
        let now = plan::now_local();
        let parsed = plan::parse_when(&dialog.date, &dialog.time, now);
        let mut open = true;
        let mut action: Option<&str> = None;
        egui::Window::new("Schedule message")
            .id(Id::new(("agent-schedule", dialog.portal.0)))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label("Message");
                ui.add(
                    egui::TextEdit::multiline(&mut dialog.prompt)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    for (label, date, time) in [
                        ("Tonight 1:00 AM", "tonight", "1:00 AM"),
                        ("Tomorrow 9:00 AM", "tomorrow", "9:00 AM"),
                    ] {
                        if ui.button(label).clicked() {
                            dialog.date = date.into();
                            dialog.time = time.into();
                        }
                    }
                    if ui.button("In 1 hour").clicked() {
                        let t = plan::hours_from(now, 1);
                        dialog.date = t.format("%Y-%m-%d").to_string();
                        dialog.time = t.format("%-I:%M %p").to_string();
                    }
                });
                ui.add_space(6.0);
                egui::Grid::new(("agent-schedule-when", dialog.portal.0))
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Date");
                        ui.add(
                            egui::TextEdit::singleline(&mut dialog.date)
                                .hint_text("tonight, Oct 1, 2026-10-01"),
                        );
                        ui.end_row();
                        ui.label("Time");
                        ui.add(
                            egui::TextEdit::singleline(&mut dialog.time)
                                .hint_text("1am, 4:55 am, 13:30"),
                        );
                        ui.end_row();
                        ui.label("Repeat");
                        ui.horizontal(|ui| {
                            for r in Repeat::ALL {
                                ui.radio_value(&mut dialog.repeat, r, r.label());
                            }
                        });
                        ui.end_row();
                    });
                ui.add_space(6.0);
                let palette = self.palette();
                match &parsed {
                    Ok(t) => {
                        let preview = AgentSchedule {
                            start: plan::format_start(*t),
                            repeat: dialog.repeat,
                            prompt: String::new(),
                            provider: String::new(),
                            cwd: String::new(),
                            ai_workspace: String::new(),
                            model: None,
                        };
                        ui.label(egui::RichText::new(preview.describe()).color(palette.sub));
                    }
                    Err(e) => {
                        ui.label(egui::RichText::new(e.as_str()).color(palette.danger));
                    }
                }
                if let Some(error) = &dialog.error {
                    ui.label(egui::RichText::new(error.as_str()).color(palette.danger));
                }
                ui.label(
                    egui::RichText::new(
                        "Runs through Windows Task Scheduler while Slate is closed. This computer must be on.",
                    )
                    .small()
                    .color(palette.sub),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let ready = parsed.is_ok() && !dialog.prompt.trim().is_empty();
                    if ui.add_enabled(ready, egui::Button::new("Schedule")).clicked() {
                        action = Some("schedule");
                    }
                    if dialog.existing && ui.button("Stop schedule").clicked() {
                        action = Some("stop");
                    }
                    if ui.button("Cancel").clicked() {
                        action = Some("cancel");
                    }
                });
            });
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = Some("cancel");
        }
        let portal = dialog.portal;
        match action {
            Some("cancel") => {
                self.agents.schedule_dialog = None;
                return;
            }
            Some(kind) => {
                let detail = if kind == "stop" {
                    serde_json::json!({"stop": true})
                } else if let Ok(t) = &parsed {
                    serde_json::json!({
                        "start": plan::format_start(*t),
                        "repeat": dialog.repeat,
                        "prompt": dialog.prompt,
                    })
                } else {
                    serde_json::Value::Null
                };
                dialog.error = None;
                self.agents.schedule_dialog = Some(dialog);
                self.board_sel = std::iter::once(portal).collect();
                let done = self.dispatch(
                    ctx,
                    atlas_commands::CommandId("portal.agent.schedule"),
                    Some(detail.to_string()),
                );
                if done {
                    self.agents.schedule_dialog = None;
                }
            }
            None => self.agents.schedule_dialog = Some(dialog),
        }
    }

    /// A clock just left of the collapse chevron while a run is still ahead.
    /// Hover names when; a click opens the dialog.
    pub(crate) fn paint_agent_clock(
        &mut self,
        ui: &egui::Ui,
        id: NodeId,
        menu_left: f32,
        cy: f32,
        z: f32,
    ) -> bool {
        let Some(schedule) = self.agent_schedule(id) else {
            return false;
        };
        if !schedule.pending(plan::now_local()) {
            return false;
        }
        let size = atlas_shell::canvas_scale::px(11.0, z);
        let center = Pos2::new(menu_left - atlas_shell::canvas_scale::px(22.0, z), cy);
        let rect = Rect::from_center_size(center, egui::Vec2::splat(size));
        let response = ui
            .interact(
                rect.expand(atlas_shell::canvas_scale::px(2.0, z)),
                Id::new(("agent-clock", id.0)),
                Sense::click(),
            )
            .on_hover_text(schedule.describe());
        let ink = self.palette().ink;
        let color = if response.hovered() {
            ink
        } else {
            ink.gamma_multiply(0.72)
        };
        atlas_shell::icons::paint(ui.painter(), rect, atlas_shell::icons::Icon::Clock, color);
        response.clicked()
    }
}
