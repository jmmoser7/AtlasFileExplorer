//! Connection setup is UI-only: the app owns transport, trust and invitations.
use crate::theme::Palette;
use eframe::egui::{self, RichText, Vec2};

pub struct PeopleSetupState {
    pub open: bool,
    pub bind: String,
    pub invite: String,
    pub status: String,
    pub host_invite: String,
}
impl Default for PeopleSetupState {
    fn default() -> Self {
        Self {
            open: false,
            bind: "127.0.0.1:0".into(),
            invite: String::new(),
            status: "Not connected".into(),
            host_invite: String::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum PeopleSetupAction {
    Host { bind: String },
    Join { invite: String },
    Leave,
    CopyInvite,
}
pub fn people_setup_ui(
    ctx: &egui::Context,
    palette: &Palette,
    state: &mut PeopleSetupState,
) -> Vec<PeopleSetupAction> {
    let mut actions = Vec::new();
    let mut open = state.open;
    if !open {
        return actions;
    }
    egui::Window::new("People on this board").id(egui::Id::new("presence-people-setup"))
        .open(&mut open).collapsible(false).resizable(false).default_width(360.0)
        .frame(egui::Frame::window(&ctx.style()).fill(palette.card).corner_radius(20).inner_margin(18))
        .show(ctx,|ui| {
            ui.label(RichText::new("Invite a person").size(18.0));
            ui.label(RichText::new("People appear beside your agents. Click a portrait to continue a direct conversation. Messages only; this connection does not share board edits. Your name and profile picture are visible to people in this room.").size(12.0).color(palette.sub));
            ui.add_space(14.0);
            if state.host_invite.is_empty() {
                if ui.add_sized(Vec2::new(ui.available_width(),36.0),egui::Button::new("Create an invitation")).clicked(){actions.push(PeopleSetupAction::Host{bind:state.bind.trim().into()});}
            } else {
                ui.add(egui::TextEdit::multiline(&mut state.host_invite).interactive(false).desired_rows(3).desired_width(ui.available_width()));
                if ui.button("Copy invitation").clicked(){actions.push(PeopleSetupAction::CopyInvite);}
            }
            ui.add_space(12.0);ui.separator();ui.add_space(10.0);
            ui.label(RichText::new("Join with an invitation").size(14.0));
            ui.add(egui::TextEdit::multiline(&mut state.invite).hint_text("Paste an invitation").desired_rows(3).desired_width(ui.available_width()));
            if ui.add_enabled(!state.invite.trim().is_empty(),egui::Button::new("Join conversation").min_size(Vec2::new(ui.available_width(),34.0))).clicked(){actions.push(PeopleSetupAction::Join{invite:state.invite.trim().into()});}
            ui.add_space(12.0);
            ui.label(RichText::new(&state.status).size(11.0).color(palette.sub));
            ui.add_space(6.0);
            egui::CollapsingHeader::new("Connection settings").show(ui,|ui| {
                ui.label("Listen address");ui.text_edit_singleline(&mut state.bind);
                ui.small("Use an address reachable by your collaborator.");
                if ui.small_button("Disconnect").clicked(){actions.push(PeopleSetupAction::Leave);}
            });
        });
    state.open = open;
    actions
}
