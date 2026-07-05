//! The AI sidebar panel — one implementation rendered by both apps so the
//! toolbar is identical in Atlas and Slate (shared-chrome rule).

use crate::config::AiConfig;
use crate::context::{now_secs, write_context, AiAppContext};
use crate::launch;
use atlas_shell::sidebar::{
    sidebar_region, sidebar_subtle_divider, sidebar_toolbar_row, SidebarTheme,
};
use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;
use std::time::Instant;

/// Minimum interval between context-beacon writes.
const BEACON_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Per-app AI panel state. Construct once, keep on the app struct, call
/// [`AiPanel::poll`] every frame and [`AiPanel::update_context`] whenever a
/// frame ends (it self-throttles).
pub struct AiPanel {
    pub config: AiConfig,
    cursor_available: bool,
    picker_tx: Sender<Option<PathBuf>>,
    picker_rx: Receiver<Option<PathBuf>>,
    picker_open: bool,
    /// Transient status line shown at the bottom of the panel.
    pub status: Option<String>,
    last_fingerprint: u64,
    last_beacon: Option<Instant>,
}

impl AiPanel {
    pub fn new() -> Self {
        let (picker_tx, picker_rx) = crossbeam_channel::unbounded();
        AiPanel {
            config: AiConfig::load(),
            cursor_available: launch::cursor_available(),
            picker_tx,
            picker_rx,
            picker_open: false,
            status: None,
            last_fingerprint: 0,
            last_beacon: None,
        }
    }

    /// True while the async folder picker is open — apps should keep
    /// repainting so [`AiPanel::poll`] sees the result promptly.
    pub fn picker_pending(&self) -> bool {
        self.picker_open
    }

    /// Drain the async folder picker. Call once per frame.
    pub fn poll(&mut self) {
        while let Ok(msg) = self.picker_rx.try_recv() {
            self.picker_open = false;
            if let Some(dir) = msg {
                match self.config.set_workspace(dir.clone()) {
                    Ok(()) => {
                        self.config.save();
                        self.status = Some(format!("AI workspace set: {}", dir.display()));
                        // Force a beacon rewrite into the new workspace.
                        self.last_fingerprint = 0;
                        self.last_beacon = None;
                    }
                    Err(e) => self.status = Some(format!("Could not use folder: {e}")),
                }
            }
        }
    }

    /// Open the async "establish AI workspace" folder picker.
    pub fn pick_workspace(&mut self) {
        if self.picker_open {
            return;
        }
        self.picker_open = true;
        let tx = self.picker_tx.clone();
        let start = self.config.workspace_dir.clone();
        std::thread::spawn(move || {
            let mut dlg = rfd::FileDialog::new().set_title("Choose the AI workspace folder");
            if let Some(d) = start {
                dlg = dlg.set_directory(d);
            }
            let _ = tx.send(dlg.pick_folder());
        });
    }

    /// Launch Cursor in the AI workspace. First launch requires the user to
    /// establish the folder, so we open the picker instead when unset.
    pub fn launch_cursor(&mut self) {
        let Some(ws) = self.config.valid_workspace().map(PathBuf::from) else {
            self.status = Some(
                "Choose an AI workspace folder first — it becomes Cursor's working directory."
                    .into(),
            );
            self.pick_workspace();
            return;
        };
        match launch::launch_cursor(&ws) {
            Ok(()) => self.status = Some("Cursor launched.".into()),
            Err(e) => {
                self.cursor_available = launch::cursor_available();
                self.status = Some(e);
            }
        }
    }

    /// Maintain the live-link beacon. `build` is only called when the
    /// throttle window has elapsed; the file is only rewritten when content
    /// actually changed.
    pub fn update_context(&mut self, build: impl FnOnce() -> AiAppContext) {
        let Some(ws) = self.config.valid_workspace().map(PathBuf::from) else {
            return;
        };
        if let Some(t) = self.last_beacon {
            if t.elapsed() < BEACON_INTERVAL {
                return;
            }
        }
        self.last_beacon = Some(Instant::now());
        let mut ctx = build();
        ctx.generated_at = now_secs();
        let fp = ctx.fingerprint();
        if fp == self.last_fingerprint {
            return;
        }
        if write_context(&ws, &ctx).is_ok() {
            self.last_fingerprint = fp;
        }
    }
}

impl Default for AiPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Panel body, rendered inside each app's `sidebar_section`. Identical in
/// Atlas and Slate by construction.
pub fn ai_body(panel: &mut AiPanel, ui: &mut egui::Ui, theme: SidebarTheme) {
    sidebar_region(ui, "Cursor", theme, |ui| {
        ui.horizontal(|ui| {
            let (dot, msg) = if panel.cursor_available {
                (
                    Color32::from_rgb(0x3f, 0xb9 - 0x10, 0x50),
                    "Cursor detected",
                )
            } else {
                (Color32::from_rgb(0xd0, 0x8a, 0x2e), "Cursor not detected")
            };
            ui.label(RichText::new("●").color(dot));
            ui.label(RichText::new(msg).small().color(theme.sub));
        });
        if ui
            .button("Launch Cursor")
            .on_hover_text(
                "Opens Cursor in the AI workspace folder. On first launch you'll be \
                 asked to establish that folder; it is shared by File Atlas and every \
                 Slate workbook.",
            )
            .clicked()
        {
            panel.launch_cursor();
        }
    });

    sidebar_subtle_divider(ui, theme);
    sidebar_region(ui, "AI workspace", theme, |ui| {
        match panel.config.valid_workspace() {
            Some(ws) => {
                let name = ws
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| ws.display().to_string());
                ui.label(RichText::new(name).small().color(theme.ink))
                    .on_hover_text(ws.display().to_string());
            }
            None => {
                ui.label(
                    RichText::new("Not set — required before the first launch")
                        .small()
                        .color(theme.sub),
                );
            }
        }
        sidebar_toolbar_row(ui, |ui| {
            let label = if panel.config.valid_workspace().is_some() {
                "Change…"
            } else {
                "Set folder…"
            };
            if ui
                .button(label)
                .on_hover_text(
                    "Establish the folder Cursor works in when launched from Atlas or Slate",
                )
                .clicked()
            {
                panel.pick_workspace();
            }
            if panel.config.valid_workspace().is_some() && ui.button("Reveal").clicked() {
                if let Some(ws) = panel.config.valid_workspace() {
                    crate::launch::reveal_dir(ws);
                }
            }
        });
    });

    sidebar_subtle_divider(ui, theme);
    ui.label(
        RichText::new(
            "A live context file in the workspace mirrors what's open here, so \
             Cursor (and upcoming MCP servers) can see and act on the files being \
             previewed.",
        )
        .small()
        .color(theme.sub),
    );

    if let Some(status) = &panel.status {
        ui.add_space(2.0);
        ui.label(RichText::new(status).small().italics().color(theme.sub));
    }
}
