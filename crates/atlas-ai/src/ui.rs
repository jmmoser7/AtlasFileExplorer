//! The AI sidebar panel — one implementation rendered by both apps so the
//! toolbar is identical in Atlas and Slate (shared-chrome rule).
//! Cursor detection runs off the UI thread.

use crate::config::AiConfig;
use crate::context::{now_secs, write_context, AiAppContext};
use crate::launch;
use atlas_shell::file_picker::{self, DialogOwner, FilePicker, PickRequest};
use atlas_shell::sidebar::{
    sidebar_region, sidebar_subtle_divider, sidebar_toolbar_row, SidebarTheme,
};
use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;
use std::time::Instant;

/// Minimum interval between context-beacon writes.
const PROGRAM_HOVER_SECONDS: f32 = 0.12;
const BEACON_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Per-app AI panel state. Construct once, keep on the app struct, call
/// [`AiPanel::poll`] every frame and [`AiPanel::update_context`] whenever a
/// frame ends (it self-throttles).
pub struct AiPanel {
    pub config: AiConfig,
    /// `None` until the background probe finishes.
    cursor_available: Option<bool>,
    cursor_rx: Option<Receiver<bool>>,
    picker: FilePicker<Option<PathBuf>>,
    /// Transient status line shown at the bottom of the panel.
    pub status: Option<String>,
    last_fingerprint: u64,
    last_beacon: Option<Instant>,
    /// The workspace may be a synced or network folder: beacons are written
    /// on their own thread, newest first.
    beacon_tx: Option<Sender<(PathBuf, AiAppContext)>>,
}

impl AiPanel {
    pub fn new() -> Self {
        let (cursor_tx, cursor_rx) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = cursor_tx.send(launch::cursor_available());
        });
        AiPanel {
            config: AiConfig::load(),
            cursor_available: None,
            cursor_rx: Some(cursor_rx),
            picker: FilePicker::default(),
            status: None,
            last_fingerprint: 0,
            last_beacon: None,
            beacon_tx: None,
        }
    }

    /// True while the workspace folder picker is open.
    pub fn picker_pending(&self) -> bool {
        self.picker.is_open()
    }

    /// The app window the workspace picker is modal to.
    pub fn set_dialog_owner(&mut self, owner: Option<DialogOwner>) {
        self.picker.set_owner(owner);
    }

    /// See [`FilePicker::gate_input`].
    pub fn gate_input(&self, raw: &mut egui::RawInput) -> bool {
        self.picker.gate_input(raw)
    }

    /// See [`FilePicker::close`].
    pub fn close_picker(&mut self) {
        self.picker.close();
    }

    /// Drain the async folder picker and the Cursor probe. Returns true while
    /// the probe is still running so the caller can wake one more frame.
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        if let Some(rx) = self.cursor_rx.take() {
            match rx.try_recv() {
                Ok(found) => self.cursor_available = Some(found),
                Err(crossbeam_channel::TryRecvError::Empty) => self.cursor_rx = Some(rx),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.cursor_available = Some(false);
                }
            }
        }
        let cursor_pending = self.cursor_rx.is_some();
        if let Some(Some(dir)) = self.picker.poll(ctx) {
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
        cursor_pending
    }

    /// Open the async "establish AI workspace" folder picker.
    pub fn pick_workspace(&mut self) {
        let mut request = PickRequest::folder().title("Choose the AI workspace folder");
        if let Some(dir) = self.config.workspace_dir.clone() {
            request = request.directory(dir);
        }
        self.picker.open(request, file_picker::first);
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
                self.cursor_available = Some(launch::cursor_available());
                self.status = Some(e);
            }
        }
    }

    /// Maintain the live-link beacon. `build` is only called when the
    /// throttle window has elapsed; the file is only rewritten when content
    /// actually changed.
    /// Whether [`AiPanel::update_context`] would build a beacon now. Lets the
    /// app skip gathering selection and file lists on the frames in between.
    pub fn beacon_due(&self) -> bool {
        self.last_beacon
            .is_none_or(|t| t.elapsed() >= BEACON_INTERVAL)
    }

    pub fn update_context(&mut self, build: impl FnOnce() -> AiAppContext) {
        if let Some(t) = self.last_beacon {
            if t.elapsed() < BEACON_INTERVAL {
                return;
            }
        }
        let Some(ws) = self.config.valid_workspace().map(PathBuf::from) else {
            return;
        };
        self.last_beacon = Some(Instant::now());
        let mut ctx = build();
        ctx.generated_at = now_secs();
        let fp = ctx.fingerprint();
        if fp == self.last_fingerprint {
            return;
        }
        self.last_fingerprint = fp;
        let tx = self.beacon_tx.get_or_insert_with(|| {
            let (tx, rx) = crossbeam_channel::unbounded::<(PathBuf, AiAppContext)>();
            std::thread::spawn(move || {
                while let Ok(mut job) = rx.recv() {
                    while let Ok(newer) = rx.try_recv() {
                        job = newer;
                    }
                    let _ = write_context(&job.0, &job.1);
                }
            });
            tx
        });
        let _ = tx.send((ws, ctx));
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
            let (dot, msg) = match panel.cursor_available {
                None => (Color32::from_rgb(0x8a, 0x90, 0x98), "Cursor status unknown"),
                Some(true) => (
                    Color32::from_rgb(0x3f, 0xb9 - 0x10, 0x50),
                    "Cursor detected",
                ),
                Some(false) => (Color32::from_rgb(0xd0, 0x8a, 0x2e), "Cursor not detected"),
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
/// Lobby for choosing a program: icons and names only. The host card is the
/// surface; there are no button wells. The portal supplies focus and
/// discovery data; this shared AI surface owns its appearance.
pub fn program_grid(
    ui: &egui::Ui,
    rect: egui::Rect,
    id: egui::Id,
    programs: &[crate::agent::AgentProvider],
    interactive: bool,
    zoom: f32,
) -> Option<String> {
    let cell = egui::vec2(112.0, 96.0) * zoom;
    let columns = ((rect.width() / cell.x).floor() as usize)
        .clamp(1, 4)
        .min(programs.len().max(1));
    let rows = programs.len().div_ceil(columns);
    let origin = rect.center() - egui::vec2(columns as f32 * cell.x, rows as f32 * cell.y) * 0.5;
    let painter = ui.painter_at(rect);
    for (i, program) in programs.iter().enumerate() {
        let slot = egui::Rect::from_min_size(
            origin + egui::vec2((i % columns) as f32 * cell.x, (i / columns) as f32 * cell.y),
            cell,
        )
        .shrink(6.0 * zoom);
        let response = ui.interact(
            slot,
            id.with(i),
            if interactive {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        let hover = ui.ctx().animate_bool_with_time(
            id.with(("hover", i)),
            response.hovered(),
            PROGRAM_HOVER_SECONDS,
        );
        let color = program_lobby_color(ui.visuals().text_color(), hover);
        let icon = match program.id.as_str() {
            "cursor" => atlas_shell::icons::Icon::ProviderCursor,
            "codex" => atlas_shell::icons::Icon::ProviderCodex,
            "ollama" => atlas_shell::icons::Icon::ProviderOllama,
            "image-link" | "comfy" => atlas_shell::icons::Icon::Brush,
            _ => atlas_shell::icons::Icon::Lens,
        };
        atlas_shell::icons::paint(
            &painter,
            egui::Rect::from_center_size(
                slot.center() - egui::vec2(0.0, 10.0 * zoom),
                egui::vec2(30.0, 30.0) * zoom,
            ),
            icon,
            color,
        );
        atlas_shell::canvas_text::text(
            &painter,
            egui::pos2(slot.center().x, slot.bottom() - 12.0 * zoom),
            egui::Align2::CENTER_CENTER,
            &program.display_name,
            egui::FontId::proportional(12.0 * zoom),
            color,
        );
        if response.clicked() {
            return Some(program.id.clone());
        }
    }
    None
}

/// Pointer feedback for a lobby entry: the glyph and name brighten, with no well.
fn program_lobby_color(base: Color32, hover: f32) -> Color32 {
    let t = (hover * 0.45).clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (base.r() as f32 + (255.0 - base.r() as f32) * t) as u8,
        (base.g() as f32 + (255.0 - base.g() as f32) * t) as u8,
        (base.b() as f32 + (255.0 - base.b() as f32) * t) as u8,
        base.a(),
    )
}
