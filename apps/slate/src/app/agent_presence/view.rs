use super::*;
use atlas_shell::selection_tools::generator::activity;
use slate_doc::scene::NodeKind;

#[derive(Deserialize)]
struct Catalog {
    activities: Vec<Activity>,
}
#[derive(Deserialize)]
struct Activity {
    id: String,
    label: String,
    expression: String,
    orientation: String,
    prop: String,
}
fn catalog() -> &'static Catalog {
    static DATA: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../../crates/atlas-agent/assets/agent-activities.json"
        ))
        .expect("activity master")
    })
}
fn pose(activity: &str) -> &'static Activity {
    catalog()
        .activities
        .iter()
        .find(|a| a.id == activity)
        .unwrap_or(&catalog().activities[0])
}
fn direction(delta: egui::Vec2) -> &'static str {
    let x = if delta.x.abs() < 10.0 {
        0
    } else if delta.x < 0.0 {
        -1
    } else {
        1
    };
    let y = if delta.y.abs() < 10.0 {
        0
    } else if delta.y < 0.0 {
        -1
    } else {
        1
    };
    match (x, y) {
        (-1, -1) => "up-left",
        (0, -1) => "up",
        (1, -1) => "up-right",
        (-1, 0) => "left",
        (1, 0) => "right",
        (-1, 1) => "down-left",
        (0, 1) => "down",
        (1, 1) => "down-right",
        _ => "forward",
    }
}

impl SlateApp {
    pub(crate) fn paint_presence(
        &mut self,
        painter: &egui::Painter,
        xf: &super::super::board::BoardXf,
    ) {
        let reduced =
            self.settings.presence_reduced_motion || chrome::system_reduced_motion(painter.ctx());
        let tab = self.tab().id;
        let now = painter.ctx().input(|i| i.time);
        let portal = if self
            .presence
            .agents
            .iter()
            .any(|a| a.tab == tab && a.departure.as_ref().is_some_and(|d| d.1 == "portal"))
        {
            self.doc()
                .scene
                .query_rect(self.board_paint_view(self.canvas_rect))
                .into_iter()
                .filter_map(|id| self.doc().scene.node(id))
                .filter(|n| matches!(n.kind, NodeKind::Portal(_)))
                .map(|n| xf.rect_w2s(n.rect))
                .find(|r| r.intersects(self.canvas_rect))
        } else {
            None
        };
        for a in self.presence.agents.iter().filter(|a| a.tab == tab) {
            let sample = self.presence.roster.session(&a.motion).map(|s| s.sample());
            let position = sample
                .map(|s| s.position)
                .or_else(|| a.departure.as_ref().map(|d| d.2));
            let Some(position) = position else {
                continue;
            };
            let center = xf.w2s(egui::pos2(position.x as f32, position.y as f32));
            let addressed =
                self.presence.chrome.canvas_chat() == Some(a.profile.id.as_str()) && a.paused;
            let pose = pose(if addressed { "listening" } else { &a.activity });
            let target_rect = a
                .work
                .as_ref()
                .and_then(|w| w.prepared.targets.first().copied())
                .or(a.observed_target)
                .and_then(|id| {
                    self.presence_preview(id)
                        .map(|node| node.rect)
                        .or_else(|| self.doc().scene.node(id).map(|node| node.rect))
                });
            let gaze = match pose.orientation.as_str() {
                "target" => target_rect
                    .map(|rect| {
                        let (x, y) = rect.center();
                        direction(xf.w2s(egui::pos2(x, y)) - center)
                    })
                    .unwrap_or("forward"),
                "toward-human" => {
                    direction(self.canvas_rect.right_top() + egui::vec2(-40.0, 24.0) - center)
                }
                "up" => "up",
                "down" => "down",
                "scan" if !reduced => {
                    if (now * 0.7).sin() < 0.0 {
                        "left"
                    } else {
                        "right"
                    }
                }
                _ => "forward",
            };
            let visual = chrome::AvatarVisual {
                id: a.motion.as_str(),
                center,
                expression: &pose.expression,
                orientation: gaze,
                hat: &a.profile.hat,
                prop: &pose.prop,
                opacity: 1.0,
            };
            let animated = if let Some((start, style, _)) = &a.departure {
                self.presence.painter.paint_world_departure(
                    painter,
                    visual,
                    style,
                    (start.elapsed().as_secs_f64() / chrome::departure_duration(style)).min(1.0)
                        as f32,
                    now,
                    reduced,
                    self.canvas_rect,
                    portal,
                    xf.z,
                )
            } else {
                self.presence
                    .painter
                    .paint_world_avatar(painter, visual, now, reduced, xf.z)
            };
            if animated {
                painter
                    .ctx()
                    .request_repaint_after(std::time::Duration::from_millis(16));
            }
            if self.canvas_rect.expand(-20.0).contains(center) && a.departure.is_none() {
                chrome::world_participant_name(
                    painter,
                    center,
                    &a.profile.name,
                    &self.palette(),
                    xf.z,
                    self.presence.chrome.font_family(),
                );
            }
        }
    }
    pub(super) fn presence_activity_window(&mut self, ctx: &egui::Context) {
        let Some(who) = self.presence.inspect.clone() else {
            return;
        };
        let Some(a) = self.presence.agents.iter().find(|a| a.profile.id == who) else {
            self.presence.inspect = None;
            return;
        };
        let title = format!("{} · public activity", a.profile.name);
        let snapshot = a.snapshot.clone();
        let status = a.status.clone();
        let palette = self.palette();
        let mut open = true;
        egui::Window::new(title).id(egui::Id::new("collaborator-conversation")).open(&mut open).default_width(420.0).show(ctx,|ui|{
            ui.label(status);
            if let Some(snapshot)=snapshot {
                egui::ScrollArea::vertical().max_height(280.0).show(ui,|ui|{for turn in snapshot.turns.iter().rev().take(32).collect::<Vec<_>>().into_iter().rev(){ui.strong(&turn.role);ui.label(&turn.text);ui.add_space(6.0);}});
                let entries:Vec<_>=snapshot.activity.iter().map(|e|activity::ActivityEntry{id:&e.id,label:match e.kind{atlas_agent::AgentActivityKind::Commentary=>"Message",atlas_agent::AgentActivityKind::ToolStarted=>"Tool started",atlas_agent::AgentActivityKind::ToolFinished=>"Tool finished"},text:&e.text}).collect();
                if !entries.is_empty(){self.presence.activity_view.fold=activity::CardFold::Open;let(rect,_)=ui.allocate_exact_size(egui::vec2(ui.available_width(),220.0),egui::Sense::hover());activity::paint(ui,rect,activity::ActivityView{entries:&entries,state:&mut self.presence.activity_view,running:matches!(snapshot.status,atlas_agent::AgentStatus::Thinking)},1.0,palette);}
            }else{ui.weak("Send a task from the participant capsule. Dot and other local sidecars connect through this session's file link.");}
        });
        if !open {
            self.presence.inspect = None;
        }
    }
    pub(crate) fn presence_advanced(&mut self, ui: &mut egui::Ui) {
        if chrome::presence_preferences_ui(ui, &mut self.settings.presence_reduced_motion) {
            self.settings.save();
        }
        let saved: Vec<_> = self
            .settings
            .collaborators
            .iter()
            .filter(|p| {
                !self
                    .presence
                    .agents
                    .iter()
                    .any(|a| a.profile.id == p.id && a.departure.is_none())
            })
            .cloned()
            .collect();
        let profiles: Vec<_> = saved
            .iter()
            .map(|p| chrome::RetiredProfile {
                id: &p.id,
                name: &p.name,
                hat: &p.hat,
                provider: &p.provider,
            })
            .collect();
        let actions = chrome::retired_profiles_ui(ui, &profiles);
        for action in actions {
            self.dispatch_presence_action(ui.ctx(), action);
        }
    }
}
