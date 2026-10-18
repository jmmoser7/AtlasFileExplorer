//! Interactive fixture of the production shell widgets. All backend messages
//! here are explicitly local simulation; shipping persistence/network lives in Slate.
use atlas_shell::{agent_presence as chrome, theme::Palette};
use eframe::egui::{self, Align2, Id, Pos2, Rect, Vec2};
use std::collections::HashMap;
#[path = "conversation_capture.rs"]
mod capture;

struct Person {
    id: String,
    name: String,
    hat: String,
    provider: String,
    human: bool,
    paused: bool,
}
struct Turn {
    id: String,
    text: String,
    outgoing: bool,
    system: bool,
}
pub struct Preview {
    painter: chrome::PresencePainter,
    state: chrome::RosterState,
    people: Vec<Person>,
    retired: Vec<Person>,
    threads: HashMap<String, Vec<Turn>>,
    drafts: HashMap<String, String>,
    zoom: f32,
    world: Pos2,
    busy: bool,
    queued: Option<String>,
    listening: bool,
    photo: Option<egui::TextureHandle>,
    connection: chrome::PeopleSetupState,
    schedule: chrome::ScheduleEditor,
    notice: String,
    capture: Option<String>,
    frames: u32,
}
impl Preview {
    pub fn new(capture: Option<String>) -> Self {
        let mut state = chrome::RosterState::default();
        state.open_chat("agent");
        let turns=[("hey hows it going?",true),("Hey! Going well—how’s it going with you?",false),("pretty well just testing out the interface",true),("Sounds good—happy to be your test conversation. How’s the interface feeling so far?",false)];
        Self {
            painter: Default::default(),
            state,
            people: vec![
                Person {
                    id: "self".into(),
                    name: "You".into(),
                    hat: "none".into(),
                    provider: String::new(),
                    human: true,
                    paused: false,
                },
                Person {
                    id: "agent".into(),
                    name: "Agent".into(),
                    hat: "cap".into(),
                    provider: "codex".into(),
                    human: false,
                    paused: false,
                },
            ],
            retired: Vec::new(),
            threads: HashMap::from([(
                "agent".into(),
                turns
                    .into_iter()
                    .enumerate()
                    .map(|(n, (text, outgoing))| Turn {
                        id: n.to_string(),
                        text: text.into(),
                        outgoing,
                        system: false,
                    })
                    .collect(),
            )]),
            drafts: HashMap::new(),
            zoom: 1.0,
            world: Pos2::new(340.0, 300.0),
            busy: false,
            queued: None,
            listening: false,
            photo: None,
            connection: Default::default(),
            schedule: Default::default(),
            notice: String::new(),
            capture,
            frames: 0,
        }
    }
    fn push(&mut self, id: &str, text: String, outgoing: bool, system: bool) {
        let turns = self.threads.entry(id.into()).or_default();
        let key = turns.len().to_string();
        turns.push(Turn {
            id: key,
            text,
            outgoing,
            system,
        });
    }
    fn apply(&mut self, ctx: &egui::Context, action: chrome::PresenceAction) {
        use chrome::PresenceAction::*;
        match action {
            Draft { id, text } => {
                self.drafts.insert(id, text);
            }
            Send { id, text } => {
                self.push(&id, text, true, false);
                self.drafts.insert(id.clone(), String::new());
                if id != "self" {
                    self.push(
                        &id,
                        "Local component fixture reply. No provider or person was contacted."
                            .into(),
                        false,
                        false,
                    );
                }
            }
            Rename { id, name } => {
                if let Some(p) = self.people.iter_mut().find(|p| p.id == id) {
                    p.name = name;
                }
            }
            SetHat { id, hat } => {
                if let Some(p) = self.people.iter_mut().find(|p| p.id == id) {
                    p.hat = hat;
                }
            }
            Pause { id, paused } => {
                if let Some(p) = self.people.iter_mut().find(|p| p.id == id) {
                    p.paused = paused;
                }
                self.listening = false;
            }
            Cancel { .. } => {
                self.busy = false;
                self.queued = None;
            }
            SetProvider { id, provider } => self.switch(&id, &provider),
            SwitchProvider {
                id,
                provider,
                after_task,
            } => {
                if after_task {
                    self.queued = Some(provider);
                } else {
                    self.busy = false;
                    self.switch(&id, &provider);
                }
            }
            CancelProviderSwitch { .. } => self.queued = None,
            Add {
                name,
                hat,
                provider,
            } => {
                let id = format!("fixture-{}", self.people.len());
                self.people.push(Person {
                    id,
                    name,
                    hat,
                    provider,
                    human: false,
                    paused: false,
                });
            }
            Retire { id, .. } => {
                if let Some(index) = self.people.iter().position(|p| p.id == id) {
                    self.retired.push(self.people.remove(index));
                }
                self.busy = false;
                self.notice="Retired in this component fixture; the shipping app preserves the retired profile.".into();
            }
            Restore { id } => {
                if let Some(index) = self.retired.iter().position(|p| p.id == id) {
                    self.people.push(self.retired.remove(index));
                }
            }
            Locate { .. } => {
                self.notice = "The board avatar is at the left of this conversation.".into()
            }
            ShowConversation { id } => self.state.open_chat(&id),
            ConnectPeople => self.connection.open = true,
            Schedule { id } => {
                self.schedule = chrome::ScheduleEditor {
                    open: true,
                    id,
                    date: "2026-10-17".into(),
                    time: "18:00".into(),
                    ..Default::default()
                };
            }
            SetPhoto { .. } => {
                let pixels: Vec<u8> = (0..64 * 64)
                    .flat_map(|n| {
                        let x = n % 64;
                        let y = n / 64;
                        let color = if (x as f32 - 32.0).hypot(y as f32 - 24.0) < 14.0 {
                            egui::Color32::from_rgb(184, 156, 128)
                        } else {
                            egui::Color32::from_rgb(219, 226, 208)
                        };
                        color.to_array()
                    })
                    .collect();
                self.photo = Some(ctx.load_texture(
                    "fixture-profile-photo",
                    egui::ColorImage::from_rgba_unmultiplied([64, 64], &pixels),
                    egui::TextureOptions::LINEAR,
                ));
            }
            other => self.notice = format!("Component fixture action: {other:?}"),
        }
    }
    fn switch(&mut self, id: &str, provider: &str) {
        if let Some(p) = self.people.iter_mut().find(|p| p.id == id) {
            if p.provider == provider {
                return;
            }
            p.provider = provider.into();
        }
        self.push(
            id,
            format!("Switched to {provider} · local component fixture"),
            false,
            true,
        );
    }
}
impl eframe::App for Preview {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let palette = Palette::light();
        ctx.set_visuals(palette.visuals());
        let now = ctx.input(|i| i.time);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(palette.bg).inner_margin(0))
            .show(ctx, |ui| {
                let canvas = ui.max_rect();
                for x in (0..canvas.width() as i32).step_by(22) {
                    for y in (0..canvas.height() as i32).step_by(22) {
                        ui.painter().circle_filled(
                            Pos2::new(x as f32, y as f32),
                            0.6,
                            palette.grid_dot,
                        );
                    }
                }
                for (n, label) in ["Reference images", "Presentation frame", "Notes"]
                    .into_iter()
                    .enumerate()
                {
                    let rect = Rect::from_min_size(
                        Pos2::new(70.0 + n as f32 * 145.0, 140.0 + n as f32 * 100.0),
                        Vec2::new(180.0, 120.0),
                    );
                    ui.painter().rect_filled(rect, 5, palette.card);
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(13.0),
                        palette.sub,
                    );
                }
                if let Some(index) = self.people.iter().position(|p| p.id == "agent") {
                    if self.busy && !self.people[index].paused {
                        self.world.x += ctx.input(|i| i.stable_dt) * 45.0;
                    }
                    let center = Pos2::new(self.world.x * self.zoom, self.world.y * self.zoom);
                    if self.state.canvas_chat().is_some() {
                        self.state.update_chat_anchor(
                            center + Vec2::new(-18.0, 42.0) * self.zoom,
                            self.zoom,
                        );
                    }
                    let avatar = Rect::from_center_size(center, Vec2::splat(44.0 * self.zoom));
                    let blocked = ctx
                        .input(|i| i.pointer.interact_pos())
                        .is_some_and(|p| self.state.occludes_point(ctx, p));
                    if !blocked
                        && ui
                            .interact(avatar, Id::new("fixture-board-agent"), egui::Sense::click())
                            .double_clicked()
                    {
                        self.people[index].paused = true;
                        self.listening = true;
                        self.state.open_chat_at(
                            "agent",
                            center + Vec2::new(-18.0, 42.0) * self.zoom,
                            self.zoom,
                        );
                    }
                    self.painter.paint_world_avatar(
                        ui.painter(),
                        chrome::AvatarVisual {
                            id: "fixture-board",
                            center,
                            expression: if self.listening {
                                "curious"
                            } else if self.busy {
                                "concentrating"
                            } else {
                                "neutral"
                            },
                            orientation: "forward",
                            hat: &self.people[index].hat,
                            prop: if self.listening {
                                "bubbles"
                            } else if self.busy {
                                "book"
                            } else {
                                "none"
                            },
                            opacity: 1.0,
                        },
                        now,
                        false,
                        self.zoom,
                    );
                    chrome::world_participant_name(
                        ui.painter(),
                        center,
                        &self.people[index].name,
                        &palette,
                        self.zoom,
                        self.state.font_family(),
                    );
                }
            });
        let messages: Vec<Vec<_>> = self
            .people
            .iter()
            .map(|p| {
                let turns = self.threads.get(&p.id).map_or(&[][..], Vec::as_slice);
                let range = if self.state.active_chat() == Some(p.id.as_str()) {
                    self.state.message_range(&p.id, turns.len())
                } else {
                    0..0
                };
                turns[range]
                    .iter()
                    .map(|t| chrome::ConversationMessage {
                        id: &t.id,
                        text: &t.text,
                        outgoing: t.outgoing,
                        system: t.system,
                        status: "",
                    })
                    .collect()
            })
            .collect();
        let participants: Vec<_> = self
            .people
            .iter()
            .enumerate()
            .map(|(i, p)| chrome::Participant {
                id: &p.id,
                name: &p.name,
                hat: &p.hat,
                status: "Native component fixture",
                provider: &p.provider,
                human: p.human,
                paused: p.paused,
                autonomous: false,
                file_inputs: false,
                own_chats: false,
                parent_name: None,
                chats: &[],
                local_profile: p.id == "self",
                photo_texture: if p.id == "self" {
                    self.photo
                        .as_ref()
                        .map(|t| egui::load::SizedTexture::new(t.id(), t.size_vec2()))
                } else {
                    None
                },
                busy: !p.human && self.busy,
                messages: &messages[i],
                message_total: self.threads.get(&p.id).map_or(0, Vec::len),
                draft: self.drafts.get(&p.id).map_or("", String::as_str),
                queued_provider: self.queued.as_deref(),
                model: "",
                models: &[],
            })
            .collect();
        let providers = [
            chrome::ProviderOption {
                id: "codex",
                label: "Codex",
            },
            chrome::ProviderOption {
                id: "cursor",
                label: "Cursor",
            },
            chrome::ProviderOption {
                id: "local",
                label: "Dot / local",
            },
        ];
        let mut actions = Vec::new();
        egui::Area::new(Id::new("fixture-participants"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(Pos2::new(ctx.screen_rect().right() - 22.0, 18.0))
            .show(ctx, |ui| {
                ui.set_max_width((ctx.screen_rect().width() - 44.0).min(620.0));
                actions = chrome::participant_capsules(
                    ui,
                    &palette,
                    &mut self.painter,
                    &mut self.state,
                    &participants,
                    &providers,
                    now,
                    false,
                );
            });
        for action in actions {
            self.apply(ctx, action);
        }
        egui::Area::new(Id::new("fixture-controls"))
            .fixed_pos(Pos2::new(20.0, ctx.screen_rect().bottom() - 105.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(palette.card)
                    .corner_radius(12)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Native component fixture · simulated backend")
                                .size(11.0)
                                .color(palette.sub),
                        );
                        ui.horizontal(|ui| {
                            for (label, zoom) in [("25%", 0.25), ("100%", 1.0), ("200%", 2.0)] {
                                if ui.selectable_label(self.zoom == zoom, label).clicked() {
                                    self.zoom = zoom;
                                }
                            }
                            ui.checkbox(&mut self.busy, "Running task");
                            if ui.small_button("Finish task").clicked() {
                                self.busy = false;
                                if let Some(provider) = self.queued.take() {
                                    self.switch("agent", &provider);
                                }
                            }
                        });
                        ui.label(
                            egui::RichText::new(&self.notice)
                                .size(10.0)
                                .color(palette.sub),
                        );
                        if !self.retired.is_empty() {
                            let profiles: Vec<_> = self
                                .retired
                                .iter()
                                .map(|p| chrome::RetiredProfile {
                                    id: &p.id,
                                    name: &p.name,
                                    hat: &p.hat,
                                    provider: &p.provider,
                                })
                                .collect();
                            for action in chrome::retired_profiles_ui(ui, &profiles) {
                                self.apply(ctx, action);
                            }
                        }
                    });
            });
        for _ in chrome::people_setup_ui(ctx, &palette, &mut self.connection) {
            self.connection.status = "Component fixture only. No network was contacted.".into();
        }
        if chrome::schedule_editor_ui(ctx, &palette, &mut self.schedule).is_some() {
            self.schedule.error =
                "Component fixture: schedule parsing and execution belong to Slate.".into();
        }
        self.capture(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}
