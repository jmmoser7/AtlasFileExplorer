//! Shared update UI. The renderer-free atlas-update crate owns the lifecycle.
use atlas_update::{State, Updater, RELEASES_URL};
use eframe::egui;

pub fn section(ui: &mut egui::Ui, updater: &Updater) -> bool {
    ui.label(egui::RichText::new("Software updates").strong());
    ui.label(format!("Version {} · {}", updater.version, updater.channel));
    ui.button("Check for updates…").clicked()
}

/// Actions are registered commands; app adapters route them through dispatch.
pub fn window(
    ctx: &egui::Context,
    updater: &mut Updater,
    close_blocked: Option<&str>,
) -> Option<&'static str> {
    updater.poll();
    if updater.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(150));
    }
    if !updater.visible {
        return None;
    }
    let mut action = None;
    egui::Window::new("Slate & File Atlas updates")
        .id(egui::Id::new("atlas-suite-updates"))
        .open(&mut updater.visible).collapsible(false).resizable(false).default_width(390.0)
        .show(ctx, |ui| {
            ui.label(format!("Installed: {} · {}", updater.version, updater.channel));
            match &updater.state {
                State::Uninstalled => { ui.label("This copy was built from source. Install the Windows setup from Releases to receive updates."); }
                State::Checking => { ui.spinner(); ui.label("Checking for updates…"); }
                State::Current => { ui.label("You have the latest version on this channel."); }
                State::Available { version, notes } => {
                    ui.label(format!("Version {version} is available."));
                    if !notes.is_empty() {
                        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| { ui.label(notes); });
                    }
                    if ui.button("Download update").clicked() { action = Some("app.updates.download"); }
                }
                State::Downloading => { ui.spinner(); ui.label("Downloading the update. You can keep working."); }
                State::Ready { version } => {
                    ui.label(format!("Version {version} is ready to install."));
                    ui.label("Installation waits until all Slate and File Atlas windows are closed, then reopens this app. Your workbooks and settings are kept.");
                    if let Some(reason) = close_blocked { ui.label(reason); }
                    if ui.add_enabled(close_blocked.is_none(), egui::Button::new("Update and restart")).clicked() {
                        action = Some("app.updates.install");
                    }
                }
                State::Scheduling => { ui.spinner(); ui.label("Preparing to restart…"); }
                State::Scheduled => {
                    ui.label("Update scheduled. Save your work and close any other Slate and File Atlas windows. Installation starts after the last window closes.");
                    if let Some(reason) = close_blocked { ui.label(reason); }
                }
                State::Failed(error) => {
                    ui.label("Could not finish the update. Your installed version is still usable.");
                    ui.label(error);
                    if ui.button("Try again").clicked() { action = Some("app.updates.check"); }
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.hyperlink_to("Releases and installers", RELEASES_URL);
                if !matches!(updater.state, State::Scheduling | State::Scheduled) && ui.button("Later").clicked() { action = Some("app.updates.later"); }
            });
        });
    action
}
