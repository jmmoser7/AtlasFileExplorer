//! Schedule entry, without a second date parser or scheduling engine.
use crate::theme::Palette;
use eframe::egui::{self, RichText, Vec2};

#[derive(Default)]
pub struct ScheduleEditor {
    pub open: bool,
    pub id: String,
    pub date: String,
    pub time: String,
    pub end_date: String,
    pub end_time: String,
    /// once | hourly | daily | weekly; interpreted by atlas-ai's schedule owner.
    pub repeat: String,
    pub prompt: String,
    pub error: String,
    pub existing: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScheduleAction {
    Save,
    Stop,
}
pub fn schedule_editor_ui(
    ctx: &egui::Context,
    palette: &Palette,
    state: &mut ScheduleEditor,
) -> Option<ScheduleAction> {
    let mut action = None;
    let mut open = state.open;
    if !open {
        return None;
    }
    if state.repeat.is_empty() {
        state.repeat = "once".into();
    }
    egui::Window::new("Schedule agent").id(egui::Id::new("presence-schedule-editor"))
        .open(&mut open).collapsible(false).resizable(false).default_width(350.0)
        .frame(egui::Frame::window(&ctx.style()).fill(palette.card).corner_radius(20).inner_margin(18))
        .show(ctx,|ui| {
            ui.label(RichText::new("While this board is open").size(16.0));
            ui.label(RichText::new("Runs only with this agent's live board access. Retiring or revoking the agent stops its schedule.").size(11.0).color(palette.sub));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {ui.label("Date");ui.add(egui::TextEdit::singleline(&mut state.date).hint_text("YYYY-MM-DD").desired_width(165.0));});
                ui.vertical(|ui| {ui.label("Local time");ui.add(egui::TextEdit::singleline(&mut state.time).hint_text("HH:MM").desired_width(95.0));});
            });
            ui.add_space(10.0);
            egui::ComboBox::from_id_salt("presence-schedule-repeat").selected_text(match state.repeat.as_str(){"hourly"=>"Every hour","daily"=>"Every day","weekly"=>"Every week",_=>"Once"}).width(ui.available_width()).show_ui(ui,|ui| {
                for (id,label) in [("once","Once"),("hourly","Every hour"),("daily","Every day"),("weekly","Every week")] {ui.selectable_value(&mut state.repeat,id.into(),label);}
            });
            ui.add_space(12.0);ui.label("Message");
            ui.add(egui::TextEdit::multiline(&mut state.prompt).hint_text("What should the agent do?").desired_rows(4).desired_width(ui.available_width()));
            ui.add_space(10.0);
            ui.label("Stops on");
            ui.small("Every schedule has an end date and local time.");
            ui.scope(|ui| {
                ui.horizontal(|ui| {ui.add(egui::TextEdit::singleline(&mut state.end_date).hint_text("YYYY-MM-DD").desired_width(165.0));ui.add(egui::TextEdit::singleline(&mut state.end_time).hint_text("HH:MM").desired_width(95.0));});
            });
            if !state.error.is_empty(){ui.label(RichText::new(&state.error).small().color(palette.danger));}
            ui.add_space(12.0);
            if ui.add_enabled(!state.prompt.trim().is_empty()&&!state.end_date.trim().is_empty()&&!state.end_time.trim().is_empty(),egui::Button::new(if state.existing{"Update schedule"}else{"Save schedule"}).min_size(Vec2::new(ui.available_width(),35.0))).clicked(){action=Some(ScheduleAction::Save);}
            if state.existing&&ui.button("Stop schedule").clicked(){action=Some(ScheduleAction::Stop);}
        });
    state.open = open;
    action
}
